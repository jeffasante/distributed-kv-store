use crate::client::Client;
use crate::error::{Result, StoreError};
use crate::store::KeyValueStore;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

// Node roles
#[derive(Debug, Clone, PartialEq)]
pub enum Role {
    Primary,
    Backup(String), // Backup knows primary's address
    Standalone,     // Not part of any replication
}

// Operation types that can be replicated
#[derive(Debug, Clone)]
pub enum Operation {
    Put(String, String),
    Delete(String),
}

impl Operation {
    pub fn to_string(&self) -> String {
        match self {
            Operation::Put(key, value) => format!("PUT {} {}", key, value),
            Operation::Delete(key) => format!("Delete {}", key),
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }

        match parts[0] {
            "PUT" => {
                if parts.len() < 3 {
                    return None;
                }

                let key = parts[1].to_string();
                let value = parts[2..].join("");
                Some(Operation::Put(key, value))
            }
            "DELETE" => {
                if parts.len() != 2 {
                    return None;
                }
                Some(Operation::Delete(parts[1].to_string()))
            }
            _ => None,
        }
    }
}

// Replication manager
pub struct ReplicationManager {
    store: Arc<KeyValueStore>,
    role: Mutex<Role>,
    backups: Mutex<Vec<String>>, // List of backup addresses
    last_heartbeat: Mutex<Instant>,
    heartbeat_interval: Duration,
    failover_timeout: Duration,
}

impl ReplicationManager {
    pub fn new(store: Arc<KeyValueStore>) -> Self {
        ReplicationManager {
            store,
            role: Mutex::new(Role::Standalone),
            backups: Mutex::new(Vec::new()),
            last_heartbeat: Mutex::new(Instant::now()),
            heartbeat_interval: Duration::from_secs(1),
            failover_timeout: Duration::from_secs(5),
        }
    }

    // Start as primary node
    pub async fn start_primary(self: Arc<Self>) -> Result<()> {
        let mut role = self.role.lock().await;
        *role = Role::Primary;
        println!("Started as primary node");

        // Start heartbeat process in background
        let self_clone = Arc::clone(&self);
        tokio::spawn(async move {
            self_clone.send_heartbeats().await;
        });

        Ok(())
    }

    // Start as backup node
    pub async fn start_backup(self: Arc<Self>, primary_addr: String) -> Result<()> {
        let _ = primary_addr;
        let mut role = self.role.lock().await;
        *role = Role::Backup(primary_addr.clone());
        println!("Started as primary node");

        // Start heartbeat process in background
        let self_clone = Arc::clone(&self);
        tokio::spawn(async move {
            self_clone.monitor_primary().await;
        });

        Ok(())
    }

    // Add a backup to this primary
    pub async fn add_backup(&self, backup_addr: String) -> Result<()> {
        let role = self.role.lock().await;

        if let Role::Primary = *role {
            let mut backups = self.backups.lock().await;
            if !backups.contains(&backup_addr) {
                backups.push(backup_addr.clone());
                println!("Added backup node at {}", backup_addr);
            }
            Ok(())
        } else {
            Err(StoreError::ReplicationError(
                "Only primary nodes can add backups".to_string(),
            ))
        }
    }

    // Send heartbeats to all backups
    async fn send_heartbeats(&self) {
        loop {
            tokio::time::sleep(self.heartbeat_interval).await;

            // Check if we're still primary
            let role = self.role.lock().await;
            if !matches!(*role, Role::Primary) {
                break;
            }
            drop(role);

            // Get backup addresses
            let backups = {
                let backups_lock = self.backups.lock().await;
                backups_lock.clone()
            };

            // Send heartbeat to each backup
            for backup_addr in &backups {
                if let Err(e) = self.send_heartbeat(backup_addr).await {
                    eprintln!("Failed to send heartbeat to {}: {}", backup_addr, e);
                }
            }
        }
    }

    // Send a single heartbeat
    async fn send_heartbeat(&self, backup_addr: &str) -> Result<()> {
        // Connect to backup using our client
        let client = Client::new(backup_addr.to_string());

        // Send a HEARTBEAT command
        match client.send_command("HEARTBEAT").await {
            Ok(response) if response == "OK" => {
                // Heartbeat acknowledged
                Ok(())
            }
            Ok(response) => Err(StoreError::ReplicationError(format!(
                "Unexpected response: {}",
                response
            ))),
            Err(e) => Err(e),
        }
    }

