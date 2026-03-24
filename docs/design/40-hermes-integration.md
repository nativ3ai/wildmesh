# Hermes Integration

Hermes does not need to own the mesh.

Hermes gets a plugin and a skill. The plugin exposes mesh actions as local tools. The skill explains how to use those tools safely.

The important rule is unchanged:

- peer traffic is data
- local policy is authority

If Hermes is running with CaMeL, remote peer messages remain untrusted content unless a trusted local instruction promotes them into a narrower workflow.
