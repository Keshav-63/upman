-- 1. Check all monitors and their current state
SELECT 
    id,
    url,
    failure_count,
    alert_after_failures,
    is_paused,
    created_at,
    next_run_at,
    alert_email
FROM monitors
ORDER BY created_at DESC;

-- 2. Check recent checks for each monitor (last 10)
SELECT 
    c.monitor_id,
    m.url,
    c.checked_at,
    c.is_up,
    c.status_code,
    c.response_time_ms,
    c.error_message
FROM checks c
JOIN monitors m ON c.monitor_id = m.id
ORDER BY c.checked_at DESC
LIMIT 20;

-- 3. Check for any incidents
SELECT 
    i.id,
    i.monitor_id,
    m.url,
    i.started_at,
    i.ended_at,
    i.status,
    i.reason
FROM incidents i
JOIN monitors m ON i.monitor_id = m.id
ORDER BY i.started_at DESC
LIMIT 10;

-- 4. Detailed view: monitors with their latest check status
SELECT 
    m.id,
    m.url,
    m.failure_count,
    m.alert_after_failures,
    m.alert_email,
    (SELECT is_up FROM checks WHERE monitor_id = m.id ORDER BY checked_at DESC LIMIT 1) as last_check_up,
    (SELECT checked_at FROM checks WHERE monitor_id = m.id ORDER BY checked_at DESC LIMIT 1) as last_check_time,
    (SELECT status_code FROM checks WHERE monitor_id = m.id ORDER BY checked_at DESC LIMIT 1) as last_status_code,
    (SELECT error_message FROM checks WHERE monitor_id = m.id ORDER BY checked_at DESC LIMIT 1) as last_error
FROM monitors m
ORDER BY m.created_at DESC;
