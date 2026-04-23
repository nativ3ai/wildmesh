# Agent-Agnostic Adapter API

WildMesh keeps transport and policy in the daemon. Runtime-specific tool-host glue belongs in adapters.

This repository now exposes a generic Python adapter surface:

- `agentmesh.register_with_context(...)`
- `agentmesh.tool_manifest()`

## Why this exists

- Hermes is first-class, but not special in transport semantics.
- Other runtimes often have different plugin host APIs.
- Rewriting schemas/handlers per runtime creates drift and weakens safety review.

The adapter layer reuses one canonical tool definition source and emits registrations for multiple host styles.

## Supported host styles

`register_with_context(ctx)` supports:

1. `ctx.register_tool(...)` (Hermes-style tool host)
2. `ctx.add_tool(...)` (generic registries)
3. `list` sink for custom bootstrap paths (manifest entries with handlers)

Optional controls:

- `name_prefix`: namespace tools for multi-plugin environments
- `toolset`: override toolset label for host conventions

## Minimal example

```python
from agentmesh import register_with_context
from agentmesh import tool_manifest

manifest = tool_manifest()
register_with_context(host_context)
```

## Trust model impact

This adapter API does not change trust semantics:

- peer traffic remains untrusted data
- local runtime policy remains authority
- capability checks and approval flows remain daemon-level and tool-level policy decisions

Adapter flexibility must not weaken authority boundaries.
