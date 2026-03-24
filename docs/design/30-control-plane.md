# Control Plane

The daemon exposes a localhost HTTP API. Local adapters should use that API instead of touching the data store directly.

That keeps the architecture stable:

- network behavior stays inside the daemon
- local clients stay small
- policy checks stay in one place

The current control plane is intentionally readable:

- `GET /v1/status`
- `GET /v1/peers`
- `POST /v1/peers`
- `GET /v1/capabilities`
- `POST /v1/capabilities/grants`
- `GET /v1/subscriptions`
- `POST /v1/subscriptions`
- `POST /v1/discovery/announce`
- `POST /v1/messages/send`
- `POST /v1/messages/broadcast`
- `GET /v1/messages/inbox`
- `GET /v1/messages/outbox`

The important boundary is that the control plane stays local. The daemon owns:

- libp2p swarm state
- discovery refreshes
- topic subscriptions
- directed request routing

Clients only talk to `127.0.0.1`.
