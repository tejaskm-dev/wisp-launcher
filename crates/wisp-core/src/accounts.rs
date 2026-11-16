//! The accounts Wisp can play with. Microsoft sign-in is still to come (it needs an Azure app that
//! Mojang has approved); until then there are offline accounts, which only developer builds can
//! create, so testing a launch doesn't wait on sign-in.

use std::path::PathBuf;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use crate::launch::hex;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    /// The Minecraft username.
    pub name: String,
    /// The player UUID, as 32 hex digits.
    pub uuid: String,
    pub kind: AccountKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    /// Plays singleplayer and offline-mode servers only.
    Offline,
}

#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    #[error("Use 3 to 16 letters, numbers or underscores.")]
    InvalidName,
    #[error("Couldn't save accounts: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AccountsFile {
    accounts: Vec<Account>,
    active: Option<String>,
}

/// Accounts live in one small JSON file; the active one is what Play uses.
pub struct AccountStore {
    path: PathBuf,
}

impl AccountStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn read(&self) -> AccountsFile {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn write(&self, file: &AccountsFile) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(file).map_err(std::io::Error::other)?;
        let partial = self.path.with_extension("json.part");
        std::fs::write(&partial, json)?;
        std::fs::rename(partial, &self.path)
    }

    pub fn active(&self) -> Option<Account> {
        let file = self.read();
        let active = file.active?;
        file.accounts
            .into_iter()
            .find(|account| account.id == active)
    }

    /// Adds (or re-selects) an offline account and makes it the active one.
    pub fn add_offline(&self, name: &str) -> Result<Account, AccountError> {
        let name = name.trim();
        if !valid_username(name) {
            return Err(AccountError::InvalidName);
        }
        let uuid = offline_uuid(name);
        let account = Account {
            id: uuid.clone(),
            name: name.to_string(),
            uuid,
            kind: AccountKind::Offline,
        };
        let mut file = self.read();
        file.accounts.retain(|existing| existing.id != account.id);
        file.accounts.push(account.clone());
        file.active = Some(account.id.clone());
        self.write(&file)?;
        Ok(account)
    }

    pub fn remove(&self, id: &str) -> Result<(), AccountError> {
        let mut file = self.read();
        file.accounts.retain(|account| account.id != id);
        if file.active.as_deref() == Some(id) {
            file.active = file.accounts.first().map(|account| account.id.clone());
        }
        self.write(&file)?;
        Ok(())
    }
}

/// Minecraft usernames: 3–16 letters, digits or underscores.
pub fn valid_username(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The UUID Minecraft itself gives an offline player: a version 3 UUID of "OfflinePlayer:<name>"
/// (Java's `UUID.nameUUIDFromBytes`).
pub fn offline_uuid(name: &str) -> String {
    let mut hash = Md5::digest(format!("OfflinePlayer:{name}").as_bytes());
    hash[6] = (hash[6] & 0x0f) | 0x30;
    hash[8] = (hash[8] & 0x3f) | 0x80;
    hex(hash.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuids_match_minecraft() {
        // Java: UUID.nameUUIDFromBytes("OfflinePlayer:Notch".getBytes(UTF_8))
        assert_eq!(offline_uuid("Notch"), "b50ad385829d3141a2167e7d7539ba7f");
    }

    #[test]
    fn usernames_follow_minecraft_rules() {
        assert!(valid_username("Steve_123"));
        assert!(!valid_username("ab"));
        assert!(!valid_username("has space"));
        assert!(!valid_username("seventeen_chars_x"));
    }

    #[test]
    fn adds_switches_and_removes_accounts() {
        let path = std::env::temp_dir().join(format!("wisp-accounts-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = AccountStore::new(&path);
        assert!(store.active().is_none());
        let steve = store.add_offline(" Steve ").unwrap();
        assert_eq!(steve.name, "Steve");
        let alex = store.add_offline("Alex").unwrap();
        assert_eq!(store.active().unwrap().id, alex.id);
        store.remove(&alex.id).unwrap();
        assert_eq!(store.active().unwrap().id, steve.id);
        assert!(matches!(
            store.add_offline("no!"),
            Err(AccountError::InvalidName)
        ));
        std::fs::remove_file(path).unwrap();
    }
}
