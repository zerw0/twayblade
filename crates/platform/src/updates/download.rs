//! Downloading a direct-download installer (#56). Builds are unsigned (PLAN.md,
//! no paid signing), so the download is checked against the release's
//! `SHA256SUMS`: it catches a corrupt or truncated download, not a compromised
//! release. Nothing is installed or replaced here: the verified file is handed
//! to the OS (the DMG opens, the MSI runs its installer), and package-managed
//! installs never download.
use super::{Installation, PackageManager};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

const RELEASES: &str = "https://github.com/zerw0/twayblade/releases/download";
const SUMS: &str = "SHA256SUMS";
const MAX_SUMS: usize = 64 * 1024;
/// No installer comes near this; anything larger is refused before download.
const MAX_ASSET: u64 = 1024 * 1024 * 1024;
/// Hosts GitHub sends release downloads through.
const DOWNLOAD_HOSTS: [&str; 3] = [
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// The installer of a release for this machine, and where its checksum is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Download {
    pub file: String,
    pub size: u64,
    /// Built from the validated tag and file name, never from a response URL.
    pub url: String,
    pub sums_url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DownloadError {
    #[error("update_download_network")]
    Network,
    #[error("update_download_http_{0}")]
    Http(u16),
    #[error("update_download_no_checksum")]
    NoChecksum,
    #[error("update_download_size")]
    Size,
    #[error("update_download_checksum_mismatch")]
    ChecksumMismatch,
    #[error("update_download_disk")]
    Disk,
    #[error("update_download_open")]
    Open,
}

#[derive(serde::Deserialize)]
pub(super) struct Asset {
    pub name: String,
    pub size: u64,
}

/// A release file name we will fetch: plain, no paths or URL syntax.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
}

/// The OS and CPU an installer must match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    MacOs { arm: bool },
    Windows,
    LinuxAppImage,
    Other,
}

impl Target {
    pub(super) fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs {
                arm: cfg!(target_arch = "aarch64"),
            }
        } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            Self::Windows
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64"))
            && std::env::var_os("APPIMAGE").is_some()
        {
            // Only a running AppImage is a direct download on Linux.
            Self::LinuxAppImage
        } else {
            Self::Other
        }
    }

    fn matches(self, name: &str) -> bool {
        match self {
            // `Twayblade-<version>-<lipo archs>.dmg`; a universal build lists both.
            Self::MacOs { arm } => name
                .strip_suffix(".dmg")
                .is_some_and(|stem| stem.contains(if arm { "arm64" } else { "x86_64" })),
            Self::Windows => name.ends_with("-x86_64.msi"),
            Self::LinuxAppImage => name.ends_with("-x86_64.AppImage"),
            Self::Other => false,
        }
    }
}

/// The installer to offer, if this install updates by direct download and the
/// release carries one for this machine and a checksum file.
pub(super) fn pick(
    tag: &str,
    assets: &[Asset],
    target: Target,
    installation: &Installation,
) -> Option<Download> {
    // Package managers own their installs; unknown ones are treated as direct
    // downloads only for this download-and-open step, never for replacement.
    if !matches!(
        installation.manager,
        PackageManager::Direct | PackageManager::Unknown
    ) {
        return None;
    }
    if !assets.iter().any(|asset| asset.name == SUMS) {
        return None;
    }
    let asset = assets
        .iter()
        .filter(|asset| safe_name(&asset.name) && target.matches(&asset.name))
        .find(|asset| asset.size > 0 && asset.size <= MAX_ASSET)?;
    Some(Download {
        file: asset.name.clone(),
        size: asset.size,
        url: format!("{RELEASES}/{tag}/{}", asset.name),
        sums_url: format!("{RELEASES}/{tag}/{SUMS}"),
    })
}

