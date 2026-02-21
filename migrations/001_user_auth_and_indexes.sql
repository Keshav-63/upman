-- UpMan Backend - Production Database Schema
-- Scheduler v3 with User Authentication and Multi-Tenant Support
-- Run this migration in your Supabase SQL Editor

-- ============================================
-- 1. ADD USER AUTHENTICATION SUPPORT
-- ============================================

-- Add user_id column to monitors (links to Supabase auth.users)
ALTER TABLE monitors
ADD COLUMN IF NOT EXISTS user_id UUID NOT NULL;

-- ============================================
-- 2. SCHEDULER V3 COLUMNS
-- ============================================

ALTER TABLE monitors
ADD COLUMN IF NOT EXISTS next_run_at TIMESTAMPTZ,
ADD COLUMN IF NOT EXISTS lease_until TIMESTAMPTZ,
ADD COLUMN IF NOT EXISTS failure_count INT DEFAULT 0;

-- Initialize next_run_at for existing monitors
UPDATE monitors
SET next_run_at = COALESCE(next_run_at, now());

-- ============================================
-- 3. PERFORMANCE & SCALABILITY INDEXES
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
-- 4. ROW-LEVEL SECURITY (RLS) - OPTIONAL
-- ============================================
-- Uncomment if using Supabase RLS for additional security

-- Enable RLS on monitors table
-- ALTER TABLE monitors ENABLE ROW LEVEL SECURITY;

-- Policy: Users can only see their own monitors
-- CREATE POLICY monitors_user_isolation ON monitors
--   FOR ALL
--   USING (auth.uid() = user_id);

-- Enable RLS on checks (inherited from monitors)
-- ALTER TABLE checks ENABLE ROW LEVEL SECURITY;

-- CREATE POLICY checks_via_monitors ON checks
--   FOR SELECT
--   USING (
--     EXISTS (
--       SELECT 1 FROM monitors 
--       WHERE monitors.id = checks.monitor_id 
--       AND monitors.user_id = auth.uid()
--     )
--   );

-- Enable RLS on incidents
-- ALTER TABLE incidents ENABLE ROW LEVEL SECURITY;

-- CREATE POLICY incidents_via_monitors ON incidents
--   FOR SELECT
--   USING (
--     EXISTS (
--       SELECT 1 FROM monitors 
--       WHERE monitors.id = incidents.monitor_id 
--       AND monitors.user_id = auth.uid()
--     )
--   );

-- ============================================
-- 5. TABLE CONSTRAINTS & VALIDATIONS
-- ============================================

-- Ensure interval is reasonable (30 seconds minimum)
ALTER TABLE monitors
ADD CONSTRAINT IF NOT EXISTS monitors_interval_min
CHECK (interval_seconds >= 30);

-- Ensure alert_after_failures is positive
ALTER TABLE monitors
ADD CONSTRAINT IF NOT EXISTS monitors_alert_threshold_positive
CHECK (alert_after_failures > 0);

-- ============================================
-- 6. ANALYTICS HELPER FUNCTIONS (OPTIONAL)
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
-- 7. CLEANUP & ARCHIVAL (OPTIONAL)
-- ============================================

-- Create archive table for old checks (optional)
-- Useful for managing data retention

-- CREATE TABLE IF NOT EXISTS checks_archive (
--   LIKE checks INCLUDING ALL
-- );

-- Function to archive old checks (run via cron)
-- CREATE OR REPLACE FUNCTION archive_old_checks()
-- RETURNS VOID AS $$
-- BEGIN
--   INSERT INTO checks_archive
--   SELECT * FROM checks
--   WHERE checked_at < now() - interval '90 days';
--   
--   DELETE FROM checks
--   WHERE checked_at < now() - interval '90 days';
-- END;
-- $$ LANGUAGE plpgsql;

-- ============================================
-- 8. VERIFICATION QUERIES
-- ============================================

-- Check that indexes were created
SELECT 
  tablename, 
  indexname, 
  indexdef 
FROM pg_indexes 
WHERE tablename IN ('monitors', 'checks', 'incidents')
ORDER BY tablename, indexname;

-- Check monitor table structure
SELECT 
  column_name, 
  data_type, 
  is_nullable 
FROM information_schema.columns 
WHERE table_name = 'monitors'
ORDER BY ordinal_position;

-- Verify constraints
SELECT 
  conname as constraint_name,
  contype as constraint_type
FROM pg_constraint
WHERE conrelid = 'monitors'::regclass;

-- ============================================
-- MIGRATION COMPLETE ✅
-- ============================================
-- 
-- Next steps:
-- 1. Update .env with SUPABASE_JWT_SECRET
-- 2. Test authentication flow
-- 3. Monitor query performance
-- 4. Set up automated backups
-- 5. Configure monitoring/alerting
