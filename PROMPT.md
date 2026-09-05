You are the principal architect for Sentinel, a production-grade Solana execution, indexing, monitoring, risk, and protocol-operations platform.

This is PHASE 0 ONLY: requirements, architecture, reliability model, security model, protocol integration contracts, ADRs, documentation, and implementation roadmap.

DO NOT implement runtime production code.
DO NOT start Phase 1.
DO NOT scaffold fake implementation merely to make the repository look complete.

The purpose of this session is to freeze enough architecture that a lower-cost implementation model such as Claude Sonnet 5 can later execute one bounded phase at a time without inventing core design decisions.

⸻

0. CRITICAL CONTEXT: AEGIS IS THE FIRST-CLASS UPSTREAM PROTOCOL

Sentinel is paired with an existing project:

Aegis Protocol
GitHub:
https://github.com/codevoks/aegis-protocol

Aegis Phase 0 has already been completed.

You MUST inspect the actual Aegis repository before designing Sentinel.

At minimum read:

1. README.md
2. AGENTS.md
3. CLAUDE.md
4. docs/project-status.md
5. docs/product.md
6. docs/architecture.md
7. docs/account-model.md
8. docs/instruction-catalogue.md
9. docs/economic-model.md
10. docs/oracle-design.md
11. docs/invariants.md
12. docs/threat-model.md
13. docs/token-compatibility.md
14. docs/composability.md
15. docs/governance.md
16. docs/coverage-matrix.md
17. docs/phase-roadmap.md
18. relevant ADRs
19. docs/implementation-handoff.md

Do not invent an imaginary Aegis interface when the real design exists.

Aegis remains authoritative for:

* protocol economics
* account ownership
* PDA derivations
* state transitions
* health calculations
* liquidation semantics
* oracle validation
* token compatibility
* governance
* on-chain invariants

Sentinel is an off-chain observer/executor, not an alternate source of protocol truth.

If the Aegis repository is inaccessible, STOP and explicitly report that rather than inventing its interfaces.

⸻

1. PRODUCT THESIS

Sentinel is NOT:

* an explorer clone
* a generic analytics dashboard
* a toy WebSocket listener
* a CRUD wrapper around Solana RPC
* a collection of resume technologies
* an off-chain system that pretends to be authoritative over on-chain state

Sentinel should be a coherent production platform capable of:

* ingesting Solana state and transactions
* maintaining historical and near-real-time protocol state
* decoding and normalizing protocol-specific data
* exposing reliable APIs and realtime streams
* deriving risk and operational signals
* constructing, simulating, submitting, and tracking transactions
* operating automated protocol workflows such as liquidation
* recovering correctly from process crashes, RPC failures, duplicate delivery, reconnects, gaps, and replay
* providing observable evidence of correctness

The flagship integration is Aegis Protocol.

Later architecture may support selected external protocols such as SPL/Token-2022 and Jupiter, but only where they serve a real product purpose.

⸻

2. CURRENT-ECOSYSTEM RESEARCH

Before freezing decisions, verify the current September 2026 Solana ecosystem using authoritative/current sources where possible.

Research and document at least:

* current Solana/Agave RPC behavior relevant to ingestion
* WebSocket subscriptions
* commitment levels and finality behavior
* transaction/version support
* @solana/kit
* Rust Solana client ecosystem
* Yellowstone / Geyser ecosystem and current interfaces
* Anchor 1.x implications where protocol decoding needs them
* SPL / Token-2022
* current Jupiter integration surface if proposed
* priority fees
* compute budget behavior
* versioned transactions
* address lookup tables
* current recommended local development/testing tooling

Do not trust remembered package versions or program IDs.

Record unstable facts as research gates, not immutable architecture.

Paid infrastructure must NOT be required for the baseline project.

A zero-cost local development and demo path is mandatory.

⸻

3. ARCHITECTURAL PRINCIPLES

Design around these defaults unless strong evidence justifies changing them.

Languages

Use:

Rust
for performance-sensitive / correctness-sensitive components such as:

* ingestion
* transaction parsing
* protocol decoding where appropriate
* high-throughput workers
* possibly keeper execution loops

