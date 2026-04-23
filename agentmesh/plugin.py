from .adapters import register_with_context

__all__ = ["register"]


def register(ctx) -> None:
    # Backwards-compatible plugin entrypoint for Hermes-style runtimes.
    register_with_context(ctx)
