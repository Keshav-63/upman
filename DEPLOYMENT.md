# 🚀 UpMan Backend - Production Deployment Guide

Complete guide for deploying the production-grade UpMan backend with Scheduler v3, JWT authentication, and advanced analytics.

---

## 📋 Prerequisites

- ✅ Rust 1.70+ installed
- ✅ PostgreSQL or Supabase account
- ✅ SMTP server credentials (Gmail, SendGrid, etc.)
- ✅ Supabase project (for auth)

---

## 🗄️ Database Setup

### 1. Run Migration

Execute the migration file in your Supabase SQL Editor:

```bash
migrations/001_user_auth_and_indexes.sql
```

This creates:
- ✅ User authentication columns
- ✅ Scheduler v3 fields (next_run_at, lease_until, failure_count)
- ✅ Performance indexes for multi-tenant queries
- ✅ Constraints and validations

### 2. Verify Migration

```sql
-- Check indexes
SELECT tablename, indexname 
FROM pg_indexes 
WHERE tablename IN ('monitors', 'checks', 'incidents');

-- Should see:
-- idx_monitors_scheduler
-- idx_monitors_user_id
-- idx_checks_monitor_checked
-- idx_incidents_monitor_status
-- etc.
```

---

## ⚙️ Environment Configuration

Create a `.env` file in the backend root:

```env
# Database
DATABASE_URL=postgresql://user:password@host:5432/upman
MAX_DB_CONNECTIONS=10

# Server
PORT=8000

# Supabase Authentication
SUPABASE_JWT_SECRET=your-supabase-jwt-secret

# SMTP Configuration
SMTP_HOST=smtp.gmail.com
SMTP_PORT=587
SMTP_USER=your-email@gmail.com
SMTP_PASS=your-app-specific-password
ALERT_FROM=alerts@upman.io
```

### Getting Your Supabase JWT Secret

1. Go to Supabase Dashboard → Project Settings → API
2. Copy the **JWT Secret** (not the anon key)
3. Add to `.env` as `SUPABASE_JWT_SECRET`

---

## 🏗️ Build & Run

### Development

```bash
cd upman--backend
cargo run
```

Logs will appear in console and `./logs/upman.log`

### Production Build

```bash
# Build optimized binary
cargo build --release

# Binary location
./target/release/upman-backend

# Run production
./target/release/upman-backend
```

### Docker Deployment (Optional)

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/upman-backend /usr/local/bin/
EXPOSE 8000
CMD ["upman-backend"]
```

Build and run:
```bash
docker build -t upman-backend .
docker run -p 8000:8000 --env-file .env upman-backend
```

---

## 🌐 API Endpoints

### Public Routes

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Health check |

### Protected Routes (Require `Authorization: Bearer <token>`)

#### Dashboard
| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/dashboard` | User dashboard statistics |

#### Monitors
| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/monitors` | Create monitor |
| GET | `/monitors?page=1&per_page=20` | List monitors (paginated) |
| GET | `/monitors/:id` | Get monitor details |
| PUT | `/monitors/:id` | Update monitor |
| DELETE | `/monitors/:id` | Delete monitor |

#### Monitor Data
| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/monitors/:id/checks?page=1` | Recent checks |
| GET | `/monitors/:id/incidents?page=1` | Incident history |

#### Analytics
| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/monitors/:id/stats` | Comprehensive stats |
| GET | `/monitors/:id/uptime?days=30` | Uptime percentage |
| GET | `/monitors/:id/latency?days=30` | Response time percentiles |
| GET | `/monitors/:id/mttr` | Mean time to recovery |
| GET | `/monitors/:id/availability?days=30` | Daily availability chart |

---

## 🔐 Authentication Flow

### 1. User Signs In (Supabase Frontend)

```javascript
// Frontend - Sign in with Supabase
const { data, error } = await supabase.auth.signInWithPassword({
  email: 'user@example.com',
  password: 'password123'
})

