# Redis Connection Pooling

## Overview

The daemon uses a **single persistent multiplexed Redis connection** via `redis::aio::ConnectionManager` instead of creating new connections per request. This prevents connection exhaustion issues, especially with cloud-hosted Redis (e.g., Upstash) that impose strict connection limits.

## Problem & Solution

### Problem
The original implementation created new Redis connections for each operation. With 100+ concurrent tasks (file saves, context compilations, brain updates), this rapidly exhausted cloud Redis connection limits (often 100 or 300 max), causing:
- `ERR max number of clients reached` errors
- Connection timeouts under load
- 100% connection utilization reported by Redis dashboards

### Solution
`ConnectionManager` from the `redis` crate provides:
- **Single TCP connection** multiplexed across all async tasks
- **Automatic reconnection** on connection loss
- **Zero connection overhead** per operation (clone is just an `Arc` clone)
- **Thread-safe** sharing via `tokio::sync::RwLock`

## Key Benefits

- **1 connection** instead of 100+ under load
- **Lazy initialization** — connection is created on first use
- **Zero config** — works automatically with any Redis URL
- **Cloud-friendly** — stays well within Upstash/ElastiCache limits

## Key File

`daemon/src/storage/redis.rs` — `RedisStorage::get_conn()` method.

## Cargo.toml

The `connection-manager` feature must be enabled:
```toml
redis = { version = "0.28", features = ["tokio-comp", "connection-manager"] }
```
