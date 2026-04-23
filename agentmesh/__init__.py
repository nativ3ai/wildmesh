"""AgentMesh package."""

from .adapters import register_with_context
from .adapters import tool_manifest

__all__ = ["__version__", "register_with_context", "tool_manifest"]
__version__ = "0.1.0"