TypeScript
for:

* orchestration
* product API
* SDK/product integration
* transaction planning where ecosystem compatibility makes TS appropriate
* web/backend integration

Do NOT duplicate entire systems in both languages merely for coverage.

Explicitly assign ownership of each responsibility to Rust or TypeScript and explain why.

Storage

Use:

PostgreSQL as canonical off-chain persistent storage.

Redis may be used only for justified ephemeral roles such as:

* caching
* fanout
* rate limiting
* transient coordination
* short-lived execution state

Do NOT make Redis the canonical database.

Messaging

Do NOT introduce Kafka initially.

Only introduce a durable queue/log technology if the architecture demonstrates that Postgres-backed workflows or simpler mechanisms are insufficient.

Document the scaling threshold that would justify Kafka later.

Deployment

Docker/Compose is appropriate.

Do NOT add Kubernetes initially.

Document when Kubernetes would become justified.

⸻

4. INGESTION ARCHITECTURE

Design the ingestion pipeline precisely.

Target conceptual architecture:

Solana
→ RPC / WebSocket
→ optional Geyser / Yellowstone
→ ingestion boundary
→ raw durable records
→ decode / normalize
→ protocol adapters
→ canonical database
→ derived state/risk
→ REST / realtime APIs
→ transaction engine / keepers
→ UI

Define:

* slot ingestion
* transaction ingestion
* account-state ingestion
* program-log/event ingestion
* WebSocket lifecycle
* reconnect semantics
* checkpointing
* backfill
* replay
* deduplication
* gap detection
* ordering assumptions
* concurrent ingestion
* idempotency
* crash recovery
* provider switching
* rate limits
* malformed data handling

Assume delivery is not magically exactly-once.

Explicitly design for at-least-once observations and idempotent effects.

⸻

5. RAW → NORMALIZED → PROTOCOL → DERIVED DATA MODEL

Define clean layers.

For example:

Raw layer

Immutable observations sufficiently complete for:

* debugging
* replay
* forensic inspection

Normalized Solana layer

Canonical representation of:

* slots
* blocks
* transactions
* instructions
* accounts
* logs
* token movements

Protocol adapter layer

Decoded domain entities such as:

* Aegis markets
* positions
* vaults
* oracle observations
* borrow/repay/deposit/withdraw activity
* liquidations

Derived layer

Examples:

* health factor
* liquidation candidates
* protocol exposure
* utilization
* stale-oracle alerts
* operational status

Define what is authoritative and what is recomputable.

Derived data should generally be rebuildable from canonical source data.

⸻

6. AEGIS INTEGRATION CONTRACT

Derive this section from the actual Aegis Phase 0 repository.

Document precisely what Sentinel must eventually understand:

* relevant Aegis program/account identifiers
* PDA relationships
* account layouts
* market state
* position state
* vault relationships
* oracle relationships
* instruction types
* event/log strategy
* relevant economic parameters
* health calculation inputs
* liquidation eligibility
* liquidation construction
* bad-debt handling
* Token-2022 considerations
* upgrade/version handling

Do NOT copy Aegis logic casually into Sentinel and allow it to drift.

Design a version-aware Aegis adapter.

Explain:

* how Sentinel detects protocol version/schema changes
* how decoder versions are registered
* how historical records remain interpretable
* how a mismatch between off-chain derived health and on-chain execution is handled

Sentinel may precompute risk.

Aegis remains the final authority.

⸻

7. COMMITMENT, FORKS, AND FINALITY MODEL

This must be explicit.

Define how Sentinel treats:

* processed
* confirmed
* finalized

Design for:

* temporary forks
* observations that disappear
* rollback/reconciliation
* transaction re-observation
* finalization promotion
* skipped slots
* duplicate notifications
* account updates arriving before/after related transaction records

State what the UI/API may expose at each commitment level.

Do not silently present provisional state as finalized truth.

⸻

8. REPLAY AND BACKFILL

Replayability is a flagship correctness property.

Design:

