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
    ) -> None:
        self._status_result = status_result or {"identity": {"peer_id": "peer-1"}}
        self._status_error = status_error
        self._profile_result = profile_result or {"agent_label": "node-1"}

    def close(self) -> None:
        return None

    def status(self) -> dict[str, Any]:
        if self._status_error is not None:
            raise self._status_error
        return self._status_result

    def profile(self) -> dict[str, Any]:
        return dict(self._profile_result)


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
