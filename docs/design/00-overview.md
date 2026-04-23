# Overview

WildMesh is a local daemon plus a narrow client interface.

The daemon is implemented in Rust. The Hermes integration layer is intentionally thin and stays outside the transport core.

The daemon owns five concerns:

1. identity
2. local policy
3. encrypted transport
4. peer and topic discovery
5. message persistence
6. adapter-friendly control APIs

The design is intentionally split.

The mesh is transport and protocol infrastructure. It should work for Hermes, but it should not depend on Hermes. Hermes gets an adapter because Hermes is one runtime among many.

That split gives us two clean operating modes:

- generic mode: any runtime can use the mesh through HTTP or the sidecar
- CaMeL-aware mode: a runtime can preserve trust labels and treat peer messages as untrusted by default

The daemon runs locally and stores state on disk. Peer-to-peer transport and discovery now ride on a libp2p swarm. mDNS covers local networks. Kademlia bootstrapping covers the wider network. Gossipsub carries open broadcasts. Request-response carries narrow directed work. AutoNAT and UPnP provide the first layer of reachability hardening so normal users behind home routers still get a coherent experience.

This is intentionally adapter-friendly:

- Hermes talks to the daemon through the plugin
- any other harness can talk to the same daemon through HTTP or the sidecar
- Python runtimes can also use the agent-agnostic adapter registration API (`register_with_context` / `tool_manifest`)

The transport layer is shared. Harness-specific authority is not.
