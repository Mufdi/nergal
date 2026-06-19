-- Add estimation scheme per team (linear-writeback R1.9).
-- notUsed | exponential | fibonacci | linear | tShirt — used by the detail view
-- to map numeric estimates to labels (e.g. tShirt: 1=XS, 2=S, 3=M, 4=L, 5=XL, 6=XXL).
ALTER TABLE linear_teams ADD COLUMN estimation_type TEXT;
