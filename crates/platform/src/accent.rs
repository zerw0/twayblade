//! One portal connection/runtime for the process, independent of UI windows.
use ashpd::desktop::{Color, settings::Settings};
use futures_util::StreamExt;
use std::{
    sync::{Arc, OnceLock, RwLock, mpsc},
    time::Duration,
};

type Cache = Arc<RwLock<Option<[f32; 3]>>>;
const TIMEOUT: Duration = Duration::from_secs(2);

pub(super) fn read() -> Option<[f32; 3]> {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| {
        let cache = Cache::default();
        let writer = cache.clone();
        let (ready, initial) = mpsc::sync_channel(1);
        if std::thread::Builder::new()
            .name("desktop-accent".into())
            .spawn(move || {
                if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    runtime.block_on(observe(writer, ready));
                }
            })
            .is_ok()
        {
            // This function is called on a background worker. Later reads only
            // read the cache, including the frontend's two-second refresh.
            let _ = initial.recv_timeout(TIMEOUT);
        }
        cache
    });
    *cache.read().ok()?
}

fn update(cache: &Cache, color: Option<Color>) {
    let color = color
        .map(|color| {
            [
                color.red() as f32,
                color.green() as f32,
                color.blue() as f32,
            ]
        })
        .filter(|rgb| {
            rgb.iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        });
    if let Ok(mut cached) = cache.write() {
        *cached = color;
    }
}

async fn observe(cache: Cache, ready: mpsc::SyncSender<()>) {
    let mut ready = Some(ready);
    loop {
        let connect = async {
            let connection = ashpd::zbus::Connection::session().await?;
            let settings = Settings::with_connection(connection.clone()).await?;
            Ok::<_, ashpd::Error>((connection, settings))
        };
        if let Ok(Ok((connection, settings))) = tokio::time::timeout(TIMEOUT, connect).await {
            // Subscribe before the initial read so a concurrent setting change
            // is queued and cannot be lost between the query and observation.
            let changes =
                tokio::time::timeout(TIMEOUT, settings.receive_accent_color_changed()).await;
            update(
                &cache,
                tokio::time::timeout(TIMEOUT, settings.accent_color())
                    .await
                    .ok()
                    .and_then(Result::ok),
            );
            if let Some(ready) = ready.take() {
                let _ = ready.send(());
            }
            if let Ok(Ok(changes)) = changes {
                tokio::pin!(changes);
                loop {
                    tokio::select! {
                        color = changes.next() => match color { Some(color) => update(&cache, Some(color)), None => break },
                        _ = connection.closed() => break,
                    }
                }
            } else {
                // Older portals may lack signals. Poll through the same proxy
                // and connection; never recreate them for an ordinary refresh.
                while !connection.is_closed() {
                    tokio::time::sleep(TIMEOUT).await;
                    update(
                        &cache,
                        tokio::time::timeout(TIMEOUT, settings.accent_color())
                            .await
                            .ok()
                            .and_then(Result::ok),
                    );
                }
            }
        }
        update(&cache, None);
        if let Some(ready) = ready.take() {
            let _ = ready.send(());
        }
        // A missing portal or disconnected bus is retried without a tight loop.
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
