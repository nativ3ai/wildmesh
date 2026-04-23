from __future__ import annotations

from typing import Any

from agentmesh.adapters import register_with_context
from agentmesh.adapters import tool_manifest


class _RegisterCtx:
    def __init__(self) -> None:
        self.calls: list[dict[str, Any]] = []

    def register_tool(self, **kwargs: Any) -> None:
        self.calls.append(kwargs)


class _AddCtx:
    def __init__(self) -> None:
        self.calls: list[dict[str, Any]] = []

    def add_tool(self, **kwargs: Any) -> None:
        self.calls.append(kwargs)


def test_tool_manifest_exposes_wildmesh_tools() -> None:
    manifest = tool_manifest()
    names = {item["name"] for item in manifest}
    assert "wildmesh_setup" in names
    assert "wildmesh_status" in names
    assert len(names) >= 20


def test_register_with_register_tool_context_supports_prefix_and_toolset_override() -> None:
    ctx = _RegisterCtx()
    names = register_with_context(ctx, name_prefix="mesh_", toolset="mesh")
    assert names
    assert names[0].startswith("mesh_")
    first = ctx.calls[0]
    assert first["name"].startswith("mesh_")
    assert first["toolset"] == "mesh"
    assert first["schema"]["name"] == first["name"]


def test_register_with_add_tool_context_works() -> None:
    ctx = _AddCtx()
    names = register_with_context(ctx)
    assert names
    first = ctx.calls[0]
    assert "name" in first
    assert "description" in first
    assert "parameters" in first
    assert callable(first["handler"])


def test_register_with_list_sink_works() -> None:
    sink: list[dict[str, Any]] = []
    names = register_with_context(sink, name_prefix="x_")
    assert names
    assert sink
    assert sink[0]["name"].startswith("x_")
    assert sink[0]["schema"]["name"] == sink[0]["name"]
