//! The non-secret list of saved accounts. Credentials stay in each account's
//! own OS credential slot; this file only says which slots exist.
use super::AuthError;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
use uuid::Uuid;

const FILE: &str = "accounts.json";
const VERSION: u32 = 1;

/// Serializes read-modify-write cycles inside the process; the instance lock
/// keeps other processes out.
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct Index {
    pub(super) version: u32,
    pub(super) active: Option<Uuid>,
    pub(super) accounts: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct Entry {
    pub(super) store_id: Uuid,
    /// Empty until sign-in completes.
    pub(super) user_id: String,
    pub(super) homeserver: String,
    /// A sign-in in progress (or interrupted by a crash): its slot and store
    /// are removed at the next launch.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(super) pending: bool,
}

/// A saved account as shown in the account switcher. No credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedAccount {
    pub id: String,
    pub user_id: String,
    pub homeserver: String,
}

impl Index {
    pub(super) fn ready(&self) -> impl Iterator<Item = &Entry> {
        self.accounts.iter().filter(|entry| !entry.pending)
    }

    /// The account to open at launch: the last active one, else the first.
    pub(super) fn launch(&self) -> Option<Uuid> {
        self.active
            .filter(|id| self.ready().any(|entry| entry.store_id == *id))
            .or_else(|| self.ready().next().map(|entry| entry.store_id))
    }

    pub(super) fn remove(&mut self, store_id: Uuid) {
        self.accounts.retain(|entry| entry.store_id != store_id);
        if self.active == Some(store_id) {
            self.active = None;
        }
    }

    pub(super) fn saved(&self) -> Vec<SavedAccount> {
        self.ready()
            .map(|entry| SavedAccount {
                id: entry.store_id.to_string(),
                user_id: entry.user_id.clone(),
                homeserver: entry.homeserver.clone(),
            })
            .collect()
    }

    /// Marks a pending sign-in as finished and active. Fails without changes
    /// when the same Matrix user is already saved under another slot.
    pub(super) fn complete(&mut self, store_id: Uuid, user_id: &str) -> Result<(), AuthError> {
        if self
            .ready()
            .any(|entry| entry.store_id != store_id && entry.user_id == user_id)
        {
            return Err(AuthError::DuplicateAccount);
        }
        let entry = self
            .accounts
            .iter_mut()
            .find(|entry| entry.store_id == store_id)
            .ok_or(AuthError::Storage)?;
        entry.user_id = user_id.to_owned();
        entry.pending = false;
        self.active = Some(store_id);
        Ok(())
    }
}

fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE)
}

/// `None` when no index has been written yet (before migration).
pub(super) fn read(data_dir: &Path) -> Result<Option<Index>, AuthError> {
    let _guard = LOCK.lock().map_err(|_| AuthError::Storage)?;
    read_unlocked(data_dir)
}

fn read_unlocked(data_dir: &Path) -> Result<Option<Index>, AuthError> {
    match std::fs::read(path(data_dir)) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| AuthError::Storage),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(AuthError::Storage),
    }
}

/// Read, change and atomically replace the index (written to a temporary
/// file and renamed, so a crash leaves the old or the new list, never half).
pub(super) fn update<R>(
    data_dir: &Path,
    change: impl FnOnce(&mut Index) -> Result<R, AuthError>,
) -> Result<R, AuthError> {
    let _guard = LOCK.lock().map_err(|_| AuthError::Storage)?;
    let mut index = read_unlocked(data_dir)?.unwrap_or_default();
    let result = change(&mut index)?;
    index.version = VERSION;
    std::fs::create_dir_all(data_dir).map_err(|_| AuthError::Storage)?;
    let target = path(data_dir);
    let temporary = target.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(&index).map_err(|_| AuthError::Storage)?;
    std::fs::write(&temporary, bytes).map_err(|_| AuthError::Storage)?;
    std::fs::rename(&temporary, &target).map_err(|_| AuthError::Storage)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(user: &str, pending: bool) -> Entry {
        Entry {
            store_id: Uuid::new_v4(),
            user_id: user.into(),
            homeserver: "https://example.org/".into(),
            pending,
        }
    }

    fn scratch() -> PathBuf {
        std::env::temp_dir().join(format!("twayblade-accounts-{}", Uuid::new_v4()))
    }

    #[test]
    fn launch_prefers_the_active_ready_account() {
        let first = entry("@a:example.org", false);
        let second = entry("@b:example.org", false);
        let pending = entry("", true);
        let mut index = Index {
            version: VERSION,
            active: Some(second.store_id),
            accounts: vec![pending.clone(), first.clone(), second.clone()],
        };
        assert_eq!(index.launch(), Some(second.store_id));
        index.active = Some(pending.store_id);
        assert_eq!(
            index.launch(),
            Some(first.store_id),
            "pending is never opened"
        );
        index.remove(first.store_id);
        index.remove(second.store_id);
        assert_eq!(index.launch(), None);
        assert!(index.saved().is_empty(), "pending sign-ins are not listed");
    }

    #[test]
    fn completing_rejects_a_user_saved_in_another_slot() {
        let saved = entry("@a:example.org", false);
        let adding = entry("", true);
        let mut index = Index {
            version: VERSION,
            active: Some(saved.store_id),
            accounts: vec![saved.clone(), adding.clone()],
        };
        assert!(matches!(
            index.complete(adding.store_id, "@a:example.org"),
            Err(AuthError::DuplicateAccount)
        ));
        assert_eq!(index.active, Some(saved.store_id), "nothing changed");
        assert!(index.accounts[1].pending);
        index
            .complete(adding.store_id, "@b:example.org")
            .expect("another user");
        assert_eq!(index.active, Some(adding.store_id));
        assert_eq!(
            index
                .saved()
                .iter()
                .map(|a| a.user_id.as_str())
                .collect::<Vec<_>>(),
            ["@a:example.org", "@b:example.org"]
        );
    }

    #[test]
    fn update_round_trips_and_leaves_no_temporary_file() {
        let dir = scratch();
        assert!(read(&dir).expect("missing index").is_none());
        let added = entry("@a:example.org", false);
        let id = added.store_id;
        update(&dir, |index| {
            index.accounts.push(added);
            index.active = Some(id);
            Ok(())
        })
        .expect("write");
        let index = read(&dir).expect("read").expect("written");
        assert_eq!(index.version, VERSION);
        assert_eq!(index.launch(), Some(id));
        assert!(!dir.join("accounts.json.tmp").exists());
        // A failed change leaves the file untouched.
        let failed: Result<(), _> = update(&dir, |index| {
            index.accounts.clear();
            Err(AuthError::Storage)
        });
        assert!(failed.is_err());
        assert_eq!(read(&dir).expect("read").expect("kept").accounts.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn index_holds_no_credentials() {
        let index = Index {
            version: VERSION,
            active: None,
            accounts: vec![entry("@a:example.org", false)],
        };
        let text = serde_json::to_string(&index).expect("json");
        for field in ["passphrase", "access_token", "refresh_token", "session"] {
            assert!(!text.contains(field), "{field} must not be in the index");
        }
    }
}
