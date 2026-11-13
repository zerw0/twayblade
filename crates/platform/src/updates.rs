//! Public-release notices only. No account/SDK inputs, downloads or installation.
mod install;
pub use install::{Installation, PackageManager};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::{sync::watch, time::Instant};

const API: &str = "https://api.github.com/repos/zerw0/twayblade/releases";
const DAILY: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_RESPONSE: usize = 4 * 1024 * 1024;
const MAX_CHANGELOG: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    #[default]
    Stable,
    Prerelease,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub enabled: bool,
    pub channel: Channel,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            enabled: true,
            channel: Channel::Stable,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UpdateError {
    #[error("update_network")]
    Network,
    #[error("update_http_{0}")]
    Http(u16),
    #[error("update_invalid_response")]
    InvalidResponse,
    #[error("update_response_too_large")]
    ResponseTooLarge,
    #[error("update_invalid_current_version")]
    InvalidCurrentVersion,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Available {
    pub version: String,
    pub prerelease: bool,
    pub changelog: String,
    pub release_url: String,
    /// Display only. No command is ever executed by the updater.
    pub upgrade_command: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Disabled,
    Checking,
    UpToDate,
    Available,
    Failed(UpdateError),
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub phase: Phase,
    pub channel: Channel,
    pub installation: Installation,
    pub checked_unix_seconds: Option<u64>,
    pub available: Option<Available>,
}

struct Checker {
    client: reqwest::Client,
    api: String,
    current: Version,
}
impl Checker {
    fn new(api: &str, current: &str) -> Result<Self, UpdateError> {
        let client = reqwest::Client::builder()
            .user_agent("Twayblade update check")
            .redirect(reqwest::redirect::Policy::none())
            .https_only(!cfg!(test))
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| UpdateError::Network)?;
        Ok(Self {
            client,
            api: api.into(),
            current: Version::parse(current).map_err(|_| UpdateError::InvalidCurrentVersion)?,
        })
    }
    async fn check(
        &self,
        channel: Channel,
        installation: &Installation,
    ) -> Result<Option<Available>, UpdateError> {
        // Stable uses the maintainer-designated latest non-draft stable release.
        // Opt-in preview checks consider the newest 100 published releases.
        let url = match channel {
            Channel::Stable => format!("{}/latest", self.api),
            Channel::Prerelease => format!("{}?per_page=100", self.api),
        };
        let mut response = self
            .client
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2026-03-10")
            .send()
            .await
            .map_err(|_| UpdateError::Network)?;
        // A new repository may not have a stable release yet.
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(UpdateError::Http(response.status().as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_RESPONSE as u64)
        {
            return Err(UpdateError::ResponseTooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| UpdateError::Network)? {
            if chunk.len() > MAX_RESPONSE.saturating_sub(bytes.len()) {
                return Err(UpdateError::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        let releases: Vec<Release> = match channel {
            Channel::Stable => {
                vec![serde_json::from_slice(&bytes).map_err(|_| UpdateError::InvalidResponse)?]
            }
            Channel::Prerelease => {
                serde_json::from_slice(&bytes).map_err(|_| UpdateError::InvalidResponse)?
            }
        };
        Ok(select(releases, channel, &self.current, installation))
    }
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    body: Option<String>,
}
fn select(
    releases: Vec<Release>,
    channel: Channel,
    current: &Version,
    installation: &Installation,
) -> Option<Available> {
    let (version, release) = releases
        .into_iter()
        .filter_map(|release| {
            if release.draft || release.tag_name.len() > 128 {
                return None;
            }
            let version = Version::parse(
                release
                    .tag_name
                    .strip_prefix('v')
                    .unwrap_or(&release.tag_name),
            )
            .ok()?;
            if !version.cmp_precedence(current).is_gt()
                || (channel == Channel::Stable && (release.prerelease || !version.pre.is_empty()))
            {
                return None;
            }
            Some((version, release))
        })
        .max_by(|(a, _), (b, _)| a.cmp_precedence(b))?;
    let mut changelog = release.body.unwrap_or_default();
    if changelog.len() > MAX_CHANGELOG {
        let mut end = MAX_CHANGELOG;
        while !changelog.is_char_boundary(end) {
            end -= 1;
        }
        changelog.truncate(end);
    }
    // Build the destination from a validated SemVer tag, never a response URL.
    Some(Available {
        version: version.to_string(),
        prerelease: release.prerelease || !version.pre.is_empty(),
        release_url: format!(
            "https://github.com/zerw0/twayblade/releases/tag/{}",
            release.tag_name
        ),
        changelog,
        upgrade_command: installation.upgrade_command(),
    })
}

/// At launch, then at most once every 24 hours while enabled. Changing channel
/// or enabling checks starts a fresh check; disabling cancels an in-flight check
/// and clears notices. Dropping the config sender ends the service.
pub async fn run(config: watch::Receiver<Preferences>, publish: impl FnMut(Snapshot)) {
    let checker = Checker::new(API, env!("CARGO_PKG_VERSION"));
    run_with(config, publish, checker, DAILY).await;
}
async fn run_with(
    mut config: watch::Receiver<Preferences>,
    mut publish: impl FnMut(Snapshot),
    checker: Result<Checker, UpdateError>,
    period: Duration,
) {
    let mut snapshot = Snapshot::default();
    let mut preferences = *config.borrow_and_update();
    let mut next = Instant::now();
    loop {
        snapshot.channel = preferences.channel;
        if !preferences.enabled {
            snapshot.phase = Phase::Disabled;
            snapshot.available = None;
            publish(snapshot.clone());
            if config.changed().await.is_err() {
                return;
            }
            preferences = *config.borrow_and_update();
            next = Instant::now();
            continue;
        }
        tokio::select! {
            changed=config.changed()=>{
                if changed.is_err() {return;}
                let updated=*config.borrow_and_update();
                if updated!=preferences {snapshot.available=None;next=Instant::now();}
                preferences=updated;
            }
            _=tokio::time::sleep_until(next)=>{
                snapshot.phase=Phase::Checking; publish(snapshot.clone());
                let channel=preferences.channel;
                let request=async {
                    let installation=install::detect().await;
                    let result=match &checker {Ok(checker)=>checker.check(channel,&installation).await,Err(error)=>Err(*error)};
                    (installation,result)
                };
                tokio::pin!(request);
                let response=loop {
                    tokio::select! {
                        changed=config.changed()=>{
                            if changed.is_err() {return;}
                            let updated=*config.borrow_and_update();
                            if updated==preferences {continue;}
                            preferences=updated; snapshot.available=None; next=Instant::now();
                            break None;
                        }
                        response=&mut request=>break Some(response),
                    }
                };
                if let Some((installation,result))=response {
                        // A disable/channel change racing a completed response
                        // must not publish a notice for the old configuration.
                        let updated=*config.borrow_and_update();
                        if updated!=preferences {
                            preferences=updated;snapshot.available=None;next=Instant::now();continue;
                        }
                        snapshot.installation=installation;
                        snapshot.checked_unix_seconds=SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|v|v.as_secs());
                        match result {
                            Ok(available)=>{snapshot.phase=if available.is_some(){Phase::Available}else{Phase::UpToDate};snapshot.available=available;}
                            Err(error)=>snapshot.phase=Phase::Failed(error),
                        }
                        publish(snapshot.clone()); next=Instant::now()+period;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "updates/tests.rs"]
mod tests;
