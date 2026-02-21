-- Check monitor configuration
SELECT 
    id,
    url,
    interval_seconds,
    alert_email,
    alert_after_failures,
    failure_count,
    is_paused,
    next_run_at,
    lease_until
FROM monitors
ORDER BY created_at DESC
LIMIT 5;

-- Check recent checks
SELECT 
    monitor_id,
    checked_at,
    status_code,
    response_time_ms,
    is_up,
    error_message
FROM checks
ORDER BY checked_at DESC
LIMIT 10;

-- Check incidents
SELECT 
    monitor_id,
    started_at,
    ended_at,
    status,
    reason
FROM incidents
ORDER BY started_at DESC
LIMIT 5;
