from __future__ import annotations

import json
import shutil
import socket
import subprocess
import time
from pathlib import Path
from typing import Any

from ..client import AgentMeshClient
from ..config import load_config

TOOLSET = "wildmesh"
_SETUP_WAIT_ATTEMPTS = 10
_SETUP_WAIT_SECS = 0.5


def _client() -> AgentMeshClient:
    return AgentMeshClient()


def _normalize_args(args: dict[str, Any] | None) -> dict[str, Any]:
    return args or {}


def _json_result(payload: Any) -> str:
    return json.dumps(payload, indent=2, sort_keys=True)


def _peer_index(client: AgentMeshClient) -> dict[str, dict[str, Any]]:
    try:
        peers = client.list_peers()
    except Exception:
        return {}
    return {
        str(peer.get("peer_id")): peer
        for peer in peers
        if isinstance(peer, dict) and peer.get("peer_id")
    }


def _grant_index(client: AgentMeshClient) -> dict[str, list[dict[str, Any]]]:
    try:
        grants = client.list_capabilities()
    except Exception:
        return {}
    index: dict[str, list[dict[str, Any]]] = {}
    for grant in grants:
        if not isinstance(grant, dict):
            continue
        peer_id = grant.get("peer_id")
        if not isinstance(peer_id, str) or not peer_id.strip():
            continue
        index.setdefault(peer_id, []).append(grant)
    return index


def _decorate_peer(peer: dict[str, Any], grants: list[dict[str, Any]]) -> dict[str, Any]:
    view = dict(peer)
    capabilities = sorted(
        {
            str(grant.get("capability")).strip()
            for grant in grants
            if isinstance(grant.get("capability"), str) and str(grant.get("capability")).strip()
        }
    )
    view["granted_capabilities"] = capabilities
    view["whitelisted_for_delegate_work"] = "delegate_work" in capabilities
    view["trust_note"] = next(
        (
            grant.get("note")
            for grant in grants
            if grant.get("capability") == "delegate_work"
            and isinstance(grant.get("note"), str)
            and str(grant.get("note")).strip()
        ),
        None,
    )
    return view


def _match_peer(
    peer: dict[str, Any],
    *,
    peer_id: str | None = None,
    peer_label: str | None = None,
) -> bool:
    if peer_id and peer.get("peer_id") == peer_id:
        return True
    if isinstance(peer_label, str) and peer_label.strip():
        label_match = peer_label.strip().lower()
        haystack = " ".join(
            str(peer.get(key, "") or "")
            for key in ("agent_label", "label", "peer_id", "agent_description")
        ).lower()
        return label_match in haystack
    return False


def _find_peer(
    peers: list[dict[str, Any]],
    *,
    peer_id: str | None = None,
    peer_label: str | None = None,
) -> dict[str, Any] | None:
    for peer in peers:
        if _match_peer(peer, peer_id=peer_id, peer_label=peer_label):
            return peer
    return None


def _peer_payload(peers: list[dict[str, Any]], grant_index: dict[str, list[dict[str, Any]]]) -> dict[str, Any]:
    items = [
        _decorate_peer(peer, grant_index.get(str(peer.get("peer_id")), []))
        for peer in peers
    ]
    active_items = [peer for peer in items if peer.get("activity_state") == "active"]
    quiet_items = [peer for peer in items if peer.get("activity_state") == "quiet"]
    manual_items = [peer for peer in items if peer.get("activity_state") == "manual"]
    return {
        "items": items,
        "active_items": active_items,
        "quiet_items": quiet_items,
        "manual_items": manual_items,
        "active_count": len(active_items),
        "quiet_count": len(quiet_items),
        "manual_count": len(manual_items),
        "note": (
            "Treat only active_items as currently up. quiet_items were seen recently but may not "
            "be reachable right now."
        ),
    }


