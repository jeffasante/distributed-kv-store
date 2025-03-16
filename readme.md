# Distributed Key-Value Store

A lightweight, distributed key-value store built in Rust with support for concurrency, networking, and primary-backup replication.

## Features

- **Concurrent Key-Value Store**: Thread-safe operations with read-write locks
- **Persistence**: Disk-based storage with JSON serialization
- **Network Interface**: TCP server for remote operations
- **Primary-Backup Replication**: Fault tolerance with leader-follower architecture
- **Comprehensive Testing**: Unit tests for all components

## Architecture

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   Primary    │     │   Backup 1   │     │   Backup 2   │
│              │     │              │     │              │
│ ┌──────────┐ │     │ ┌──────────┐ │     │ ┌──────────┐ │
│ │ Key-Value│ │     │ │ Key-Value│ │     │ │ Key-Value│ │
│ │  Store   │ │     │ │  Store   │ │     │ │  Store   │ │
│ └──────────┘ │     │ └──────────┘ │     │ └──────────┘ │
└──────┬───────┘     └──────┬───────┘     └──────┬───────┘
       │                    │                    │
       │                    │                    │
       ▼                    ▼                    ▼
┌─────────────────────────────────────────────────────┐
│                   Clients                            │
└─────────────────────────────────────────────────────┘
```

The system uses a primary-backup architecture where:
- All write operations go to the primary node
- The primary replicates operations to backup nodes
- Backup nodes can be promoted to primary if needed

## Getting Started

### Prerequisites

- Rust and Cargo (1.54.0 or newer)

### Installation

```bash
# Clone the repository
git clone https://github.com/jeffasante/distributed-kv-store.git
cd distributed-kv-store

# Build the project
cargo build --release
```

### Running the Key-Value Store

#### Start a Primary Node

```bash
cargo run -- --db-path primary.json server --address 127.0.0.1:7001 --role primary
```

#### Start a Backup Node

```bash
cargo run -- --db-path backup.json server --address 127.0.0.1:7002 --role backup --primary 127.0.0.1:7001
```

#### Add a Backup to the Primary

```bash
cargo run -- add-backup --primary 127.0.0.1:7001 --backup 127.0.0.1:7002
```

#### Set a Value

```bash
cargo run -- put mykey myvalue
```

#### Get a Value

```bash
cargo run -- get mykey
```

## Implementation Details

### Store Module

The core key-value store provides:

- Thread-safe access using `RwLock`
- Persistence with JSON serialization
- Basic CRUD operations (get, set, delete, keys)

### Network Module

The network layer provides:

- TCP server for handling client connections
- Simple text-based protocol for operations
- Connection management with Tokio async I/O

### Replication Module

The replication system implements:

- Primary-backup architecture
- Operation replication from primary to backups
- Heartbeat mechanism for failure detection
- Manual failover capability

## Protocol

The system uses a simple text-based protocol:

| Command | Description | Example |
|---------|-------------|---------|
| `GET <key>` | Retrieve a value | `GET mykey` |
| `PUT <key> <value>` | Store a value | `PUT mykey myvalue` |
| `DELETE <key>` | Remove a key | `DELETE mykey` |
| `KEYS` | List all keys | `KEYS` |
| `HEARTBEAT` | Internal command for replicas | `HEARTBEAT` |
| `REPLICATE <operation>` | Internal command for replication | `REPLICATE PUT key value` |
| `ADD_BACKUP <address>` | Add a backup node | `ADD_BACKUP 127.0.0.1:7002` |

## Future Directions

- Automatic failover
- Write-ahead logging for stronger durability
- Sharding for horizontal scaling
- Full Raft consensus implementation
- Performance benchmarking against Redis

## Testing

To run the test suite:

```bash
cargo test
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.