* checkpoint structure
* replay boundaries
* deterministic processors
* idempotent materialization
* rebuilding derived tables
* partial backfills
* date/slot range reprocessing
* decoder upgrades
* schema migrations
* historical re-decoding
* reconciliation against chain truth

Create explicit acceptance criteria demonstrating:

“Delete derived state → replay canonical observations → reproduce the same result.”

⸻

9. TRANSACTION EXECUTION ENGINE

Design a durable transaction lifecycle.

Use or improve this conceptual state machine:

CREATED
→ PLANNED
→ SIMULATING
→ SIMULATED
→ SIGNING
→ SIGNED
→ SUBMITTING
→ SUBMITTED
→ OBSERVED
→ CONFIRMED
→ FINALIZED

with explicit failure/retry states.

Cover:

* recent blockhash
* blockhash expiry
* durable nonce concepts and whether they belong
* simulation
* compute estimation
* compute-budget instructions
* priority fees
* signer boundary
* partial signing if relevant
* submission
* duplicate submission
* ambiguous submission outcome
* retry policy
* RPC failover
* confirmation
* finalization
* dropped transactions
* versioned transactions
* address lookup tables
* idempotent intent semantics

Distinguish:

transaction identity

from

business intent identity.

A retried user intent must not accidentally execute an economically sensitive operation twice.

⸻

10. SIGNER AND KEY SECURITY

Define signer architecture.

The backend must not casually hold unrestricted user private keys.

Cover:

* wallet-signed user transactions
* server-operated keeper authority
* least privilege
* hot-key blast radius
* environment separation
* secret management
* rotation
* audit logs
* transaction policy checks
* allowlisted programs/instructions
* simulation before signing where appropriate

If a keeper key requires funds, define explicit budget/risk controls.

⸻

11. AEGIS LIQUIDATION KEEPER

Design the flagship closed loop:

oracle/state update
→ ingest
→ decode
→ materialize
→ calculate health
→ identify unhealthy position
→ create liquidation candidate
→ validate profitability/constraints
→ simulate
→ estimate compute/priority fee
→ submit liquidation
→ track observation/confirmation/finalization
→ ingest resulting Aegis state
→ reconcile DB
→ publish realtime result

Handle:

* competing liquidators
* position recovered before execution
* stale oracle
* simulation succeeds but execution fails
* blockhash expires
* RPC returns ambiguous status
* liquidation becomes unprofitable
* duplicate candidate creation
* worker crashes halfway
* Sentinel database lags chain
* Aegis rejects the transaction despite Sentinel prediction

A rejected Aegis execution is NOT automatically a bug in Aegis.

Treat it as a reconciliation signal.

⸻

12. RPC ABSTRACTION

Design an RPC provider abstraction with:

* provider pools
* health tracking
* timeout policy
* bounded retries
* exponential backoff
* jitter
* rate-limit handling
* circuit breaker semantics
* capability discovery
* consistency considerations
* WebSocket reconnect
* provider failover
* request correlation and observability

Baseline local development must work without paid RPC.

⸻

13. OPTIONAL GEYSER / YELLOWSTONE PATH

Basic Sentinel must work using standard RPC/WebSocket.

Then design an optional high-performance Geyser/Yellowstone adapter.

Explain:

* what bottleneck it solves
* what interface remains identical
* operational cost
* local/testing strategy
* failure/fallback behavior

Do not architect Sentinel so that a paid hosted Geyser service is mandatory.

⸻

14. POSTGRES DATA MODEL

Produce a thoughtful schema plan.

Cover likely entities such as:

* chain slots
* blocks
* transactions
* instruction observations
* account observations
* token movements
* protocol instances
* decoder versions
* Aegis markets
* Aegis positions
* Aegis state snapshots or materialized state
* oracle observations
* liquidation candidates
* execution intents
* transaction attempts
* confirmations/finalization
* worker checkpoints
* reconciliation results
* alerts

For each important table specify:

* primary key
* natural/idempotency key
* uniqueness requirements
* indexing strategy
* retention/replay considerations
* immutable vs mutable semantics

Do not build an enormous warehouse without product need.

⸻