/// The SHA-256 `sha256sum` lists for `file` (text or `*binary` mode).
pub(super) fn expected_sum(sums: &str, file: &str) -> Option<[u8; 32]> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once(char::is_whitespace)?;
        let name = name.trim_start().trim_start_matches('*');
        if name != file || hash.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (index, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(hash.get(index * 2..index * 2 + 2)?, 16).ok()?;
        }
        Some(out)
    })
}

fn client() -> Result<reqwest::Client, DownloadError> {
    reqwest::Client::builder()
        .user_agent("Twayblade update download")
        .https_only(!cfg!(test))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let allowed = attempt.previous().len() < 5
                && attempt.url().scheme() == "https"
                && attempt
                    .url()
                    .host_str()
                    .is_some_and(|host| DOWNLOAD_HOSTS.contains(&host));
            if allowed {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(|_| DownloadError::Network)
}

async fn get(client: &reqwest::Client, url: &str) -> Result<reqwest::Response, DownloadError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| DownloadError::Network)?;
    if !response.status().is_success() {
        return Err(DownloadError::Http(response.status().as_u16()));
    }
    Ok(response)
}

/// Downloads `download` into `directory`, checks its size and SHA-256 against
/// the release, and only then gives it its final name. `progress` gets the
/// bytes received so far. A failed check leaves no file behind.
pub async fn fetch(
    download: &Download,
    directory: &Path,
    mut progress: impl FnMut(u64),
) -> Result<PathBuf, DownloadError> {
    let client = client()?;
    let mut response = get(&client, &download.sums_url).await?;
    let mut sums = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| DownloadError::Network)? {
        if chunk.len() > MAX_SUMS.saturating_sub(sums.len()) {
            return Err(DownloadError::NoChecksum);
        }
        sums.extend_from_slice(&chunk);
    }
    let expected = std::str::from_utf8(&sums)
        .ok()
        .and_then(|sums| expected_sum(sums, &download.file))
        .ok_or(DownloadError::NoChecksum)?;

    tokio::fs::create_dir_all(directory)
        .await
        .map_err(|_| DownloadError::Disk)?;
    let partial = directory.join(format!(".{}.partial", download.file));
    let result = async {
        let mut response = get(&client, &download.url).await?;
        if response
            .content_length()
            .is_some_and(|length| length != download.size)
        {
            return Err(DownloadError::Size);
        }
        let mut file = tokio::fs::File::create(&partial)
            .await
            .map_err(|_| DownloadError::Disk)?;
        let mut hasher = Sha256::new();
        let mut received = 0u64;
        while let Some(chunk) = response.chunk().await.map_err(|_| DownloadError::Network)? {
            received = received.saturating_add(chunk.len() as u64);
            if received > download.size {
                return Err(DownloadError::Size);
            }
            hasher.update(&chunk);
            tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
                .await
                .map_err(|_| DownloadError::Disk)?;
            progress(received);
        }
        tokio::io::AsyncWriteExt::flush(&mut file)
            .await
            .map_err(|_| DownloadError::Disk)?;
        if received != download.size {
            return Err(DownloadError::Size);
        }
        if hasher.finalize().as_slice() != expected {
            return Err(DownloadError::ChecksumMismatch);
        }
        let target = directory.join(&download.file);
        tokio::fs::rename(&partial, &target)
            .await
            .map_err(|_| DownloadError::Disk)?;
        Ok(target)
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&partial).await;
    }
    result
}

/// Where downloads go: the user's Downloads folder, else the app cache.
pub fn directory() -> Option<PathBuf> {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(Path::to_path_buf))
        .or_else(|| {
            directories::ProjectDirs::from("", "", "Twayblade")
                .map(|dirs| dirs.cache_dir().join("updates"))
        })
}

