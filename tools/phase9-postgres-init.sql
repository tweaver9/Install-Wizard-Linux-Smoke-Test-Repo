-- Phase 9 E2E Verification: Postgres Role Setup
-- This script runs automatically when the container starts

-- Create role WITH CREATEDB privilege (can create databases)
CREATE ROLE p9_admin WITH LOGIN PASSWORD 'P9_Admin_Pass_2024!' CREATEDB;

-- Create role WITHOUT CREATEDB privilege (cannot create databases)
CREATE ROLE p9_limited WITH LOGIN PASSWORD 'P9_Limited_Pass_2024!';

-- Grant connect on template1 for both roles
GRANT CONNECT ON DATABASE postgres TO p9_admin;
GRANT CONNECT ON DATABASE postgres TO p9_limited;

-- Verify roles created
DO $$
BEGIN
  RAISE NOTICE 'Phase 9 roles created: p9_admin (CREATEDB), p9_limited (no CREATEDB)';
END $$;

