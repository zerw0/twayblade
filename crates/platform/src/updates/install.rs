//! Read-only ownership detection; no upgrade commands are executed.
use std::path::Path;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PackageManager {
    Direct,
    Homebrew,
    Deb,
    Rpm,
    Aur,
    Nix,
    Winget,
    #[default]
    Unknown,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Installation {
    pub manager: PackageManager,
    /// A whitelisted package name, never subprocess text or a local path.
    pub package: Option<String>,
    /// Present only when an installed AUR helper is found.
    pub aur_helper: Option<String>,
}
impl Installation {
    pub fn upgrade_command(&self) -> Option<String> {
        let package = self
            .package
            .as_deref()
            .filter(|name| matches!(*name, "twayblade" | "twayblade-bin" | "twayblade-git"))
            .unwrap_or("twayblade");
        Some(match self.manager {
            PackageManager::Homebrew => "brew upgrade --cask twayblade".into(),
            PackageManager::Deb => "sudo apt install --only-upgrade twayblade".into(),
            PackageManager::Rpm => "sudo dnf upgrade twayblade".into(),
            PackageManager::Aur => {
                let helper = self
                    .aur_helper
                    .as_deref()
                    .filter(|h| matches!(*h, "yay" | "paru"))?;
                format!("{helper} -Syu {package}")
            }
            PackageManager::Nix => "nix profile upgrade twayblade".into(),
            PackageManager::Winget => "winget upgrade --id zerw0.Twayblade --exact".into(),
            PackageManager::Direct | PackageManager::Unknown => return None,
        })
    }
}
fn managed(manager: PackageManager, package: Option<&str>) -> Installation {
    Installation {
        manager,
        package: package.map(str::to_owned),
        aur_helper: None,
    }
}
fn from_path(path: &Path) -> Option<Installation> {
    let path = path.to_string_lossy().replace('\\', "/");
    if path.starts_with("/nix/store/") {
        return Some(managed(PackageManager::Nix, None));
    }
    if path
        .split('/')
        .collect::<Vec<_>>()
        .windows(2)
        .any(|p| p == ["Caskroom", "twayblade"])
    {
        return Some(managed(PackageManager::Homebrew, None));
    }
    let lower = path.to_ascii_lowercase();
    if lower.contains("/microsoft/winget/packages/zerw0.twayblade_") {
        return Some(managed(PackageManager::Winget, None));
    }
    None
}
#[cfg(any(target_os = "linux", test))]
fn owner(text: &str) -> Option<&str> {
    text.split_whitespace().find_map(|word| {
        let word = word.trim_end_matches(':');
        ["twayblade", "twayblade-bin", "twayblade-git"]
            .into_iter()
            .find(|package| {
                word == *package
                    || word
                        .strip_prefix(package)
                        .is_some_and(|suffix| suffix.starts_with(':'))
            })
    })
}
#[cfg(any(target_os = "linux", target_os = "windows"))]
async fn query(program: &str, args: &[&str]) -> Option<String> {
    use tokio::io::AsyncReadExt;
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut child = tokio::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .ok()?;
        let mut bytes = Vec::new();
        child
            .stdout
            .take()?
            .take(16 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await
            .ok()?;
        if bytes.len() > 16 * 1024 {
            return None;
        }
        if !child.wait().await.ok()?.success() {
            return None;
        }
        String::from_utf8(bytes).ok()
    })
    .await
    .ok()
    .flatten()
}
fn aur_helper() -> Option<String> {
    let paths = std::env::var_os("PATH")?;
    ["paru", "yay"]
        .into_iter()
        .find(|name| std::env::split_paths(&paths).any(|p| p.join(name).is_file()))
        .map(str::to_owned)
}
pub(super) async fn detect() -> Installation {
    // Package builders may provide provenance for layouts without useful paths.
    let built = match option_env!("TWAYBLADE_PACKAGE_MANAGER") {
        Some("direct") => Some(PackageManager::Direct),
        Some("nix") => Some(PackageManager::Nix),
        Some("homebrew") => Some(PackageManager::Homebrew),
        Some("deb") => Some(PackageManager::Deb),
        Some("rpm") => Some(PackageManager::Rpm),
        Some("aur") => Some(PackageManager::Aur),
        Some("winget") => Some(PackageManager::Winget),
        _ => None,
    };
    if let Some(manager) = built {
        return Installation {
            aur_helper: if manager == PackageManager::Aur {
                aur_helper()
            } else {
                None
            },
            ..managed(manager, None)
        };
    }
    let Ok(executable) = std::env::current_exe().and_then(std::fs::canonicalize) else {
        return Installation::default();
    };
    if let Some(origin) = from_path(&executable) {
        return origin;
    }
    detect_owned(&executable).await
}
#[cfg(target_os = "macos")]
async fn detect_owned(executable: &Path) -> Installation {
    // Homebrew casks can move the app into /Applications rather than symlink it.
    if executable.starts_with("/Applications/Twayblade.app/") {
        for prefix in ["/opt/homebrew", "/usr/local"] {
            if Path::new(prefix)
                .join("Caskroom/twayblade/.metadata")
                .is_dir()
            {
                return managed(PackageManager::Homebrew, None);
            }
        }
    }
    managed(PackageManager::Unknown, None)
}
#[cfg(target_os = "linux")]
async fn detect_owned(executable: &Path) -> Installation {
    let Some(path) = executable.to_str() else {
        return Installation::default();
    };
    for (manager, program, args) in [
        (PackageManager::Deb, "dpkg-query", vec!["-S", path]),
        (
            PackageManager::Rpm,
            "rpm",
            vec!["-qf", "--qf", "%{NAME}", path],
        ),
        (PackageManager::Aur, "pacman", vec!["-Qoq", path]),
    ] {
        if let Some(output) = query(program, &args).await
            && let Some(package) = owner(&output)
        {
            if manager == PackageManager::Aur && query("pacman", &["-Qm", package]).await.is_none()
            {
                // Repository-owned Arch packages are not proof of AUR origin.
                return managed(PackageManager::Unknown, Some(package));
            }
            return Installation {
                manager,
                package: Some(package.into()),
                aur_helper: if manager == PackageManager::Aur {
                    aur_helper()
                } else {
                    None
                },
            };
        }
    }
    managed(PackageManager::Unknown, None)
}
#[cfg(target_os = "windows")]
async fn detect_owned(_executable: &Path) -> Installation {
    // winget list reads installed-package registration. Do not accept source
    // agreements or trigger installation, authentication or elevation.
    if let Some(output) = query(
        "winget",
        &[
            "list",
            "--id",
            "zerw0.Twayblade",
            "--exact",
            "--disable-interactivity",
        ],
    )
    .await
        && output
            .split_whitespace()
            .any(|word| word.eq_ignore_ascii_case("zerw0.Twayblade"))
    {
        return managed(PackageManager::Winget, None);
    }
    managed(PackageManager::Unknown, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifies_canonical_package_layouts_without_recording_paths() {
        for (path, manager) in [
            (
                "/nix/store/hash-twayblade/bin/twayblade",
                PackageManager::Nix,
            ),
            (
                "/opt/homebrew/Caskroom/twayblade/1.0/Twayblade.app/Contents/MacOS/twayblade",
                PackageManager::Homebrew,
            ),
            (
                "C:\\Users\\fixture\\AppData\\Local\\Microsoft\\WinGet\\Packages\\zerw0.Twayblade_source\\twayblade.exe",
                PackageManager::Winget,
            ),
        ] {
            assert_eq!(from_path(Path::new(path)).unwrap().manager, manager);
        }
        assert_eq!(from_path(Path::new("/tmp/not-Caskroom/twayblade")), None);
        assert_eq!(
            from_path(Path::new("/opt/homebrew/Caskroom/other/app")),
            None
        );
    }
    #[test]
    fn accepts_only_known_owned_package_names_and_commands() {
        assert_eq!(
            owner("twayblade:amd64: /usr/bin/twayblade"),
            Some("twayblade")
        );
        assert_eq!(owner("twayblade-bin"), Some("twayblade-bin"));
        assert_eq!(owner("other-twayblade"), None);
        assert_eq!(owner("twayblade-malware"), None);
        let mut installation = managed(PackageManager::Aur, Some("twayblade-bin"));
        assert_eq!(installation.upgrade_command(), None);
        installation.aur_helper = Some("paru".into());
        assert_eq!(
            installation.upgrade_command().as_deref(),
            Some("paru -Syu twayblade-bin")
        );
        installation.aur_helper = Some("$(untrusted)".into());
        assert_eq!(installation.upgrade_command(), None);
        installation.manager = PackageManager::Deb;
        installation.package = Some("$(untrusted)".into());
        assert_eq!(
            installation.upgrade_command().as_deref(),
            Some("sudo apt install --only-upgrade twayblade")
        );
    }
    #[test]
    fn managed_commands_are_informational_and_direct_downloads_have_none() {
        for manager in [
            PackageManager::Homebrew,
            PackageManager::Deb,
            PackageManager::Rpm,
            PackageManager::Nix,
            PackageManager::Winget,
        ] {
            assert!(managed(manager, None).upgrade_command().is_some());
        }
        assert_eq!(
            managed(PackageManager::Direct, None).upgrade_command(),
            None
        );
    }
}