15. DISTRIBUTED-SYSTEM CORRECTNESS

Specify invariants around:

* duplicate processing
* idempotent consumers
* race conditions
* worker ownership
* leases if used
* stale workers
* crash recovery
* retries
* poison jobs
* DLQ semantics if justified
* transaction boundaries
* database locking
* optimistic/pessimistic concurrency
* replay
* eventual consistency
* API visibility

Name concrete failure scenarios rather than saying “use retries.”

⸻

16. OBSERVABILITY

Design actionable observability.

Include:

* structured logs
* metrics
* traces where justified
* ingestion lag
* finalized-slot lag
* RPC error rate
* reconnect count
* decode failures
* replay throughput
* queue/work backlog
* keeper candidate rate
* simulation failures
* submit failures
* confirmation latency
* reconciliation mismatch
* DB latency
* WebSocket connection health

Every alert should correspond to an operator action.

Prefer OpenTelemetry-compatible instrumentation.

⸻

17. SECURITY MODEL

Create a Sentinel-specific threat model.

Include at least:

* malicious RPC data
* inconsistent providers
* malformed transactions/accounts
* decoder bugs
* forged protocol identity
* stale chain state
* stale oracle interpretation
* duplicate execution
* arbitrary transaction construction
* arbitrary program invocation
* signer misuse
* secret leakage
* API authorization failure
* replay attacks
* poisoned queue jobs
* DoS via expensive decode
* oversized input
* cache poisoning
* SQL injection
* WebSocket abuse
* rate-limit exhaustion
* SSRF if applicable
* dependency compromise
* compromised keeper key
* chain reorg/fork handling bugs

Define mitigations and tests.

⸻

18. TESTING STRATEGY

Design multiple levels.

Unit tests

parsers, codecs, state machines, retry logic, risk derivation.

Property tests

idempotency, replay determinism, state-machine legal transitions.

Integration tests

local Solana environment + Postgres + Sentinel components.

Protocol integration

local Aegis fixtures when Aegis implementation exists.

Failure injection

* kill workers
* drop RPC connections
* duplicate notifications
* reorder observations
* stale provider
* DB restart
* Redis loss if Redis is used
* corrupted/malformed payload
* partial backfill
* blockhash expiry
* ambiguous tx submission

Replay tests

historical/captured fixture data.

Load tests

measure actual capacity and bottlenecks.

Do not count mocked happy-path tests as distributed-system evidence.

⸻

19. PERFORMANCE MODEL

Define measurable targets, not fake claims.

Possible metrics:

* transactions decoded/sec
* account updates/sec
* ingestion lag
* replay throughput
* API p95 latency
* WS fanout latency
* keeper detection latency
* simulation-to-submission latency
* confirmation latency
* DB write amplification
* memory usage

Do not invent benchmark results in Phase 0.

Create benchmark methodology and future acceptance gates.

⸻

20. API + REALTIME PRODUCT SURFACE

Design a useful external surface.

Potential REST/WS APIs:

* protocol health
* market state
* positions
* position history
* liquidation candidates
* oracle state
* transaction intent status
* chain/indexer health
* realtime account/protocol updates

Specify:

* pagination
* versioning
* authorization
* rate limits
* idempotency keys
* error model
* provisional vs finalized fields
* WebSocket subscription semantics
* reconnect/resume behavior

⸻

21. FRONTEND

Keep frontend credible but secondary.

Next.js/React may show:

* Sentinel health
* Aegis markets
* positions
* risk status
* liquidation events
* oracle health
* transaction lifecycle
* realtime updates

The UI must demonstrate backend capabilities, not become a styling project.

⸻

22. EXTERNAL INTEGRATIONS

Consider SPL/Token-2022 first.

Jupiter may be integrated only if it creates a real execution/liquidation use case.

If included, define:

* why
* trust boundary
* quote validity
* slippage policy
* transaction inspection
* allowed programs
* simulation
* failure modes

Do not add protocols just for keyword coverage.

⸻

23. COVERAGE MATRIX

Create a topic-to-evidence matrix categorizing every major target skill as one of:

