# ADR-0014 — Docker Compose; no Kubernetes initially

**Status:** Accepted · **Date:** 2026-09-05 · **Phase:** 0

## Context

Sentinel is six processes and a database. Kubernetes is the reflexive answer for "a backend platform"
and is usually adopted long before anything it solves is a real problem.

## Decision

**Docker Compose for local development, CI, and single-node deployment.** No Kubernetes, no Helm, no
service mesh, no operator, no CRDs in v1.

```
infra/compose/
  docker-compose.yml            # postgres, otel-collector, prometheus, grafana
  docker-compose.local.yml      # + surfpool, [redis]
  docker-compose.observability.yml
```

Supporting decisions:
- Each service is a small container with an explicit health check.
- Process supervision and restart policy come from Compose.
- Migrations run as an explicit step, never implicitly on container start — an implicit migration on
  every replica start is a race waiting to happen.
- Configuration is environment-based with no network defaults (ADR-0013).

## Why this is right at this scale

1. **Sentinel does not need multi-node.** Ingestion is a singleton by design; the database is a single
   primary; the API scales horizontally but not to a count that needs an orchestrator.
2. **Kubernetes solves problems Sentinel does not have** — bin-packing across nodes, rolling deploys
   across replicas, service discovery among many services, multi-tenancy. Sentinel has six processes on
   one machine.
3. **It would cost the zero-cost path.** "Run this locally in two minutes" becomes "install a local
   cluster, apply manifests, wire ingress." Every reviewer pays that tax to see a demo.
4. **The failure modes it introduces are worse than the ones it solves here** — pod eviction mid-write,
   an unhealthy readiness probe cycling the indexer singleton, a network policy silently breaking
   database access. Each is an operational incident that Compose simply cannot produce.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **Kubernetes from the start** | Solves nothing Sentinel has; costs the local path; introduces failure modes Compose cannot produce. |
| **Nomad** | Lighter than Kubernetes, still an orchestrator for six processes on one host. |
| **Systemd units on a host** | Viable for deployment, worse for local development and CI reproducibility. Compose gives both. |
| **A managed container platform (ECS/Cloud Run/Fly)** | Deployment target choice, not architecture, and it would push the local path toward a cloud dependency. |
| **Bare processes with a supervisor** | Loses reproducible dependency and environment management, which is most of Compose's value here. |

## Adoption threshold

Adopt an orchestrator when **either** is demonstrated:

1. **More than one machine is genuinely required** — the indexer singleton is CPU-bound while the host
   is not (hypothesis B-4), or the database and workers can no longer share a host.
2. **An availability SLO demands automated failover** that Compose cannot provide — which implies a
   Postgres HA story first, since the database is the actual single point of availability.

Note the ordering: **the database's availability is the binding constraint, not the workers'.** Adding
Kubernetes to a system with one Postgres primary would improve nothing about the thing that actually
stops the platform.

Neither condition is present. Adopting Kubernetes before one is would be exactly the CV-driven
architecture `AGENTS.md` §14 forbids, and the coverage matrix records it as an explicit **ADR-covered
tradeoff** rather than a gap.

## Consequences

**Positive**
- `make up` brings up the entire platform, including telemetry, in one command.
- CI and local development use the same definitions.
- No orchestration failure modes.
- Deployment is `docker compose up` on one host, which is honest about what Sentinel is.

**Negative**
- No automated failover. **Accepted**: workers resume from durable state, and the database is the real
  availability constraint anyway.
- Manual scaling. Accepted at this scale.
- Rolling deploys are simpler and cruder. Mitigated by every worker being restart-safe by design —
  which is a property Sentinel needs regardless of how it is deployed.
