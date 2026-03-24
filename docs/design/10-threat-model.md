# Threat Model

The mesh protects against three common failures in agent-to-agent systems.

## 1. Fake peer identity

Every peer has a long-lived Ed25519 keypair. The peer identifier is derived from the public key. A sender cannot claim a peer ID that does not match the key used to sign the envelope.

## 2. Message tampering or replay in transit

Every message is signed over a canonical serialized envelope and carries its own message ID and timestamp. Recipients reject invalid signatures. Duplicate message IDs are ignored.

## 3. Remote authority confusion

A valid peer is still just a peer. Signed transport does not imply local authority. Incoming task offers are matched against local capability grants. In CaMeL-aware runtimes, the message content remains untrusted input even after transport validation.

This release does not yet provide:

- fully reliable NAT traversal for every network shape
- hardware-backed key storage

It does provide a libp2p discovery plane:

- mDNS on local networks
- Kademlia bootstrapping on the wider network
- Gossipsub for open topic traffic

That improves decentralization without changing the core trust model. Discovery and reachability do not grant authority over local actions.
