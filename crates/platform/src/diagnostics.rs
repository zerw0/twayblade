//! Opt-in, payload-free tracing. SDK free text/debug values are intentionally
//! never formatted: error Display/Debug can contain HTTP bodies or credentials.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id, Record},
};
use tracing_subscriber::{EnvFilter, Layer, layer::Context, prelude::*, registry::LookupSpan};

const MAX_BYTES: u64 = 2 * 1024 * 1024;
const FILES: usize = 4;
const DEFAULT_FILTER: &str = "off,twayblade=debug,twayblade_core=debug,twayblade_platform=debug,twayblade_media=debug,matrix_sdk=info,matrix_sdk_ui=info,matrix_sdk_base=warn,matrix_sdk_common::cross_process_lock=info,matrix_sdk_sqlite=info,matrix_sdk_sqlite::crypto_store=warn";

pub fn log_directory() -> Option<PathBuf> {
    let base = directories::BaseDirs::new()?;
    if cfg!(target_os = "macos") {
        Some(base.home_dir().join("Library/Logs/Twayblade"))
    } else if cfg!(target_os = "windows") {
        Some(base.data_local_dir().join("Twayblade/logs"))
    } else {
        Some(
            base.state_dir()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| base.home_dir().join(".local/state"))
                .join("Twayblade/logs"),
        )
    }
}

/// Off unless explicitly set. Returns only a typed I/O error; caller must never
/// format an SDK error or the environment filter into a diagnostic message.
pub fn init() -> io::Result<()> {
    let Some(value) = std::env::var_os("TWAYBLADE_LOG") else {
        return Ok(());
    };
    let value = value
        .to_str()
        .ok_or_else(|| io::Error::other("Invalid diagnostics filter"))?;
    let filter = if value == "1" { DEFAULT_FILTER } else { value };
    let filter = EnvFilter::builder()
        .with_regex(false)
        .parse(filter)
        .map_err(|_| io::Error::other("Invalid diagnostics filter"))?;
    let directory = log_directory().ok_or_else(|| io::Error::other("Log directory unavailable"))?;
    let writer = RotatingFile::new(&directory, MAX_BYTES, FILES)?;
    tracing_subscriber::registry()
        .with(filter)
        .with(SafeLayer {
            writer: Mutex::new(writer),
        })
        .try_init()
        .map_err(|_| io::Error::other("Diagnostics subscriber unavailable"))
}

struct RotatingFile {
    _lock: File,
    directory: PathBuf,
    file: Option<File>,
    bytes: u64,
    limit: u64,
    files: usize,
}
impl RotatingFile {
    fn open(path: &Path) -> io::Result<File> {
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        Self::open_with(path, &mut options)
    }
    /// Windows `LockFileEx` needs read or write access; append-only handles
    /// lack `FILE_WRITE_DATA` and fail with "Access is denied".
    fn open_lock(path: &Path) -> io::Result<File> {
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true).truncate(false);
        Self::open_with(path, &mut options)
    }
    fn open_with(path: &Path, options: &mut OpenOptions) -> io::Result<File> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        Ok(file)
    }
    fn new(directory: &Path, limit: u64, files: usize) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        }
        // A preview or a second launch must not race the active log writer's
        // size accounting/rotation. The OS releases this lock on process exit.
        let lock = Self::open_lock(&directory.join(".diagnostics.lock"))?;
        lock.try_lock().map_err(io::Error::other)?;
        let file = Self::open(&directory.join("diagnostics.log"))?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            _lock: lock,
            directory: directory.to_path_buf(),
            file: Some(file),
            bytes,
            limit,
            files,
        })
    }
    fn rotate(&mut self) -> io::Result<()> {
        if let Some(mut file) = self.file.take() {
            file.flush()?;
        }
        for index in (1..self.files).rev() {
            let source = self.directory.join(if index == 1 {
                "diagnostics.log".into()
            } else {
                format!("diagnostics.{}.log", index - 1)
            });
            let destination = self.directory.join(format!("diagnostics.{index}.log"));
            if destination.exists() {
                fs::remove_file(&destination)?;
            }
            if source.exists() {
                fs::rename(source, destination)?;
            }
        }
        self.file = Some(Self::open(&self.directory.join("diagnostics.log"))?);
        self.bytes = 0;
        Ok(())
    }
}
impl Write for RotatingFile {
    fn write(&mut self, value: &[u8]) -> io::Result<usize> {
        // One oversized event cannot exceed the per-file cap.
        let value = &value[..value.len().min(self.limit as usize)];
        if self.bytes + value.len() as u64 > self.limit {
            self.rotate()?;
        }
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("Log file unavailable"))?
            .write_all(value)?;
        self.bytes += value.len() as u64;
        Ok(value.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            file.flush()
        } else {
            Ok(())
        }
    }
}

