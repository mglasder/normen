use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct SessionStore {
    positions: HashMap<String, String>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, slug: &str) -> Option<&str> {
        self.positions.get(slug).map(String::as_str)
    }

    pub fn set(&mut self, slug: &str, citation: impl Into<String>) {
        self.positions.insert(slug.to_string(), citation.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_remembers_in_the_same_instance() {
        let mut store = SessionStore::new();
        store.set("bgb", "§ 433");
        assert_eq!(store.get("bgb"), Some("§ 433"));
    }

    #[test]
    fn session_is_not_shared_across_instances() {
        let mut store = SessionStore::new();
        store.set("bgb", "§ 433");
        assert!(SessionStore::new().get("bgb").is_none());
    }

    #[test]
    fn session_does_not_write_state_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = SessionStore::new();
        store.set("bgb", "§ 433");
        assert!(dir.path().read_dir().unwrap().next().is_none());
    }

    #[test]
    fn session_missing_key_is_none() {
        assert!(SessionStore::new().get("gg").is_none());
    }
}
