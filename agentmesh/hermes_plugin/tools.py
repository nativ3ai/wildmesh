from __future__ import annotations

import json
import shutil
import socket
import subprocess
import time
from pathlib import Path
from typing import Any

from ..client import AgentMeshClient

TOOLSET = "wildmesh"
_SETUP_WAIT_ATTEMPTS = 10
_SETUP_WAIT_SECS = 0.5


def _client() -> AgentMeshClient:
    return AgentMeshClient()


def _normalize_args(args: dict[str, Any] | None) -> dict[str, Any]:
    return args or {}


def _json_result(payload: Any) -> str:
    return json.dumps(payload, indent=2, sort_keys=True)


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
        return _json_result({
            "daemon_ready": True,
            "status": status,
            "profile": client.profile(),
        })
    except Exception as exc:  # noqa: BLE001
        return _json_result({
            "daemon_ready": False,
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
            profile["reachability"] = status.get("reachability", {})
            profile["identity"] = status.get("identity", {})
            profile["queue"] = status.get("queue", {})
        except Exception as exc:  # noqa: BLE001
            profile["daemon_ready"] = False
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
    label = payload.get("agent_label") or socket.gethostname()
    description = payload.get("agent_description") or "WildMesh node managed through Hermes"
    interests = payload.get("interests") or []
    if isinstance(interests, str):
        interests = [interests]
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
    if payload.get("home"):
        command.extend(["--home", str(payload["home"])])
    if payload.get("control_port") is not None:
        command.extend(["--control-port", str(payload["control_port"])])
    if payload.get("p2p_port") is not None:
        command.extend(["--p2p-port", str(payload["p2p_port"])])
    if payload.get("advertise_host"):
        command.extend(["--advertise-host", str(payload["advertise_host"])])
    for interest in interests:
        command.extend(["--interest", str(interest)])
    for bootstrap_url in payload.get("bootstrap_urls") or []:
        command.extend(["--bootstrap-url", str(bootstrap_url)])
    if payload.get("cooperate"):
        command.append("--cooperate")
    if payload.get("executor_mode"):
        command.extend(["--executor-mode", str(payload["executor_mode"])])
    if payload.get("executor_url"):
        command.extend(["--executor-url", str(payload["executor_url"])])
    if payload.get("executor_model"):
        command.extend(["--executor-model", str(payload["executor_model"])])
    if payload.get("executor_api_key_env"):
        command.extend(["--executor-api-key-env", str(payload["executor_api_key_env"])])
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
        return _json_result({"items": client.list_peers()})
    finally:
        client.close()


def mesh_browse_peers(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    args = _normalize_args(args)
    client = _client()
    try:
        return _json_result({
            "items": client.browse_peers(
                interest=args.get("interest"),
                text=args.get("text"),
                discovered_only=bool(args.get("discovered_only", False)),
                refresh=bool(args.get("refresh", True)),
            )
        })
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


def mesh_subscribe_topic(args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        return _json_result(client.subscribe(_normalize_args(args)))
    finally:
        client.close()


def mesh_list_subscriptions(_args: dict[str, Any] | None = None, **_meta: Any) -> str:
    client = _client()
    try:
        return _json_result({"items": client.list_subscriptions()})
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
        return _json_result({"items": client.inbox(limit=args.get("limit", 50))})
    finally:
        client.close()
