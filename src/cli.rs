use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::ArgAction;
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde_json::{Value, json};
use tracing_subscriber::EnvFilter;

use crate::api;
use crate::config::AgentMeshConfig;
use crate::payment;
use crate::service::{MeshService, initialize_home};

#[derive(Debug, Parser)]
#[command(name = "wildmesh", version, about = "WildMesh CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Setup {
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long, default_value_t = 8877)]
        control_port: u16,
        #[arg(long, default_value_t = 4500)]
        p2p_port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        advertise_host: String,
        #[arg(long)]
        agent_label: Option<String>,
        #[arg(long)]
        agent_description: Option<String>,
        #[arg(long = "interest")]
        interests: Vec<String>,
        #[arg(long = "bootstrap-url")]
        bootstrap_urls: Vec<String>,
        #[arg(long, default_value_t = false, action = ArgAction::SetTrue)]
        local_only: bool,
        #[arg(long, default_value_t = false, action = ArgAction::SetTrue)]
        cooperate: bool,
        #[arg(long, default_value = "disabled")]
        executor_mode: String,
        #[arg(long)]
        executor_url: Option<String>,
        #[arg(long)]
        executor_model: Option<String>,
        #[arg(long)]
        executor_api_key_env: Option<String>,
        #[arg(long, hide = true, default_value_t = false, action = ArgAction::Set)]
        with_hermes: bool,
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        launch_agent: bool,
        #[arg(long, hide = true, default_value_os_t = hermes_home())]
        hermes_home: PathBuf,
    },
    Init {
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long, default_value_t = 8877)]
        control_port: u16,
        #[arg(long, default_value_t = 4500)]
        p2p_port: u16,
        #[arg(long, default_value_t = 45150)]
        discovery_port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        advertise_host: String,
        #[arg(long)]
        agent_label: Option<String>,
        #[arg(long)]
        agent_description: Option<String>,
        #[arg(long = "interest")]
        interests: Vec<String>,
        #[arg(long, default_value = "0.0.0.0")]
        discovery_host: String,
        #[arg(long, default_value = "255.255.255.255")]
        discovery_broadcast_addr: String,
        #[arg(long, default_value_t = 45200)]
        public_api_port: u16,
        #[arg(long, default_value = "0.0.0.0")]
        public_api_host: String,
        #[arg(long = "bootstrap-url")]
        bootstrap_urls: Vec<String>,
        #[arg(long, default_value_t = false, action = ArgAction::SetTrue)]
        local_only: bool,
        #[arg(long, default_value_t = 30)]
        announce_interval_secs: u64,
        #[arg(long, default_value_t = 5)]
        relay_poll_interval_secs: u64,
        #[arg(long, default_value_t = 2)]
        direct_connect_timeout_secs: u64,
        #[arg(long, default_value_t = 45)]
        peer_exchange_interval_secs: u64,
        #[arg(long, default_value_t = false, action = ArgAction::SetTrue)]
        cooperate: bool,
        #[arg(long, default_value = "disabled")]
        executor_mode: String,
        #[arg(long)]
        executor_url: Option<String>,
        #[arg(long)]
        executor_model: Option<String>,
        #[arg(long)]
        executor_api_key_env: Option<String>,
    },
    Run {
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long, default_value_t = false, action = ArgAction::SetTrue)]
        detach: bool,
    },
    Status {
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Profile {
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    SetProfile {
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long)]
        agent_label: Option<String>,
        #[arg(long)]
        agent_description: Option<String>,
        #[arg(long = "interest")]
        interests: Vec<String>,
    },
    AddPeer {
        peer_id: String,
        host: String,
        port: u16,
        public_key: String,
        encryption_public_key: String,
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        notes: Option<String>,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Peers {
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Browse {
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long)]
        interest: Option<String>,
        #[arg(long)]
        text: Option<String>,
        #[arg(long)]
        refresh: bool,
        #[arg(long)]
        discovered_only: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        watch: Option<u64>,
    },
    Dashboard {
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Roam {
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long)]
        discovered_only: bool,
    },
    Grant {
        peer_id: String,
        capability: String,
        #[arg(long)]
        expires_at: Option<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Grants {
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Revoke {
        peer_id: String,
        capability: String,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Subscribe {
        topic: String,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    CreateChannel {
        topic: String,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Cooperate {
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long)]
        enable: bool,
        #[arg(long)]
        disable: bool,
        #[arg(long)]
        executor_mode: Option<String>,
        #[arg(long)]
        executor_url: Option<String>,
        #[arg(long)]
        executor_model: Option<String>,
        #[arg(long)]
        executor_api_key_env: Option<String>,
    },
    Subscriptions {
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Channels {
        #[arg(long)]
        home: Option<PathBuf>,
    },
    ContextSend {
        peer_id: String,
        #[arg(long, default_value = "{}")]
        context: String,
        #[arg(long)]
        capability: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long = "tag")]
        tags: Vec<String>,
        #[arg(long)]
        ttl_secs: Option<u64>,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    ArtifactOffer {
        peer_id: String,
        path: PathBuf,
        #[arg(long)]
        capability: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        mime_type: Option<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    ArtifactFetch {
        peer_id: String,
        artifact_id: String,
        #[arg(long)]
        capability: Option<String>,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Artifacts {
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Delegate {
        peer_id: String,
        task_type: String,
        #[arg(long)]
        instruction: String,
        #[arg(long, default_value = "{}")]
        input: String,
        #[arg(long)]
        capability: Option<String>,
        #[arg(long)]
        context: Option<String>,
        #[arg(long)]
        max_output_chars: Option<usize>,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Pending {
        #[arg(long, default_value_t = 50)]
        limit: i64,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    AcceptRequest {
        message_id: String,
        #[arg(long)]
        always_allow: bool,
        #[arg(long)]
        grant_note: Option<String>,
        #[arg(long)]
        grant_capability: Option<String>,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    DenyRequest {
        message_id: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Send {
        peer_id: String,
        kind: String,
        #[arg(long, default_value = "{}")]
        body: String,
        #[arg(long)]
        capability: Option<String>,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Broadcast {
        topic: String,
        #[arg(long, default_value = "{}")]
        body: String,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Share {
        #[arg(long)]
        peer_id: Option<String>,
        #[arg(long)]
        topic: Option<String>,
        #[arg(long)]
        text: String,
        #[arg(long, default_value = "note")]
        kind: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        task_type: Option<String>,
        #[arg(long)]
        capability: Option<String>,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Inbox {
        #[arg(long, default_value_t = 50)]
        limit: i64,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Outbox {
        #[arg(long, default_value_t = 50)]
        limit: i64,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Sidecar {
        #[arg(long)]
        home: Option<PathBuf>,
    },
    InstallHermesPlugin {
        #[arg(long, default_value_os_t = asset_root())]
        asset_root: PathBuf,
        #[arg(long, default_value_os_t = hermes_home())]
        hermes_home: PathBuf,
    },
    DiscoverNow {
        #[arg(long)]
        target_host: Option<String>,
        #[arg(long)]
        target_port: Option<u16>,
        #[arg(long)]
        home: Option<PathBuf>,
    },
}

fn asset_root() -> PathBuf {
    if let Ok(path) = env::var("WILDMESH_ASSET_ROOT") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return candidate;
        }
    }

    let manifest_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if manifest_root.join("plugin.yaml").exists() {
        return manifest_root;
    }

    if let Ok(exe) = env::current_exe() {
        let mut candidates = Vec::new();
        candidates.push(exe.clone());
        if let Ok(canonical) = exe.canonicalize() {
            candidates.push(canonical);
        }
        for value in candidates {
            if let Some(bin_dir) = value.parent() {
                let share_dir = bin_dir
                    .parent()
                    .map(|prefix| prefix.join("share").join("wildmesh"));
                if let Some(candidate) = share_dir {
                    if candidate.join("plugin.yaml").exists() {
                        return candidate;
                    }
                }
            }
        }
    }

    manifest_root
}

fn hermes_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".hermes")
}

pub async fn main_entry() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    match cli.command {
        Commands::Setup {
            home,
            control_port,
            p2p_port,
            advertise_host,
            agent_label,
            agent_description,
            interests,
            bootstrap_urls,
            local_only,
            cooperate,
            executor_mode,
            executor_url,
            executor_model,
            executor_api_key_env,
            with_hermes,
            launch_agent,
            hermes_home,
        } => {
            let home = home.unwrap_or_else(AgentMeshConfig::home_dir);
            let config_exists = AgentMeshConfig::config_path(&home).exists();
            let mut config = if config_exists {
                AgentMeshConfig::load_or_create(&home)?
            } else {
                AgentMeshConfig::default()
            };
            if !config_exists || control_port != 8877 || p2p_port != 4500 {
                apply_home_port_defaults(&home, &mut config, control_port, p2p_port, None, None);
            }
            if !config_exists || advertise_host != "127.0.0.1" {
                config.advertise_host = advertise_host;
            }
            if let Some(agent_label) = agent_label {
                config.agent_label = Some(agent_label);
            }
            if let Some(agent_description) = agent_description {
                config.agent_description = Some(agent_description);
            }
            if !interests.is_empty() {
                config.interests = interests;
            }
            config.local_only = local_only;
            if local_only {
                config.bootstrap_urls = Vec::new();
            } else if !bootstrap_urls.is_empty() {
                config.bootstrap_urls = bootstrap_urls;
            } else if config.bootstrap_urls.is_empty() {
                config.bootstrap_urls = AgentMeshConfig::default_bootstrap_urls();
            }
            if cooperate {
                config.cooperate_enabled = true;
            } else if !config_exists {
                config.cooperate_enabled = false;
            }
            if executor_mode != "disabled" || !config_exists {
                config.executor_mode = executor_mode;
            }
            if executor_url.is_some() || !config_exists {
                config.executor_url = executor_url;
            }
            if executor_model.is_some() || !config_exists {
                config.executor_model = executor_model;
            }
            if executor_api_key_env.is_some() || !config_exists {
                config.executor_api_key_env = executor_api_key_env;
            }
            config.persist(&home)?;
            if with_hermes {
                install_hermes_plugin(&asset_root(), &hermes_home)?;
            }
            if launch_agent {
                stop_daemon_for_home(&home)?;
            }
            let profile = initialize_home(&home, &config).await?;
            let launch_agent_path = if launch_agent {
                Some(install_launch_agent(&home)?)
            } else {
                None
            };
            let daemon_ready = if launch_agent {
                let mut ready = wait_for_healthy_daemon(
                    &config.control_url(),
                    std::time::Duration::from_secs(6),
                )
                .await;
                if !ready {
                    let _ = spawn_detached_daemon(&home);
                    ready = wait_for_healthy_daemon(
                        &config.control_url(),
                        std::time::Duration::from_secs(4),
                    )
                    .await;
                }
                ready
            } else {
                false
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "status": "ready",
                    "product": "WildMesh",
                    "home": home,
                    "profile": profile,
                    "integration": {
                        "hermes_plugin_installed": with_hermes,
                        "hermes_home": hermes_home,
                        "hermes_next": if with_hermes {
                            Value::Null
                        } else {
                            Value::String(format!(
                                "wildmesh install-hermes-plugin --hermes-home {}",
                                hermes_home.display()
                            ))
                        }
                    },
                    "launch_agent": launch_agent_path,
                    "daemon_ready": daemon_ready,
                    "next": if launch_agent {
                        vec![
                            "wildmesh profile".to_string(),
                            "wildmesh dashboard".to_string(),
                            "wildmesh roam".to_string(),
                        ]
                    } else {
                        vec![
                            format!("wildmesh run --detach --home {}", home.display()),
                            format!("wildmesh profile --home {}", home.display()),
                            format!("wildmesh dashboard --home {}", home.display()),
                        ]
                    }
                }))?
            );
        }
        Commands::Init {
            home,
            control_port,
            p2p_port,
            discovery_port,
            advertise_host,
            agent_label,
            agent_description,
            interests,
            discovery_host,
            discovery_broadcast_addr,
            public_api_port,
            public_api_host,
            bootstrap_urls,
            local_only,
            announce_interval_secs,
            relay_poll_interval_secs,
            direct_connect_timeout_secs,
            peer_exchange_interval_secs,
            cooperate,
            executor_mode,
            executor_url,
            executor_model,
            executor_api_key_env,
        } => {
            let home = home.unwrap_or_else(AgentMeshConfig::home_dir);
            let bootstrap_urls = if local_only {
                Vec::new()
            } else if bootstrap_urls.is_empty() {
                AgentMeshConfig::default_bootstrap_urls()
            } else {
                bootstrap_urls
            };
            let config = AgentMeshConfig {
                control_host: "127.0.0.1".to_string(),
                control_port: 8877,
                p2p_host: "0.0.0.0".to_string(),
                p2p_port: 4500,
                advertise_host,
                agent_label,
                agent_description,
                interests,
                discovery_host,
                discovery_port: 45150,
                discovery_broadcast_addr,
                public_api_host,
                public_api_port: 45200,
                local_only,
                bootstrap_urls,
                announce_interval_secs,
                relay_poll_interval_secs,
                direct_connect_timeout_secs,
                peer_exchange_interval_secs,
                cooperate_enabled: cooperate,
                executor_mode,
                executor_url,
                executor_model,
                executor_api_key_env,
                ..AgentMeshConfig::default()
            };
            let mut config = config;
            apply_home_port_defaults(
                &home,
                &mut config,
                control_port,
                p2p_port,
                Some(discovery_port),
                Some(public_api_port),
            );
            let service = MeshService::bootstrap(&home, config).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&service.status().await?)?
            );
        }
        Commands::Run { home, detach } => {
            let home = home.unwrap_or_else(AgentMeshConfig::home_dir);
            if detach {
                let config = AgentMeshConfig::load_or_create(&home)?;
                if let Some(status) = fetch_daemon_status(&config.control_url()).await {
                    if mesh_worker_alive(&status) {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&json!({
                                "status": "running",
                                "home": home,
                                "control_url": config.control_url(),
                                "daemon_ready": true,
                            }))?
                        );
                        return Ok(());
                    }
                    stop_daemon_for_home(&home)?;
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
                spawn_detached_daemon(&home)?;
                let ready = wait_for_healthy_daemon(
                    &config.control_url(),
                    std::time::Duration::from_secs(6),
                )
                .await;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "status": if ready { "running" } else { "starting" },
                        "home": home,
                        "control_url": config.control_url(),
                        "daemon_ready": ready,
                    }))?
                );
            } else {
                let config = AgentMeshConfig::load_or_create(&home)?;
                let service = MeshService::bootstrap(&home, config).await?;
                api::serve(service).await?;
            }
        }
        Commands::Status { home } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&get_json(home, "/v1/status").await?)?
            );
        }
        Commands::Profile { home, json } => {
            let home = home.unwrap_or_else(AgentMeshConfig::home_dir);
            let cfg = AgentMeshConfig::load_or_create(&home)?;
            let mut profile = json!({
                "agent_label": cfg.agent_label,
                "agent_description": cfg.agent_description,
                "node_type": "agent",
                "runtime_name": "wildmesh",
                "interests": cfg.interests,
                "control_url": cfg.control_url(),
                "p2p_endpoint": cfg.p2p_endpoint(),
                "public_api_url": cfg.public_api_url(),
                "local_only": cfg.local_only,
                "network_scope": if cfg.local_only { "local_only" } else { "global" },
                "bootstrap_urls": cfg.bootstrap_urls,
                "payment_identity": payment::load_payment_identity(),
                "collaboration": {
                    "cooperate_enabled": cfg.cooperate_enabled,
                    "executor_mode": cfg.executor_mode,
                    "accepts_context_capsules": true,
                    "accepts_artifact_exchange": true,
                    "accepts_delegate_work": cfg.executor_mode != "disabled"
                },
                "nat_status": "unknown",
                "public_address": Value::Null,
                "listen_addrs": [],
                "external_addrs": [],
                "upnp_mapped_addrs": [],
            });
            if let Ok(status) = get_json(Some(home.clone()), "/v1/status").await {
                if let Some(reachability) = status.get("reachability").cloned() {
                    if let Some(map) = profile.as_object_mut() {
                        map.insert(
                            "nat_status".to_string(),
                            reachability
                                .get("nat_status")
                                .cloned()
                                .unwrap_or(Value::String("unknown".to_string())),
                        );
                        map.insert(
                            "public_address".to_string(),
                            reachability
                                .get("public_address")
                                .cloned()
                                .unwrap_or(Value::Null),
                        );
                        map.insert(
                            "listen_addrs".to_string(),
                            reachability
                                .get("listen_addrs")
                                .cloned()
                                .unwrap_or_else(|| json!([])),
                        );
                        map.insert(
                            "external_addrs".to_string(),
                            reachability
                                .get("external_addrs")
                                .cloned()
                                .unwrap_or_else(|| json!([])),
                        );
                        map.insert(
                            "upnp_mapped_addrs".to_string(),
                            reachability
                                .get("upnp_mapped_addrs")
                                .cloned()
                                .unwrap_or_else(|| json!([])),
                        );
                    }
                }
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&profile)?);
            } else {
                println!("{}", render_profile(&profile));
            }
        }
        Commands::SetProfile {
            home,
            agent_label,
            agent_description,
            interests,
        } => {
            let home = home.unwrap_or_else(AgentMeshConfig::home_dir);
            let mut cfg = AgentMeshConfig::load_or_create(&home)?;
            if agent_label.is_some() {
                cfg.agent_label = agent_label;
            }
            if agent_description.is_some() {
                cfg.agent_description = agent_description;
            }
            if !interests.is_empty() {
                cfg.interests = interests;
            }
            cfg.persist(&home)?;
            println!(
                "{}",
                render_profile(&json!({
                    "agent_label": cfg.agent_label,
                    "agent_description": cfg.agent_description,
                    "interests": cfg.interests,
                    "control_url": cfg.control_url(),
                    "p2p_endpoint": cfg.p2p_endpoint(),
                    "public_api_url": cfg.public_api_url(),
                    "local_only": cfg.local_only,
                    "network_scope": if cfg.local_only { "local_only" } else { "global" },
                    "bootstrap_urls": cfg.bootstrap_urls,
                }))
            );
            println!(
                "profile updated; restart the daemon for outbound announcements to reflect changes"
            );
        }
        Commands::AddPeer {
            peer_id,
            host,
            port,
            public_key,
            encryption_public_key,
            label,
            notes,
            home,
        } => {
            let payload = json!({
                "peer_id": peer_id,
                "host": host,
                "port": port,
                "public_key": public_key,
                "encryption_public_key": encryption_public_key,
                "label": label,
                "notes": notes,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&post_json(home, "/v1/peers", &payload).await?)?
            );
        }
        Commands::Peers { home } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&get_json(home, "/v1/peers").await?)?
            );
        }
        Commands::Browse {
            home,
            interest,
            text,
            refresh,
            discovered_only,
            json,
            watch,
        } => run_browse(home, interest, text, refresh, discovered_only, json, watch).await?,
        Commands::Dashboard { home } => {
            tokio::task::spawn_blocking(move || crate::dashboard::run(home)).await??
        }
        Commands::Roam {
            home,
            discovered_only,
        } => run_roam(home, discovered_only).await?,
        Commands::Grant {
            peer_id,
            capability,
            expires_at,
            note,
            home,
        } => {
            let payload = json!({
                "peer_id": peer_id,
                "capability": capability,
                "expires_at": expires_at,
                "note": note,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &post_json(home, "/v1/capabilities/grants", &payload).await?
                )?
            );
        }
        Commands::Grants { home } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&get_json(home, "/v1/capabilities").await?)?
            );
        }
        Commands::Revoke {
            peer_id,
            capability,
            home,
        } => {
            let payload = json!({
                "peer_id": peer_id,
                "capability": capability,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &post_json(home, "/v1/capabilities/revoke", &payload).await?
                )?
            );
        }
        Commands::Subscribe { topic, home } => {
            let payload = json!({ "topic": topic });
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &post_json(home, "/v1/subscriptions", &payload).await?
                )?
            );
        }
        Commands::CreateChannel { topic, home } => {
            let payload = json!({ "topic": topic });
            println!(
                "{}",
                serde_json::to_string_pretty(&post_json(home, "/v1/topics", &payload).await?)?
            );
        }
        Commands::Cooperate {
            home,
            enable,
            disable,
            executor_mode,
            executor_url,
            executor_model,
            executor_api_key_env,
        } => {
            if enable && disable {
                bail!("--enable and --disable are mutually exclusive");
            }
            let home = home.unwrap_or_else(AgentMeshConfig::home_dir);
            let mut cfg = AgentMeshConfig::load_or_create(&home)?;
            if enable {
                cfg.cooperate_enabled = true;
            }
            if disable {
                cfg.cooperate_enabled = false;
            }
            if let Some(value) = executor_mode {
                cfg.executor_mode = value;
            }
            if let Some(value) = executor_url {
                cfg.executor_url = Some(value);
            }
            if let Some(value) = executor_model {
                cfg.executor_model = Some(value);
            }
            if let Some(value) = executor_api_key_env {
                cfg.executor_api_key_env = Some(value);
            }
            cfg.persist(&home)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "home": home,
                    "cooperate_enabled": cfg.cooperate_enabled,
                    "executor_mode": cfg.executor_mode,
                    "executor_url": cfg.executor_url,
                    "executor_model": cfg.executor_model,
                    "executor_api_key_env": cfg.executor_api_key_env,
                    "note": "restart the daemon if it is already running",
                }))?
            );
        }
        Commands::Subscriptions { home } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&get_json(home, "/v1/subscriptions").await?)?
            );
        }
        Commands::Channels { home } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&get_json(home, "/v1/topics").await?)?
            );
        }
        Commands::ContextSend {
            peer_id,
            context,
            capability,
            title,
            tags,
            ttl_secs,
            home,
        } => {
            let payload = json!({
                "peer_id": peer_id,
                "capability": capability,
                "title": title,
                "tags": tags,
                "ttl_secs": ttl_secs,
                "context": serde_json::from_str::<Value>(&context).context("parse --context as JSON")?,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &post_json(home, "/v1/context/send", &payload).await?
                )?
            );
        }
        Commands::ArtifactOffer {
            peer_id,
            path,
            capability,
            name,
            mime_type,
            note,
            home,
        } => {
            let payload = json!({
                "peer_id": peer_id,
                "path": path,
                "capability": capability,
                "name": name,
                "mime_type": mime_type,
                "note": note,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &post_json(home, "/v1/artifacts/offer", &payload).await?
                )?
            );
        }
        Commands::ArtifactFetch {
            peer_id,
            artifact_id,
            capability,
            home,
        } => {
            let payload = json!({
                "peer_id": peer_id,
                "artifact_id": artifact_id,
                "capability": capability,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &post_json(home, "/v1/artifacts/fetch", &payload).await?
                )?
            );
        }
        Commands::Artifacts { home } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&get_json(home, "/v1/artifacts").await?)?
            );
        }
        Commands::Delegate {
            peer_id,
            task_type,
            instruction,
            input,
            capability,
            context,
            max_output_chars,
            home,
        } => {
            let payload = json!({
                "peer_id": peer_id,
                "task_type": task_type,
                "instruction": instruction,
                "input": serde_json::from_str::<Value>(&input).context("parse --input as JSON")?,
                "capability": capability,
                "context": match context {
                    Some(value) => Some(serde_json::from_str::<Value>(&value).context("parse --context as JSON")?),
                    None => None,
                },
                "max_output_chars": max_output_chars,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&post_json(home, "/v1/delegate", &payload).await?)?
            );
        }
        Commands::Pending { limit, home } => {
            let url = format!("/v1/delegate/pending?limit={}", limit.clamp(1, 200));
            println!(
                "{}",
                serde_json::to_string_pretty(&get_json(home, &url).await?)?
            );
        }
        Commands::AcceptRequest {
            message_id,
            always_allow,
            grant_note,
            grant_capability,
            home,
        } => {
            let payload = json!({
                "message_id": message_id,
                "always_allow": always_allow,
                "grant_note": grant_note,
                "grant_capability": grant_capability,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &post_json(home, "/v1/delegate/accept", &payload).await?
                )?
            );
        }
        Commands::DenyRequest {
            message_id,
            reason,
            home,
        } => {
            let payload = json!({ "message_id": message_id, "reason": reason });
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &post_json(home, "/v1/delegate/deny", &payload).await?
                )?
            );
        }
        Commands::Send {
            peer_id,
            kind,
            body,
            capability,
            home,
        } => {
            let kind_value = map_kind(&kind)?;
            let payload = json!({
                "peer_id": peer_id,
                "kind": kind_value,
                "body": serde_json::from_str::<Value>(&body).context("parse --body as JSON")?,
                "capability": capability,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &post_json(home, "/v1/messages/send", &payload).await?
                )?
            );
        }
        Commands::Broadcast { topic, body, home } => {
            let payload = json!({
                "topic": topic,
                "body": serde_json::from_str::<Value>(&body).context("parse --body as JSON")?,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &post_json(home, "/v1/messages/broadcast", &payload).await?
                )?
            );
        }
        Commands::Share {
            peer_id,
            topic,
            text,
            kind,
            title,
            task_type,
            capability,
            home,
        } => {
            if peer_id.is_some() == topic.is_some() {
                bail!("use exactly one destination: --peer-id <id> or --topic <name>");
            }
            let title_for_body = title.clone();
            let body = if kind == "task_offer" {
                json!({
                    "task_type": task_type.unwrap_or_else(|| "share".to_string()),
                    "instruction": text.clone(),
                    "input": {"text": text.clone()},
                    "title": title_for_body,
                })
            } else if kind == "context_capsule" {
                json!({
                    "title": title_for_body
                        .unwrap_or_else(|| "shared context".to_string()),
                    "context": {"text": text.clone()},
                })
            } else {
                json!({
                    "title": title_for_body,
                    "text": text.clone(),
                    "format": "plain_text",
                })
            };

            if let Some(peer_id) = peer_id {
                let payload = json!({
                    "peer_id": peer_id,
                    "kind": map_kind(&kind)?,
                    "body": body,
                    "capability": capability,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &post_json(home, "/v1/messages/send", &payload).await?
                    )?
                );
            } else if let Some(topic) = topic {
                let payload = json!({
                    "topic": topic,
                    "body": {
                        "kind": kind.clone(),
                        "title": title.clone(),
                        "text": text.clone(),
                        "format": "plain_text",
                    },
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &post_json(home, "/v1/messages/broadcast", &payload).await?
                    )?
                );
            }
        }
        Commands::Inbox { limit, home } => {
            let url = format!("/v1/messages/inbox?limit={}", limit.clamp(1, 200));
            println!(
                "{}",
                serde_json::to_string_pretty(&get_json(home, &url).await?)?
            );
        }
        Commands::Outbox { limit, home } => {
            let url = format!("/v1/messages/outbox?limit={}", limit.clamp(1, 200));
            println!(
                "{}",
                serde_json::to_string_pretty(&get_json(home, &url).await?)?
            );
        }
        Commands::Sidecar { home } => run_sidecar(home).await?,
        Commands::InstallHermesPlugin {
            asset_root,
            hermes_home,
        } => install_hermes_plugin(&asset_root, &hermes_home)?,
        Commands::DiscoverNow {
            target_host,
            target_port,
            home,
        } => {
            if target_host.is_some() ^ target_port.is_some() {
                bail!("--target-host and --target-port must be supplied together");
            }
            let payload = match (target_host, target_port) {
                (Some(host), Some(port)) => json!({
                    "host": host,
                    "port": port,
                }),
                (None, None) => json!({}),
                _ => unreachable!("validated host/port pairing above"),
            };
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &post_json(home, "/v1/discovery/announce", &payload).await?
                )?
            );
        }
    }
    Ok(())
}

