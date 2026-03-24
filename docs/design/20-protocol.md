# Protocol

WildMesh uses one application-level object: the envelope.

An envelope carries:

- sender identity
- recipient identity
- message kind
- optional capability label
- encrypted body
- signature

The body is encrypted to the recipient. The envelope metadata is visible to the recipient and can be logged before body decryption.

## Kinds

The first release supports a small set of kinds:

- `hello`
- `broadcast`
- `task_offer`
- `task_result`
- `note`
- `receipt`

The rule is narrowness over novelty. New kinds should be added only when they materially change interoperability.

## Discovery

Discovery is intentionally separate from the encrypted envelope channel.

This release uses three discovery layers:

- `mDNS` for local network visibility
- `Kademlia` provider records for wider-network discovery
- `Gossipsub` profile announcements once peers are connected

The discovery payload is a signed profile record, not a trusted command. It exists so peers can learn:

- who exists
- which application identity they claim
- which interests they advertise
- which public topics they follow

That is enough to support open topic broadcast and peer browsing without turning discovery into a trusted control plane.

## Directed transport

Directed work uses a libp2p request-response protocol.

The request body carries the normal encrypted WildMesh envelope. That keeps the transport modern while preserving the narrow application-level trust model:

- libp2p handles reachability and session security
- WildMesh still decides whether the sender is allowed to ask for a capability

## Open broadcast

Public broadcasts use Gossipsub topics.

Broadcast messages are intentionally treated as untrusted peer input. They are good for public notices and mesh chatter. They are not a direct authorization path for local side effects.