const accessToken = data.session.access_token
```

### 2. Make Authenticated Request

```javascript
// Frontend - Call UpMan API
const response = await fetch('http://localhost:8000/monitors', {
  headers: {
    'Authorization': `Bearer ${accessToken}`,
    'Content-Type': 'application/json'
  }
})
```

### 3. Backend Validates Token

The `auth_middleware` automatically:
1. Extracts token from `Authorization` header
2. Validates JWT signature using `SUPABASE_JWT_SECRET`
3. Checks expiration
4. Injects `user_id` into request handlers

---

## 📊 Monitoring & Logs

### Log Files

Logs are written to `./logs/upman.log` with daily rotation:

```bash
tail -f ./logs/upman.log
```

Log format includes:
- Timestamp
- Thread ID
- Log level
- File & line number
- Message

### Key Metrics to Monitor

1. **Worker Health**: Check logs for worker metrics
   ```
   Worker metrics total_checks=1234 iteration=100
   ```

2. **Database Pool**: Monitor connection pool usage
3. **API Latency**: Track request duration in logs
4. **Error Rates**: Watch for authentication failures

---

## 🚀 Production Checklist

### Security
- [ ] Change default JWT secret
- [ ] Use HTTPS in production
- [ ] Enable rate limiting (add middleware)
- [ ] Set up firewall rules
- [ ] Enable database connection encryption

### Performance
- [ ] Tune `MAX_DB_CONNECTIONS` based on load
- [ ] Set up CDN for static assets
- [ ] Enable database query caching
- [ ] Monitor index usage

### Reliability
- [ ] Set up automated database backups
- [ ] Configure health check monitoring
- [ ] Set up error alerting (Sentry, etc.)
- [ ] Document disaster recovery plan

### Scalability
- [ ] Test with multiple worker instances
- [ ] Verify `FOR UPDATE SKIP LOCKED` works correctly
- [ ] Monitor queue depth and lag
- [ ] Plan horizontal scaling strategy

---

## 🧪 Testing

### Test Authentication

```bash
# Get a JWT token from Supabase (use your real token)
TOKEN="eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."

# Test protected endpoint
curl -H "Authorization: Bearer $TOKEN" http://localhost:8000/dashboard

# Should return dashboard stats, not 401
```

### Test Monitor Creation

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

### Test Pagination

```bash
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8000/monitors?page=1&per_page=10"
```

---

## 🔄 Multi-Worker Scalability

The system is designed to run multiple instances safely:

### Same Machine (Test)
```rust
// In main.rs - already configured
start_checker(pool.clone()).await;  // Worker 1
start_checker(pool.clone()).await;  // Worker 2
```

### Multiple Machines (Production)

Deploy the same binary to multiple servers. Workers automatically:
- ✅ Claim jobs atomically via `FOR UPDATE SKIP LOCKED`
- ✅ Never process the same monitor twice
- ✅ Recover from crashes via 60-second lease timeout

Example with Docker Compose:
```yaml
version: '3.8'
services:
  worker-1:
    image: upman-backend
    env_file: .env
  worker-2:
    image: upman-backend
    env_file: .env
  worker-3:
    image: upman-backend
    env_file: .env
```

---

## 📈 Performance Tuning

### Database Connection Pool

Adjust based on concurrent load:
```env
# For high traffic
MAX_DB_CONNECTIONS=50

# For low traffic  
MAX_DB_CONNECTIONS=10
```

### Worker Batch Size

Edit `services/checker.rs`:
```rust
let rows = claim_jobs(&pool, 100).await;  // Increase from 50
```

### Check Retention

To prevent table bloat, archive old checks:
```sql
-- Delete checks older than 90 days
DELETE FROM checks 
WHERE checked_at < now() - interval '90 days';
```

---

## 🐛 Troubleshooting

### Authentication Errors (401)

**Problem**: `Invalid or expired token`

**Solution**:
1. Verify `SUPABASE_JWT_SECRET` matches your project
2. Check token hasn't expired
3. Ensure `Bearer ` prefix in Authorization header

### Database Connection Errors

**Problem**: `Failed to connect to database`

**Solution**:
1. Verify `DATABASE_URL` format
2. Check database is running
3. Verify firewall allows connections
4. Test with `psql` directly

### No Jobs Being Processed

**Problem**: Workers idle, monitors not checked

**Solution**:
1. Check `is_paused` column (should be FALSE)
2. Verify `next_run_at` is in the past
3. Check `lease_until` isn't stuck

```sql
-- Reset stuck leases
UPDATE monitors 
SET lease_until = NULL 
WHERE lease_until < now() - interval '5 minutes';
```

---

## 📞 Support

For issues or questions:
1. Check logs: `./logs/upman.log`
2. Review migration: `migrations/001_user_auth_and_indexes.sql`
3. Check database indexes and constraints
4. Verify environment variables

---

## 🎉 You're Ready!

Your UpMan backend is now production-ready with:
- ✅ Multi-tenant user authentication
- ✅ Crash-safe job scheduling
- ✅ Multi-worker scalability
- ✅ Advanced analytics & pagination
- ✅ Comprehensive logging
- ✅ Performance-optimized indexes

Happy monitoring! 🚀
