from __future__ import annotations

from pathlib import Path
from typing import Any

from ..client import AgentMeshClient

TOOLSET = "wildmesh"


def _client() -> AgentMeshClient:
    return AgentMeshClient()


def check_agentmesh_available() -> tuple[bool, str | None]:
    try:
        client = _client()
        client.status()
        client.close()
        return True, None
    except Exception as exc:  # noqa: BLE001
        return False, str(exc)


def mesh_status(_args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return client.status()
    finally:
        client.close()


def mesh_profile(_args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return client.profile()
    finally:
        client.close()


def mesh_list_peers(_args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return {"items": client.list_peers()}
    finally:
        client.close()


def mesh_browse_peers(args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return {
            "items": client.browse_peers(
                interest=args.get("interest"),
                text=args.get("text"),
                discovered_only=bool(args.get("discovered_only", False)),
                refresh=bool(args.get("refresh", True)),
            )
        }
    finally:
        client.close()


def mesh_add_peer(args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return client.add_peer(args)
    finally:
        client.close()


def mesh_grant_capability(args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return client.grant(args)
    finally:
        client.close()


def mesh_subscribe_topic(args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return client.subscribe(args)
    finally:
        client.close()


def mesh_list_subscriptions(_args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return {"items": client.list_subscriptions()}
    finally:
        client.close()


def mesh_send_context(args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return client.send_context(args)
    finally:
        client.close()


def mesh_list_artifacts(_args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return {"items": client.list_artifacts()}
    finally:
        client.close()


def mesh_offer_artifact(args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return client.offer_artifact(args)
    finally:
        client.close()


def mesh_fetch_artifact(args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return client.fetch_artifact(args)
    finally:
        client.close()


def mesh_delegate_work(args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return client.delegate(args)
    finally:
        client.close()


def mesh_list_pending_requests(args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return {"items": client.pending(limit=args.get("limit", 50))}
    finally:
        client.close()


def mesh_accept_request(args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return client.accept_request({"message_id": args["message_id"]})
    finally:
        client.close()


def mesh_deny_request(args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return client.deny_request(
            {"message_id": args["message_id"], "reason": args.get("reason")}
        )
    finally:
        client.close()


def mesh_send_task(args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        payload = {
            "peer_id": args["peer_id"],
            "kind": args.get("kind", "task_offer"),
            "capability": args.get("capability"),
            "body": args.get("body", {}),
        }
        return client.send(payload)
    finally:
        client.close()


def mesh_broadcast(args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return client.broadcast({"topic": args["topic"], "body": args.get("body", {})})
    finally:
        client.close()


def mesh_discover_now(_args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        payload = {}
        if _args.get("host"):
            payload["host"] = _args["host"]
        if _args.get("port") is not None:
            payload["port"] = _args["port"]
        return client.discover_now(payload)
    finally:
        client.close()


def mesh_fetch_inbox(args: dict[str, Any]) -> dict[str, Any]:
    client = _client()
    try:
        return {"items": client.inbox(limit=args.get("limit", 50))}
    finally:
        client.close()