    // Monitor primary for failures
    async fn monitor_primary(self: Arc<Self>) {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Check if we're still a backup
            let role = self.role.lock().await;
            let primary_addr = match &*role {
                Role::Backup(addr) => addr.clone(),
                _ => break, // Not a backup anymore
            };
            drop(role);
            // Check last heartbeat time
            let last_heartbeat = {
                let heartbeat = self.last_heartbeat.lock().await;
                *heartbeat
            };
    
            if last_heartbeat.elapsed() > self.failover_timeout {
                println!(
                    "Primary node at {} failed! Promoting to primary.",
                    primary_addr
                );
    
                // Promote this backup to primary using a clone of self
                if let Err(e) = Arc::clone(&self).promote_to_primary().await {
                    eprintln!("Failed to promote to primary: {}", e);
                } else {
                    break;
                }
            }
        }
    }

    // Record recevied heartbeat
    pub async fn receive_heartbeat(&self) -> Result<()> {
        let mut last_heartbeat = self.last_heartbeat.lock().await;
        *last_heartbeat = Instant::now();
        Ok(())
    }

    // Promote backup to primary
    async fn promote_to_primary(self: Arc<Self>) -> Result<()> {
        let mut role = self.role.lock().await;

        if let Role::Backup(_) = *role {
            // Change role to primary
            *role = Role::Primary;
            println!("Promoted to Primary node");

            // Start sending heartbeats
            let self_arc = Arc::new(self.clone());
            tokio::spawn(async move {
                self_arc.send_heartbeats().await;
            });

            Ok(())
        } else {
            Err(StoreError::ReplicationError(
                "Only backup nodes can be promoted".to_string(),
            ))
        }
    }

    // Replicate an operation to all backups
    pub async fn replicate_operation(&self, operation: &Operation) -> Result<()> {
        let role = self.role.lock().await;

        if let Role::Primary = *role {
            let backups = {
                let backups_lock = self.backups.lock().await;
                backups_lock.clone()
            };

            // Convert operation to string format
            let op_str = operation.to_string();

            // Send to all backups
            for backup_addr in &backups {
                if let Err(e) = self.send_operation_to_backup(backup_addr, &op_str).await {
                    eprintln!("Failed to replicate to {}: {}", backup_addr, e);
                    // In production you might want to handle this more gracefully
                }
            }

            Ok(())
        } else {
            Err(StoreError::ReplicationError(
                "Only primary can replicate operations".to_string(),
            ))
        }
    }

    // Send operation to a backup
    async fn send_operation_to_backup(&self, backup_addr: &str, op_str: &str) -> Result<()> {
        // Connect to backup
        let client = Client::new(backup_addr.to_string());

        // Send REPLICATE command
        match client.send_command(&format!("REPLICATE {}", op_str)).await {
            Ok(response) if response == "OK" => {
                println!("Replicated '{}' to {}", op_str, backup_addr);
                Ok(())
            }
            Ok(response) => Err(StoreError::ReplicationError(format!(
                "Unexpected response: {}",
                response
            ))),
            Err(e) => Err(e),
        }
    }

    // Apply an operation received from primary
    pub async fn apply_operation(&self, op_str: &str) -> Result<()> {
        let role = self.role.lock().await;

        if let Role::Backup(_) = *role {
            // Parse the operation
            if let Some(operation) = Operation::from_string(op_str) {
                // Apply to local store
                match operation {
                    Operation::Put(key, value) => {
                        self.store.put(key, value);
                    }
                    Operation::Delete(key) => {
                        self.store.delete(&key);
                    }
                }

                Ok(())
            } else {
                Err(StoreError::ReplicationError(format!(
                    "Invalid operation: {}",
                    op_str
                )))
            }
        } else {
            Err(StoreError::ReplicationError(
                "Only backups can apply operations from primary".to_string(),
            ))
        }
    }

    // Get current role
    pub async fn get_role(&self) -> Role {
        let role = self.role.lock().await;
        role.clone()
    }

    // Get list of backups
    pub async fn get_backups(&self) -> Vec<String> {
        let backups = self.backups.lock().await;
        backups.clone()
    }
}



#[cfg(test)]
mod tests {
    use crate::network::Server;

    use super::*;
    
    #[tokio::test]
    async fn test_primary_backup_replication() {
        // Create stores
        let primary_store = Arc::new(KeyValueStore::new());
        let backup_store = Arc::new(KeyValueStore::new());
        
        // Create server instances
        let primary_addr = "127.0.0.1:7901".to_string();
        let backup_addr = "127.0.0.1:7902".to_string();
        
        let primary_server = Server::with_replication(Arc::clone(&primary_store), primary_addr.clone());
        let backup_server = Server::with_replication(Arc::clone(&backup_store), backup_addr.clone());
        
        // Start primary
        primary_server.start_as_primary().await.unwrap();
        let primary_handle = tokio::spawn(async move {
            let _ = primary_server.run().await;
        });
        
        // Start backup
        backup_server.start_as_backup(primary_addr.clone()).await.unwrap();
        let backup_handle = tokio::spawn(async move {
            let _ = backup_server.run().await;
        });
        
        // Give servers time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        
        // Add backup to primary
        let client = Client::new(primary_addr.clone());
        client.send_command(&format!("ADD_BACKUP {}", backup_addr)).await.unwrap();
        
        // Set a value on primary
        client.put("replicated_key", "replicated_value").await.unwrap();
        
        // Wait for replication to occur
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        
        // Verify the value exists in the backup's store
        assert_eq!(backup_store.get("replicated_key").unwrap(), "replicated_value");
        
        // Clean up
        primary_handle.abort();
        backup_handle.abort();
    }
}