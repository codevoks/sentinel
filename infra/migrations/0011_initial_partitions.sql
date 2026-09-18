-- Phase 2 — initial slot-range partitions for the six tables
-- phase-02-data-model.md §1.2 names: raw_observations, transactions,
-- instructions, program_logs, account_observations, token_balance_deltas.
--
-- Partition size: 10,000,000 slots, per data-model.md §2's explicit
-- statement for raw_observations ("Range on slot, 10M-slot partitions").
-- The other five tables state only "Range on slot" with no explicit size;
-- this migration uses the same 10M-slot size for all six so partition
-- boundaries line up across tables sharing a slot range (a single ingested
-- block touches all six), which is the only stated policy to generalize
-- from without inventing a new number.
--
-- This migration creates plain, static partitions (ordinary forward-only
-- DDL — the same thing as any other CREATE TABLE in this migration set) for
-- an initial low-slot range so a fresh database can accept data starting at
-- slot 0 (Surfpool/local-validator slots start near 0). ONGOING,
-- ahead-of-head automation is application code, not schema: Phase 2's
-- non-scope explicitly forbids stored procedures and triggers, and the
-- phase's own file list names `crates/sentinel-db/src/partitions.rs` as the
-- automation's home — see that module for the "create N partitions ahead
-- of the observed chain head" logic and its test coverage.
--
-- Deliberately NO DEFAULT partition on any of these six tables: a slot
-- outside every created partition must fail loudly ("no partition of
-- relation found for row"), never silently land in a catch-all. A default
-- partition would defeat the entire point of the missing-partition test
-- required by phase-02-data-model.md §7.

CREATE TABLE raw_observations_p0 PARTITION OF raw_observations FOR VALUES FROM (0) TO (10000000);
CREATE TABLE raw_observations_p1 PARTITION OF raw_observations FOR VALUES FROM (10000000) TO (20000000);

CREATE TABLE transactions_p0 PARTITION OF transactions FOR VALUES FROM (0) TO (10000000);
CREATE TABLE transactions_p1 PARTITION OF transactions FOR VALUES FROM (10000000) TO (20000000);

CREATE TABLE instructions_p0 PARTITION OF instructions FOR VALUES FROM (0) TO (10000000);
CREATE TABLE instructions_p1 PARTITION OF instructions FOR VALUES FROM (10000000) TO (20000000);

CREATE TABLE program_logs_p0 PARTITION OF program_logs FOR VALUES FROM (0) TO (10000000);
CREATE TABLE program_logs_p1 PARTITION OF program_logs FOR VALUES FROM (10000000) TO (20000000);

CREATE TABLE account_observations_p0 PARTITION OF account_observations FOR VALUES FROM (0) TO (10000000);
CREATE TABLE account_observations_p1 PARTITION OF account_observations FOR VALUES FROM (10000000) TO (20000000);

CREATE TABLE token_balance_deltas_p0 PARTITION OF token_balance_deltas FOR VALUES FROM (0) TO (10000000);
CREATE TABLE token_balance_deltas_p1 PARTITION OF token_balance_deltas FOR VALUES FROM (10000000) TO (20000000);
