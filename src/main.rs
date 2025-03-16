// src/main.rs

use clap::{Parser, Subcommand};
use client::Client;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;

mod client;
mod error;
mod network;
mod replication;
mod store;

use error::{Result, StoreError};
use network::Server;
use store::KeyValueStore;

#[derive(Parser)]
#[clap(
    name = "kv-store",
    about = "A simple key-value store",
    version = "0.1.0",
    author = "jeffasante"
)]
struct Cli {
    // Database file path
    #[clap(short, long, default_value = "kv-store.json")]
    db_path: PathBuf,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    // Server mode
    Server {
        #[clap(short, long, default_value = "127.0.0.1:7000")]
        address: String,

        // Replication role
        #[clap(long)]
        role: Option<String>, // "primary" or "backup"

        // Primary address (for backup nodes)
        #[clap(long)]
        primary: Option<String>,
    },
    // Add backup to primary
    AddBackup {
        #[clap(long)]
        primary: String,

        #[clap(long)]
        backup: String,
    },

    // Client commands
    Get {
        key: String,
    },
    Put {
        key: String,
        value: String,
    },
    Delete {
        key: String,
    },
    Keys,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse the command-line arguments
    let cli = Cli::parse();

    // Load the store
    let store = KeyValueStore::load(&cli.db_path)?;
    let store = Arc::new(store);

    match cli.command {
        Command::Server { address, role, primary } => {
            // Create server with or without replication
            let server = if role.is_some() {
                Server::with_replication(Arc::clone(&store), address.clone())
            } else {
                Server::new(Arc::clone(&store), address.clone())
            };
            
            // Configure replication if requested
            if let Some(role_str) = role {
                match role_str.to_lowercase().as_str() {
                    "primary" => {
                        server.start_as_primary().await?;
                    },
                    "backup" => {
                        if let Some(primary_addr) = primary {
                            server.start_as_backup(primary_addr).await?;
                        } else {
                            return Err(StoreError::ReplicationError(
                                "Backup nodes require --primary".to_string()).into());
                        }
                    },
                    _ => {
                        return Err(StoreError::ReplicationError(
                            "Role must be 'primary' or 'backup'".to_string()).into());
                    }
                }
            }
            
            // Run the server
            server.run().await?;
        },
        Command::AddBackup { primary, backup } => {
            // Connect to primary
            let client = Client::new(primary);
            
            // Send add_backup command
            let response = client.send_command(&format!("ADD_BACKUP {}", backup)).await?;
            println!("Response: {}", response);
        },

        
        // Client mode commands
        Command::Get { key } => match store.get(&key) {
            Some(value) => println!("{}", value),
            None => {
                eprint!("Key not found: {}", key);
                process::exit(1);
            }
        },
        Command::Put { key, value } => {
            store.put(key, value);
            println!("Value stored successfully");
            // Save changes
            store.save(&cli.db_path)?;
        }
        Command::Delete { key } => {
            if store.delete(&key) {
                println!("Key deleted successfully");
                // Save changes
                store.save(&cli.db_path)?;
            } else {
                eprint!("Key not found: {}", key);
                process::exit(1);
            }
        }
        Command::Keys => {
            let keys = store.keys();
            if keys.is_empty() {
                println!("No keys found");
            } else {
                print!("Keys: ");
                for key in keys {
                    print!("    {} ", key);
                }
            }
        }
    }

    // Save the store
    // store.save(&cli.db_path)?;
    Ok(())
}
