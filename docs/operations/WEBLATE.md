# Weblate Translation Operations

## Purpose

This runbook defines Duskcue's Weblate project setup, translation import policy, and locale activation workflow. It implements the Pre-v1.0 Hardening Task 7 requirement from [BUILD_ORDER.md](../../BUILD_ORDER.md) and the activation policy in [I18N.md](../design/I18N.md).

## Project Setup

Create one Weblate project:

| Field | Value |
|---|---|
| Project name | `Duskcue` |
| Source language | English (`en`) |
| Repository | Duskcue GitHub repository |
| Default branch | `main` |
| Translation license | Same as project source unless changed by maintainers |
| Reviews | Enabled |
| Translation quality filter | Approved translations only |

Use Weblate's GitHub integration or repository webhooks so translation changes flow through pull requests. Translation PRs must pass the normal CI gates before merge.

## Components

Configure two components against the same repository clone.

| Component | Format | Source | File mask | Notes |
|---|---|---|---|---|
| Web client | JSON | `clients/web/messages/en.json` | `clients/web/messages/*.json` | Inlang message-format catalogs compiled by Paraglide. |
| Server notifications | Fluent | `server/locales/en/notifications.ftl` | `server/locales/*/notifications.ftl` | Fluent notification templates compiled into the server binary. |

Keep English as the source language for both components. Do not create locale-specific source strings in application code.

## AI-Initial Import Policy

The seven non-English launch-window locales (`fr`, `de`, `es`, `it`, `ar`, `zh-Hans`, `zh-Hant`) already exist in the repository as AI-initial preview translations. Import them into Weblate as existing translations that still need human review.

Rules:

- Keep the `AI-GENERATED INITIAL TRANSLATION` marker until a native-speaker reviewer signs off.
- Treat machine-translated or AI-translated strings as suggestions, not activation evidence.
- Prefer reviewer edits over wholesale regeneration once community review starts.
- Preserve Fluent variable names and selectors exactly.
- Preserve Paraglide message keys exactly.

## Activation Gate

A locale becomes selectable in Duskcue only after all criteria are met:

| Gate | Requirement |
|---|---|
| Completeness | At least 90% translated across web JSON and server Fluent components |
| Review | Maintainer sign-off after native-speaker review |
| RTL | Additional layout review for RTL locales |
| Build | Paraglide compile, catalog parity, Fluent render tests, server checks, and web build pass |

The runtime activation list lives in `server/src/services/i18n.rs` as `REVIEWED_UI_LOCALES`. Adding a locale to Weblate is not enough to activate it. A maintainer must update that list after the gate is satisfied.

Current reviewed UI locales:

| Locale | Status |
|---|---|
| `en` | Active |
| `fr` | Preview only |
| `de` | Preview only |
| `es` | Preview only |
| `it` | Preview only |
| `ar` | Preview only; RTL layout-reviewed but translation-review gated |
| `zh-Hans` | Preview only |
| `zh-Hant` | Preview only |

## Pull Request Checks

Translation PRs should prove:

- All `clients/web/messages/*.json` files parse.
- All web catalogs have the same key set as `clients/web/messages/en.json`.
- `npx @inlang/paraglide-js compile --project ./project.inlang --outdir ./src/lib/paraglide` succeeds from `clients/web`.
- `npx svelte-check --tsconfig ./jsconfig.json` succeeds from `clients/web`.
- `npm run build` succeeds from `clients/web`.
- `cargo test -p duskcue services::i18n` succeeds from the repository root.
- `cargo check -p duskcue` succeeds from the repository root.

## Operational Notes

- Weblate owns translation edits; developers own English source strings and message IDs.
- Keep API error titles/details English-only; error codes are the searchable stable contract.
- Do not activate partially reviewed locales because mixed-language UI is worse than full English.
- Locale activation is per UI language. Media metadata language, subtitle preferences, and notification recipient language negotiation remain separate concerns.

## Research Sources

- [Weblate translation projects](https://docs.weblate.org/en/latest/admin/projects.html)
- [Weblate continuous localization](https://docs.weblate.org/en/latest/admin/continuous.html)
- [Weblate localization file formats](https://docs.weblate.org/en/latest/formats.html)
- [Weblate translation workflows](https://docs.weblate.org/en/latest/workflows.html)
