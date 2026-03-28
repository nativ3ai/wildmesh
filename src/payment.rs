use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::models::PaymentIdentity;

#[derive(Debug, Deserialize)]
struct WildaddyConfig {
    address: String,
    chain: Option<String>,
    network: Option<String>,
    #[serde(rename = "rpcUrl")]
    rpc_url: Option<String>,
    relay: Option<WildaddyRelayConfig>,
}

#[derive(Debug, Deserialize)]
struct WildaddyRelayConfig {
    path: Option<String>,
}

pub fn load_payment_identity() -> Option<PaymentIdentity> {
    resolve_wildaddy_home()
        .and_then(|home| load_payment_identity_from_home(&home).ok())
        .flatten()
}

pub fn load_payment_identity_from_home(home: &Path) -> Result<Option<PaymentIdentity>> {
    let config_path = home.join("config.json");
    if !config_path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&config_path)
        .with_context(|| format!("read {}", config_path.display()))?;
    let config: WildaddyConfig =
        serde_json::from_str(&raw).with_context(|| format!("parse {}", config_path.display()))?;
    let relay_path = config.relay.and_then(|value| value.path);
    let relay_installed = relay_path
        .as_ref()
        .is_some_and(|path| Path::new(path).exists());
    let mut settlement_rails = vec!["usdc".to_string()];
    if relay_installed {
        settlement_rails.push("cctp".to_string());
    }
    Ok(Some(PaymentIdentity {
        provider: "wildaddy".to_string(),
        kind: "evm_wallet".to_string(),
        address: config.address,
        chain: config.chain.unwrap_or_else(|| "base".to_string()),
        network: config.network.unwrap_or_else(|| "mainnet".to_string()),
        rpc_url: config.rpc_url,
        relay_installed,
        relay_path,
        settlement_rails,
    }))
}

fn resolve_wildaddy_home() -> Option<PathBuf> {
    if let Ok(value) = env::var("WILDADDY_HOME") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    dirs::home_dir().map(|home| home.join(".wildaddy"))
}

#[cfg(test)]
mod tests {
    use super::load_payment_identity_from_home;

    #[test]
    fn loads_payment_identity_from_wildaddy_config() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            tmp.path().join("config.json"),
            r#"{
  "address": "0xabc123",
  "chain": "base",
  "network": "mainnet",
  "rpcUrl": "https://rpc.example",
  "relay": {
    "path": "/does/not/exist"
  }
}"#,
        )
        .expect("write config");
        let identity = load_payment_identity_from_home(tmp.path())
            .expect("load config")
            .expect("payment identity");
        assert_eq!(identity.provider, "wildaddy");
        assert_eq!(identity.kind, "evm_wallet");
        assert_eq!(identity.address, "0xabc123");
        assert_eq!(identity.chain, "base");
        assert_eq!(identity.network, "mainnet");
        assert_eq!(identity.rpc_url.as_deref(), Some("https://rpc.example"));
        assert_eq!(identity.settlement_rails, vec!["usdc"]);
        assert!(!identity.relay_installed);
    }
}
