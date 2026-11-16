use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl Loader {
    /// The id Modrinth and the loaders' own services use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
        }
    }
}

/// One self-contained Minecraft setup: a version, an optional mod loader, and its own game folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub game_version: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    /// Unix time in milliseconds.
    pub created: i64,
    pub last_played: Option<i64>,
    /// "wisp:<n>" for one of Wisp's built-in icons, or an image URL (a modpack's icon).
    #[serde(default)]
    pub icon: Option<String>,
    /// Set when the instance was made from a modpack.
    #[serde(default)]
    pub modpack: Option<ModpackOrigin>,
    /// Memory for this instance in MB, instead of the one in Settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
    /// A newer Java than Minecraft asks for, when a mod needs one (by major version).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_major: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModpackOrigin {
    pub project_id: String,
    pub version_id: String,
    pub title: String,
    pub version_number: String,
}

/// What the "New instance" dialog sends.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewInstance {
    pub name: String,
    pub game_version: String,
    pub loader: Loader,
    /// A specific loader version; left out, the recommended one is used.
    #[serde(default)]
    pub loader_version: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum InstanceError {
    #[error("Give the instance a name.")]
    EmptyName,
    #[error("{0} doesn't support Minecraft {1} yet.")]
    LoaderUnsupported(&'static str, String),
    #[error("That instance no longer exists.")]
    NotFound,
    #[error("Couldn't save the instance: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Network(#[from] crate::Error),
}

const MANIFEST: &str = "instance.json";
/// The folder Minecraft itself runs in (worlds, options.txt, mods, screenshots...).
pub const GAME_DIR: &str = "minecraft";

/// Instances live in `<root>/<id>/`, each with an `instance.json` and a `minecraft/` game folder.
pub struct InstanceStore {
    root: PathBuf,
}

impl InstanceStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn dir_of(&self, id: &str) -> Option<PathBuf> {
        // Only plain folder names: never let an id walk outside the instances folder.
        let valid = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        valid
            .then(|| self.root.join(id))
            .filter(|dir| dir.join(MANIFEST).is_file())
    }

    pub fn get(&self, id: &str) -> Option<Instance> {
        let bytes = std::fs::read(self.dir_of(id)?.join(MANIFEST)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// The folder the game runs in for this instance.
    pub fn game_dir(&self, id: &str) -> Option<PathBuf> {
        Some(self.dir_of(id)?.join(GAME_DIR))
    }

    /// Records that the instance was just played, so it sorts first.
    pub fn mark_played(&self, id: &str, now_ms: i64) -> std::io::Result<()> {
        self.update(id, |instance| instance.last_played = Some(now_ms))
            .map(|_| ())
            .or_else(|error| match error {
                InstanceError::NotFound => Ok(()),
                InstanceError::Io(error) => Err(error),
                other => Err(std::io::Error::other(other.to_string())),
            })
    }

    /// Changes an instance's manifest (its name or icon, say) and saves it.
    pub fn update(
        &self,
        id: &str,
        change: impl FnOnce(&mut Instance),
    ) -> Result<Instance, InstanceError> {
        let mut instance = self.get(id).ok_or(InstanceError::NotFound)?;
        change(&mut instance);
        instance.name = instance.name.trim().to_string();
        if instance.name.is_empty() {
            return Err(InstanceError::EmptyName);
        }
        let json = serde_json::to_vec_pretty(&instance).map_err(std::io::Error::other)?;
        std::fs::write(self.root.join(id).join(MANIFEST), json)?;
        Ok(instance)
    }

    /// Deletes an instance and everything in it (worlds included).
    pub fn delete(&self, id: &str) -> Result<(), InstanceError> {
        let dir = self.dir_of(id).ok_or(InstanceError::NotFound)?;
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// Every readable instance, most recently used first.
    pub fn list(&self) -> Vec<Instance> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut instances: Vec<Instance> = entries
            .flatten()
            .filter_map(|entry| std::fs::read(entry.path().join(MANIFEST)).ok())
            .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
            .collect();
        instances.sort_by_key(|instance| {
            std::cmp::Reverse(instance.last_played.unwrap_or(instance.created))
        });
        instances
    }

    /// Creates the instance's folder and manifest. The game itself downloads on first launch.
    pub fn create(
        &self,
        request: NewInstance,
        loader_version: Option<String>,
        now_ms: i64,
    ) -> Result<Instance, InstanceError> {
        self.create_with(request, loader_version, None, now_ms)
    }

    /// Like `create`, also recording the modpack the instance came from.
    pub fn create_with(
        &self,
        request: NewInstance,
        loader_version: Option<String>,
        modpack: Option<ModpackOrigin>,
        now_ms: i64,
    ) -> Result<Instance, InstanceError> {
        let name = request.name.trim();
        if name.is_empty() {
            return Err(InstanceError::EmptyName);
        }
        let id = self.unique_id(name);
        let dir = self.root.join(&id);
        std::fs::create_dir_all(dir.join(GAME_DIR))?;
        let instance = Instance {
            id,
            name: name.to_string(),
            game_version: request.game_version,
            loader: request.loader,
            loader_version,
            created: now_ms,
            last_played: None,
            icon: request.icon,
            modpack,
            memory_mb: None,
            java_major: None,
        };
        let json = serde_json::to_vec_pretty(&instance).map_err(std::io::Error::other)?;
        std::fs::write(dir.join(MANIFEST), json)?;
        Ok(instance)
    }

    fn unique_id(&self, name: &str) -> String {
        let base = slugify(name);
        let base = if base.is_empty() {
            "instance".to_string()
        } else {
            base
        };
        std::iter::once(base.clone())
            .chain((2..).map(|n| format!("{base}-{n}")))
            .find(|id| !self.root.join(id).exists())
            .expect("an unused id always exists")
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// "My Cool World!" → "my-cool-world"
fn slugify(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(tag: &str) -> InstanceStore {
        let root =
            std::env::temp_dir().join(format!("wisp-instances-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        InstanceStore::new(root)
    }

    fn request(name: &str) -> NewInstance {
        NewInstance {
            name: name.into(),
            game_version: "26.3".into(),
            loader: Loader::Vanilla,
            loader_version: None,
            icon: None,
        }
    }

    #[test]
    fn updates_and_deletes_instances() {
        let store = temp_store("update");
        let made = store.create(request("Survival"), None, 1).unwrap();
        let renamed = store
            .update(&made.id, |instance| {
                instance.name = "  Hardcore ".into();
                instance.icon = Some("wisp:3".into());
            })
            .unwrap();
        assert_eq!(renamed.name, "Hardcore");
        assert_eq!(store.get(&made.id).unwrap().icon.as_deref(), Some("wisp:3"));
        assert!(matches!(
            store.update(&made.id, |instance| instance.name = " ".into()),
            Err(InstanceError::EmptyName)
        ));
        store.delete(&made.id).unwrap();
        assert!(store.get(&made.id).is_none());
        std::fs::remove_dir_all(store.root()).unwrap();
    }

    #[test]
    fn creates_lists_and_finds_instances() {
        let store = temp_store("create");
        let first = store.create(request("My Cool World!"), None, 1).unwrap();
        let second = store.create(request("My Cool World!"), None, 2).unwrap();
        assert_eq!(first.id, "my-cool-world");
        assert_eq!(second.id, "my-cool-world-2");
        assert!(store.root().join("my-cool-world").join(GAME_DIR).is_dir());

        let listed = store.list();
        assert_eq!(
            listed.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            ["my-cool-world-2", "my-cool-world"]
        );
        assert!(store.dir_of("my-cool-world").is_some());
        assert!(store.dir_of("../escape").is_none());
        assert!(store.dir_of("missing").is_none());
        std::fs::remove_dir_all(store.root()).unwrap();
    }

    #[test]
    fn rejects_blank_names_and_handles_symbol_only_names() {
        let store = temp_store("names");
        assert!(matches!(
            store.create(request("   "), None, 1),
            Err(InstanceError::EmptyName)
        ));
        assert_eq!(
            store.create(request("~~ ·· ~~"), None, 1).unwrap().id,
            "instance"
        );
        std::fs::remove_dir_all(store.root()).unwrap();
    }

    #[test]
    fn missing_root_lists_nothing() {
        assert!(InstanceStore::new("/definitely/not/here").list().is_empty());
    }
}
