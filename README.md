# UpMan Backend

Production grade uptime monitoring backend built with Rust, featuring **Scheduler v3** with multi worker safety, **Supabase JWT authentication**, crash-safe job leasing, and advanced analytics

## Project Structure 

```
src/
├── main.rs              # Application entry point & routing
├── config.rs            # Environment configuration
├── auth.rs              # JWT authentication middleware
├── error.rs             # Custom error types & handling
├── models/              # Data structures & DTOs
│   ├── mod.rs
│   ├── monitor.rs       # Monitor models & pagination
│   └── analytics.rs     # Analytics response types
├── handlers/            # HTTP request handlers
│   ├── mod.rs
│   ├── monitor.rs       # Monitor CRUD & data endpoints
│   └── analytics.rs     # Uptime, latency, MTTR, charts
└── services/            # Business logic
    ├── mod.rs
    ├── checker.rs       # Scheduler v3 uptime checker
    └── email.rs         # SMTP email alerts

migrations/
└── 001_user_auth_and_indexes.sql   # Production schema

logs/                    # Application logs (auto-created)
└── upman.log

DEPLOYMENT.md            # Complete deployment guide
.env.example             # Environment template
```

## Features 

### 🔐 Authentication & Multi-Tenancy
- ✅ **Supabase JWT validation** - Secure token-based auth
- ✅ **User-scoped data** - Complete tenant isolation
- ✅ **Row-level security ready** - Optional RLS policies included

### ⚡ Scheduler v3
- ✅ **Multi-worker safe** - `FOR UPDATE SKIP LOCKED` prevents conflicts
- ✅ **Crash-safe** - 60-second job leasing with automatic recovery
- ✅ **Scalable** - Run multiple instances (Docker, HF Spaces, K8s)
- ✅ **Deterministic** - `next_run_at` based scheduling
- ✅ **Smart backoff** - Exponential backoff + jitter on failures

### 📊 Advanced Analytics
- 📈 **Dashboard stats** - Overview of all monitors, incidents, uptime
- 📊 **Monitor stats** - 24h/7d/30d uptime, MTTR, current streak
- ⚡ **Latency metrics** - p50, p95, p99, avg, min, max
- 🕐 **MTTR tracking** - Mean time to recovery
- 📉 **Daily charts** - Availability over time
- 🔍 **Recent checks** - Paginated check history

### 🚀 Production Ready
- ✅ **Pagination** - All list endpoints support `?page=1&per_page=20`
- ✅ **Proper indexing** - Optimized queries for multi-tenant scale
- ✅ **Error handling** - Structured errors with proper HTTP status codes
- ✅ **Logging** - Daily rotating logs with structured tracing
- ✅ **CORS enabled** - Cross-origin requests supported
- ✅ **Request tracing** - HTTP middleware for observability

### 📧 Alerting
- 🚨 **Email notifications** - SMTP-based alerts
- 📊 **Incident tracking** - Open/resolved status with duration
- ⚙️ **Configurable thresholds** - Set failure count before alerting

## Getting Started

### Prerequisites
- Rust 1.70+
- PostgreSQL or Supabase
- SMTP server access
- Supabase project (for auth)

### 1. Database Migration

Run the migration in your Supabase SQL Editor:

```sql
-- See: migrations/001_user_auth_and_indexes.sql
-- This creates:
-- - user_id column for multi-tenancy
-- - Scheduler v3 columns (next_run_at, lease_until, failure_count)
-- - Performance indexes
-- - Constraints and validations
```

### 2. Environment Setup

```bash
# Copy the example env file
cp .env.example .env

# Edit .env and set:
# - DATABASE_URL (your Postgres/Supabase connection string)
# - SUPABASE_JWT_SECRET (from Supabase Dashboard → API → JWT Secret)
# - SMTP credentials
# - PORT (default: 8000)
```

### 3. Run

```bash
# Development
cargo run

# Production (optimized)
cargo build --release
./target/release/upman-backend
```

Logs will be written to `./logs/upman.log` with daily rotation.

### 4. Test

```bash
# Health check (public)
curl http://localhost:8000/health

# Get dashboard (requires auth)
curl -H "Authorization: Bearer YOUR_SUPABASE_JWT" \
  http://localhost:8000/dashboard
```

See [DEPLOYMENT.md](DEPLOYMENT.md) for complete deployment instructions.

---

## 📡 API Endpoints

All endpoints except `/health` require authentication:
```
Authorization: Bearer <supabase_jwt_token>
```

### Public
- `GET /health` - Health check

### Dashboard
- `GET /dashboard` - User dashboard with aggregate stats

### Monitors
- `POST /monitors` - Create monitor
- `GET /monitors?page=1&per_page=20` - List monitors (paginated)
- `GET /monitors/:id` - Get monitor details
- `PUT /monitors/:id` - Update monitor
- `DELETE /monitors/:id` - Delete monitor

### Monitor Data
- `GET /monitors/:id/checks?page=1&per_page=50` - Recent checks
- `GET /monitors/:id/incidents?page=1` - Incident history

### Analytics
- `GET /monitors/:id/stats` - Comprehensive statistics
- `GET /monitors/:id/uptime?days=30` - Uptime percentage
- `GET /monitors/:id/latency?days=30` - Response time percentiles
- `GET /monitors/:id/mttr` - Mean time to recovery
- `GET /monitors/:id/availability?days=30` - Daily availability chart

### Request/Response Examples

#### Create Monitor
```bash
curl -X POST http://localhost:8000/monitors \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://example.com",
    "interval_seconds": 60,
    "alert_email": "you@example.com",
    "alert_after_failures": 3
  }'
```

Response:
```json
{
  "success": true,
  "monitor_id": "123e4567-e89b-12d3-a456-426614174000"
}
```

#### Get Dashboard
```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8000/dashboard
```

Response:
```json
{
  "total_monitors": 10,
  "active_monitors": 8,
  "paused_monitors": 2,
  "monitors_up": 7,
  "monitors_down": 1,
  "open_incidents": 1,
  "avg_uptime_24h": 99.2,
  "total_checks_24h": 1440
}
```

#### List Monitors (Paginated)
```bash
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8000/monitors?page=1&per_page=10"
```

Response:
```json
{
  "data": [
    {
      "id": "uuid",
      "url": "https://example.com",
      "interval_seconds": 60,
      "alert_email": "user@example.com",
      "alert_after_failures": 3,
      "is_paused": false,
      "created_at": "2026-01-01T00:00:00Z",
      "last_status": "up",
      "uptime_24h": 99.5
    }
  ],
  "pagination": {
    "page": 1,
    "per_page": 10,
    "total": 25,
    "total_pages": 3
  }
}
```

---

## 🧪 Testing Multi-Worker Safety

The application starts 2 checker workers by default:

```rust
start_checker(pool.clone()).await;
start_checker(pool.clone()).await;
```

Both workers will:
- ✅ Never process the same monitor simultaneously
- ✅ Automatically split work via `FOR UPDATE SKIP LOCKED`
- ✅ Release leases on completion or timeout

## 📦 Dependencies

- `axum` - Web framework
- `sqlx` - Async PostgreSQL client
- `tokio` - Async runtime
- `reqwest` - HTTP client for checks
- `lettre` - Email delivery
- `serde` - Serialization
- `chrono` - Time handling
- `uuid` - Unique identifiers
- `rand` - Jitter generation

## 🔜 Future Enhancements

- 🔐 Authentication (Supabase JWT)
- 📊 Advanced analytics (rollups, percentiles)
- 📦 Webhook alerts
- 💬 Slack/Discord integrations
- ⚙️ Worker health metrics

## 📄 License

MIT
