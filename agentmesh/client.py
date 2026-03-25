from __future__ import annotations

from pathlib import Path
from typing import Any

import httpx

from .config import load_config


class AgentMeshClient:
    def __init__(self, base_url: str | None = None, home: Path | None = None):
        self.home = home
        if base_url is None:
            base_url = load_config(home).control_url
        self.base_url = base_url.rstrip("/")
        self._client = httpx.Client(base_url=self.base_url, timeout=10)

    def close(self) -> None:
        self._client.close()

    def status(self) -> dict[str, Any]:
        return self._client.get("/v1/status").raise_for_status().json()

    def profile(self) -> dict[str, Any]:
        cfg = load_config(self.home)
        profile = {
            "agent_label": cfg.agent_label,
            "agent_description": cfg.agent_description,
            "node_type": "agent",
            "runtime_name": "wildmesh",
            "interests": cfg.interests or [],
            "control_url": cfg.control_url,
            "p2p_endpoint": cfg.p2p_endpoint,
            "public_api_url": f"http://{cfg.advertise_host}:{cfg.public_api_port}",
            "bootstrap_urls": cfg.bootstrap_urls or [],
            "collaboration": {
                "cooperate_enabled": cfg.cooperate_enabled,
                "executor_mode": cfg.executor_mode,
                "accepts_context_capsules": True,
                "accepts_artifact_exchange": True,
                "accepts_delegate_work": cfg.cooperate_enabled and cfg.executor_mode != "disabled",
            },
            "nat_status": "unknown",
            "public_address": None,
            "listen_addrs": [],
            "external_addrs": [],
            "upnp_mapped_addrs": [],
        }
        try:
            status = self.status()
            reachability = status.get("reachability", {})
            profile["nat_status"] = reachability.get("nat_status", "unknown")
            profile["public_address"] = reachability.get("public_address")
            profile["listen_addrs"] = reachability.get("listen_addrs", []) or []
            profile["external_addrs"] = reachability.get("external_addrs", []) or []
            profile["upnp_mapped_addrs"] = reachability.get("upnp_mapped_addrs", []) or []
        except Exception:
            pass
        return profile

    def list_peers(self) -> list[dict[str, Any]]:
        return self._client.get("/v1/peers").raise_for_status().json()

    def add_peer(self, payload: dict[str, Any]) -> dict[str, Any]:
        return self._client.post("/v1/peers", json=payload).raise_for_status().json()

    def list_capabilities(self) -> list[dict[str, Any]]:
        return self._client.get("/v1/capabilities").raise_for_status().json()

    def grant(self, payload: dict[str, Any]) -> dict[str, Any]:
        return self._client.post("/v1/capabilities/grants", json=payload).raise_for_status().json()

    def revoke(self, payload: dict[str, Any]) -> dict[str, Any]:
        return self._client.post("/v1/capabilities/revoke", json=payload).raise_for_status().json()

    def list_subscriptions(self) -> list[dict[str, Any]]:
        return self._client.get("/v1/subscriptions").raise_for_status().json()

    def subscribe(self, payload: dict[str, Any]) -> dict[str, Any]:
        return self._client.post("/v1/subscriptions", json=payload).raise_for_status().json()

    def send_context(self, payload: dict[str, Any]) -> dict[str, Any]:
        return self._client.post("/v1/context/send", json=payload).raise_for_status().json()

    def list_artifacts(self) -> list[dict[str, Any]]:
        return self._client.get("/v1/artifacts").raise_for_status().json()

    def offer_artifact(self, payload: dict[str, Any]) -> dict[str, Any]:
        return self._client.post("/v1/artifacts/offer", json=payload).raise_for_status().json()

    def fetch_artifact(self, payload: dict[str, Any]) -> dict[str, Any]:
        return self._client.post("/v1/artifacts/fetch", json=payload).raise_for_status().json()

    def delegate(self, payload: dict[str, Any]) -> dict[str, Any]:
        return self._client.post("/v1/delegate", json=payload).raise_for_status().json()

    def pending(self, limit: int = 50) -> list[dict[str, Any]]:
        return (
            self._client.get("/v1/delegate/pending", params={"limit": limit})
            .raise_for_status()
            .json()
        )

    def accept_request(self, payload: dict[str, Any]) -> dict[str, Any]:
        return self._client.post("/v1/delegate/accept", json=payload).raise_for_status().json()

    def deny_request(self, payload: dict[str, Any]) -> dict[str, Any]:
        return self._client.post("/v1/delegate/deny", json=payload).raise_for_status().json()

    def send(self, payload: dict[str, Any]) -> dict[str, Any]:
        return self._client.post("/v1/messages/send", json=payload).raise_for_status().json()

    def broadcast(self, payload: dict[str, Any]) -> dict[str, Any]:
        return self._client.post("/v1/messages/broadcast", json=payload).raise_for_status().json()

    def inbox(self, limit: int = 50) -> list[dict[str, Any]]:
        return self._client.get("/v1/messages/inbox", params={"limit": limit}).raise_for_status().json()

    def outbox(self, limit: int = 50) -> list[dict[str, Any]]:
        return self._client.get("/v1/messages/outbox", params={"limit": limit}).raise_for_status().json()

    def discover_now(self, payload: dict[str, Any] | None = None) -> dict[str, Any]:
        return self._client.post("/v1/discovery/announce", json=payload or {}).raise_for_status().json()

    def browse_peers(
        self,
        *,
        interest: str | None = None,
        text: str | None = None,
        discovered_only: bool = False,
        refresh: bool = True,
    ) -> list[dict[str, Any]]:
        if refresh:
            self.discover_now({})
        peers = self.list_peers()
        items: list[dict[str, Any]] = []
        interest_match = interest.lower() if interest else None
        text_match = text.lower() if text else None
        for peer in peers:
            if discovered_only and not peer.get("discovered", False):
                continue
            if interest_match:
                values = [value.lower() for value in peer.get("interests", []) if isinstance(value, str)]
                if not any(interest_match in value for value in values):
                    continue
            if text_match:
                haystack = " ".join(
                    str(peer.get(key, "") or "")
                    for key in ("peer_id", "label", "agent_label", "agent_description", "host")
                ).lower()
                if text_match not in haystack:
                    continue
            items.append(peer)
        return items
