-- clickup-sync group 4 (poller). `stale` alone cannot drive a retention
-- window: GC needs to know WHEN a row was tombstoned. `stale_since` records
-- the 0→1 transition; rows un-tombstoned by reappearance leave the old
-- timestamp behind harmlessly (GC only considers stale=1 rows).
ALTER TABLE clickup_tasks ADD COLUMN stale_since INTEGER;
ALTER TABLE clickup_lists ADD COLUMN stale_since INTEGER;
ALTER TABLE clickup_folders ADD COLUMN stale_since INTEGER;
