from __future__ import annotations

import json

from agentmesh import client


def test_load_payment_identity_missing_config_returns_none(monkeypatch, tmp_path):
    monkeypatch.setenv("WILDADDY_HOME", str(tmp_path))
    assert client._load_payment_identity() is None


def test_load_payment_identity_reads_config(monkeypatch, tmp_path):
    monkeypatch.setenv("WILDADDY_HOME", str(tmp_path))
    cfg_path = tmp_path / "config.json"
    cfg_path.write_text(
        json.dumps(
            {
                "address": "0xabc",
                "chain": "base",
                "network": "mainnet",
                "relay": {"path": str(tmp_path / "relay")},
            }
        )
    )

    payload = client._load_payment_identity()
    assert payload is not None
    assert payload["provider"] == "wildaddy"
    assert payload["address"] == "0xabc"
    assert payload["relay_installed"] is False
    assert payload["settlement_rails"] == ["usdc"]
