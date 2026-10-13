-- Check last refresh result
SELECT database, view, status, progress, last_refresh_time, next_refresh_time FROM system.view_refreshes;

-- Manually trigger refresh and watch
SYSTEM REFRESH VIEW native_balances_final;