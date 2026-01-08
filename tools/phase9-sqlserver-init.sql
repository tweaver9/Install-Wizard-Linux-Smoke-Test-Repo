-- Phase 9 E2E Verification: SQL Server Login Setup
-- Run this after container is healthy:
-- sqlcmd -S localhost,11433 -U sa -P "P9_Test_SqlServer_2024!" -C -i phase9-sqlserver-init.sql

-- Create login WITH sysadmin/dbcreator privilege (can create databases)
IF NOT EXISTS (SELECT * FROM sys.server_principals WHERE name = 'p9_admin')
BEGIN
    CREATE LOGIN p9_admin WITH PASSWORD = 'P9_Admin_Pass_2024!';
    ALTER SERVER ROLE dbcreator ADD MEMBER p9_admin;
    PRINT 'Created login p9_admin with dbcreator role';
END
ELSE
    PRINT 'Login p9_admin already exists';

-- Create login WITHOUT dbcreator privilege (cannot create databases)
IF NOT EXISTS (SELECT * FROM sys.server_principals WHERE name = 'p9_limited')
BEGIN
    CREATE LOGIN p9_limited WITH PASSWORD = 'P9_Limited_Pass_2024!';
    -- Only public role, no dbcreator
    PRINT 'Created login p9_limited with public role only';
END
ELSE
    PRINT 'Login p9_limited already exists';

-- Verify logins created
SELECT name, type_desc, 
       CASE WHEN IS_SRVROLEMEMBER('dbcreator', name) = 1 THEN 'YES' ELSE 'NO' END AS has_dbcreator
FROM sys.server_principals 
WHERE name IN ('p9_admin', 'p9_limited');

PRINT 'Phase 9 logins ready: p9_admin (dbcreator), p9_limited (public only)';
GO

