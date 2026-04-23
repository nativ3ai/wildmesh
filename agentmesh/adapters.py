from __future__ import annotations

from dataclasses import dataclass
from typing import Any
from typing import Callable
from typing import Protocol

from .hermes_plugin import plugin as hermes_plugin


class _RegisterToolContext(Protocol):
    def register_tool(
        self,
        *,
        name: str,
        toolset: str,
        schema: dict[str, Any],
        handler: Callable[..., Any],
        check_fn: Callable[..., bool] | None = None,
        is_async: bool = False,
        description: str | None = None,
        emoji: str | None = None,
    ) -> None: ...


class _AddToolContext(Protocol):
    def add_tool(
        self,
        *,
        name: str,
        description: str,
        parameters: dict[str, Any],
        handler: Callable[..., Any],
    ) -> None: ...


@dataclass(slots=True)
class ToolSpec:
    name: str
    toolset: str
    schema: dict[str, Any]
    handler: Callable[..., Any]
    check_fn: Callable[..., bool] | None
    is_async: bool
    description: str | None
    emoji: str | None


class _CaptureContext:
    def __init__(self) -> None:
        self._specs: list[ToolSpec] = []

    def register_tool(self, **kwargs: Any) -> None:
        self._specs.append(
            ToolSpec(
                name=kwargs["name"],
                toolset=kwargs["toolset"],
                schema=kwargs["schema"],
                handler=kwargs["handler"],
                check_fn=kwargs.get("check_fn"),
                is_async=bool(kwargs.get("is_async", False)),
                description=kwargs.get("description"),
                emoji=kwargs.get("emoji"),
            )
        )

    @property
    def specs(self) -> list[ToolSpec]:
        return list(self._specs)


def _clone_schema_with_name(schema: dict[str, Any], name: str) -> dict[str, Any]:
    cloned = dict(schema)
    cloned["name"] = name
    return cloned


def tool_specs() -> list[ToolSpec]:
    """Return canonical WildMesh tool specs, independent from runtime."""
    capture = _CaptureContext()
    hermes_plugin.register(capture)
    return capture.specs


def tool_manifest() -> list[dict[str, Any]]:
    """Return serializable metadata for discovery/documentation."""
    return [
        {
            "name": spec.name,
            "toolset": spec.toolset,
            "schema": spec.schema,
            "is_async": spec.is_async,
            "description": spec.description,
            "emoji": spec.emoji,
        }
        for spec in tool_specs()
    ]


def register_with_context(
    ctx: Any,
    *,
    name_prefix: str = "",
    toolset: str | None = None,
) -> list[str]:
    """
    Register WildMesh tools into different host runtimes.

    Supported context styles:
    - `register_tool(...)` (Hermes-like plugin contexts)
    - `add_tool(...)` (generic tool registries)
    - `list` (append-only manifest sink for custom bootstraps)
    """
    specs = tool_specs()
    names: list[str] = []

    for spec in specs:
        final_name = f"{name_prefix}{spec.name}"
        final_toolset = toolset or spec.toolset
        final_schema = _clone_schema_with_name(spec.schema, final_name)
        names.append(final_name)

        if hasattr(ctx, "register_tool"):
            cast_ctx = ctx  # runtime duck typing
            cast_ctx.register_tool(
                name=final_name,
                toolset=final_toolset,
                schema=final_schema,
                handler=spec.handler,
                check_fn=spec.check_fn,
                is_async=spec.is_async,
                description=spec.description,
                emoji=spec.emoji,
            )
            continue

        if hasattr(ctx, "add_tool"):
            cast_ctx = ctx  # runtime duck typing
            cast_ctx.add_tool(
                name=final_name,
                description=spec.description or final_name,
                parameters=final_schema.get("parameters", {}),
                handler=spec.handler,
            )
            continue

        if isinstance(ctx, list):
            ctx.append(
                {
                    "name": final_name,
                    "toolset": final_toolset,
                    "schema": final_schema,
                    "handler": spec.handler,
                    "check_fn": spec.check_fn,
                    "is_async": spec.is_async,
                    "description": spec.description,
                    "emoji": spec.emoji,
                }
            )
            continue

        raise TypeError(
            "Unsupported context type for WildMesh registration. "
            "Expected register_tool/add_tool context or list sink."
        )

    return names