/// Allowlist rather than name-only redaction: secrets can also be embedded in
/// `message`, `error`, `response`, `event`, `url`, or an unknown future SDK field.
#[derive(Default)]
struct SafeFields(String);
impl SafeFields {
    fn numeric(name: &str) -> bool {
        matches!(
            name,
            "elapsed_ms"
                | "attempt"
                | "generation"
                | "retry_ms"
                | "status"
                | "num_attempt"
                | "success"
                | "timeout"
                | "cancelled"
                | "pending"
                | "matched"
                | "ready"
                | "revision"
                | "stale"
        )
    }
}
impl Visit for SafeFields {
    fn record_debug(&mut self, _: &Field, _: &dyn std::fmt::Debug) {} // Do not even evaluate Debug.
    fn record_str(&mut self, _: &Field, _: &str) {}
    fn record_u64(&mut self, field: &Field, value: u64) {
        if Self::numeric(field.name()) {
            use std::fmt::Write;
            let _ = write!(self.0, " {}={value}", field.name());
        }
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        if Self::numeric(field.name()) {
            use std::fmt::Write;
            let _ = write!(self.0, " {}={value}", field.name());
        }
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        if Self::numeric(field.name()) {
            use std::fmt::Write;
            let _ = write!(self.0, " {}={value}", field.name());
        }
    }
}
struct Started(Instant);
struct SafeLayer<W> {
    writer: Mutex<W>,
}
impl<W: Write> SafeLayer<W> {
    fn line(
        &self,
        kind: &str,
        meta: &tracing::Metadata<'_>,
        span: Option<u64>,
        parent: Option<u64>,
        fields: SafeFields,
    ) {
        if let Ok(mut writer) = self.writer.lock() {
            let time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            // Names/targets are static tracing metadata, never runtime values.
            let _ = writeln!(
                writer,
                "{time} {} {} {} {kind} span={span:?} parent={parent:?}{}",
                meta.level(),
                meta.target(),
                meta.name(),
                fields.0
            );
        }
    }
}
impl<S, W> Layer<S> for SafeLayer<W>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    W: Write + Send + 'static,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, cx: Context<'_, S>) {
        let mut fields = SafeFields::default();
        attrs.record(&mut fields);
        let parent = attrs.parent().cloned().or_else(|| {
            attrs
                .is_contextual()
                .then(|| cx.current_span().id().cloned())
                .flatten()
        });
        self.line(
            "start",
            attrs.metadata(),
            Some(id.into_u64()),
            parent.as_ref().map(Id::into_u64),
            fields,
        );
        if let Some(span) = cx.span(id) {
            span.extensions_mut().insert(Started(Instant::now()));
        }
    }
    fn on_record(&self, id: &Id, record: &Record<'_>, cx: Context<'_, S>) {
        if let Some(span) = cx.span(id) {
            let mut fields = SafeFields::default();
            record.record(&mut fields);
            self.line("record", span.metadata(), Some(id.into_u64()), None, fields);
        }
    }
    fn on_event(&self, event: &Event<'_>, cx: Context<'_, S>) {
        let mut fields = SafeFields::default();
        event.record(&mut fields);
        self.line(
            "event",
            event.metadata(),
            event
                .parent()
                .map(Id::into_u64)
                .or_else(|| cx.current_span().id().map(Id::into_u64)),
            None,
            fields,
        );
    }
    fn on_close(&self, id: Id, cx: Context<'_, S>) {
        if let Some(span) = cx.span(&id) {
            let fields = span
                .extensions()
                .get::<Started>()
                .map(|t| SafeFields(format!(" elapsed_ms={}", t.0.elapsed().as_millis())))
                .unwrap_or_default();
            self.line("end", span.metadata(), Some(id.into_u64()), None, fields);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[derive(Clone)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);
    impl Write for Buffer {
        fn write(&mut self, value: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend(value);
            Ok(value.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn formatter_drops_sensitive_fields_and_sdk_free_text_even_at_trace() {
        struct NeverFormat;
        impl std::fmt::Debug for NeverFormat {
            fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                panic!("private Debug must never run")
            }
        }
        let buffer = Buffer(Arc::default());
        let inspect = buffer.clone();
        let subscriber = tracing_subscriber::registry().with(SafeLayer {
            writer: Mutex::new(buffer),
        });
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!(
                "room_open",
                body = "secret-body",
                formatted_body = "secret-html",
                token = "secret-token",
                password = "secret-password",
                callback_url = "secret-callback",
                generation = 7u64
            );
            let _guard = span.enter();
            tracing::trace!(target: "matrix_sdk", body="secret-body", formatted_body="secret-html", token="secret-token", password="secret-password", error=?NeverFormat, response="secret-response", elapsed_ms=12u64, "secret-message");
            span.record("body", "secret-later");
        });
        let bytes = inspect.0.lock().unwrap();
        let output = String::from_utf8_lossy(&bytes);
        assert!(!output.contains("secret"));
        assert!(output.contains("generation=7"));
        assert!(output.contains("elapsed_ms=12"));
        assert!(output.contains("room_open"));
    }
    #[test]
    fn rotation_bounds_file_count_and_sizes() {
        let path =
            std::env::temp_dir().join(format!("twayblade-diagnostics-{}", uuid::Uuid::new_v4()));
        let mut writer = RotatingFile::new(&path, 128, 4).unwrap();
        assert!(
            RotatingFile::new(&path, 128, 4).is_err(),
            "only one process may rotate the log set"
        );
        for _ in 0..50 {
            writer.write_all(&[b'x'; 80]).unwrap();
        }
        drop(writer);
        let files: Vec<_> = fs::read_dir(&path)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .is_ok_and(|e| e.path().extension().is_some_and(|ext| ext == "log"))
            })
            .collect();
        assert_eq!(files.len(), 4);
        assert!(
            files
                .into_iter()
                .all(|f| f.unwrap().metadata().unwrap().len() <= 128)
        );
        fs::remove_dir_all(path).unwrap();
    }
}
