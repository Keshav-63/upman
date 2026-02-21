# Logging Configuration

## Default Log Levels

The backend now uses filtered logging to reduce noise:

- **Application** (`upman_backend`): `INFO` level
- **HTTP** (`tower_http`, `axum`): `INFO` level  
- **Database** (`sqlx`): `WARN` level (no query logs)
- **HTTP client** (`hyper`, `reqwest`): `WARN` level

## What You'll See

### ✅ You WILL see:
- Server startup/shutdown
- Worker status (every 5 minutes when idle)
- Monitor failures and warnings
- Incident creation/resolution
- Email sending status
- Errors and warnings
- JWKS fetch events (when cache expires)

### ❌ You WON'T see:
- Every SQL query (DEBUG level)
- Successful health checks (too verbose)
- HTTP connection details (TRACE level)
- Authentication details (TRACE level)
- Monitor state updates (TRACE level)

## Override Log Levels

Set `RUST_LOG` environment variable in `.env` file:

```bash
# Debug mode - see everything
RUST_LOG=upman_backend=debug,sqlx=debug

# See all SQL queries
RUST_LOG=upman_backend=info,sqlx=debug

# Quieter - only warnings and errors
RUST_LOG=warn

# Custom per-module
RUST_LOG=upman_backend=debug,tower_http=warn,sqlx=warn
```

## Log Locations

- **Console**: Real-time logs to stdout
- **Files**: `./logs/upman.log.*` (daily rotation)

## Example Clean Output

```
2026-02-21T14:30:00.000Z INFO upman_backend: 🚀 Starting UpMan Backend v3
2026-02-21T14:30:00.100Z INFO upman_backend: ✅ Connected to database (pool size: 10)
2026-02-21T14:30:00.200Z INFO upman_backend::services::checker: 🔄 Checker worker started
2026-02-21T14:31:00.000Z WARN upman_backend::services::checker: Monitor is DOWN - incrementing failure count
2026-02-21T14:31:00.100Z WARN upman_backend::services::checker: 🚨 Opening incident - threshold reached
2026-02-21T14:31:00.200Z INFO upman_backend::services::checker: ✅ Incident created successfully
2026-02-21T14:31:00.300Z INFO upman_backend::services::checker: 📧 Sending alert email...
2026-02-21T14:31:01.000Z INFO upman_backend::services::email: 📧 Email sent successfully
```

Much cleaner than before! 🎉