def _peer_label(peer_index: dict[str, dict[str, Any]], peer_id: str | None) -> str | None:
    if not peer_id:
        return None
    peer = peer_index.get(peer_id, {})
    for key in ("agent_label", "label", "peer_id"):
        value = peer.get(key)
        if isinstance(value, str) and value.strip():
            return value
    return peer_id


def _delegate_result_view(
    message: dict[str, Any],
    peer_index: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    body = message.get("body", {}) if isinstance(message.get("body"), dict) else {}
    output = body.get("output")
    nested_result = output.get("result") if isinstance(output, dict) and isinstance(output.get("result"), dict) else {}
    summary = (
        body.get("summary")
        or (output.get("summary") if isinstance(output, dict) else None)
        or nested_result.get("summary")
        or (nested_result.get("output") if isinstance(nested_result.get("output"), str) else None)
    )
    text_output = (
        (output.get("output") if isinstance(output, dict) and isinstance(output.get("output"), str) else None)
        or (nested_result.get("output") if isinstance(nested_result.get("output"), str) else None)
    )
    peer_id = message.get("peer_id")
    return {
        "message_id": message.get("id"),
        "created_at": message.get("created_at"),
        "peer_id": peer_id,
        "peer_label": _peer_label(peer_index, peer_id),
        "status": body.get("status") or message.get("status"),
        "reason": message.get("reason"),
        "task_id": body.get("task_id"),
        "task_type": body.get("task_type"),
        "handled_by": body.get("handled_by"),
        "summary": summary,
        "text_output": text_output,
        "output": output,
        "raw_body": body,
    }


def _delegate_results(
    messages: list[dict[str, Any]],
    peer_index: dict[str, dict[str, Any]],
    *,
    peer_id: str | None = None,
    peer_label: str | None = None,
) -> list[dict[str, Any]]:
    label_match = peer_label.lower() if isinstance(peer_label, str) and peer_label.strip() else None
    results: list[dict[str, Any]] = []
    for message in messages:
        if message.get("kind") != "delegate_result":
            continue
        if peer_id and message.get("peer_id") != peer_id:
            continue
        view = _delegate_result_view(message, peer_index)
        if label_match:
            haystack = " ".join(
                value
                for value in [
                    view.get("peer_label"),
                    view.get("handled_by"),
                    view.get("peer_id"),
                ]
                if isinstance(value, str)
            ).lower()
            if label_match not in haystack:
                continue
        results.append(view)
    return results


def _binary_path() -> str:
    binary = shutil.which("wildmesh")
    if not binary:
        raise RuntimeError("wildmesh binary is not installed or not on PATH")
    return binary


def _profile_snapshot(client: AgentMeshClient) -> dict[str, Any]:
    profile = client.profile()
    profile.setdefault("daemon_ready", False)
    return profile


def _wait_for_daemon(client: AgentMeshClient) -> dict[str, Any]:
    last_error = None
    for _ in range(_SETUP_WAIT_ATTEMPTS):
        try:
            return client.status()
        except Exception as exc:  # noqa: BLE001
            last_error = str(exc)
            time.sleep(_SETUP_WAIT_SECS)
    raise RuntimeError(last_error or "wildmesh daemon did not become ready")


def check_agentmesh_available() -> bool:
    return shutil.which("wildmesh") is not None


def mesh_status(_args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        status = client.status()
        mesh_ready = bool(status.get("reachability", {}).get("mesh_worker_alive", False))
        return _json_result({
            "daemon_ready": True,
            "mesh_ready": mesh_ready,
            "status": status,
            "profile": client.profile(),
            "next_steps": [] if mesh_ready else [
                "Use wildmesh_setup to repair and restart the current local node.",
            ],
        })
    except Exception as exc:  # noqa: BLE001
        return _json_result({
            "daemon_ready": False,
            "mesh_ready": False,
            "error": str(exc),
            "profile": _profile_snapshot(client),
            "next_steps": [
                "Use wildmesh_setup to initialize and start the local daemon.",
                "Or run `wildmesh setup --agent-label <label>` in the shell.",
            ],
        })
    finally:
        client.close()


def mesh_profile(_args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        profile = client.profile()
        try:
            status = client.status()
            profile["daemon_ready"] = True
            profile["mesh_ready"] = bool(status.get("reachability", {}).get("mesh_worker_alive", False))
            profile["reachability"] = status.get("reachability", {})
            profile["identity"] = status.get("identity", {})
            profile["queue"] = status.get("queue", {})
            if not profile["mesh_ready"]:
                profile["next_steps"] = [
                    "Use wildmesh_setup to repair and restart the current local node.",
                ]
        except Exception as exc:  # noqa: BLE001
            profile["daemon_ready"] = False
            profile["mesh_ready"] = False
            profile["status_error"] = str(exc)
            profile["next_steps"] = [
                "Use wildmesh_setup to initialize and start the local daemon.",
                "Or run `wildmesh setup --agent-label <label>` in the shell.",
            ]
        return _json_result(profile)
    finally:
        client.close()


def mesh_setup(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    payload = _normalize_args(args)
    binary = _binary_path()
    home = Path(payload["home"]) if payload.get("home") else None
    cfg = load_config(home)
    label = payload.get("agent_label") or cfg.agent_label or socket.gethostname()
    description = (
        payload.get("agent_description")
        or cfg.agent_description
        or "WildMesh node managed through Hermes"
    )
    interests = payload.get("interests") or cfg.interests or []
    if isinstance(interests, str):
        interests = [interests]
    local_only = bool(payload.get("local_only", getattr(cfg, "local_only", False)))
    command = [
        binary,
        "setup",
        "--agent-label",
        label,
        "--agent-description",
        description,
        "--with-hermes",
        "false",
    ]
    if home:
        command.extend(["--home", str(home)])
    if payload.get("control_port") is not None:
        command.extend(["--control-port", str(payload["control_port"])])
    if payload.get("p2p_port") is not None:
        command.extend(["--p2p-port", str(payload["p2p_port"])])
    if payload.get("advertise_host"):
        command.extend(["--advertise-host", str(payload["advertise_host"])])
    for interest in interests:
        command.extend(["--interest", str(interest)])
    if local_only:
        command.append("--local-only")
    else:
        for bootstrap_url in payload.get("bootstrap_urls") or []:
            command.extend(["--bootstrap-url", str(bootstrap_url)])
    if payload.get("cooperate") or cfg.cooperate_enabled:
        command.append("--cooperate")
    executor_mode = payload.get("executor_mode") or cfg.executor_mode
    if executor_mode:
        command.extend(["--executor-mode", str(executor_mode)])
    executor_url = payload.get("executor_url") or cfg.executor_url
    if executor_url:
        command.extend(["--executor-url", str(executor_url)])
    executor_model = payload.get("executor_model") or cfg.executor_model
    if executor_model:
        command.extend(["--executor-model", str(executor_model)])
    executor_api_key_env = payload.get("executor_api_key_env") or cfg.executor_api_key_env
    if executor_api_key_env:
        command.extend(["--executor-api-key-env", str(executor_api_key_env)])
    command.extend(["--launch-agent", str(bool(payload.get("launch_agent", True))).lower()])
    if payload.get("hermes_home"):
        command.extend(["--hermes-home", str(payload["hermes_home"])])

    completed = subprocess.run(command, capture_output=True, text=True, check=False)
    stdout = completed.stdout.strip()
    stderr = completed.stderr.strip()
    output: dict[str, Any] = {}
    if stdout:
        try:
            output = json.loads(stdout)
        except json.JSONDecodeError:
            output = {"stdout": stdout}
    client = _client()
    try:
        daemon_status = _wait_for_daemon(client) if completed.returncode == 0 else None
        profile = client.profile()
        return _json_result({
            "ok": completed.returncode == 0,
            "command": command,
            "stdout": stdout,
            "stderr": stderr,
            "setup_output": output,
            "daemon_ready": daemon_status is not None,
            "status": daemon_status,
            "profile": profile,
        })
    except Exception as exc:  # noqa: BLE001
        return _json_result({
            "ok": completed.returncode == 0,
            "command": command,
            "stdout": stdout,
            "stderr": stderr,
            "setup_output": output,
            "daemon_ready": False,
            "error": str(exc),
            "profile": _profile_snapshot(client),
        })
    finally:
        client.close()


def mesh_list_peers(_args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        peers = client.list_peers()
        grants = _grant_index(client)
        return _json_result(_peer_payload(peers, grants))
    finally:
        client.close()


def mesh_browse_peers(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    args = _normalize_args(args)
    client = _client()
    try:
        peers = client.browse_peers(
            interest=args.get("interest"),
            text=args.get("text"),
            discovered_only=bool(args.get("discovered_only", False)),
            refresh=bool(args.get("refresh", True)),
        )
        grants = _grant_index(client)
        return _json_result(_peer_payload(peers, grants))
    finally:
        client.close()


def mesh_add_peer(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        return _json_result(client.add_peer(_normalize_args(args)))
    finally:
        client.close()


def mesh_grant_capability(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        return _json_result(client.grant(_normalize_args(args)))
    finally:
        client.close()


def mesh_list_grants(_args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        peers = client.list_peers()
        peer_index = {str(peer.get("peer_id")): peer for peer in peers if isinstance(peer, dict)}
        items = client.list_capabilities()
        for item in items:
            if not isinstance(item, dict):
                continue
            peer_id = item.get("peer_id")
            if isinstance(peer_id, str):
                item["peer_label"] = _peer_label(peer_index, peer_id)
        return _json_result({"items": items})
    finally:
        client.close()


def mesh_whitelist_status(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    args = _normalize_args(args)
    capability = str(args.get("capability") or "delegate_work")
    client = _client()
    try:
        peers = client.list_peers()
        grants = client.list_capabilities()
        peer = _find_peer(
            peers,
            peer_id=args.get("peer_id"),
            peer_label=args.get("peer_label"),
        )
        if peer is None:
            return _json_result(
                {
                    "found": False,
                    "peer_id": args.get("peer_id"),
                    "peer_label": args.get("peer_label"),
                    "capability": capability,
                    "whitelisted": False,
                    "note": "peer not found in the local WildMesh peer view",
                }
            )
        matching = [
            grant
            for grant in grants
            if isinstance(grant, dict)
            and grant.get("peer_id") == peer.get("peer_id")
            and grant.get("capability") == capability
        ]
        return _json_result(
            {
                "found": True,
                "peer_id": peer.get("peer_id"),
                "peer_label": _peer_label({str(peer.get("peer_id")): peer}, str(peer.get("peer_id"))),
                "capability": capability,
                "whitelisted": bool(matching),
                "grant": matching[0] if matching else None,
                "granted_capabilities": sorted(
                    {
                        str(grant.get("capability")).strip()
                        for grant in grants
                        if isinstance(grant, dict)
                        and grant.get("peer_id") == peer.get("peer_id")
                        and isinstance(grant.get("capability"), str)
                    }
                ),
            }
        )
    finally:
        client.close()


def mesh_revoke_capability(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    args = _normalize_args(args)
    client = _client()
    try:
        return _json_result(
            client.revoke(
                {
                    "peer_id": args["peer_id"],
                    "capability": args["capability"],
                }
            )
        )
    finally:
        client.close()


def mesh_subscribe_topic(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        return _json_result(client.subscribe(_normalize_args(args)))
    finally:
        client.close()


def mesh_create_channel(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        return _json_result(client.create_channel(_normalize_args(args)))
    finally:
        client.close()


def mesh_list_subscriptions(_args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        return _json_result({"items": client.list_subscriptions()})
    finally:
        client.close()


def mesh_list_channels(_args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        return _json_result({"items": client.list_topics()})
    finally:
        client.close()


def mesh_send_context(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        return _json_result(client.send_context(_normalize_args(args)))
    finally:
        client.close()


def mesh_list_artifacts(_args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        return _json_result({"items": client.list_artifacts()})
    finally:
        client.close()


def mesh_offer_artifact(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        return _json_result(client.offer_artifact(_normalize_args(args)))
    finally:
        client.close()


def mesh_fetch_artifact(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        return _json_result(client.fetch_artifact(_normalize_args(args)))
    finally:
        client.close()


def mesh_delegate_work(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        return _json_result(client.delegate(_normalize_args(args)))
    finally:
        client.close()


def mesh_list_pending_requests(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    args = _normalize_args(args)
    client = _client()
    try:
        return _json_result({"items": client.pending(limit=args.get("limit", 50))})
    finally:
        client.close()


def mesh_accept_request(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    args = _normalize_args(args)
    client = _client()
    try:
        return _json_result(
            client.accept_request(
                {
                    "message_id": args["message_id"],
                    "always_allow": args.get("always_allow", False),
                    "grant_note": args.get("grant_note"),
                    "grant_capability": args.get("grant_capability"),
                }
            )
        )
    finally:
        client.close()


def mesh_deny_request(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    args = _normalize_args(args)
    client = _client()
    try:
        return _json_result(client.deny_request(
            {"message_id": args["message_id"], "reason": args.get("reason")}
        ))
    finally:
        client.close()


def mesh_send_task(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    args = _normalize_args(args)
    client = _client()
    try:
        payload = {
            "peer_id": args["peer_id"],
            "kind": args.get("kind", "task_offer"),
            "capability": args.get("capability"),
            "body": args.get("body", {}),
        }
        return _json_result(client.send(payload))
    finally:
        client.close()


def mesh_broadcast(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    args = _normalize_args(args)
    client = _client()
    try:
        return _json_result(client.broadcast({"topic": args["topic"], "body": args.get("body", {})}))
    finally:
        client.close()


def mesh_discover_now(_args: dict[str, Any] | None = None, **_meta: Any) -> str:
    _args = _normalize_args(_args)
    client = _client()
    try:
        payload = {}
        if _args.get("host"):
            payload["host"] = _args["host"]
        if _args.get("port") is not None:
            payload["port"] = _args["port"]
        return _json_result(client.discover_now(payload))
    finally:
        client.close()


def mesh_fetch_inbox(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    args = _normalize_args(args)
    client = _client()
    try:
        items = client.inbox(limit=args.get("limit", 50))
        peer_index = _peer_index(client)
        delegate_results = _delegate_results(
            items,
            peer_index,
            peer_id=args.get("peer_id"),
            peer_label=args.get("peer_label"),
        )
        return _json_result({
            "items": items,
            "delegate_results": delegate_results,
            "latest_delegate_result": delegate_results[0] if delegate_results else None,
            "note": (
                "delegate_result entries already include inline summary/output when the worker "
                "returns them. task_id is not an artifact id. Only fetch artifacts when a peer "
                "explicitly offered one."
            ),
        })
    finally:
        client.close()


def mesh_latest_delegate_result(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    args = _normalize_args(args)
    client = _client()
    try:
        items = client.inbox(limit=args.get("limit", 50))
        peer_index = _peer_index(client)
        delegate_results = _delegate_results(
            items,
            peer_index,
            peer_id=args.get("peer_id"),
            peer_label=args.get("peer_label"),
        )
        latest = delegate_results[0] if delegate_results else None
        return _json_result({
            "found": latest is not None,
            "result": latest,
            "note": (
                "Use the inline output above for normal delegated-work replies. "
                "Artifacts are separate and should only be fetched after an explicit artifact offer."
            ),
        })
    finally:
        client.close()