* PRODUCTION
* LAB
* TEST
* ADR
* NOT COVERED

Include at least:

Solana fundamentals
commitments/finality/forks
Rust async/Tokio
RPC
WebSockets
Geyser/Yellowstone
transaction parsing
account decoding
Anchor interoperability
SPL
Token-2022
versioned transactions
ALTs
compute budgets
priority fees
simulation
blockhash expiry
signing
indexing
backfill
replay
deduplication
Postgres
Redis
queues
workers
idempotency
distributed correctness
REST
WebSockets
observability
load testing
failure injection
security
Aegis integration
liquidation keeper
Jupiter/external CPI awareness
Docker
CI
Kubernetes tradeoff
MEV/Jito concepts

For each skill name the exact future repository artifact proving the claim.

No artifact = no claim.

⸻

24. DOCUMENTATION STRUCTURE

Create useful Phase 0 documents. At minimum consider:

* README.md
* AGENTS.md
* CLAUDE.md
* docs/product.md
* docs/architecture.md
* docs/aegis-integration.md
* docs/ingestion-model.md
* docs/finality-and-forks.md
* docs/data-model.md
* docs/replay-and-backfill.md
* docs/transaction-engine.md
* docs/keeper-design.md
* docs/rpc-strategy.md
* docs/security.md or docs/threat-model.md
* docs/testing-strategy.md
* docs/performance-strategy.md
* docs/observability.md
* docs/api-design.md
* docs/coverage-matrix.md
* docs/phase-roadmap.md
* docs/project-status.md
* docs/implementation-handoff.md
* docs/adr/
* docs/phases/

Prefer multiple focused documents over one giant document.

⸻

25. AGENTS.md

Create AGENTS.md as the model/tool-independent engineering constitution.

It must establish permanently:

* correctness before velocity
* security before convenience
* no fake evidence
* one phase at a time
* no scope creep
* no silent weakening of checks
* tests must actually be run
* failures must be investigated, not bypassed
* all chain observations have explicit commitment semantics
* idempotency is mandatory for externally visible effects
* derived state must be replayable
* on-chain protocols are authoritative over off-chain predictions
* secrets never enter Git
* zero-cost local path remains viable
* architecture changes require ADRs
* docs/status must match reality
* no unnecessary technology for CV keywords

⸻

26. CLAUDE.md

Create a Claude-specific operating guide.

It should:

* point to AGENTS.md
* instruct Claude to read docs/project-status.md
* read the current phase file
* read relevant ADRs
* implement only the current phase
* distinguish planning from implemented/tested/demoed evidence
* verify dependency versions instead of relying on memory
* stop on contradictions in frozen documents
* report deviations
* update status accurately
* stop before the next phase

Do not duplicate the entire AGENTS.md.

⸻

27. PROJECT STATUS MODEL

docs/project-status.md must separately track:

* IMPLEMENTED
* TESTED
* DEMOED
* DOCUMENTED
* COMMITTED

Never allow:

“code exists” → “complete”

without evidence.

Phase 0 should end with planning/documents complete and runtime implementation not started.

⸻

28. ADRs

Create ADRs for consequential decisions, potentially including:

* Rust/TypeScript responsibility split
* Postgres canonical storage
* Redis role
* no Kafka initially
* RPC/WS baseline
* optional Yellowstone/Geyser
* at-least-once observation/idempotent processing
* raw durable observation boundary
* commitment/finality model
* transaction intent vs transaction attempt
* signer boundary
* Aegis adapter versioning
* zero-cost local architecture
* no Kubernetes initially

Do not create ADRs for trivial choices.

⸻

29. PHASE ROADMAP

Produce a dependency-aware implementation roadmap.

You may improve this provisional sequence:

Phase 0 — planning and architecture

Phase 1 — repository/toolchain/local infrastructure foundation

Phase 2 — canonical data model + Postgres migrations

Phase 3 — RPC abstraction + resilient RPC client

Phase 4 — basic slot/transaction/account ingestion

Phase 5 — raw durable observation boundary + checkpoints

Phase 6 — decode/normalize pipeline + replay/backfill

