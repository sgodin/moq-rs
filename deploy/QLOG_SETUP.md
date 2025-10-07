# Qlog Setup on Fly.io

This guide explains how to enable qlog logging for your moq-relay deployment on Fly.io.

## What is Qlog?

Qlog is a structured logging format for QUIC connections that captures detailed protocol-level events. The qlog files can be visualized using tools like [qvis.quictools.info](https://qvis.quictools.info/).

## Quick Start (Ephemeral Storage)

Qlog is **enabled by default** and writes to `/tmp/qlog` (ephemeral storage).

### Deploy

```bash
fly deploy
```

### Verify

Check the logs to confirm qlog is enabled:

```bash
fly logs
```

You should see:
```
qlog enabled: writing to /tmp/qlog
[INFO moq_native_ietf::quic] qlog output enabled: /tmp/qlog
```

### Access qlog files

SSH into a Machine and list the qlog files:

```bash
fly ssh console
ls -lh /tmp/qlog/
```

Download a qlog file to analyze:

```bash
fly ssh sftp get /tmp/qlog/<connection_id>_server.qlog
```

Then upload to [qvis.quictools.info](https://qvis.quictools.info/) for visualization.

### Disabling Qlog

To disable qlog, remove or comment out the `QLOG_DIR` environment variable in `fly.toml`:

```toml
[env]
# QLOG_DIR="/tmp/qlog"
```

Then redeploy:

```bash
fly deploy
```

## Important Notes

**Ephemeral Storage**: Files in `/tmp/qlog/` are **lost on restart/redeploy**. This is fine for debugging but not suitable for long-term logging.

**For Persistent Storage**: If you need qlog files to survive restarts, set up Fly Volumes:
1. Create volumes: `fly volumes create qlog_data --region <region> --size 1` (one per Machine)
2. Add mount in `fly.toml`: `[mounts] source = "qlog_data", destination = "/data"`
3. Change `QLOG_DIR="/data/qlog"`

**File Names**: Qlog files are named using the QUIC Connection ID: `<cid>_server.qlog`

**Disk Usage**: Each connection creates one qlog file. Files accumulate until restart. Monitor with `du -sh /tmp/qlog/` if running for extended periods.
