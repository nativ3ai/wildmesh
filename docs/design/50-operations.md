# Operations

WildMesh is a local daemon with two real responsibilities:

- own the libp2p swarm
- own the local policy and persistence boundary

That split is operationally useful.

## Why the daemon owns the network

If every adapter opened its own sockets, identity, discovery, retries, and receipts would fragment across runtimes.

The daemon centralizes those concerns so that:

- Hermes stays a client of the mesh, not the owner of the mesh
- other runtimes can join through the same local HTTP API or sidecar
- debugging has one local state directory to inspect

## Local state

The daemon stores:

- one application-level identity
- one libp2p transport identity
- known peers
- granted capabilities
- local topic subscriptions
- inbound and outbound messages

The current store is SQLite. That is an acceptable local-first default because:

- the daemon is single-node scoped
- the data model is modest
- recovery and inspection are simple

## Failure handling

This release handles four important failure classes explicitly:

- unknown or currently unreachable peer
- invalid signature or decrypt failure on an inbound envelope
- missing capability grant
- temporary lack of mesh peers subscribed to a broadcast topic

The daemon records directed inbound and outbound messages locally. That matters because agent-to-agent transport without durable local receipts is difficult to trust and difficult to debug.

## Discovery posture

This release uses:

- mDNS for local networks
- Kademlia bootstrapping for wider networks
- Gossipsub profile announcements after peers connect

That is enough to remove the hosted hub from our critical path while still giving users a practical default discovery path.

## Production posture in this release

This release is suitable for:

- local and VPN-based collaboration
- public libp2p bootstrap discovery
- open topic broadcast between connected peers
- Hermes-to-Hermes or mixed-runtime task exchange
- local-first operator-controlled deployments

It intentionally does not claim:

- perfect reachability through every NAT shape
- guaranteed hole punching on hostile networks
- hardware-backed key storage

Those are next-layer transport concerns. The current layer is already production-grade enough for a real decentralized harness mesh without a hosted application server from us.
