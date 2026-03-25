from __future__ import annotations

import json
import subprocess
from typing import Any

from agentmesh.hermes_plugin import tools


class _ClientStub:
    def __init__(
        self,
        *,
        status_result: dict[str, Any] | None = None,
        status_error: Exception | None = None,
        profile_result: dict[str, Any] | None = None,
        peers_result: list[dict[str, Any]] | None = None,
        inbox_result: list[dict[str, Any]] | None = None,
        grants_result: list[dict[str, Any]] | None = None,
    ) -> None:
        self._status_result = status_result or {"identity": {"peer_id": "peer-1"}}
        self._status_error = status_error
        self._profile_result = profile_result or {"agent_label": "node-1"}
        self._peers_result = peers_result or []
        self._inbox_result = inbox_result or []
        self._grants_result = grants_result or []

    def close(self) -> None:
        return None

    def status(self) -> dict[str, Any]:
        if self._status_error is not None:
            raise self._status_error
        return self._status_result

    def profile(self) -> dict[str, Any]:
        return dict(self._profile_result)

    def list_peers(self) -> list[dict[str, Any]]:
        return list(self._peers_result)

    def list_capabilities(self) -> list[dict[str, Any]]:
        return list(self._grants_result)

    def inbox(self, limit: int = 50) -> list[dict[str, Any]]:
        return list(self._inbox_result)[:limit]

    def revoke(self, payload: dict[str, Any]) -> dict[str, Any]:
        return {
            "peer_id": payload["peer_id"],
            "capability": payload["capability"],
            "revoked": True,
        }


def test_mesh_profile_ignores_task_metadata(monkeypatch):
    monkeypatch.setattr(tools, "_client", lambda: _ClientStub())
    result = json.loads(tools.mesh_profile({}, task_id="task-123", user_task="inspect"))
    assert result["agent_label"] == "node-1"
    assert result["daemon_ready"] is True
    assert result["identity"]["peer_id"] == "peer-1"


def test_mesh_status_reports_offline_daemon(monkeypatch):
    monkeypatch.setattr(
        tools,
        "_client",
        lambda: _ClientStub(
            status_error=RuntimeError("connection refused"),
            profile_result={"agent_label": "node-offline"},
        ),
    )
    result = json.loads(tools.mesh_status({}, task_id="task-123"))
    assert result["daemon_ready"] is False
    assert "connection refused" in result["error"]
    assert result["profile"]["agent_label"] == "node-offline"
    assert result["next_steps"]


def test_mesh_setup_runs_local_bootstrap(monkeypatch):
    calls: list[list[str]] = []
    statuses: list[Any] = [RuntimeError("starting"), {"identity": {"peer_id": "peer-setup"}}]

    def fake_run(cmd, capture_output, text, check):
        calls.append(cmd)
        return subprocess.CompletedProcess(cmd, 0, '{"status":"ready"}', "")

    class _SetupClient(_ClientStub):
        def status(self) -> dict[str, Any]:
            value = statuses.pop(0)
            if isinstance(value, Exception):
                raise value
            return value

    monkeypatch.setattr(tools.shutil, "which", lambda _name: "/opt/homebrew/bin/wildmesh")
    monkeypatch.setattr(tools.subprocess, "run", fake_run)
    monkeypatch.setattr(tools.time, "sleep", lambda _secs: None)
    monkeypatch.setattr(tools, "_client", lambda: _SetupClient(profile_result={"agent_label": "mesh-node"}))

    result = json.loads(
        tools.mesh_setup(
            {
                "agent_label": "mesh-node",
                "agent_description": "Hermes-managed node",
                "interests": ["general", "local-first"],
                "cooperate": True,
                "launch_agent": False,
            },
            task_id="task-123",
        )
    )

    assert calls
    command = calls[0]
    assert command[:2] == ["/opt/homebrew/bin/wildmesh", "setup"]
    assert "--with-hermes" in command
    assert "false" in command
    assert "--cooperate" in command
    assert result["ok"] is True
    assert result["daemon_ready"] is True
    assert result["profile"]["agent_label"] == "mesh-node"


