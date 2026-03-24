use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const DEFAULT_BOOTSTRAP_PEERS: &[&str] = &[
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
    "/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentMeshConfig {
    pub control_host: String,
    pub control_port: u16,
    pub p2p_host: String,
    pub p2p_port: u16,
    pub advertise_host: String,
    pub agent_label: Option<String>,
    pub agent_description: Option<String>,
    pub interests: Vec<String>,
    pub discovery_host: String,
    pub discovery_port: u16,
    pub discovery_broadcast_addr: String,
    pub public_api_host: String,
    pub public_api_port: u16,
    pub bootstrap_urls: Vec<String>,
    pub relay_poll_interval_secs: u64,
    pub announce_interval_secs: u64,
    pub direct_connect_timeout_secs: u64,
    pub peer_exchange_interval_secs: u64,
}

impl Default for AgentMeshConfig {
    fn default() -> Self {
        Self {
            control_host: "127.0.0.1".to_string(),
            control_port: 8877,
            p2p_host: "0.0.0.0".to_string(),
            p2p_port: 4500,
            advertise_host: "127.0.0.1".to_string(),
            agent_label: None,
            agent_description: None,
            interests: Vec::new(),
            discovery_host: "0.0.0.0".to_string(),
            discovery_port: 45150,
            discovery_broadcast_addr: "255.255.255.255".to_string(),
            public_api_host: "0.0.0.0".to_string(),
            public_api_port: 45200,
            bootstrap_urls: Self::default_bootstrap_urls(),
            relay_poll_interval_secs: 5,
            announce_interval_secs: 30,
            direct_connect_timeout_secs: 2,
            peer_exchange_interval_secs: 45,
        }
    }
}

impl AgentMeshConfig {
    pub fn peer_active_window_secs(&self) -> u64 {
        (self.announce_interval_secs.max(30) * 2 + 15).max(75)
    }

    pub fn peer_visible_window_secs(&self) -> u64 {
        (self.announce_interval_secs.max(30) * 8).max(300)
    }

    pub fn default_bootstrap_urls() -> Vec<String> {
        for key in ["WILDMESH_BOOTSTRAP_URLS", "AGENTMESH_BOOTSTRAP_URLS"] {
            if let Ok(raw) = env::var(key) {
                let values = raw
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                if !values.is_empty() {
                    return values;
                }
            }
        }
        DEFAULT_BOOTSTRAP_PEERS
            .iter()
            .map(|value| (*value).to_string())
            .collect()
    }

    pub fn home_dir() -> PathBuf {
        for key in ["WILDMESH_HOME", "AGENTMESH_HOME"] {
            if let Ok(home) = env::var(key) {
                return PathBuf::from(home);
            }
        }
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".wildmesh")
    }

    pub fn control_url(&self) -> String {
        format!("http://{}:{}", self.control_host, self.control_port)
    }

    pub fn p2p_endpoint(&self) -> String {
        format!("{}:{}", self.advertise_host, self.p2p_port)
    }

    pub fn public_api_url(&self) -> String {
        format!("http://{}:{}", self.advertise_host, self.public_api_port)
    }

    pub fn config_path(home: &Path) -> PathBuf {
        home.join("config.json")
    }

    pub fn db_path(home: &Path) -> PathBuf {
        home.join("state.db")
    }

    pub fn load_or_create(home: &Path) -> Result<Self> {
        let path = Self::config_path(home);
        if !path.exists() {
            let cfg = Self::default();
            cfg.persist(home)?;
            return Ok(cfg);
        }
        let raw =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let mut cfg: Self =
            serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
        if cfg.bootstrap_urls.is_empty() {
            cfg.bootstrap_urls = Self::default_bootstrap_urls();
        }
        Ok(cfg)
    }

    pub fn persist(&self, home: &Path) -> Result<()> {
        std::fs::create_dir_all(home).with_context(|| format!("mkdir {}", home.display()))?;
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(Self::config_path(home), raw)?;
        Ok(())
    }
}
