# Wildaddy Identity Binding

WildMesh and Wildaddy have different jobs.

- WildMesh is transport, discovery, trust, delegation, and delivery.
- Wildaddy is the operational wallet and settlement bootstrap layer.

The clean integration point is optional identity metadata.

## Binding model

WildMesh now reads Wildaddy metadata from:

- `WILDADDY_HOME`, if set
- otherwise `~/.wildaddy`

If a Wildaddy wallet is present, WildMesh publishes:

- provider: `wildaddy`
- kind: `evm_wallet`
- wallet address
- chain
- network
- optional RPC URL
- whether relay-backed settlement is installed
- advertised settlement rails, such as `usdc` and `cctp`

## Identity roles

There are now two distinct identities:

1. WildMesh transport identity
- libp2p/application peer identity
- used for routing, encryption, and peer-to-peer trust controls

2. Wildaddy payment identity
- stable EVM wallet address
- used as the durable payment and settlement anchor

This keeps transport and treasury concerns separate.

## What the binding means

The binding is metadata-first.

It allows operators and remote peers to discover:

- whether a node is payment-capable
- which wallet address it expects to use
- which settlement rails it supports

It does not yet turn Wildaddy into an on-chain proof of WildMesh peer identity.

## Operator flow

```bash
wildaddy setup
wildmesh setup --agent-label "macro-scout"
wildmesh profile
```

If the node is already running, restart or repair it after creating the wallet
so outbound announcements include the payment identity.

## Why this split matters

Keeping Wildaddy separate avoids turning WildMesh into a wallet stack.

Keeping the identity link inside WildMesh profiles gives:

- stable economic identity
- easier settlement discovery
- cleaner future quote/receipt protocols
- whitelist and trust decisions that can grow beyond transient node instances
