# ADR-0008 — An immutable raw observation boundary before any decoding

**Status:** Accepted · **Date:** 2026-09-05 · **Phase:** 0

## Context

Most indexers decode on ingest and persist only the decoded result. It is simpler, cheaper in storage,
and it means that **the input to every decision the system ever made is gone**.

Sentinel's flagship correctness property is "delete derived state, replay, reproduce byte-identical
state." That property is meaningless unless the *inputs* survive independently of the code that
consumed them.

## Decision

**Every observation is persisted, byte-exact and immutable, before anything interprets it.**

```
raw_observations(kind, natural_key, payload, payload_hash, slot, commitment,
                 source, provider_id, request_id, observed_at)
UNIQUE (kind, natural_key, payload_hash)
```

- **Append-only.** No `UPDATE`, no `DELETE` from application code, enforced by the database role.
- **`payload_hash` is part of the uniqueness key**, so two providers returning *different bytes* for
  the same logical observation both persist.
- **Exactly one writer** (`sentinel-ingest`).
- Decoding is a **separate, restartable stage** reading from this table.

## What this buys, concretely

1. **Replay is real.** Truncate protocol and derived tables, replay, compare digests (RP-01). Without a
   durable input, "replay" would mean re-fetching from the network — non-deterministic, rate-limited,
   and impossible for pruned history.
2. **Decoder bugs become retroactively fixable.** A layout error found in month three is corrected by
   registering a new decoder version and replaying (`aegis-integration.md` §8.3). Without raw, the
   corrupted rows are all that is left and the history is permanently wrong.
3. **Forensics.** When Sentinel and Aegis disagree, the argument is settled by the bytes actually
   observed, tagged with which provider returned them and when. `request_id` links every byte to the
   call that fetched it.
4. **Provider divergence is detectable.** Because differing payloads both persist, divergence becomes
   evidence rather than a coin flip (`ingestion-model.md` §10). Any last-write-wins scheme destroys the
   mechanism.
5. **Decode failures are recoverable.** A malformed payload produces a `decode_failures` row pointing
   at the exact bytes, so a fix can replay precisely the failing inputs.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **Decode on ingest, persist only decoded rows** | No replay, no retroactive decoder fixes, no forensics, no divergence detection. Cheaper and structurally worse. |
| **Persist raw only for records that fail to decode** | Solves forensics for known failures and nothing else. A decoder that is *wrong but does not error* — the dangerous case — leaves no evidence at all. |
| **Persist raw to object storage instead of Postgres** | Breaks "insert raw and advance the checkpoint in one transaction" (ING-03), which is the entire crash-safety argument for ingestion. Object storage is the right **archive** target, and partition detach feeds it (ADR-0002). |
| **Keep raw in memory / a short buffer** | Lost on restart, exactly when it is needed. |
| **Store a decoded canonical form as "raw"** | That is a decoder, so a decoder bug is baked in irreversibly. The point is bytes before interpretation. |
| **Last-write-wins on the natural key** | Destroys divergence detection and makes the store non-deterministic under provider fan-out. |

## Consequences

**Positive**
- Replay determinism, retroactive decoder fixes, forensics, divergence detection — all four fall out of
  one decision.
- Decoding can be restarted, parallelized, or upgraded without touching ingestion.
- An incident is debuggable from the data, not from logs.

**Negative**
- **Storage amplification.** The dominant cost of this decision. Mitigated by compression, slot-range
  partitioning, and a stated hot/cold retention policy (`data-model.md` §9). Measured in Phase 14
  (hypothesis B-1).
- **A second write on the hot path.** Deliberate, and explicitly listed as a non-optimization
  (`performance-strategy.md` §7).
- Replaying a slot range whose partitions have been archived requires re-attaching them. The CLI states
  this rather than failing obscurely (BF-7).

**Enforcement**
- ING-02 / DM-02: no application code updates or deletes raw rows — role permissions **and** a test.
- ING-01 / DM-01: every normalized row traces to a raw observation.
- RP-01..RP-12: the replay proof.
- RP-09: replay with a missing raw observation **fails loudly** and enqueues a backfill; it never
  silently fetches, because that would quietly reintroduce non-determinism.
