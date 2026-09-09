use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceTab {
    pub slug: String,
    pub citation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: u32,
    pub last_opened: u64,
    pub active: i32,
    pub tabs: Vec<WorkspaceTab>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoreFile {
    next_id: u32,
    mru: Vec<u32>,
    sessions: Vec<Workspace>,
}

impl Default for StoreFile {
    fn default() -> Self {
        Self {
            next_id: 1,
            mru: Vec::new(),
            sessions: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct WorkspaceStore {
    path: PathBuf,
    data: StoreFile,
}

impl WorkspaceStore {
    pub fn open(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let data = load_file(&path);
        Self { path, data }
    }

    pub fn save(
        &mut self,
        session_id: Option<u32>,
        active: i32,
        tabs: Vec<WorkspaceTab>,
    ) -> u32 {
        let now = unix_now();
        let id = match session_id {
            Some(id) if self.data.sessions.iter().any(|ws| ws.id == id) => {
                if let Some(existing) = self.data.sessions.iter_mut().find(|ws| ws.id == id) {
                    existing.active = active;
                    existing.tabs = tabs;
                    existing.last_opened = now;
                }
                id
            }
            _ => {
                let id = self.data.next_id;
                self.data.next_id = self.data.next_id.saturating_add(1);
                self.data.sessions.push(Workspace {
                    id,
                    last_opened: now,
                    active,
                    tabs,
                });
                id
            }
        };
        self.bump_mru(id);
        self.persist();
        id
    }

    pub fn get(&self, id: u32) -> Option<&Workspace> {
        self.data.sessions.iter().find(|ws| ws.id == id)
    }

    pub fn list(&self) -> Vec<&Workspace> {
        self.data
            .mru
            .iter()
            .filter_map(|id| self.get(*id))
            .collect()
    }

    pub fn mru_id(&self) -> Option<u32> {
        self.data.mru.first().copied()
    }

    pub fn touch(&mut self, id: u32) {
        if !self.data.sessions.iter().any(|ws| ws.id == id) {
            return;
        }
        if let Some(existing) = self.data.sessions.iter_mut().find(|ws| ws.id == id) {
            existing.last_opened = unix_now();
        }
        self.bump_mru(id);
        self.persist();
    }

    pub fn remove(&mut self, id: u32) -> bool {
        let before = self.data.sessions.len();
        self.data.sessions.retain(|ws| ws.id != id);
        self.data.mru.retain(|mru| *mru != id);
        let removed = self.data.sessions.len() != before;
        if removed {
            self.persist();
        }
        removed
    }

    pub fn remove_all(&mut self) {
        self.data.sessions.clear();
        self.data.mru.clear();
        self.persist();
    }

    fn bump_mru(&mut self, id: u32) {
        self.data.mru.retain(|mru| *mru != id);
        self.data.mru.insert(0, id);
    }

    fn persist(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.data) {
            let _ = fs::write(&self.path, json);
        }
    }
}

fn load_file(path: &Path) -> StoreFile {
    let Ok(raw) = fs::read_to_string(path) else {
        return StoreFile::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(slug: &str, citation: &str) -> WorkspaceTab {
        WorkspaceTab {
            slug: slug.into(),
            citation: citation.into(),
        }
    }

    #[test]
    fn save_roundtrips_a_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.json");
        let mut store = WorkspaceStore::open(&path);
        let id = store.save(None, 0, vec![tab("bgb", "§ 433")]);
        assert_eq!(id, 1);
        let reopened = WorkspaceStore::open(&path);
        let workspace = reopened.get(1).expect("saved workspace");
        assert_eq!(workspace.tabs, vec![tab("bgb", "§ 433")]);
        assert_eq!(workspace.active, 0);
        assert_eq!(reopened.mru_id(), Some(1));
    }

    #[test]
    fn save_does_not_reuse_removed_ids() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.json");
        let mut store = WorkspaceStore::open(&path);
        assert_eq!(store.save(None, 0, vec![tab("bgb", "§ 1")]), 1);
        assert!(store.remove(1));
        assert_eq!(store.save(None, 0, vec![tab("gg", "Art. 1")]), 2);
        assert!(store.get(1).is_none());
        assert_eq!(store.get(2).unwrap().tabs[0].slug, "gg");
    }

    #[test]
    fn list_is_mru_and_remove_updates_mru() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.json");
        let mut store = WorkspaceStore::open(&path);
        assert_eq!(store.save(None, 0, vec![tab("bgb", "§ 1")]), 1);
        assert_eq!(store.save(None, 0, vec![tab("gg", "Art. 1")]), 2);
        assert_eq!(
            store.list().iter().map(|ws| ws.id).collect::<Vec<_>>(),
            vec![2, 1]
        );
        store.touch(1);
        assert_eq!(
            store.list().iter().map(|ws| ws.id).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(store.mru_id(), Some(1));
        assert!(store.remove(1));
        assert_eq!(store.mru_id(), Some(2));
        store.remove_all();
        assert_eq!(store.mru_id(), None);
        assert!(store.list().is_empty());
    }
}