pub async fn main_sidecar_entry() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    run_sidecar(None).await
}

fn map_kind(kind: &str) -> Result<&'static str> {
    match kind {
        "hello" => Ok("hello"),
        "broadcast" => Ok("broadcast"),
        "peer_exchange" => Ok("peer_exchange"),
        "task_offer" => Ok("task_offer"),
        "task_result" => Ok("task_result"),
        "context_capsule" => Ok("context_capsule"),
        "artifact_offer" => Ok("artifact_offer"),
        "artifact_fetch" => Ok("artifact_fetch"),
        "artifact_payload" => Ok("artifact_payload"),
        "delegate_request" => Ok("delegate_request"),
        "delegate_result" => Ok("delegate_result"),
        "note" => Ok("note"),
        "receipt" => Ok("receipt"),
        _ => bail!("unknown kind: {kind}"),
    }
}

fn base_url(home: Option<PathBuf>) -> Result<String> {
    let cfg = AgentMeshConfig::load_or_create(&home.unwrap_or_else(AgentMeshConfig::home_dir))?;
    Ok(cfg.control_url())
}

async fn get_json(home: Option<PathBuf>, path: &str) -> Result<Value> {
    let client = Client::builder().build()?;
    Ok(client
        .get(format!("{}{}", base_url(home)?, path))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

async fn fetch_daemon_status(control_url: &str) -> Option<Value> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .ok()?;
    client
        .get(format!("{control_url}/v1/status"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<Value>()
        .await
        .ok()
}

fn mesh_worker_alive(status: &Value) -> bool {
    status
        .get("reachability")
        .and_then(|value| value.get("mesh_worker_alive"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

async fn wait_for_healthy_daemon(control_url: &str, timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if let Some(status) = fetch_daemon_status(control_url).await {
            if mesh_worker_alive(&status) {
                return true;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    false
}

fn is_default_home(home: &Path) -> bool {
    let default = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".wildmesh");
    home == default || fs::canonicalize(home).ok() == fs::canonicalize(default).ok()
}

fn path_hash_suffix(path: &Path) -> u16 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    ((hasher.finish() % 700) as u16) + 1
}

fn apply_home_port_defaults(
    home: &Path,
    config: &mut AgentMeshConfig,
    control_port: u16,
    p2p_port: u16,
    discovery_port: Option<u16>,
    public_api_port: Option<u16>,
) {
    if is_default_home(home) {
        config.control_port = control_port;
        config.p2p_port = p2p_port;
        if let Some(value) = discovery_port {
            config.discovery_port = value;
        }
        if let Some(value) = public_api_port {
            config.public_api_port = value;
        }
        return;
    }

    let offset = path_hash_suffix(home);
    let using_default_pair = control_port == 8877 && p2p_port == 4500;
    config.control_port = if using_default_pair {
        8877 + offset
    } else {
        control_port
    };
    config.p2p_port = if using_default_pair {
        4500 + offset
    } else {
        p2p_port
    };

    match discovery_port {
        Some(value) => config.discovery_port = value,
        None if using_default_pair => config.discovery_port = 45150 + offset,
        None => {}
    }
    match public_api_port {
        Some(value) => config.public_api_port = value,
        None if using_default_pair => config.public_api_port = 45200 + offset,
        None => {}
    }
}

fn daemon_match_pattern(home: &Path) -> String {
    format!("wildmesh run --home {}", home.display())
}

fn stop_daemon_for_home(home: &Path) -> Result<()> {
    let output = Command::new("pgrep")
        .args(["-f", &daemon_match_pattern(home)])
        .output();
    let Ok(output) = output else {
        return Ok(());
    };
    if !output.status.success() {
        return Ok(());
    }
    let current_pid = std::process::id();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(pid) = line.trim().parse::<u32>() else {
            continue;
        };
        if pid == current_pid {
            continue;
        }
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }
    Ok(())
}

fn spawn_detached_daemon(home: &Path) -> Result<()> {
    let binary = env::current_exe()
        .context("resolve current executable")?
        .canonicalize()
        .context("canonicalize current executable")?;
    let log_path = home.join("wildmesh.log");
    let stdout = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .context("open daemon stdout log")?;
    let stderr = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .context("open daemon stderr log")?;
    Command::new(binary)
        .args(["run", "--home", home.to_string_lossy().as_ref()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(stdout))
        .stderr(std::process::Stdio::from(stderr))
        .spawn()
        .context("spawn detached daemon fallback")?;
    Ok(())
}

async fn post_json(home: Option<PathBuf>, path: &str, payload: &Value) -> Result<Value> {
    let client = Client::builder().build()?;
    Ok(client
        .post(format!("{}{}", base_url(home)?, path))
        .json(payload)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

fn render_profile(profile: &Value) -> String {
    let label = profile
        .get("agent_label")
        .and_then(Value::as_str)
        .unwrap_or("<unset>");
    let description = profile
        .get("agent_description")
        .and_then(Value::as_str)
        .unwrap_or("<unset>");
    let interests = profile
        .get("interests")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|joined| !joined.is_empty())
        .unwrap_or_else(|| "<none>".to_string());
    let bootstrap_urls = profile
        .get("bootstrap_urls")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|joined| !joined.is_empty())
        .unwrap_or_else(|| "<none>".to_string());
    let network_scope = profile
        .get("network_scope")
        .and_then(Value::as_str)
        .unwrap_or("global");
    let nat_status = profile
        .get("nat_status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let public_address = profile
        .get("public_address")
        .and_then(Value::as_str)
        .unwrap_or("<none>");
    let payment_identity = profile
        .get("payment_identity")
        .and_then(Value::as_object)
        .map(|value| {
            let address = value
                .get("address")
                .and_then(Value::as_str)
                .unwrap_or("<unknown>");
            let chain = value.get("chain").and_then(Value::as_str).unwrap_or("base");
            let network = value
                .get("network")
                .and_then(Value::as_str)
                .unwrap_or("mainnet");
            let rails = value
                .get("settlement_rails")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .filter(|joined| !joined.is_empty())
                .unwrap_or_else(|| "usdc".to_string());
            format!("{address} ({chain}/{network}; rails: {rails})")
        })
        .unwrap_or_else(|| "<none>".to_string());
    let collaboration = profile
        .get("collaboration")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let cooperate_enabled = collaboration
        .get("cooperate_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let executor_mode = collaboration
        .get("executor_mode")
        .and_then(Value::as_str)
        .unwrap_or("disabled");
    format!(
        "agent_label: {label}\nagent_description: {description}\ninterests: {interests}\nnetwork_scope: {network_scope}\ncontrol_url: {}\np2p_endpoint: {}\npublic_api_url: {}\nbootstrap_urls: {bootstrap_urls}\nnat_status: {nat_status}\npublic_address: {public_address}\npayment_identity: {payment_identity}\ncooperate_enabled: {cooperate_enabled}\nexecutor_mode: {executor_mode}",
        profile
            .get("control_url")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>"),
        profile
            .get("p2p_endpoint")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>"),
        profile
            .get("public_api_url")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>"),
    )
}

async fn run_browse(
    home: Option<PathBuf>,
    interest: Option<String>,
    text: Option<String>,
    refresh: bool,
    discovered_only: bool,
    json_output: bool,
    watch: Option<u64>,
) -> Result<()> {
    let mut first_pass = true;
    loop {
        if refresh || first_pass {
            let _ = post_json(home.clone(), "/v1/discovery/announce", &json!({})).await;
            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        }
        first_pass = false;
        let peers = get_json(home.clone(), "/v1/peers").await?;
        let mut peers = peers.as_array().cloned().unwrap_or_default();
        peers.retain(|peer| {
            matches_peer(peer, interest.as_deref(), text.as_deref(), discovered_only)
        });
        if json_output {
            println!(
                "{}",
                serde_json::to_string_pretty(&Value::Array(peers.clone()))?
            );
        } else {
            print!("\x1b[2J\x1b[H");
            println!("{}", render_peer_table(&peers));
        }
        if let Some(interval) = watch {
            tokio::time::sleep(std::time::Duration::from_secs(interval.max(1))).await;
            continue;
        }
        break;
    }
    Ok(())
}

async fn run_roam(home: Option<PathBuf>, discovered_only: bool) -> Result<()> {
    let mut interest: Option<String> = None;
    let mut text: Option<String> = None;
    let mut refresh = true;
    loop {
        run_browse(
            home.clone(),
            interest.clone(),
            text.clone(),
            refresh,
            discovered_only,
            false,
            None,
        )
        .await?;
        refresh = false;
        println!();
        println!(
            "commands: refresh | interest <term> | text <term> | clear | inspect <peer-prefix> | help | quit"
        );
        print!("wildmesh> ");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if matches!(line, "q" | "quit" | "exit") {
            break;
        }
        if matches!(line, "h" | "help") {
            println!("refresh: announce and rediscover peers");
            println!("interest <term>: filter peers by interest label");
            println!("text <term>: filter peers by free text");
            println!("clear: remove active filters");
            println!("inspect <peer-prefix>: print the full stored peer record");
            println!("quit: exit roam mode");
            continue;
        }
        if matches!(line, "r" | "refresh") {
            refresh = true;
            continue;
        }
        if line == "clear" {
            interest = None;
            text = None;
            continue;
        }
        if let Some(value) = line.strip_prefix("interest ") {
            interest = Some(value.trim().to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("text ") {
            text = Some(value.trim().to_string());
            continue;
        }
        if let Some(prefix) = line.strip_prefix("inspect ") {
            let peers = get_json(home.clone(), "/v1/peers").await?;
            let Some(peer) = peers
                .as_array()
                .and_then(|items| {
                    items.iter().find(|peer| {
                        peer.get("peer_id")
                            .and_then(Value::as_str)
                            .map(|peer_id| peer_id.starts_with(prefix.trim()))
                            .unwrap_or(false)
                    })
                })
                .cloned()
            else {
                println!("no peer matched prefix {}", prefix.trim());
                continue;
            };
            println!("{}", serde_json::to_string_pretty(&peer)?);
            continue;
        }
        println!("unknown command: {line}");
    }
    Ok(())
}

fn matches_peer(
    peer: &Value,
    interest: Option<&str>,
    text: Option<&str>,
    discovered_only: bool,
) -> bool {
    if discovered_only
        && !peer
            .get("discovered")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return false;
    }
    if let Some(interest) = interest {
        let interest = interest.to_lowercase();
        let matches = peer
            .get("interests")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|value| value.to_lowercase().contains(&interest))
            })
            .unwrap_or(false);
        if !matches {
            return false;
        }
    }
    if let Some(text) = text {
        let text = text.to_lowercase();
        let haystack = [
            peer.get("peer_id")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            peer.get("label")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            peer.get("agent_label")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            peer.get("agent_description")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            peer.get("host").and_then(Value::as_str).unwrap_or_default(),
        ]
        .join(" ")
        .to_lowercase();
        if !haystack.contains(&text) {
            return false;
        }
    }
    true
}

fn render_peer_table(peers: &[Value]) -> String {
    if peers.is_empty() {
        return "No agents matched the current filter.".to_string();
    }
    let mut lines = vec![
        format!(
            "{:<14}  {:<18}  {:<10}  {:<22}  {:<20}  {:<9}  {}",
            "peer", "agent", "state", "interests", "endpoint", "relay", "description"
        ),
        format!(
            "{:-<14}  {:-<18}  {:-<10}  {:-<22}  {:-<20}  {:-<9}  {:-<1}",
            "", "", "", "", "", "", ""
        ),
    ];
    for peer in peers {
        let peer_id = peer
            .get("peer_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let short_peer = &peer_id[..peer_id.len().min(12)];
        let agent = peer
            .get("agent_label")
            .and_then(Value::as_str)
            .or_else(|| peer.get("label").and_then(Value::as_str))
            .unwrap_or("<unknown>");
        let interests = peer
            .get("interests")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .filter(|joined| !joined.is_empty())
            .unwrap_or_else(|| "-".to_string());
        let endpoint = format!(
            "{}:{}",
            peer.get("host").and_then(Value::as_str).unwrap_or("?"),
            peer.get("port").and_then(Value::as_u64).unwrap_or(0)
        );
        let state = peer
            .get("activity_state")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let relay = if peer.get("relay_url").and_then(Value::as_str).is_some() {
            "hub"
        } else {
            "direct"
        };
        let description = peer
            .get("agent_description")
            .and_then(Value::as_str)
            .unwrap_or("-");
        lines.push(format!(
            "{:<14}  {:<18}  {:<10}  {:<22}  {:<20}  {:<9}  {}",
            short_peer,
            truncate(agent, 18),
            truncate(state, 10),
            truncate(&interests, 22),
            truncate(&endpoint, 20),
            relay,
            description
        ));
    }
    lines.join("\n")
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    let mut truncated = value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

async fn run_sidecar(home: Option<PathBuf>) -> Result<()> {
    let client = Client::builder().build()?;
    let config_home = home.unwrap_or_else(AgentMeshConfig::home_dir);
    let base = base_url(Some(config_home.clone()))?;
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(err) => {
                writeln!(
                    stdout,
                    "{}",
                    serde_json::to_string(&json!({"error": err.to_string()}))?
                )?;
                stdout.flush()?;
                continue;
            }
        };
        let response = match value.get("op").and_then(Value::as_str) {
            Some("status") => {
                client
                    .get(format!("{base}/v1/status"))
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?
            }
            Some("profile") => {
                let cfg = AgentMeshConfig::load_or_create(&config_home)?;
                let mut profile = json!({
                    "agent_label": cfg.agent_label,
                    "agent_description": cfg.agent_description,
                    "node_type": "agent",
                    "runtime_name": "wildmesh",
                    "interests": cfg.interests,
                    "control_url": cfg.control_url(),
                    "p2p_endpoint": cfg.p2p_endpoint(),
                    "public_api_url": cfg.public_api_url(),
                    "local_only": cfg.local_only,
                    "network_scope": if cfg.local_only { "local_only" } else { "global" },
                    "bootstrap_urls": cfg.bootstrap_urls,
                    "payment_identity": payment::load_payment_identity(),
                    "collaboration": {
                        "cooperate_enabled": cfg.cooperate_enabled,
                        "executor_mode": cfg.executor_mode,
                        "accepts_context_capsules": true,
                        "accepts_artifact_exchange": true,
                        "accepts_delegate_work": cfg.executor_mode != "disabled"
                    },
                    "nat_status": "unknown",
                    "public_address": Value::Null,
                    "listen_addrs": [],
                    "external_addrs": [],
                    "upnp_mapped_addrs": [],
                });
                if let Ok(status) = client
                    .get(format!("{base}/v1/status"))
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await
                {
                    if let Some(reachability) = status.get("reachability").cloned() {
                        if let Some(map) = profile.as_object_mut() {
                            map.insert(
                                "nat_status".to_string(),
                                reachability
                                    .get("nat_status")
                                    .cloned()
                                    .unwrap_or(Value::String("unknown".to_string())),
                            );
                            map.insert(
                                "public_address".to_string(),
                                reachability
                                    .get("public_address")
                                    .cloned()
                                    .unwrap_or(Value::Null),
                            );
                            map.insert(
                                "listen_addrs".to_string(),
                                reachability
                                    .get("listen_addrs")
                                    .cloned()
                                    .unwrap_or_else(|| json!([])),
                            );
                            map.insert(
                                "external_addrs".to_string(),
                                reachability
                                    .get("external_addrs")
                                    .cloned()
                                    .unwrap_or_else(|| json!([])),
                            );
                            map.insert(
                                "upnp_mapped_addrs".to_string(),
                                reachability
                                    .get("upnp_mapped_addrs")
                                    .cloned()
                                    .unwrap_or_else(|| json!([])),
                            );
                        }
                    }
                }
                profile
            }
            Some("list_peers") => {
                json!({"items": client.get(format!("{base}/v1/peers")).send().await?.error_for_status()?.json::<Value>().await?})
            }
            Some("browse") => {
                client
                    .post(format!("{base}/v1/discovery/announce"))
                    .json(&json!({}))
                    .send()
                    .await?
                    .error_for_status()?;
                let mut items = client
                    .get(format!("{base}/v1/peers"))
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Vec<Value>>()
                    .await?;
                if let Some(interest) = value.get("interest").and_then(Value::as_str) {
                    let interest = interest.to_lowercase();
                    items.retain(|peer| {
                        peer.get("interests")
                            .and_then(Value::as_array)
                            .map(|values| {
                                values
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .any(|item| item.to_lowercase().contains(&interest))
                            })
                            .unwrap_or(false)
                    });
                }
                if let Some(text) = value.get("text").and_then(Value::as_str) {
                    let text = text.to_lowercase();
                    items.retain(|peer| {
                        [
                            peer.get("peer_id")
                                .and_then(Value::as_str)
                                .unwrap_or_default(),
                            peer.get("label")
                                .and_then(Value::as_str)
                                .unwrap_or_default(),
                            peer.get("agent_label")
                                .and_then(Value::as_str)
                                .unwrap_or_default(),
                            peer.get("agent_description")
                                .and_then(Value::as_str)
                                .unwrap_or_default(),
                            peer.get("host").and_then(Value::as_str).unwrap_or_default(),
                        ]
                        .join(" ")
                        .to_lowercase()
                        .contains(&text)
                    });
                }
                if value
                    .get("discovered_only")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    items.retain(|peer| {
                        peer.get("discovered")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    });
                }
                json!({"items": items})
            }
            Some("add_peer") => {
                client
                    .post(format!("{base}/v1/peers"))
                    .json(&value["payload"])
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?
            }
            Some("grant") => {
                client
                    .post(format!("{base}/v1/capabilities/grants"))
                    .json(&value["payload"])
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?
            }
            Some("subscribe") => {
                client
                    .post(format!("{base}/v1/subscriptions"))
                    .json(&value["payload"])
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?
            }
            Some("create_channel") => {
                client
                    .post(format!("{base}/v1/topics"))
                    .json(&value["payload"])
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?
            }
            Some("list_subscriptions") => {
                json!({"items": client.get(format!("{base}/v1/subscriptions")).send().await?.error_for_status()?.json::<Value>().await?})
            }
            Some("list_channels") => {
                json!({"items": client.get(format!("{base}/v1/topics")).send().await?.error_for_status()?.json::<Value>().await?})
            }
            Some("send_context") => {
                client
                    .post(format!("{base}/v1/context/send"))
                    .json(&value["payload"])
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?
            }
            Some("list_artifacts") => {
                json!({"items": client.get(format!("{base}/v1/artifacts")).send().await?.error_for_status()?.json::<Value>().await?})
            }
            Some("offer_artifact") => {
                client
                    .post(format!("{base}/v1/artifacts/offer"))
                    .json(&value["payload"])
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?
            }
            Some("fetch_artifact") => {
                client
                    .post(format!("{base}/v1/artifacts/fetch"))
                    .json(&value["payload"])
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?
            }
            Some("delegate") => {
                client
                    .post(format!("{base}/v1/delegate"))
                    .json(&value["payload"])
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?
            }
            Some("pending") => {
                json!({"items": client.get(format!("{base}/v1/delegate/pending?limit={}", value.get("limit").and_then(Value::as_i64).unwrap_or(50))).send().await?.error_for_status()?.json::<Value>().await?})
            }
            Some("accept_request") => {
                client
                    .post(format!("{base}/v1/delegate/accept"))
                    .json(&value["payload"])
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?
            }
            Some("deny_request") => {
                client
                    .post(format!("{base}/v1/delegate/deny"))
                    .json(&value["payload"])
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?
            }
            Some("send") => {
                client
                    .post(format!("{base}/v1/messages/send"))
                    .json(&value["payload"])
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?
            }
            Some("broadcast") => {
                client
                    .post(format!("{base}/v1/messages/broadcast"))
                    .json(&value["payload"])
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?
            }
            Some("share") => {
                let peer_id = value.get("peer_id").and_then(Value::as_str).map(str::to_string);
                let topic = value.get("topic").and_then(Value::as_str).map(str::to_string);
                if peer_id.is_some() == topic.is_some() {
                    json!({"error":"use exactly one destination: peer_id or topic"})
                } else {
                    let text = value
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let kind = value
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or("note")
                        .to_string();
                    let title = value.get("title").and_then(Value::as_str).map(str::to_string);
                    let capability = value
                        .get("capability")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let task_type = value
                        .get("task_type")
                        .and_then(Value::as_str)
                        .map(str::to_string);

                    let title_for_body = title.clone();
                    let body = if kind == "task_offer" {
                        json!({
                            "task_type": task_type.unwrap_or_else(|| "share".to_string()),
                            "instruction": text.clone(),
                            "input": {"text": text.clone()},
                            "title": title_for_body,
                        })
                    } else if kind == "context_capsule" {
                        json!({
                            "title": title_for_body
                                .unwrap_or_else(|| "shared context".to_string()),
                            "context": {"text": text.clone()},
                        })
                    } else {
                        json!({
                            "title": title_for_body,
                            "text": text.clone(),
                            "format": "plain_text",
                        })
                    };

                    if let Some(peer_id) = peer_id {
                        let payload = json!({
                            "peer_id": peer_id,
                            "kind": map_kind(&kind)?,
                            "body": body,
                            "capability": capability,
                        });
                        client
                            .post(format!("{base}/v1/messages/send"))
                            .json(&payload)
                            .send()
                            .await?
                            .error_for_status()?
                            .json::<Value>()
                            .await?
                    } else if let Some(topic) = topic {
                        let payload = json!({
                            "topic": topic,
                            "body": {
                                "kind": kind.clone(),
                                "title": title.clone(),
                                "text": text.clone(),
                                "format": "plain_text",
                            },
                        });
                        client
                            .post(format!("{base}/v1/messages/broadcast"))
                            .json(&payload)
                            .send()
                            .await?
                            .error_for_status()?
                            .json::<Value>()
                            .await?
                    } else {
                        json!({"error":"missing destination"})
                    }
                }
            }
            Some("inbox") => {
                json!({"items": client.get(format!("{base}/v1/messages/inbox?limit={}", value.get("limit").and_then(Value::as_i64).unwrap_or(50))).send().await?.error_for_status()?.json::<Value>().await?})
            }
            Some("outbox") => {
                json!({"items": client.get(format!("{base}/v1/messages/outbox?limit={}", value.get("limit").and_then(Value::as_i64).unwrap_or(50))).send().await?.error_for_status()?.json::<Value>().await?})
            }
            Some("discover_now") => {
                let payload = value
                    .get("payload")
                    .cloned()
                    .filter(|item| !item.is_null())
                    .unwrap_or_else(|| json!({}));
                client
                    .post(format!("{base}/v1/discovery/announce"))
                    .json(&payload)
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?
            }
            Some(other) => json!({"error": format!("unknown op: {other}")}),
            None => json!({"error": "missing op"}),
        };
        writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
        stdout.flush()?;
    }
    Ok(())
}

fn install_hermes_plugin(asset_root: &Path, hermes_home: &Path) -> Result<()> {
    let plugin_target = hermes_home.join("plugins").join("wildmesh");
    let skill_target = hermes_home
        .join("skills")
        .join("networking")
        .join("wildmesh");
    fs::create_dir_all(plugin_target.parent().context("plugin parent")?)?;
    fs::create_dir_all(skill_target.parent().context("skill parent")?)?;
    if plugin_target.exists() {
        remove_path(&plugin_target)?;
    }
    if skill_target.exists() {
        remove_path(&skill_target)?;
    }
    fs::create_dir_all(&plugin_target)?;
    fs::copy(
        asset_root.join("__init__.py"),
        plugin_target.join("__init__.py"),
    )
    .context("copy plugin __init__.py")?;
    fs::copy(
        asset_root.join("plugin.yaml"),
        plugin_target.join("plugin.yaml"),
    )
    .context("copy plugin.yaml")?;
    fs::copy(
        asset_root.join("plugin.py"),
        plugin_target.join("plugin.py"),
    )
    .context("copy plugin.py")?;
    copy_dir_all(
        &asset_root.join("agentmesh"),
        &plugin_target.join("agentmesh"),
    )?;
    copy_dir_all(&asset_root.join("skill").join("wildmesh"), &skill_target)?;
    println!("installed plugin -> {}", plugin_target.display());
    println!("installed skill -> {}", skill_target.display());
    println!("next -> restart Hermes if it is already running");
    println!(
        "inside Hermes -> ask it to run `Use WildMesh to set up the local node.`"
    );
    Ok(())
}

#[cfg(test)]
mod cli_tests {
    use super::install_hermes_plugin;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn install_hermes_plugin_copies_directory_plugin_layout() {
        let assets = tempdir().expect("asset dir");
        let hermes_home = tempdir().expect("hermes home");
        let root = assets.path();

        fs::write(root.join("__init__.py"), "from .plugin import register\n")
            .expect("write root init");
        fs::write(
            root.join("plugin.py"),
            "from .agentmesh.plugin import register\n",
        )
        .expect("write root plugin");
        fs::write(root.join("plugin.yaml"), "name: wildmesh\n").expect("write manifest");

        let package_dir = root.join("agentmesh");
        let hermes_plugin_dir = package_dir.join("hermes_plugin");
        fs::create_dir_all(&hermes_plugin_dir).expect("create agentmesh tree");
        fs::write(
            package_dir.join("__init__.py"),
            "from .hermes_plugin.plugin import register\n",
        )
        .expect("write package init");
        fs::write(
            package_dir.join("plugin.py"),
            "from .hermes_plugin.plugin import register\n",
        )
        .expect("write package plugin");
        fs::write(
            hermes_plugin_dir.join("__init__.py"),
            "from .plugin import register\n",
        )
        .expect("write hermes plugin init");
        fs::write(
            hermes_plugin_dir.join("plugin.py"),
            "def register(ctx):\n    return None\n",
        )
        .expect("write hermes plugin register");

        let skill_dir = root.join("skill").join("wildmesh");
        fs::create_dir_all(&skill_dir).expect("create skill tree");
        fs::write(skill_dir.join("SKILL.md"), "# WildMesh\n").expect("write skill");

        install_hermes_plugin(root, hermes_home.path()).expect("install plugin");

        let plugin_root = hermes_home.path().join("plugins").join("wildmesh");
        assert!(plugin_root.join("__init__.py").is_file());
        assert!(plugin_root.join("plugin.py").is_file());
        assert!(plugin_root.join("plugin.yaml").is_file());
        assert!(plugin_root.join("agentmesh").join("__init__.py").is_file());
        assert!(
            plugin_root
                .join("agentmesh")
                .join("hermes_plugin")
                .join("__init__.py")
                .is_file()
        );
        assert!(
            hermes_home
                .path()
                .join("skills")
                .join("networking")
                .join("wildmesh")
                .join("SKILL.md")
                .is_file()
        );
    }
}

fn install_launch_agent(home: &Path) -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let uid = current_uid()?;
        let label = if is_default_home(home) {
            "com.nativ3ai.wildmesh".to_string()
        } else {
            format!("com.nativ3ai.wildmesh.{}", path_hash_suffix(home))
        };
        let launch_agents = dirs::home_dir()
            .context("resolve home directory")?
            .join("Library")
            .join("LaunchAgents");
        fs::create_dir_all(&launch_agents)?;
        let plist_path = launch_agents.join(format!("{label}.plist"));
        let binary = env::current_exe()
            .context("resolve current executable")?
            .canonicalize()
            .context("canonicalize current executable")?;
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>run</string>
    <string>--home</string>
    <string>{}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>WorkingDirectory</key>
  <string>{}</string>
  <key>StandardOutPath</key>
  <string>{}</string>
  <key>StandardErrorPath</key>
  <string>{}</string>
</dict>
</plist>
"#,
            xml_escape(&label),
            xml_escape(&binary.display().to_string()),
            xml_escape(&home.display().to_string()),
            xml_escape(&home.display().to_string()),
            xml_escape(&home.join("wildmesh.log").display().to_string()),
            xml_escape(&home.join("wildmesh.log").display().to_string()),
        );
        fs::write(&plist_path, plist)?;
        let domain = format!("gui/{uid}");
        let _ = run_launchctl(
            ["bootout", &domain, plist_path.to_string_lossy().as_ref()],
            true,
        );
        run_launchctl(
            ["bootstrap", &domain, plist_path.to_string_lossy().as_ref()],
            false,
        )
        .context("run launchctl bootstrap")?;
        let _ = run_launchctl(["kickstart", "-k", &format!("{domain}/{label}")], true);
        Ok(plist_path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = home;
        bail!("launch-agent setup is only supported on macOS right now")
    }
}

fn run_launchctl<const N: usize>(args: [&str; N], tolerate_failure: bool) -> Result<()> {
    let output = Command::new("launchctl")
        .args(args)
        .output()
        .context("run launchctl")?;
    if output.status.success() || tolerate_failure {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stderr.is_empty() {
        bail!("launchctl failed: {stderr}");
    }
    if !stdout.is_empty() {
        bail!("launchctl failed: {stdout}");
    }
    bail!("launchctl failed with status {}", output.status);
}

fn current_uid() -> Result<u32> {
    let output = Command::new("id").arg("-u").output().context("run id -u")?;
    if !output.status.success() {
        bail!("id -u failed");
    }
    let raw = String::from_utf8(output.stdout)?.trim().to_string();
    Ok(raw.parse()?)
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
        .replace('\'', "&apos;")
}

fn remove_path(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path)?;
    } else {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "__pycache__" || name.ends_with(".pyc") {
            continue;
        }
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
