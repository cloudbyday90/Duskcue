# Contributing to Duskcue

Thank you for your interest in contributing to Duskcue. This document outlines the process and expectations for contributions.

## Code of Conduct

Be respectful. Be constructive. We are all here to build something good together.

## Getting Started

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes
4. Submit a pull request

## Pull Request Process

### Scope

Pull requests must be narrowly scoped. Each PR should address one concern:

| Good | Avoid |
|---|---|
| Fix a single bug | Refactor the entire auth module plus fix a bug |
| Add one API endpoint | Add five endpoints across three domains |
| Update one migration | Restructure all migrations |
| Fix a typo in one file | Mass-rename variables across the codebase |

Large changes should be broken into a series of smaller, independently reviewable PRs. If you are unsure whether your change is too broad, ask before investing time in implementation.

### Requirements

All PRs must:

- Compile without warnings (`cargo build`)
- Pass all tests (`cargo test`)
- Follow the code style defined in [CONTEXT.md](CONTEXT.md)
- Follow the domain five-file pattern from [PROJECT_STRUCTURE.md](docs/design/PROJECT_STRUCTURE.md)
- Use ES Modules in all JavaScript/TypeScript (no `require`/`module.exports`)
- Not include comments unless explicitly requested
- Not update [CHANGELOG.md](CHANGELOG.md) — maintainers handle changelog entries

### Review Process

1. A maintainer will be assigned to review your PR
2. Address review feedback by pushing new commits (do not rebase during review)
3. Once approved, a maintainer will merge your PR

### What to Expect

- PRs are reviewed in the order they are received
- Complex PRs may take longer to review
- Maintainers may request changes or suggest alternatives
- Not all PRs will be merged — if a change does not align with the project direction, a maintainer will explain why

## Developer Certificate of Origin (DCO)

By contributing to Duskcue, you agree to the Developer Certificate of Origin, version 1.1:

```
Developer Certificate of Origin
Version 1.1

Copyright (C) 2004, 2006 The Linux Foundation and its contributors.

Everyone is permitted to copy and distribute verbatim copies of this
license document, but changing it is not allowed.


Developer's Certificate of Origin 1.1

By making a contribution to this project, I certify that:

(a) The contribution was created in whole or in part by me and I
    have the right to submit it under the open source license
    indicated in the file; or

(b) The contribution is based upon previous work that, to the best
    of my knowledge, is covered under an appropriate open source
    license and I have the right under that license to submit that
    work with modifications, whether created in whole or in part
    by me, under the same open source license (unless I am
    permitted to submit under a different license), as indicated
    in the file; or

(c) The contribution was provided directly to me by some other
    person who certified (a), (b) or (c) and I have not modified
    it.

(d) I understand and agree that this project and the contribution
    are public and that a record of the contribution (including all
    personal information I submit with it, including my sign-off) is
    maintained indefinitely and may be redistributed consistent with
    this project or the open source license(s) involved.
```

### Sign-Off

Every commit must include a `Signed-off-by` line:

```
Signed-off-by: Jane Doe <jane@example.com>
```

This can be added automatically with `git commit -s`:

```bash
git commit -s -m "fix: correct playback resume position calculation"
```

PRs without proper sign-off on every commit will not be merged.

## Commit Messages

Follow the [Conventional Commits](https://www.conventionalcommits.org/) format:

```
type(scope): description

[optional body]

Signed-off-by: Your Name <your@email.com>
```

### Types

| Type | Use for |
|---|---|
| `feat` | New features |
| `fix` | Bug fixes |
| `docs` | Documentation changes |
| `refactor` | Code restructuring with no behavior change |
| `test` | Adding or updating tests |
| `chore` | Build, CI, tooling changes |
| `perf` | Performance improvements |

### Examples

```
feat(streaming): add HLS fMP4 segment generation
fix(auth): correct passkey challenge validation
docs(api): update endpoint documentation for libraries
refactor(db): extract connection pool initialization
```

## Code Standards

### Rust

- Edition 2024, resolver 3
- Domain modules follow the five-file pattern: `mod.rs`, `handlers.rs`, `service.rs`, `error.rs`, `types.rs`
- Three-type DTO pattern: `XxxRow` (no Serialize), `XxxRequest` (Deserialize + Validate), `XxxResponse` (Serialize only)
- Handler to Service to DB layering: no SQL in handlers, no business logic in handlers
- No comments in code unless explicitly requested
- `cargo build` and `cargo test` must pass

### JavaScript/TypeScript (Web Client)

- ES Modules only: `import`/`export`, never `require`/`module.exports`
- `package.json` must include `"type": "module"`
- No comments in code unless explicitly requested

### Naming

| Context | Convention | Example |
|---|---|---|
| Prose, documentation | Duskcue | "Duskcue is a media server" |
| Binary, CLI, Docker, DB, Rust modules | duskcue | `duskcue --version` |
| Environment variables | DUSKCUE_ | `DUSKCUE_DATABASE_URL` |
| macOS/Windows paths | Duskcue | `~/Library/Application Support/Duskcue` |

## Architecture

Before contributing, familiarize yourself with:

- [PROJECT.md](PROJECT.md) — Architecture overview, tech stack, key decisions
- [BUILD_ORDER.md](BUILD_ORDER.md) — Implementation sequence and phase dependencies
- [CONTEXT.md](CONTEXT.md) — Standing directives for development sessions
- [docs/design/PROJECT_STRUCTURE.md](docs/design/PROJECT_STRUCTURE.md) — Monorepo layout and module conventions

## Reporting Issues

- Search existing issues before filing a new one
- Include steps to reproduce, expected behavior, and actual behavior
- Include server version, OS, and relevant configuration (redact secrets)

## Security Vulnerabilities

Do not report security vulnerabilities through public GitHub issues. See [docs/security/SECURITY.md](docs/security/SECURITY.md) for the responsible disclosure process.

## License

By contributing, you agree that your contributions will be licensed under the [GNU Affero General Public License v3](LICENSE).
