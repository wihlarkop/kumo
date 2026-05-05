# Security Policy

## Supported Versions

Before the first public release, security fixes are applied on `main`.

After `kumo` is published to crates.io, security fixes will target the latest
released minor version unless a release note says otherwise.

## Reporting a Vulnerability

Please do not open a public issue for a security vulnerability.

Report security issues by emailing the maintainer or by opening a private GitHub
security advisory if the repository has advisories enabled. Include:

- affected crate and version
- a minimal reproduction, if available
- impact and any known workaround

The maintainer will acknowledge confirmed reports and coordinate a fix and
release before public disclosure.

## Scope

Security-sensitive areas include request handling, redirects, proxy support,
cache path generation, database stores, cloud storage, browser execution, and
LLM-provider integrations.

## Known Dependency Advisory

The optional `mysql` feature currently pulls in `rsa` through `sqlx-mysql`.
RustSec advisory `RUSTSEC-2023-0071` has no fixed upgrade available upstream at
the time of writing. The feature is disabled by default, and the advisory is
tracked in `.cargo/audit.toml` so future audits still fail on other fixable
issues.
