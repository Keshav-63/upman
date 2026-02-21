-- UpMan: Add Performance Indexes and Constraints
-- Run this in Supabase SQL Editor

-- ============================================
-- 1. PERFORMANCE INDEXES
-- ============================================

-- Scheduler v3 index (critical for job claiming)
CREATE INDEX IF NOT EXISTS idx_monitors_scheduler
ON monitors (next_run_at, lease_until)
WHERE is_paused = FALSE;

-- User-scoped queries (multi-tenant isolation)
CREATE INDEX IF NOT EXISTS idx_monitors_user_id
ON monitors (user_id, created_at DESC);

-- Monitor lookups with ownership check
CREATE INDEX IF NOT EXISTS idx_monitors_user_id_monitor_id
ON monitors (user_id, id);

-- Check queries (analytics, recent checks)
CREATE INDEX IF NOT EXISTS idx_checks_monitor_checked
ON checks (monitor_id, checked_at DESC);

-- Check analytics time-range queries
CREATE INDEX IF NOT EXISTS idx_checks_monitor_time_up
ON checks (monitor_id, checked_at)
WHERE response_time_ms IS NOT NULL;

-- Incident queries
CREATE INDEX IF NOT EXISTS idx_incidents_monitor_status
ON incidents (monitor_id, status, started_at DESC);

-- Dashboard open incidents query
CREATE INDEX IF NOT EXISTS idx_incidents_status_started
ON incidents (status, started_at DESC)
WHERE status = 'open';

-- ============================================
-- 2. CONSTRAINTS & VALIDATIONS
-- ============================================

-- Fix any existing monitors with invalid interval (< 30 seconds)
UPDATE monitors
SET interval_seconds = 30
WHERE interval_seconds < 30;

-- Fix any existing monitors with invalid alert threshold
UPDATE monitors
SET alert_after_failures = 3
WHERE alert_after_failures <= 0;

-- Ensure interval is reasonable (30 seconds minimum)
DO $$ 
BEGIN
  ALTER TABLE monitors
  ADD CONSTRAINT monitors_interval_min
  CHECK (interval_seconds >= 30);
EXCEPTION
  WHEN duplicate_object THEN NULL;
END $$;

-- Ensure alert_after_failures is positive
DO $$ 
BEGIN
  ALTER TABLE monitors
  ADD CONSTRAINT monitors_alert_threshold_positive
  CHECK (alert_after_failures > 0);
EXCEPTION
  WHEN duplicate_object THEN NULL;
END $$;

-- ============================================
-- 3. HELPER FUNCTIONS (Optional but recommended)
-- ============================================

-- Function to calculate uptime for a monitor
CREATE OR REPLACE FUNCTION calculate_uptime(
  p_monitor_id UUID,
  p_days INT DEFAULT 30
)
RETURNS NUMERIC AS $$
  SELECT 
    COALESCE(
      (SUM(CASE WHEN is_up THEN 1 ELSE 0 END)::float / NULLIF(COUNT(*), 0)) * 100,
      0
    )
  FROM checks
  WHERE monitor_id = p_monitor_id
    AND checked_at > now() - (p_days || ' days')::interval;
$$ LANGUAGE SQL STABLE;

-- Function to get monitor status (up/down)
CREATE OR REPLACE FUNCTION get_monitor_status(p_monitor_id UUID)
RETURNS TEXT AS $$
  SELECT CASE WHEN is_up THEN 'up' ELSE 'down' END
  FROM checks
  WHERE monitor_id = p_monitor_id
  ORDER BY checked_at DESC
  LIMIT 1;
$$ LANGUAGE SQL STABLE;

-- ============================================
-- 4. VERIFICATION
-- ============================================

-- Check indexes were created
SELECT 
  tablename, 
  indexname 
FROM pg_indexes 
WHERE tablename IN ('monitors', 'checks', 'incidents')
  AND indexname LIKE 'idx_%'
ORDER BY tablename, indexname;

-- Check constraints
SELECT 
  conname as constraint_name
FROM pg_constraint
WHERE conrelid = 'monitors'::regclass
  AND conname LIKE 'monitors_%';

-- ✅ DONE! Now configure your .env and start the backend
