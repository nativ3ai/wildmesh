from __future__ import annotations

import json
import os
import hashlib
from dataclasses import dataclass
from pathlib import Path

DEFAULT_BOOTSTRAP_PEERS = [
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
    "/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ",
]


def _is_default_home(home: Path) -> bool:
    return home == (Path.home() / ".wildmesh")


def _home_suffix(home: Path) -> int:
    digest = hashlib.sha256(str(home).encode("utf-8")).digest()
    return (int.from_bytes(digest[:8], "big") % 700) + 1


def _apply_home_port_defaults(cfg: "AgentMeshConfig") -> "AgentMeshConfig":
    if _is_default_home(cfg.home):
        return cfg
    suffix = _home_suffix(cfg.home)
    if cfg.control_port == 8877 and cfg.p2p_port == 4500:
        cfg.control_port = 8877 + suffix
        cfg.p2p_port = 4500 + suffix
        cfg.discovery_port = 45150 + suffix
        cfg.public_api_port = 45200 + suffix
    return cfg


@dataclass(slots=True)
class AgentMeshConfig:
    home: Path
    control_host: str = "127.0.0.1"
    control_port: int = 8877
    p2p_host: str = "0.0.0.0"
    p2p_port: int = 4500
    advertise_host: str = "127.0.0.1"
    agent_label: str | None = None
    agent_description: str | None = None
    interests: list[str] | None = None
    discovery_host: str = "0.0.0.0"
    discovery_port: int = 45150
    discovery_broadcast_addr: str = "255.255.255.255"
    public_api_host: str = "0.0.0.0"
    public_api_port: int = 45200
    local_only: bool = False
    bootstrap_urls: list[str] | None = None
    relay_poll_interval_secs: int = 5
    announce_interval_secs: int = 30
    direct_connect_timeout_secs: int = 2
    peer_exchange_interval_secs: int = 45
    cooperate_enabled: bool = False
    executor_mode: str = "disabled"
    executor_url: str | None = None
    executor_model: str | None = None
    executor_api_key_env: str | None = None
    executor_timeout_secs: int = 25
    artifact_inline_limit_bytes: int = 128 * 1024

    @property
    def db_path(self) -> Path:
        return self.home / "state.db"

    @property
    def config_path(self) -> Path:
        return self.home / "config.json"

    @property
    def control_url(self) -> str:
        return f"http://{self.control_host}:{self.control_port}"

    @property
    def p2p_endpoint(self) -> str:
        return f"{self.advertise_host}:{self.p2p_port}"

    def persist(self) -> None:
        self.home.mkdir(parents=True, exist_ok=True)
        self.config_path.write_text(
            json.dumps(
                {
                    "control_host": self.control_host,
                    "control_port": self.control_port,
                    "p2p_host": self.p2p_host,
                    "p2p_port": self.p2p_port,
                    "advertise_host": self.advertise_host,
                    "agent_label": self.agent_label,
                    "agent_description": self.agent_description,
                    "interests": self.interests or [],
                    "discovery_host": self.discovery_host,
                    "discovery_port": self.discovery_port,
                    "discovery_broadcast_addr": self.discovery_broadcast_addr,
                    "public_api_host": self.public_api_host,
                    "public_api_port": self.public_api_port,
                    "local_only": self.local_only,
                    "bootstrap_urls": [] if self.local_only else (self.bootstrap_urls or DEFAULT_BOOTSTRAP_PEERS),
                    "relay_poll_interval_secs": self.relay_poll_interval_secs,
                    "announce_interval_secs": self.announce_interval_secs,
                    "direct_connect_timeout_secs": self.direct_connect_timeout_secs,
                    "peer_exchange_interval_secs": self.peer_exchange_interval_secs,
                    "cooperate_enabled": self.cooperate_enabled,
                    "executor_mode": self.executor_mode,
                    "executor_url": self.executor_url,
                    "executor_model": self.executor_model,
                    "executor_api_key_env": self.executor_api_key_env,
                    "executor_timeout_secs": self.executor_timeout_secs,
                    "artifact_inline_limit_bytes": self.artifact_inline_limit_bytes,
                },
                indent=2,
            )
        )


def default_home() -> Path:
    return Path(
        os.environ.get(
            "WILDMESH_HOME",
            os.environ.get("AGENTMESH_HOME", Path.home() / ".wildmesh"),
        )
    )


def load_config(home: Path | None = None) -> AgentMeshConfig:
    root = home or default_home()
    path = root / "config.json"
    if not path.exists():
        cfg = AgentMeshConfig(home=root, bootstrap_urls=DEFAULT_BOOTSTRAP_PEERS.copy(), interests=[])
        cfg = _apply_home_port_defaults(cfg)
        cfg.persist()
        return cfg
    raw = json.loads(path.read_text())
    raw.setdefault("local_only", False)
    if not raw.get("bootstrap_urls") and not raw.get("local_only"):
        env_value = os.environ.get("WILDMESH_BOOTSTRAP_URLS") or os.environ.get("AGENTMESH_BOOTSTRAP_URLS")
        if env_value:
            raw["bootstrap_urls"] = [item.strip() for item in env_value.split(",") if item.strip()]
    raw.setdefault("bootstrap_urls", [] if raw.get("local_only") else DEFAULT_BOOTSTRAP_PEERS.copy())
    if not raw.get("bootstrap_urls") and not raw.get("local_only"):
        raw["bootstrap_urls"] = DEFAULT_BOOTSTRAP_PEERS.copy()
    raw.setdefault("interests", [])
    raw.setdefault("cooperate_enabled", False)
    raw.setdefault("executor_mode", "disabled")
    raw.setdefault("executor_url", None)
    raw.setdefault("executor_model", None)
    raw.setdefault("executor_api_key_env", None)
    raw.setdefault("executor_timeout_secs", 25)
    raw.setdefault("artifact_inline_limit_bytes", 128 * 1024)
    cfg = AgentMeshConfig(home=root, **raw)
    return _apply_home_port_defaults(cfg)