def test_mesh_latest_delegate_result_prefers_inline_output(monkeypatch):
    monkeypatch.setattr(
        tools,
        "_client",
        lambda: _ClientStub(
            peers_result=[
                {
                    "peer_id": "peer-gamma",
                    "agent_label": "gamma-live",
                }
            ],
            inbox_result=[
                {
                    "id": "msg-1",
                    "kind": "delegate_result",
                    "peer_id": "peer-gamma",
                    "created_at": "2026-03-25T20:36:26Z",
                    "status": "received",
                    "reason": "accepted",
                    "body": {
                        "status": "completed",
                        "task_id": "task-1",
                        "task_type": "summary",
                        "handled_by": "gamma-live",
                        "summary": "gamma summary",
                        "output": {"summary": "actual delegated text"},
                    },
                }
            ],
        ),
    )
    result = json.loads(tools.mesh_latest_delegate_result({"peer_label": "gamma-live"}))
    assert result["found"] is True
    assert result["result"]["peer_label"] == "gamma-live"
    assert result["result"]["task_id"] == "task-1"
    assert result["result"]["summary"] == "gamma summary"
    assert result["result"]["output"]["summary"] == "actual delegated text"


def test_mesh_latest_delegate_result_extracts_nested_executor_summary(monkeypatch):
    monkeypatch.setattr(
        tools,
        "_client",
        lambda: _ClientStub(
            peers_result=[
                {
                    "peer_id": "peer-gamma",
                    "agent_label": "gamma-live",
                }
            ],
            inbox_result=[
                {
                    "id": "msg-3",
                    "kind": "delegate_result",
                    "peer_id": "peer-gamma",
                    "created_at": "2026-03-25T21:13:59Z",
                    "status": "received",
                    "reason": "accepted",
                    "body": {
                        "status": "completed",
                        "task_id": "task-3",
                        "task_type": "summary",
                        "handled_by": "gamma-live",
                        "summary": None,
                        "output": {
                            "mode": "openai_compat",
                            "model": "hermes-agent",
                            "result": {
                                "summary": "nested summary",
                                "output": "nested output",
                            },
                        },
                    },
                }
            ],
        ),
    )
    result = json.loads(tools.mesh_latest_delegate_result({"peer_label": "gamma-live"}))
    assert result["found"] is True
    assert result["result"]["task_id"] == "task-3"
    assert result["result"]["summary"] == "nested summary"
    assert result["result"]["text_output"] == "nested output"


def test_mesh_fetch_inbox_surfaces_latest_delegate_result(monkeypatch):
    monkeypatch.setattr(
        tools,
        "_client",
        lambda: _ClientStub(
            peers_result=[
                {
                    "peer_id": "peer-gamma",
                    "agent_label": "gamma-live",
                }
            ],
            inbox_result=[
                {
                    "id": "msg-2",
                    "kind": "delegate_result",
                    "peer_id": "peer-gamma",
                    "created_at": "2026-03-25T20:40:00Z",
                    "body": {
                        "status": "completed",
                        "task_id": "task-2",
                        "task_type": "task",
                        "handled_by": "gamma-live",
                        "summary": "inline summary",
                        "output": "inline output",
                    },
                }
            ],
        ),
    )
    result = json.loads(tools.mesh_fetch_inbox({"peer_label": "gamma-live"}))
    assert result["latest_delegate_result"]["task_id"] == "task-2"
    assert result["delegate_results"][0]["output"] == "inline output"
    assert "task_id is not an artifact id" in result["note"]


def test_mesh_whitelist_status_detects_delegate_trust(monkeypatch):
    monkeypatch.setattr(
        tools,
        "_client",
        lambda: _ClientStub(
            peers_result=[
                {
                    "peer_id": "peer-gamma",
                    "agent_label": "gamma-live",
                    "agent_description": "worker",
                }
            ],
            grants_result=[
                {
                    "peer_id": "peer-gamma",
                    "capability": "delegate_work",
                    "note": "trusted worker",
                }
            ],
        ),
    )
    result = json.loads(tools.mesh_whitelist_status({"peer_label": "gamma-live"}))
    assert result["found"] is True
    assert result["whitelisted"] is True
    assert result["grant"]["note"] == "trusted worker"


def test_mesh_list_peers_surfaces_granted_capabilities(monkeypatch):
    monkeypatch.setattr(
        tools,
        "_client",
        lambda: _ClientStub(
            peers_result=[
                {
                    "peer_id": "peer-gamma",
                    "agent_label": "gamma-live",
                    "activity_state": "active",
                }
            ],
            grants_result=[
                {"peer_id": "peer-gamma", "capability": "delegate_work", "note": "trusted"},
                {"peer_id": "peer-gamma", "capability": "artifact_exchange"},
            ],
        ),
    )
    result = json.loads(tools.mesh_list_peers())
    assert result["active_count"] == 1
    assert result["items"][0]["whitelisted_for_delegate_work"] is True
    assert "artifact_exchange" in result["items"][0]["granted_capabilities"]
