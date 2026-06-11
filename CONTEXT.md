# Context

## Purpose

This document captures the standing directives for working on Duskcue. It replaces per-prompt repetition of methodology, tooling constraints, code standards, and documentation rules. Reference this document at the start of every session.

For the full build sequence and phase-specific task lists, see [BUILD_ORDER.md](BUILD_ORDER.md). For architecture, tech stack, and key decisions, see [PROJECT.md](PROJECT.md).

## Working Methodology

Every feature or domain follows the same three-phase process:

**Phase 1 — Research.** Research official online sources for current best practices as of June 2026. Do NOT assume URLs. Use web search tools to identify correct URLs and gather relevant information. Present findings with pros, cons, and a final recommendation.

**Phase 2 — Document.** Update relevant MDs with the design decisions and outcomes, or create new MDs if the domain does not yet have one. Each MD is a separate authoritative document that details the design and outcome for its domain. MDs under `docs/` are organized by category:
- `docs/design/` — domain design documents
- `docs/security/` — security design documents
- `docs/operations/` — operational design documents
- `docs/ci/` — CI/CD pipeline documents
- `docs/governance/` — governance documents
- `docs/branding/` — branding and UI documents

**Documentation maintenance rules** — when completing a build phase:
1. Update [BUILD_ORDER.md](BUILD_ORDER.md) — mark the phase complete with commit hash, what was built, key decisions, and deferred items; annotate the next phase with prerequisites and context from the just-completed phase
2. Update [PROJECT.md](PROJECT.md) — update the Current Implementation Status table; add any new resolved decisions to Open Questions; update any section summaries that the implementation affects
3. Update domain-specific MDs — add implementation notes or decisions to the relevant authoritative document (e.g., MEMORY.md for allocator/shutdown decisions, CONFIGURATION.md for config struct changes, PROJECT_STRUCTURE.md for workspace dependency changes)
4. Cross-reference — ensure decisions documented in one MD are referenced from related MDs (e.g., TLS backend decision in MEMORY.md should be noted in PROJECT.md Open Questions)

**Phase 3 — Implement.** Proceed with high-quality code changes reflecting the explored recommendations. Wire up all module declarations, follow the domain five-file pattern, and ensure the project compiles.

## Research Tools

| Tool | Usage | Constraints |
|---|---|---|
| **Serper API** | Primary web search. API key authenticated, no rate limit concerns for development use. | — |
| **context7 MCP** | Library/framework documentation lookups | Monthly quota exceeded — do not use |
| **webfetch** | Fallback for fetching specific known URLs | Cannot bypass anti-bot (e.g., FFmpeg trac wiki) |

## Code Standards

- **ES Modules** — All JavaScript/TypeScript uses `import`/`export`, never `require`/`module.exports`
- **Shared services over singletons** — Prioritize composable shared services over large monolithic files
- **No comments in code** — unless explicitly requested
- **No CHANGELOG updates** — per standing instruction
- **Product naming** — `Duskcue` (prose), `duskcue` (binary/CLI/Docker/DB/Rust modules/volumes), `DUSKCUE_` (env vars)
- **Server port** — `48027`
- **Rust edition** — 2024, resolver 3
- **Domain five-file pattern** — `mod.rs`, `handlers.rs`, `service.rs`, `error.rs`, `types.rs`
- **Three-type DTO** — `XxxRow` (no Serialize), `XxxRequest` (Deserialize + Validate), `XxxResponse` (Serialize only)
- **Handler → Service → DB** — handlers are thin HTTP translation; business logic in service; SQL in service or db/models

## Security Posture

The underlying system must be secure by design. Security decisions are opt-in and local-first — secure tokens and sessions are opt-in rather than opt-out, so local-only deployments only need internal network security. All security design is documented in [docs/security/SECURITY.md](docs/security/SECURITY.md) and [docs/security/API_SECURITY.md](docs/security/API_SECURITY.md).

## Verification

After completing code changes, run build and lint commands to verify correctness. If the correct commands are unknown, ask before proceeding.

## Key Reference Documents

| Document | Purpose |
|---|---|
| [PROJECT.md](PROJECT.md) | Architecture overview, tech stack, key decisions, domain table |
| [BUILD_ORDER.md](BUILD_ORDER.md) | 16-phase dependency-ordered build sequence with verification criteria |
| [docs/design/PROJECT_STRUCTURE.md](docs/design/PROJECT_STRUCTURE.md) | Monorepo layout, Cargo workspace, domain module conventions |
| [docs/design/DATABASE.md](docs/design/DATABASE.md) | Full DDL, UUIDv7 key strategy, naming conventions |
| [docs/design/ERROR_HANDLING.md](docs/design/ERROR_HANDLING.md) | 121 error codes, `thiserror` + `anyhow`, RFC 9457 |
| [docs/design/API_CONVENTIONS.md](docs/design/API_CONVENTIONS.md) | REST conventions, pagination, rate limiting |
| [docs/operations/CONFIGURATION.md](docs/operations/CONFIGURATION.md) | Two-tier config, 14-step startup sequence |