Phase 7 — Aegis protocol adapter

Phase 8 — materialized protocol/risk state

Phase 9 — REST + realtime WebSocket API

Phase 10 — transaction execution state machine

Phase 11 — Aegis liquidation keeper

Phase 12 — observability + failure injection + recovery

Phase 13 — optional Yellowstone/Geyser high-performance adapter

Phase 14 — load/performance campaign + selected optimizations

Phase 15 — UI/integrated demo/security review/GitHub polish

If dependencies suggest a better order, change it and document why.

Every later phase specification must include:

* scope
* non-scope
* learning/evidence objective
* implementation requirements
* tests
* adversarial/failure cases
* acceptance criteria
* live demo
* docs updates
* project-status updates
* Git requirements
* stop condition

⸻

30. PORTFOLIO EVIDENCE

The eventual repository should visibly prove:

RPC resilience
→ provider abstraction + failure tests

WebSockets
→ reconnect/resume/realtime API

indexing
→ replayable ingestion pipeline

distributed systems
→ durable jobs/idempotency/crash recovery

Postgres
→ canonical schema and migrations

Rust
→ real high-throughput/performance-sensitive services

TypeScript
→ real orchestration/API/product layer

Solana transactions
→ durable execution state machine

security
→ threat model + adversarial tests

performance
→ measured load/latency benchmarks

protocol integration
→ version-aware Aegis adapter

automation
→ end-to-end liquidation keeper

observability
→ metrics/traces/operator runbooks

architecture
→ ADRs and failure-mode reasoning

Do not create fake complexity to manufacture these claims.

⸻

31. FINAL SELF-ATTACK

Before declaring Phase 0 complete, aggressively critique your own architecture.

Answer explicitly:

* Where can duplicate observations cause duplicate effects?
* What happens if Sentinel crashes after submitting a transaction but before recording the signature?
* What happens if two providers disagree?
* What happens if WebSocket events are lost?
* What happens if backfill and realtime ingestion overlap?
* How does a fork affect derived risk state?
* Can replay reproduce state deterministically?
* Can a malformed transaction crash a parser worker?
* Can one bad job poison the pipeline?
* Could Sentinel execute an economically sensitive action twice?
* Could the keeper use stale state?
* Could a compromised API caller make the backend sign arbitrary transactions?
* Could the keeper key drain funds outside its intended role?
* What happens when Aegis upgrades?
* How are old Aegis records decoded after schema changes?
* What happens if Sentinel computes a position as liquidatable and Aegis rejects it?
* Is any component present purely for resume coverage?
* Is Redis actually necessary?
* Is a queue actually necessary?
* Is Geyser actually necessary?
* Is Rust being used where it matters rather than ceremonially?
* Is TypeScript being used where ecosystem/product velocity matters?
* Is every claimed skill backed by future observable evidence?
* Can a future implementation model execute phases without redesigning the architecture?

Fix material problems found by this attack before completion.

⸻

32. PHASE 0 COMPLETION CONDITIONS

Phase 0 is complete only when:

* product thesis is coherent
* Aegis integration is grounded in actual Aegis Phase 0 artifacts
* architecture is frozen enough for bounded implementation
* data/replay/finality semantics are explicit
* transaction execution semantics are explicit
* distributed-system failure modes are explicit
* threat model exists
* testing strategy exists
* performance methodology exists
* observability plan exists
* coverage matrix exists
* ADRs exist
* AGENTS.md exists
* CLAUDE.md exists
* project status exists
* implementation handoff exists
* phase specifications exist
* unresolved ecosystem facts are recorded as research gates
* no production runtime Phase 1 work has begun

At completion respond with:

SENTINEL PHASE 0 PLANNING COMPLETE

Then report:

1. documents created
2. ADRs created
3. major architecture decisions
4. Aegis artifacts/interfaces consumed
5. research gates still open
6. largest residual architectural risks
7. confirmation that no runtime Phase 1 implementation was started

End with:

Next action: review and archive Phase 0. Phase 1 has NOT been started.

Then STOP.

Do not begin Phase 1.