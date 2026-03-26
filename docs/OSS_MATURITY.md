# WildMesh OSS Maturity Checklist

WildMesh should become the default answer for one category before it tries to be
"for everyone."

Target category:

- local-first P2P networking and coordination for agent runtimes

This checklist exists to keep the project narrow, stable, and useful.

## Core category

- [ ] One-sentence positioning stays stable across the README, docs, packaging, and releases
- [ ] The first user success is obvious: install, spin up a node, discover a peer, send a task
- [ ] Features that do not strengthen peer discovery, delegation, trust, context, or artifacts stay out of the core

## First-run success

- [ ] Install path works in under five minutes on a clean machine
- [ ] Homebrew install is documented and maintained
- [ ] `scripts/install.sh` can bootstrap install + `wildmesh setup`
- [ ] `wildmesh setup` is the single obvious path for operators
- [ ] Hermes path is documented as a first-class option, not an afterthought
- [ ] Other harnesses can reach first success through the sidecar or local API

## Stable primitives

The public model should stay small:

- [ ] node
- [ ] peer
- [ ] channel
- [ ] grant
- [ ] request
- [ ] artifact
- [ ] context capsule

Rules:

- do not add overlapping nouns
- do not overload existing nouns with unrelated meaning
- keep CLI, TUI, API, and plugin language aligned

## Reliability

- [ ] `cargo build`
- [ ] `cargo test`
- [ ] Python plugin tests
- [ ] bottle install verification
- [ ] upgrade verification from previous release
- [ ] explicit failure-mode tests for discovery, delegation, grants, and channels

Infrastructure wins by being boring and correct.

## API discipline

- [ ] stable JSON output for all machine-facing commands
- [ ] stable local HTTP schemas
- [ ] stable sidecar operations
- [ ] semantic versioning is actually enforced
- [ ] deprecations are documented before removal
- [ ] release notes explain breaking changes clearly

## Documentation

- [ ] README covers install, setup, global vs local-only, dashboard, channels, permissions, and delegation
- [ ] skill explains the operator/bootstrap path clearly
- [ ] usage reference covers CLI + sidecar
- [ ] trust model is documented
- [ ] troubleshooting exists for common failures
- [ ] architecture docs remain current
- [ ] comparison to adjacent systems stays factual and narrow

## Distribution

- [ ] Homebrew formula stays current
- [ ] bottle assets are published for current releases
- [ ] Cargo install path stays current
- [ ] install script stays current
- [ ] release artifacts are reproducible

## Interoperability

- [ ] Hermes adapter stays first-class
- [ ] sidecar stays harness-agnostic
- [ ] local HTTP API stays harness-agnostic
- [ ] at least one non-Hermes reference integration exists
- [ ] adapter examples exist in Python and TypeScript

## Security and trust

- [ ] discovery never implies trust
- [ ] delegation never implies authority
- [ ] grants stay narrow and explicit
- [ ] local/private/global modes remain clear
- [ ] defaults stay safe
- [ ] remote content remains untrusted by default

## Operator UX

- [ ] status/profile/dashboard paths are obvious
- [ ] error messages explain what to do next
- [ ] TUI is operational, not decorative
- [ ] approval and whitelist flows are legible
- [ ] channels, peers, and grants are explainable to a new operator quickly

## Ecosystem

- [ ] contribution guide exists
- [ ] releasing guide exists
- [ ] issue templates and roadmap discipline exist
- [ ] examples are easy to run
- [ ] third-party adapters can be added without core surgery

## v1.0 gate

Do not call WildMesh `v1.0` until all of the following are true:

- [ ] first-run install/setup succeeds cleanly for new users
- [ ] node, peer, channel, grant, request, artifact, and context capsule semantics are stable
- [ ] CLI, dashboard, sidecar, API, and Hermes adapter agree on the same model
- [ ] release process is repeatable
- [ ] docs match actual behavior
- [ ] at least one external user or harness is using it without maintainer hand-holding

## Immediate priorities

These are the highest-value short-term items:

1. Keep install + setup friction low.
2. Keep node/channel/delegation semantics stable.
3. Keep docs synchronized with real behavior.
4. Keep releases reproducible.
5. Keep the trust model explicit and defensible.
