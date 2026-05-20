# Stealth Client Evaluation Spike

This crate is a temporary compile and API-fit spike for replacing Kumo's
current `rquest`-backed HTTP-level stealth fetcher.

It intentionally does not change Kumo production code. The goal is to let CI
compile small adapters for both candidates and make the migration decision from
evidence.

## Candidates

- `hpx/`: `hpx 2.4.11` + `hpx-emulation 2.4.11`
- `wreq/`: `wreq 5.3.0` + `wreq-util 2.2.6`

## Evaluation Criteria

- Builds on Kumo's supported Rust toolchain.
- Supports proxy configuration.
- Supports cookies.
- Supports request method, headers, and body.
- Supports response status, headers, text body, and bytes body conversion.
- Can map Kumo's current `StealthProfile` variants to browser emulation.
- Can run a real request to `https://tls.peet.ws/api/all` on CI or a machine
  with BoringSSL build dependencies.

## Current License Notes

- `hpx` and `hpx-emulation` are published as Apache-2.0.
- `wreq` is published as Apache-2.0.
- `wreq-util 2.2.6` is published as GPL-3.0. This makes it unsuitable for a
  direct production dependency unless the utility crate's published license
  changes or Kumo avoids depending on it.

## Manual Fingerprint Probe

The ignored tests perform a real request from each standalone candidate crate:

```bash
cargo test --manifest-path spikes/stealth-clients/hpx/Cargo.toml -- --ignored
cargo test --manifest-path spikes/stealth-clients/wreq/Cargo.toml -- --ignored
```

They require network access and the native build tools needed by the candidate
TLS stacks.

## Local Windows Result

On the current Windows workstation, both candidates reached their BoringSSL
build scripts and failed because CMake could not find NASM:

- `hpx`: `No CMAKE_ASM_NASM_COMPILER could be found`
- `wreq`: `No CMAKE_ASM_NASM_COMPILER could be found`

That is a local toolchain limitation, not an adapter API result. The dedicated
GitHub Actions workflow installs `cmake` and `nasm` on Ubuntu so CI can evaluate
the candidates further.
