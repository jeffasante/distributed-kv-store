// src/network.rs

use crate::error::{Result, StoreError};
use crate::store::KeyValueStore;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use crate::replication::{Operation, ReplicationManager, Role};

pub struct Server {
    store: Arc<KeyValueStore>,
    address: String,
    replication_manager: Option<Arc<ReplicationManager>>,
}

impl Server {
    // Create a server with replication enabled
    pub fn with_replication(store: Arc<KeyValueStore>, address: String) -> Self {
        let replication_manager = Arc::new(ReplicationManager::new(Arc::clone(&store)));

        Server {
            store,
            address,
            replication_manager: Some(replication_manager),
        }
    }

    // Start as primary
    pub async fn start_as_primary(&self) -> Result<()> {
        if let Some(rm) = &self.replication_manager {
            rm.clone().start_primary().await?;
            Ok(())
        } else {
            Err(StoreError::ReplicationError(
                "Replication not enabled".to_string(),
            ))
        }
    }
    // Start as backup
    pub async fn start_as_backup(&self, primary_addr: String) -> Result<()> {
        if let Some(rm) = &self.replication_manager {
            rm.clone().start_backup(primary_addr).await?;
            Ok(())
        } else {
            Err(StoreError::ReplicationError(
                "Replication not enabled".to_string(),
            ))
        }
    }

    // Add a backup node to this primary
    pub async fn add_backup(&self, backup_addr: String) -> Result<()> {
        if let Some(rm) = &self.replication_manager {
            rm.add_backup(backup_addr).await?;
            Ok(())
        } else {
            Err(StoreError::ReplicationError(
                "Replication not enabled".to_string(),
            ))
        }
    }

    pub fn new(store: Arc<KeyValueStore>, address: String) -> Self {
        Server {
            store,
            address,
            replication_manager: None,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.address)
            .await
            .map_err(|e| StoreError::IoError(e))?;
        println!("Server listening on {}", self.address);

        loop {
            match listener.accept().await {
                Ok((socket, addr)) => {
                    println!("New connection from: {}", addr);

                    // Clone store and replication_manager for the new connection
                    let store = Arc::clone(&self.store);
                    let replication_manager = self.replication_manager.clone(); // Clone the Option<Arc<ReplicationManager>>

                    // Spawn a new task to handle the connection
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(socket, store, replication_manager).await
                        {
                            eprintln!("Error handling connection: {}", e);
                        }
                    });
                }
                Err(e) => {
                    eprintln!("Error accepting connection: {}", e);
                }
            }
        }
    }



}

async fn handle_connection(
    socket: TcpStream,
    store: Arc<KeyValueStore>,
    replication_manager: Option<Arc<ReplicationManager>>,
) -> Result<()> {
    let (reader, mut writer) = tokio::io::split(socket);
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        // Read command
        line.clear();
        if reader
            .read_line(&mut line)
            .await
            .map_err(|e| StoreError::IoError(e))?
            == 0
        {
            break;
        }

        // Parse and execute command
        let response = execute_command(line.trim(), &store, &replication_manager).await?;

        // Send response
        writer
            .write_all(response.as_bytes())
            .await
            .map_err(|e| StoreError::IoError(e))?;
        writer
            .write_all(b"\n")
            .await
            .map_err(|e| StoreError::IoError(e))?;
        writer.flush().await.map_err(|e| StoreError::IoError(e))?;
    }

    Ok(())
}

async fn execute_command(
    command: &str,
    store: &KeyValueStore,
    replication_manager: &Option<Arc<ReplicationManager>>,
) -> Result<String> {
    let parts: Vec<&str> = command.split_whitespace().collect();

    if parts.is_empty() {
        return Ok("Error: Empty command".to_string());
    }

    // Match the command
    match parts[0].to_uppercase().as_str() {
        // Special replication commands
        "HEARTBEAT" => {
            if let Some(rm) = replication_manager {
                rm.receive_heartbeat().await?;
                Ok("OK".to_string())
            } else {
                Ok("ERROR: Replication not enabled".to_string())
            }
        }
        "REPLICATE" => {
            if parts.len() < 2 {
                return Ok("ERROR: Usage: REPLICATE <operation>".to_string());
            }

            if let Some(rm) = replication_manager {
                let op_str = parts[1..].join(" ");
                rm.apply_operation(&op_str).await?;
                Ok("OK".to_string())
            } else {
                Ok("ERROR: Replication not enabled".to_string())
            }
        }
        "ADD_BACKUP" => {
            if parts.len() != 2 {
                return Ok("ERROR: Usage: ADD_BACKUP <address>".to_string());
            }

            if let Some(rm) = replication_manager {
                rm.add_backup(parts[1].to_string()).await?;
                Ok("OK".to_string())
            } else {
                Ok("ERROR: Replication not enabled".to_string())
            }
        }

        "GET" => {
            if parts.len() != 2 {
                return Ok("Error: GET <key>".to_string());
            }

            match store.get(parts[1]) {
                Some(value) => Ok(value),
                None => Ok("Key not found".to_string()),
            }
        }

        "PUT" => {
            if parts.len() < 3 {
                return Ok("Error: Usage: PUT <key> <value>".to_string());
            }
            // Join all remaining parts for value (to allow spaces)
            let key = parts[1].to_string();
            let value = parts[2..].join(" ");

            // Apply locally
            store.put(key.clone(), value.clone());

            // Replicate if we're primary
            if let Some(rm) = replication_manager {
                if let Role::Primary = rm.get_role().await {
                    let op = Operation::Put(key, value);
                    rm.replicate_operation(&op).await?;
                }
            }

            Ok("OK".to_string())
        }

        "DELETE" => {
            if parts.len() != 2 {
                return Ok("Error: DELETE <key>".to_string());
            }
            let key = parts[1].to_string();
            let deleted = store.delete(&key);

            // Replicate if we're primary and it was deleted
            if deleted {
                if let Some(rm) = replication_manager {
                    if let Role::Primary = rm.get_role().await {
                        let op = Operation::Delete(key);
                        rm.replicate_operation(&op).await?;
                    }
                }
            }

            if deleted {
                Ok("OK".to_string())
            } else {
                Ok("NULL".to_string())
            }
        }

        "KEYS" => {
            // if parts.len() != 1 {
            //     return Ok("Error: KEYS".to_string());
            // }
            let keys = store.keys();
            if keys.is_empty() {
                Ok("No keys found".to_string())
            } else {
                Ok(keys.join(", "))
            }
        }
        _ => Ok(format!("Error: Unknown command '{}'", parts[0])),
    }
}