/// Hands a verified installer to the OS: macOS opens the disk image, Windows
/// starts the MSI installer, Linux marks the AppImage executable and shows its
/// folder. The running app is left alone; the user quits it to finish.
pub fn open(path: &Path) -> Result<(), DownloadError> {
    let mut command = if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("/usr/bin/open");
        command.arg(path);
        command
    } else if cfg!(target_os = "windows") {
        let mut command = std::process::Command::new("msiexec");
        command.arg("/i").arg(path);
        command
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
                .map_err(|_| DownloadError::Open)?;
        }
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path.parent().ok_or(DownloadError::Open)?);
        command
    };
    command.spawn().map(|_| ()).map_err(|_| DownloadError::Open)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str, size: u64) -> Asset {
        Asset {
            name: name.into(),
            size,
        }
    }

    fn release() -> Vec<Asset> {
        vec![
            asset("Twayblade-0.2.0-arm64.dmg", 90),
            asset("Twayblade-0.2.0-x86_64.dmg", 91),
            asset("Twayblade-0.2.0-x86_64.msi", 92),
            asset("Twayblade-0.2.0-x86_64.AppImage", 93),
            asset("twayblade_0.2.0_amd64.deb", 94),
            asset("SHA256SUMS", 1),
        ]
    }

    #[test]
    fn picks_this_machines_installer_from_a_fixed_origin() {
        let direct = Installation::default();
        let mac = pick("v0.2.0", &release(), Target::MacOs { arm: true }, &direct).unwrap();
        assert_eq!(mac.file, "Twayblade-0.2.0-arm64.dmg");
        assert_eq!(
            mac.url,
            "https://github.com/zerw0/twayblade/releases/download/v0.2.0/Twayblade-0.2.0-arm64.dmg"
        );
        assert!(mac.sums_url.ends_with("/v0.2.0/SHA256SUMS"));
        let intel = pick("v0.2.0", &release(), Target::MacOs { arm: false }, &direct).unwrap();
        assert_eq!(intel.file, "Twayblade-0.2.0-x86_64.dmg");
        assert_eq!(
            pick("v0.2.0", &release(), Target::Windows, &direct)
                .unwrap()
                .file,
            "Twayblade-0.2.0-x86_64.msi"
        );
        assert_eq!(
            pick("v0.2.0", &release(), Target::LinuxAppImage, &direct)
                .unwrap()
                .file,
            "Twayblade-0.2.0-x86_64.AppImage"
        );
        assert!(pick("v0.2.0", &release(), Target::Other, &direct).is_none());
    }

    #[test]
    fn package_managers_unsafe_names_and_missing_checksums_get_nothing() {
        let brew = Installation {
            manager: PackageManager::Homebrew,
            ..Default::default()
        };
        assert!(pick("v0.2.0", &release(), Target::MacOs { arm: true }, &brew).is_none());
        let direct = Installation::default();
        let no_sums: Vec<_> = release()
            .into_iter()
            .filter(|asset| asset.name != "SHA256SUMS")
            .collect();
        assert!(pick("v0.2.0", &no_sums, Target::MacOs { arm: true }, &direct).is_none());
        let hostile = vec![
            asset("../Twayblade-arm64.dmg", 9),
            asset("Twayblade%2F-arm64.dmg", 9),
            asset("Twayblade-huge-arm64.dmg", MAX_ASSET + 1),
            asset("SHA256SUMS", 1),
        ];
        assert!(pick("v0.2.0", &hostile, Target::MacOs { arm: true }, &direct).is_none());
    }

    #[test]
    fn reads_sha256sum_lines_for_one_file() {
        let hash = "ab".repeat(32);
        let sums = format!(
            "{}  other.dmg\n{hash}  Twayblade-0.2.0-arm64.dmg\n{hash} *Twayblade-bin.msi\nshort  x.dmg\n",
            "cd".repeat(32)
        );
        assert_eq!(
            expected_sum(&sums, "Twayblade-0.2.0-arm64.dmg"),
            Some([0xab; 32])
        );
        assert_eq!(expected_sum(&sums, "Twayblade-bin.msi"), Some([0xab; 32]));
        assert_eq!(expected_sum(&sums, "x.dmg"), None);
        assert_eq!(expected_sum(&sums, "missing.dmg"), None);
    }
}
