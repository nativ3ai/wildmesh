# Contributing

WildMesh is infrastructure. Contributions should improve stability, clarity, and
interoperability before they increase feature surface.

## Principles

- keep the core category narrow
- preserve the public nouns: node, peer, channel, grant, request, artifact, context capsule
- do not merge convenience features that muddy the trust model
- prefer explicit behavior over hidden magic

## Local development

Rust build:

```bash
cargo build
```

Rust tests:

```bash
cargo test
```

Python plugin tests:

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -e '.[test]'
pytest tests/test_hermes_plugin.py
```

Run the dashboard locally:

```bash
cargo build
./target/debug/wildmesh dashboard
```

## Repo layout

- `src/`: Rust core, daemon, CLI, TUI
- `agentmesh/`: Python client + Hermes adapter
- `skill/wildmesh/`: public Hermes skill
- `docs/design/`: literate design docs
- `tests/`: mesh and plugin tests
- `Formula/`: source-side Homebrew formula
- `scripts/`: install and release helpers

## Before opening a change

Make sure the change:

- matches the current WildMesh category
- does not break the trust model
- is reflected in user-facing docs if behavior changed
- keeps CLI/API/plugin language aligned

## Before merging a change

Minimum bar:

```bash
cargo fmt
cargo build
cargo test
```

If the Python plugin or skill changed:

```bash
pytest tests/test_hermes_plugin.py
```

If packaging changed:

- verify Homebrew formula state
- verify install path in the README still matches reality

## Design expectations

- new protocol concepts need a clear noun and scope
- new operator behavior needs docs
- new adapter behavior should avoid leaking harness-specific assumptions into the core
- UI changes should not silently change protocol behavior

## Good contribution candidates

- reliability improvements
- better diagnostics
- safer defaults
- better adapter examples
- clearer docs
- better test coverage for failure modes

## Bad contribution candidates

- features that bypass the trust boundary
- features that add cloud dependency to the core path
- features that add new top-level concepts without strong justification
- UI-only complexity that hides operational state instead of clarifying it
