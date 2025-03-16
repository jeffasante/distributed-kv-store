// src/store.rs

// // Module for the key-value store
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
// use std::io::{BufReader, BufWriter, Read, Write};
use crate::error::{Result, StoreError};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::RwLock;

// A thread-safe key-value store
#[derive(Serialize, Deserialize)]
pub struct KeyValueStore {
    // Wrap the HashMap in a RwLock to allow concurrent access
    data_lock: RwLock<HashMap<String, String>>,

    // Keep a separate field for serialization/deserialization
    #[serde(rename = "data")]
    data_for_serde: Option<HashMap<String, String>>,
}

impl KeyValueStore {
    // Create a new empty store
    pub fn new() -> Self {
        KeyValueStore {
            data_lock: RwLock::new(HashMap::new()),
            data_for_serde: None,
        }
    }

    // Load from file
    pub fn load(path: &Path) -> Result<Self> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => return Ok(Self::new()),
                _ => return Err(StoreError::IoError(e)),
            },
        };

        // Deserialize the store
        let reader = BufReader::new(file);
        let mut store: Self = serde_json::from_reader(reader)
            .map_err(|e| StoreError::SerializationError(e.to_string()))?;

        // Transfer data from serialization field to the RWLock
        if let Some(data) = store.data_for_serde.take() {
            *store.data_lock.write().unwrap() = data;
        }

        Ok(store)
    }

    // Save to file
    pub fn save(&self, path: &Path) -> Result<()> {
        // Create a temporary structure for serialization
        let temp_store = KeyValueStore {
            data_lock: RwLock::new(HashMap::new()),
            data_for_serde: Some(self.data_lock.read().unwrap().clone()),
        };

        // Serialize the store
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &temp_store)
            .map_err(|e| StoreError::SerializationError(e.to_string()))
    }

    // Get a value by key (only needs read access)
    pub fn get(&self, key: &str) -> Option<String> {
        // Acquire read lock, then look up the key
        let data = self.data_lock.read().unwrap();
        data.get(key).cloned() // Return a copy of the value to avoid lifetime issues
    }

    // Set a value by key (needs write access)
    pub fn put(&self, key: String, value: String) {
        // Acquire write lock, then insert the key-value pair
        let mut data = self.data_lock.write().unwrap();
        data.insert(key, value);
    }

    // Delete a key (needs write access)
    pub fn delete(&self, key: &str) -> bool {
        // Acquire write lock, then remove the key
        let mut data = self.data_lock.write().unwrap();
        data.remove(key).is_some()
    }

    // List all keys (only needs read access)
    pub fn keys(&self) -> Vec<String> {
        // Acquire read lock, then return a copy of the keys
        let data = self.data_lock.read().unwrap();
        data.keys().cloned().collect()
    }
}

// Unit tests -> Cannot test private functions inside [ tests/store_tests.rs] thats why we have the test codes here.

#[cfg(test)]
mod tests {
    use super::*;
    // use std::path::PathBuf;
    use std::sync::Arc;
    use std::thread;
    use tempfile::tempdir;

    #[test]
    fn test_store_operations() {
        // Create a new store
        let store = KeyValueStore::new();

        // Test set and get
        store.put("key1".to_string(), "value1".to_string());
        assert_eq!(store.get("key1"), Some("value1".to_string()));

        // Test non-existent key
        assert_eq!(store.get("nonexistent"), None);

        // Test delete
        assert_eq!(store.delete("key1"), true);
        assert_eq!(store.get("key1"), None);

        // Test delete non-existent key
        assert_eq!(store.delete("nonexistent"), false);

        // Test keys
        store.put("keys2".to_string(), "value2".to_string());
        store.put("keys3".to_string(), "value3".to_string());
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
            let store = KeyValueStore::new();
            store.put("persist1".to_string(), "value1".to_string());
            store.put("persist2".to_string(), "value2".to_string());
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

    #[test]
    fn test_concurrent_access() {
        // Create a store and wrap it in an Arc for sharing acroos threads
        let store = Arc::new(RwLock::new(KeyValueStore::new()));

        // Initialize the store with a shared key
        store
            .write()
            .unwrap()
            .put("shared_key".to_string(), "initial".to_string());

        // Number of threads to create
        let num_threads = 10;
        let num_operations = 100;

        // Create a vector to hold our thread handles
        let mut handles = vec![];

        // Spawn threads to perform concurrent operations
        for i in 0..num_threads {
            let store_clone = Arc::clone(&store);

            let handle = thread::spawn(move || {
                for j in 0..num_operations {
                    // Read the shared key
                    let _value = store_clone.read().unwrap().get("shared_key");

                    // Add thread-specific keys occasionally
                    if j % 10 == 0 {
                        let thread_key = format!("thread_{}_key_{}", i, j);
                        store_clone
                            .write()
                            .unwrap()
                            .put(thread_key, format!("value_{}", j));
                    }
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to finish
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify we have the expected number of keys
        let keys = store.read().unwrap().keys();
        assert_eq!(keys.len(), 1 + num_threads * (num_operations / 10));
    }
}
