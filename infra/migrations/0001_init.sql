-- Phase 1 — Foundation & Local Infrastructure
--
-- Migration bookkeeping itself is handled automatically by sqlx's migration
-- runner (the `_sqlx_migrations` table it creates and checksums on first
-- run — see crates/sentinel-db/src/lib.rs). This file's job is the one
-- thing Phase 1 is scoped to add: least-privilege database roles, created
-- locally too (ADR-0002 "one writer per table", `docs/architecture.md` §5:
-- "TypeScript never writes raw, normalized, protocol, or derived tables").
--
-- This migration itself runs as the bootstrap superuser account
-- (`sentinel_bootstrap` locally — `POSTGRES_USER` in
-- infra/compose/docker-compose.yml), which is the only role with the
-- CREATEROLE privilege needed to create the two roles below, and is never
-- used by application code. "Least privilege" describes the two roles it
-- creates, not the account that creates them — someone has to own DDL.
--
-- No business tables exist yet. Phase 2 owns the data model
-- (docs/data-model.md, frozen) and adds the per-table GRANTs that make
-- these roles' least privilege real.
--
-- Credentials below are **local/CI-only fixture values**. They only ever
-- bind to loopback Postgres in `infra/compose/*` and CI — never a real
-- deployment. This is the database-role analogue of the fixed-seed
-- test-keypair allowance in CLAUDE.md §15 ("Test keypairs come from fixed
-- seeds in code"). A production deployment provisions real credentials out
-- of band via its own secret store, never from this file.

DO $$
BEGIN
  -- Owns writes to raw / normalized / chain-state / protocol / derived /
  -- job-queue tables from Phase 2 onward (docs/data-model.md §10).
  IF NOT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname = 'sentinel_rust') THEN
    CREATE ROLE sentinel_rust LOGIN PASSWORD 'sentinel_local_dev_only_rust'
      NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION;
  END IF;

  -- Reads everything the API needs and owns writes to execution_* tables
  -- only (docs/architecture.md §5: "TypeScript never writes raw,
  -- normalized, protocol, or derived tables").
  IF NOT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname = 'sentinel_ts') THEN
    CREATE ROLE sentinel_ts LOGIN PASSWORD 'sentinel_local_dev_only_ts'
      NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION;
  END IF;
END
$$;

-- Least privilege from the first migration: nobody gets ambient CREATE
-- rights on the public schema, and the two application roles get only
-- USAGE until Phase 2's migrations grant specific table privileges.
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
GRANT USAGE ON SCHEMA public TO sentinel_rust, sentinel_ts;
