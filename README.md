# fuse_rust

A dummy FUSE filesystem written in Rust, backed by a pluggable storage layer.

## Storage backends

- **in-mem** – default; all data lives in the daemon's memory.
- **etcd** – persists files and metadata in an etcd cluster.

Set the backend with `FUSEFS_STORE_TYPE` (`in-mem` or `etcd`).  
Set the mountpoint with `FUSEFS_MOUNTPOINT` (default `/tmp/fusefs`).

## Building & running

```bash
# default (in-memory store, /tmp/fusefs)
cargo run
```

To use the etcd backend, start an etcd instance first:

```bash
./start_etcd_docker.sh   # runs etcd in Docker on port 2379
FUSEFS_STORE_TYPE=etcd cargo run # defaults to http://localhost:2379 for the etcd endpoint
```

## Graceful upgrade

The daemon supports zero-downtime restarts: a new process can take over the FUSE session from a running one without unmounting the filesystem.

### How it works

1. The running daemon is started with `--upgrade` and serves the filesystem normally. When it receives a `SIGTERM` or `SIGINT`, it initiates the upgrade process instead of exiting immediately:
   - Creates a Unix socket at `/tmp/fusefs_upgrade.sock`.
   - Finds the open `/dev/fuse` file descriptor.
   - Prepares the fd for transfer using the upgrade socket.
2. The new daemon is started with `--upgrade`. It detects the socket, connects, and receives the fd. It then resumes the FUSE session directly from that fd—no remount, no kernel interruption.
3. Because the new process also sets up signal handling and the upgrade socket machinery, it can itself hand off the fd again when it is time to restart (the same sequence with the new daemon started with the `--upgrade` flag).

### Testing upgrades

It is recommended to test upgrades with the **etcd** backend. The in-memory store lives only within the running process, so once the old daemon exits its data is gone—any files created before the upgrade will be lost. With etcd the data is external to the daemon and survives across restarts.

```bash
# Terminal 1 – start etcd
./start_etcd_docker.sh

# Terminal 2 – start the first daemon
FUSEFS_STORE_TYPE=etcd cargo run -- --upgrade
# SIGINT or SIGTERM to trigger the upgrade (e.g. Ctrl+C)

# Terminal 3 – start a new daemon to take over the FUSE session
FUSEFS_STORE_TYPE=etcd cargo run -- --upgrade &  # start the new daemon (connects to the socket)
```

After the handoff the new process serves the filesystem. Repeat the same sequence to upgrade again.
