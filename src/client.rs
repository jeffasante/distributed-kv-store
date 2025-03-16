// src/client.rs

// a client to connect to our server

use crate::error::{Result, StoreError};
use crate::network::Server;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

pub struct Client {
    address: String,
}

impl Client {

    pub fn new(address: String) -> Self {
        Client { address }
    }

    pub async fn send_command(&self, command: &str) -> Result<String> {
        // Connect to server
        let mut stream = TcpStream::connect(&self.address)
            .await
            .map_err(|e| StoreError::IoError(e))?;

        // Send command
        stream
            .write_all(command.as_bytes())
            .await
            .map_err(|e| StoreError::IoError(e))?;
        stream
            .write_all(b"\n")
            .await
            .map_err(|e| StoreError::IoError(e))?;
        stream.flush().await.map_err(|e| StoreError::IoError(e))?;

        // Read response
        let mut reader = BufReader::new(stream);
        let mut response = String::new();
        reader
            .read_line(&mut response)
            .await
            .map_err(|e| StoreError::IoError(e))?;

        Ok(response.trim().to_string())
    }

    // Convience methods
    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        let response = self.send_command(&format!("GET {}", key)).await?;

        if response == "NULL" || response == "Key not found" {
            Ok(None)
        } else {
            Ok(Some(response))
        }
    }

    pub async fn put(&self, key: &str, value: &str) -> Result<()> {
        let response = self.send_command(&format!("PUT {} {}", key, value)).await?;

        if response == "OK" {
            Ok(())
        } else {
            Err(StoreError::SerializationError(response))
        }
    }

    pub async fn delete(&self, key: &str) -> Result<bool> {
        let response = self.send_command(&format!("DELETE {}", key)).await?;

        Ok(response == "OK")
    }

    pub async fn keys(&self) -> Result<Vec<String>> {
        let response = self.send_command("KEYS").await?;

        if response == "(empty list)" {
            Ok(vec![])
        } else {
            Ok(response.split_whitespace().map(|s| s.to_string()).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::store::KeyValueStore;

    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_client_server() {
        // Create a run time for running async code in a test
        let rt = Runtime::new().unwrap();

        rt.block_on(async {
            // Create store
            let store = Arc::new(KeyValueStore::new());

            // Start server in background
            let server_addr = "127.0.0.1:7890".to_string();
            let server = Server::new(Arc::clone(&store), server_addr.clone());

            let _server_handle = tokio::spawn(async move {
                let _ = server.run().await.unwrap();
            });

            // Give the server a moement to start
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            // Create client
            let client = Client::new(server_addr);

            // Test operations
            client.put("network_key", "network_value").await.unwrap();
            assert_eq!(
                client.get("network_key").await.unwrap(),
                Some("network_value".to_string())
            );

            let keys = client.keys().await.unwrap();
            assert!(keys.contains(&"network_key".to_string()));

            assert_eq!(client.delete("network_key").await.unwrap(), true);
            assert_eq!(client.get("network_key").await.unwrap(), None);

            // Server is not stopped in this test. It will run until the test completes
            // server_handle.abort();
        })
    }
}
