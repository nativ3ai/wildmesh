from __future__ import annotations

from .tools import (
    TOOLSET,
    check_agentmesh_available,
    mesh_add_peer,
    mesh_browse_peers,
    mesh_broadcast,
    mesh_discover_now,
    mesh_delegate_work,
    mesh_accept_request,
    mesh_fetch_artifact,
    mesh_fetch_inbox,
    mesh_deny_request,
    mesh_grant_capability,
    mesh_list_artifacts,
    mesh_list_pending_requests,
    mesh_list_peers,
    mesh_latest_delegate_result,
    mesh_list_subscriptions,
    mesh_offer_artifact,
    mesh_profile,
    mesh_setup,
    mesh_send_context,
    mesh_send_task,
    mesh_status,
    mesh_subscribe_topic,
)


def register(ctx) -> None:
    ctx.register_tool(
        name="wildmesh_setup",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_setup",
            "description": "Initialize, repair, or refresh the current local WildMesh node and start its daemon. Use this for the current machine; do not use it to create extra local peers unless the operator explicitly asks for another node/home.",
            "parameters": {
                "type": "object",
                "properties": {
                    "home": {"type": "string"},
                    "agent_label": {"type": "string"},
                    "agent_description": {"type": "string"},
                    "interests": {"type": "array", "items": {"type": "string"}},
                    "control_port": {"type": "integer"},
                    "p2p_port": {"type": "integer"},
                    "advertise_host": {"type": "string"},
                    "bootstrap_urls": {"type": "array", "items": {"type": "string"}},
                    "cooperate": {"type": "boolean"},
                    "executor_mode": {"type": "string"},
                    "executor_url": {"type": "string"},
                    "executor_model": {"type": "string"},
                    "executor_api_key_env": {"type": "string"},
                    "launch_agent": {"type": "boolean"},
                    "hermes_home": {"type": "string"},
                },
                "additionalProperties": False,
            },
        },
        handler=mesh_setup,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Initialize or repair the current local WildMesh node and bring the daemon online.",
        emoji="🛠️",
    )
    ctx.register_tool(
        name="wildmesh_status",
        toolset=TOOLSET,
        schema={"name": "wildmesh_status", "description": "Inspect the local Wildmesh daemon.", "parameters": {"type": "object", "properties": {}, "additionalProperties": False}},
        handler=mesh_status,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Inspect whether the local WildMesh daemon is up and whether the mesh worker is actually live.",
        emoji="🕸️",
    )
    ctx.register_tool(
        name="wildmesh_profile",
        toolset=TOOLSET,
        schema={"name": "wildmesh_profile", "description": "Inspect the local Wildmesh profile and shared-realm configuration.", "parameters": {"type": "object", "properties": {}, "additionalProperties": False}},
        handler=mesh_profile,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Inspect local WildMesh profile metadata, bootstrap realm settings, and current mesh readiness.",
        emoji="🪪",
    )
    ctx.register_tool(
        name="wildmesh_list_peers",
        toolset=TOOLSET,
        schema={"name": "wildmesh_list_peers", "description": "List known Wildmesh peers.", "parameters": {"type": "object", "properties": {}, "additionalProperties": False}},
        handler=mesh_list_peers,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="List known peers.",
        emoji="📡",
    )
    ctx.register_tool(
        name="wildmesh_browse_peers",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_browse_peers",
            "description": "Refresh discovery and browse known peers, optionally filtered by interest or free text.",
            "parameters": {
                "type": "object",
                "properties": {
                    "interest": {"type": "string"},
                    "text": {"type": "string"},
                    "discovered_only": {"type": "boolean"},
                    "refresh": {"type": "boolean"},
                },
                "additionalProperties": False,
            },
        },
        handler=mesh_browse_peers,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Discover and filter peers by profile metadata.",
        emoji="🧭",
    )
    ctx.register_tool(
        name="wildmesh_add_peer",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_add_peer",
            "description": "Register a known Wildmesh peer by address and keys.",
            "parameters": {
                "type": "object",
                "properties": {
                    "peer_id": {"type": "string"},
                    "host": {"type": "string"},
                    "port": {"type": "integer"},
                    "public_key": {"type": "string"},
                    "encryption_public_key": {"type": "string"},
                    "label": {"type": "string"},
                    "notes": {"type": "string"},
                },
                "required": ["peer_id", "host", "port", "public_key", "encryption_public_key"],
                "additionalProperties": False,
            },
        },
        handler=mesh_add_peer,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Register a peer so Hermes can exchange Wildmesh messages with it.",
        emoji="🤝",
    )
    ctx.register_tool(
        name="wildmesh_grant_capability",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_grant_capability",
            "description": "Grant a peer a local capability label.",
            "parameters": {
                "type": "object",
                "properties": {
                    "peer_id": {"type": "string"},
                    "capability": {"type": "string"},
                    "expires_at": {"type": "string"},
                    "note": {"type": "string"},
                },
                "required": ["peer_id", "capability"],
                "additionalProperties": False,
            },
        },
        handler=mesh_grant_capability,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Grant a capability to a peer.",
        emoji="🛡️",
    )
    ctx.register_tool(
        name="wildmesh_subscribe_topic",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_subscribe_topic",
            "description": "Subscribe the local node to a public Wildmesh topic so peers can discover interest and send broadcasts.",
            "parameters": {
                "type": "object",
                "properties": {
                    "topic": {"type": "string"}
                },
                "required": ["topic"],
                "additionalProperties": False,
            },
        },
        handler=mesh_subscribe_topic,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Subscribe the local node to a topic.",
        emoji="📣",
    )
    ctx.register_tool(
        name="wildmesh_list_subscriptions",
        toolset=TOOLSET,
        schema={"name": "wildmesh_list_subscriptions", "description": "List local Wildmesh topic subscriptions.", "parameters": {"type": "object", "properties": {}, "additionalProperties": False}},
        handler=mesh_list_subscriptions,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="List topic subscriptions.",
        emoji="🧭",
    )
    ctx.register_tool(
        name="wildmesh_send_context",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_send_context",
            "description": "Send a structured context capsule to a peer.",
            "parameters": {
                "type": "object",
                "properties": {
                    "peer_id": {"type": "string"},
                    "capability": {"type": "string"},
                    "title": {"type": "string"},
                    "tags": {"type": "array", "items": {"type": "string"}},
                    "ttl_secs": {"type": "integer"},
                    "context": {"type": "object"},
                },
                "required": ["peer_id", "context"],
                "additionalProperties": False,
            },
        },
        handler=mesh_send_context,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Send compact context to a peer for shared work.",
        emoji="🧠",
    )
    ctx.register_tool(
        name="wildmesh_list_artifacts",
        toolset=TOOLSET,
        schema={"name": "wildmesh_list_artifacts", "description": "List local WildMesh artifacts.", "parameters": {"type": "object", "properties": {}, "additionalProperties": False}},
        handler=mesh_list_artifacts,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Inspect locally stored artifact manifests.",
        emoji="🗂️",
    )
    ctx.register_tool(
        name="wildmesh_offer_artifact",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_offer_artifact",
            "description": "Offer a local file to a peer over WildMesh.",
            "parameters": {
                "type": "object",
                "properties": {
                    "peer_id": {"type": "string"},
                    "path": {"type": "string"},
                    "capability": {"type": "string"},
                    "name": {"type": "string"},
                    "mime_type": {"type": "string"},
                    "note": {"type": "string"},
                },
                "required": ["peer_id", "path"],
                "additionalProperties": False,
            },
        },
        handler=mesh_offer_artifact,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Store a local artifact and offer it to a peer.",
        emoji="📦",
    )
    ctx.register_tool(
        name="wildmesh_fetch_artifact",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_fetch_artifact",
            "description": "Request an offered artifact from a peer.",
            "parameters": {
                "type": "object",
                "properties": {
                    "peer_id": {"type": "string"},
                    "artifact_id": {"type": "string"},
                    "capability": {"type": "string"},
                },
                "required": ["peer_id", "artifact_id"],
                "additionalProperties": False,
            },
        },
        handler=mesh_fetch_artifact,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Fetch a remote artifact into the local artifact spool.",
        emoji="📥",
    )
    ctx.register_tool(
        name="wildmesh_delegate_work",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_delegate_work",
            "description": "Delegate a scoped unit of work to a peer.",
            "parameters": {
                "type": "object",
                "properties": {
                    "peer_id": {"type": "string"},
                    "task_type": {"type": "string"},
                    "instruction": {"type": "string"},
                    "input": {"type": "object"},
                    "context": {"type": "object"},
                    "capability": {"type": "string"},
                    "max_output_chars": {"type": "integer"},
                },
                "required": ["peer_id", "task_type", "instruction"],
                "additionalProperties": False,
            },
        },
        handler=mesh_delegate_work,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Send a delegated work request to a cooperating peer.",
        emoji="⚙️",
    )
    ctx.register_tool(
        name="wildmesh_list_pending_requests",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_list_pending_requests",
            "description": "List inbound delegated work requests that are waiting for local approval.",
            "parameters": {
                "type": "object",
                "properties": {"limit": {"type": "integer", "minimum": 1, "maximum": 200}},
                "additionalProperties": False,
            },
        },
        handler=mesh_list_pending_requests,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Inspect pending inbound delegate requests awaiting approval.",
        emoji="📨",
    )
    ctx.register_tool(
        name="wildmesh_accept_request",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_accept_request",
            "description": "Approve a pending WildMesh delegated work request. Set always_allow=true to persist trust for future delegated work from that peer.",
            "parameters": {
                "type": "object",
                "properties": {
                    "message_id": {"type": "string"},
                    "always_allow": {"type": "boolean"},
                    "grant_note": {"type": "string"},
                    "grant_capability": {"type": "string"},
                },
                "required": ["message_id"],
                "additionalProperties": False,
            },
        },
        handler=mesh_accept_request,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Approve a pending delegated request once or trust the peer for future delegated work.",
        emoji="✅",
    )
    ctx.register_tool(
        name="wildmesh_deny_request",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_deny_request",
            "description": "Deny a pending WildMesh delegated work request and notify the requester.",
            "parameters": {
                "type": "object",
                "properties": {
                    "message_id": {"type": "string"},
                    "reason": {"type": "string"},
                },
                "required": ["message_id"],
                "additionalProperties": False,
            },
        },
        handler=mesh_deny_request,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Deny a pending delegated request.",
        emoji="⛔",
    )
    ctx.register_tool(
        name="wildmesh_send_task",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_send_task",
            "description": "Send a task or note to a peer over Wildmesh.",
            "parameters": {
                "type": "object",
                "properties": {
                    "peer_id": {"type": "string"},
                    "kind": {"type": "string", "enum": ["task_offer", "task_result", "note", "hello", "receipt"]},
                    "capability": {"type": "string"},
                    "body": {"type": "object"},
                },
                "required": ["peer_id", "body"],
                "additionalProperties": False,
            },
        },
        handler=mesh_send_task,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Send an Wildmesh envelope to a peer.",
        emoji="✉️",
    )
    ctx.register_tool(
        name="wildmesh_broadcast",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_broadcast",
            "description": "Broadcast a public message to peers that have announced interest in a topic.",
            "parameters": {
                "type": "object",
                "properties": {
                    "topic": {"type": "string"},
                    "body": {"type": "object"},
                },
                "required": ["topic", "body"],
                "additionalProperties": False,
            },
        },
        handler=mesh_broadcast,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Broadcast a public topic message.",
        emoji="📡",
    )
    ctx.register_tool(
        name="wildmesh_discover_now",
        toolset=TOOLSET,
        schema={"name": "wildmesh_discover_now", "description": "Broadcast a signed discovery announcement immediately.", "parameters": {"type": "object", "properties": {}, "additionalProperties": False}},
        handler=mesh_discover_now,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Trigger a discovery announcement now.",
        emoji="🌐",
    )
    ctx.register_tool(
        name="wildmesh_fetch_inbox",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_fetch_inbox",
            "description": "Fetch recent inbound Wildmesh messages. For delegated work replies, prefer the inline delegate_results/latest_delegate_result fields before attempting any artifact fetch.",
            "parameters": {
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200},
                    "peer_id": {"type": "string"},
                    "peer_label": {"type": "string"},
                },
                "additionalProperties": False,
            },
        },
        handler=mesh_fetch_inbox,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Inspect recent inbound peer messages.",
        emoji="📥",
    )
    ctx.register_tool(
        name="wildmesh_latest_delegate_result",
        toolset=TOOLSET,
        schema={
            "name": "wildmesh_latest_delegate_result",
            "description": "Return the newest inbound delegate_result with inline summary/output. Use this instead of artifact fetch when the user asks for the latest completed delegated job.",
            "parameters": {
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200},
                    "peer_id": {"type": "string"},
                    "peer_label": {"type": "string"},
                },
                "additionalProperties": False,
            },
        },
        handler=mesh_latest_delegate_result,
        check_fn=check_agentmesh_available,
        is_async=False,
        description="Get the latest delegated-work result directly, including inline output when present.",
        emoji="✅",
    )
