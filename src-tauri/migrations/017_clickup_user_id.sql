-- Cache the token user's id so the assigned-to-me filter and post-baseline
-- assignment detection survive restarts without waiting on a live GET /user.
ALTER TABLE clickup_sync_state ADD COLUMN user_id INTEGER;
