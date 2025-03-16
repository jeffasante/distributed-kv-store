// src/store_test.rs

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_store_operations() {
        // Create a new store
        let mut store = KeyValueStore::new();

        // Test set and get
        store.set("key1".to_string(), "value1".to_string());
        assert_eq!(store.get("key1"), Some("value1".to_string()));

        // Test non-existent key
        assert_eq!(store.get("nonexistent"), None);

        // Test delete
        assert_eq!(store.delete("key1"), true);
        assert_eq!(store.get("key1"), None);

        // Test delete non-existent key
        assert_eq!(store.delete("nonexistent"), false);

        // Test keys
        store.set("keys2".to_string(), "value2".to_string());
        store.set("keys3".to_string(), "value3".to_string());
        let keys = store.keys();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"keys2".to_string()));
        assert!(keys.contains(&"keys3".to_string()));
    }

    #[test]
    fn test_persistence() -> Result<()> {
        // Create a temporary directory
        let dir = tempdir()?;
        let file_path = dir.path().join("test-db.json");

        // Create and populate a store
        {
            let mut store = KeyValueStore::new();
            store.set("persist1".to_string(), "value1".to_string());
            store.set("persist2".to_string(), "value2".to_string());
            store.save(&file_path)?;
        }

        // Load the store from disk and verify data
        {
            let store = KeyValueStore::load(&file_path)?;
            assert_eq!(store.get("persist1"), Some("value1".to_string()));
            assert_eq!(store.get("persist2"), Some("value2".to_string()));
        }

        Ok(())  
    }

}