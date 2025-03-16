// src/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    // #[error("Key not found: {0}")]
    // KeyNotFound(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serilization error: {0}")]
    SerializationError(String),

    #[error("Replication error: {0}")]
    ReplicationError(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;