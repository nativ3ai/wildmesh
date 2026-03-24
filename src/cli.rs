use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::ArgAction;
use clap::{Parser, Subcommand};
use reqwest::blocking::Client;
use serde_json::{Value, json};
use tracing_subscriber::EnvFilter;

use crate::api;
use crate::config::AgentMeshConfig;
use crate::service::MeshService;

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
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        with_hermes: bool,
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        launch_agent: bool,
        #[arg(long, default_value_os_t = hermes_home())]
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
        #[arg(long, default_value_t = 30)]
        announce_interval_secs: u64,
        #[arg(long, default_value_t = 5)]
        relay_poll_interval_secs: u64,
        #[arg(long, default_value_t = 2)]
        direct_connect_timeout_secs: u64,
        #[arg(long, default_value_t = 45)]
        peer_exchange_interval_secs: u64,
    },
    Run {
        #[arg(long)]
        home: Option<PathBuf>,
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
    Subscribe {
        topic: String,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    Subscriptions {
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
            with_hermes,
            launch_agent,
            hermes_home,
        } => {
            let home = home.unwrap_or_else(AgentMeshConfig::home_dir);
            let mut config = if AgentMeshConfig::config_path(&home).exists() {
                AgentMeshConfig::load_or_create(&home)?
            } else {
                AgentMeshConfig::default()
            };
            config.control_port = control_port;
            config.p2p_port = p2p_port;
            config.advertise_host = advertise_host;
            if let Some(agent_label) = agent_label {
                config.agent_label = Some(agent_label);
            }
            if let Some(agent_description) = agent_description {
                config.agent_description = Some(agent_description);
            }
            if !interests.is_empty() {
                config.interests = interests;
            }
            if !bootstrap_urls.is_empty() {
                config.bootstrap_urls = bootstrap_urls;
            } else if config.bootstrap_urls.is_empty() {
                config.bootstrap_urls = AgentMeshConfig::default_bootstrap_urls();
            }
            config.persist(&home)?;
            let service = MeshService::bootstrap(&home, config.clone()).await?;
            if with_hermes {
                install_hermes_plugin(&asset_root(), &hermes_home)?;
            }
            let launch_agent_path = if launch_agent {
                Some(install_launch_agent(&home)?)
            } else {
                None
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "status": "ready",
                    "product": "WildMesh",
                    "home": home,
                    "profile": service.local_profile(),
                    "with_hermes": with_hermes,
                    "hermes_home": hermes_home,
                    "launch_agent": launch_agent_path,
                    "next": if launch_agent {
                        vec![
                            "wildmesh profile".to_string(),
                            "wildmesh browse".to_string(),
                            "wildmesh roam".to_string(),
                        ]
                    } else {
                        vec![
                            format!("wildmesh run --home {}", home.display()),
                            format!("wildmesh profile --home {}", home.display()),
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
            announce_interval_secs,
            relay_poll_interval_secs,
            direct_connect_timeout_secs,
            peer_exchange_interval_secs,
        } => {
            let home = home.unwrap_or_else(AgentMeshConfig::home_dir);
            let bootstrap_urls = if bootstrap_urls.is_empty() {
                AgentMeshConfig::default_bootstrap_urls()
            } else {
                bootstrap_urls
            };
            let config = AgentMeshConfig {
                control_host: "127.0.0.1".to_string(),
                control_port,
                p2p_host: "0.0.0.0".to_string(),
                p2p_port,
                advertise_host,
                agent_label,
                agent_description,
                interests,
                discovery_host,
                discovery_port,
                discovery_broadcast_addr,
                public_api_host,
                public_api_port,
                bootstrap_urls,
                announce_interval_secs,
                relay_poll_interval_secs,
                direct_connect_timeout_secs,
                peer_exchange_interval_secs,
                ..AgentMeshConfig::default()
            };
            let service = MeshService::bootstrap(&home, config).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&service.status().await?)?
            );
        }
        Commands::Run { home } => {
            let home = home.unwrap_or_else(AgentMeshConfig::home_dir);
            let config = AgentMeshConfig::load_or_create(&home)?;
            let service = MeshService::bootstrap(&home, config).await?;
            api::serve(service).await?;
        }
        Commands::Status { home } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&get_json(home, "/v1/status")?)?
            );
        }
        Commands::Profile { home, json } => {
            let home = home.unwrap_or_else(AgentMeshConfig::home_dir);
            let cfg = AgentMeshConfig::load_or_create(&home)?;
            let mut profile = json!({
                "agent_label": cfg.agent_label,
                "agent_description": cfg.agent_description,
                "interests": cfg.interests,
                "control_url": cfg.control_url(),
                "p2p_endpoint": cfg.p2p_endpoint(),
                "public_api_url": cfg.public_api_url(),
                "bootstrap_urls": cfg.bootstrap_urls,
                "nat_status": "unknown",
                "public_address": Value::Null,
                "listen_addrs": [],
                "external_addrs": [],
                "upnp_mapped_addrs": [],
            });
            if let Ok(status) = get_json(Some(home.clone()), "/v1/status") {
                if let Some(reachability) = status.get("reachability").cloned() {
                    if let Some(map) = profile.as_object_mut() {
                        map.insert("nat_status".to_string(), reachability.get("nat_status").cloned().unwrap_or(Value::String("unknown".to_string())));
                        map.insert("public_address".to_string(), reachability.get("public_address").cloned().unwrap_or(Value::Null));
                        map.insert("listen_addrs".to_string(), reachability.get("listen_addrs").cloned().unwrap_or_else(|| json!([])));
                        map.insert("external_addrs".to_string(), reachability.get("external_addrs").cloned().unwrap_or_else(|| json!([])));
                        map.insert("upnp_mapped_addrs".to_string(), reachability.get("upnp_mapped_addrs").cloned().unwrap_or_else(|| json!([])));
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
                    "bootstrap_urls": cfg.bootstrap_urls,
                }))
            );
            println!("profile updated; restart the daemon for outbound announcements to reflect changes");
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
                serde_json::to_string_pretty(&post_json(home, "/v1/peers", &payload)?)?
            );
        }
        Commands::Peers { home } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&get_json(home, "/v1/peers")?)?
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
        } => run_browse(home, interest, text, refresh, discovered_only, json, watch)?,
        Commands::Dashboard { home } => crate::dashboard::run(home)?,
        Commands::Roam {
            home,
            discovered_only,
        } => run_roam(home, discovered_only)?,
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
                serde_json::to_string_pretty(&post_json(
                    home,
                    "/v1/capabilities/grants",
                    &payload
                )?)?
            );
        }
        Commands::Subscribe { topic, home } => {
            let payload = json!({ "topic": topic });
            println!(
                "{}",
                serde_json::to_string_pretty(&post_json(home, "/v1/subscriptions", &payload)?)?
            );
        }
        Commands::Subscriptions { home } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&get_json(home, "/v1/subscriptions")?)?
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
                serde_json::to_string_pretty(&post_json(home, "/v1/messages/send", &payload)?)?
            );
        }
        Commands::Broadcast { topic, body, home } => {
            let payload = json!({
                "topic": topic,
                "body": serde_json::from_str::<Value>(&body).context("parse --body as JSON")?,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&post_json(
                    home,
                    "/v1/messages/broadcast",
                    &payload
                )?)?
            );
        }
        Commands::Inbox { limit, home } => {
            let url = format!("/v1/messages/inbox?limit={}", limit.clamp(1, 200));
            println!("{}", serde_json::to_string_pretty(&get_json(home, &url)?)?);
        }
        Commands::Outbox { limit, home } => {
            let url = format!("/v1/messages/outbox?limit={}", limit.clamp(1, 200));
            println!("{}", serde_json::to_string_pretty(&get_json(home, &url)?)?);
        }
        Commands::Sidecar { home } => run_sidecar(home)?,
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
            println!(
                "{}",
                serde_json::to_string_pretty(&post_json(
                    home,
                    "/v1/discovery/announce",
                    &json!({
                        "host": target_host,
                        "port": target_port,
                    }),
                )?)?
            );
        }
    }
    Ok(())
}

pub async fn main_sidecar_entry() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    run_sidecar(None)
}

fn map_kind(kind: &str) -> Result<&'static str> {
    match kind {
        "hello" => Ok("hello"),
        "broadcast" => Ok("broadcast"),
        "peer_exchange" => Ok("peer_exchange"),
        "task_offer" => Ok("task_offer"),
        "task_result" => Ok("task_result"),
        "note" => Ok("note"),
        "receipt" => Ok("receipt"),
        _ => bail!("unknown kind: {kind}"),
    }
}

fn base_url(home: Option<PathBuf>) -> Result<String> {
    let cfg = AgentMeshConfig::load_or_create(&home.unwrap_or_else(AgentMeshConfig::home_dir))?;
    Ok(cfg.control_url())
}

fn get_json(home: Option<PathBuf>, path: &str) -> Result<Value> {
    let client = Client::builder().build()?;
    Ok(client
        .get(format!("{}{}", base_url(home)?, path))
        .send()?
        .error_for_status()?
        .json()?)
}

fn post_json(home: Option<PathBuf>, path: &str, payload: &Value) -> Result<Value> {
    let client = Client::builder().build()?;
    Ok(client
        .post(format!("{}{}", base_url(home)?, path))
        .json(payload)
        .send()?
        .error_for_status()?
        .json()?)
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
    let nat_status = profile
        .get("nat_status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let public_address = profile
        .get("public_address")
        .and_then(Value::as_str)
        .unwrap_or("<none>");
    format!(
        "agent_label: {label}\nagent_description: {description}\ninterests: {interests}\ncontrol_url: {}\np2p_endpoint: {}\npublic_api_url: {}\nbootstrap_urls: {bootstrap_urls}\nnat_status: {nat_status}\npublic_address: {public_address}",
        profile.get("control_url").and_then(Value::as_str).unwrap_or("<unknown>"),
        profile.get("p2p_endpoint").and_then(Value::as_str).unwrap_or("<unknown>"),
        profile.get("public_api_url").and_then(Value::as_str).unwrap_or("<unknown>"),
    )
}

fn run_browse(
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
            let _ = post_json(home.clone(), "/v1/discovery/announce", &json!({}));
        }
        first_pass = false;
        let peers = get_json(home.clone(), "/v1/peers")?;
        let mut peers = peers.as_array().cloned().unwrap_or_default();
        peers.retain(|peer| matches_peer(peer, interest.as_deref(), text.as_deref(), discovered_only));
        if json_output {
            println!("{}", serde_json::to_string_pretty(&Value::Array(peers.clone()))?);
        } else {
            print!("\x1b[2J\x1b[H");
            println!("{}", render_peer_table(&peers));
        }
        if let Some(interval) = watch {
            std::thread::sleep(std::time::Duration::from_secs(interval.max(1)));
            continue;
        }
        break;
    }
    Ok(())
}

fn run_roam(home: Option<PathBuf>, discovered_only: bool) -> Result<()> {
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
        )?;
        refresh = false;
        println!();
        println!("commands: refresh | interest <term> | text <term> | clear | inspect <peer-prefix> | help | quit");
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
            let peers = get_json(home.clone(), "/v1/peers")?;
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

fn matches_peer(peer: &Value, interest: Option<&str>, text: Option<&str>, discovered_only: bool) -> bool {
    if discovered_only && !peer.get("discovered").and_then(Value::as_bool).unwrap_or(false) {
        return false;
    }
    if let Some(interest) = interest {
        let interest = interest.to_lowercase();
        let matches = peer
            .get("interests")
            .and_then(Value::as_array)
            .map(|values| {
                values.iter().filter_map(Value::as_str).any(|value| value.to_lowercase().contains(&interest))
            })
            .unwrap_or(false);
        if !matches {
            return false;
        }
    }
    if let Some(text) = text {
        let text = text.to_lowercase();
        let haystack = [
            peer.get("peer_id").and_then(Value::as_str).unwrap_or_default(),
            peer.get("label").and_then(Value::as_str).unwrap_or_default(),
            peer.get("agent_label").and_then(Value::as_str).unwrap_or_default(),
            peer.get("agent_description").and_then(Value::as_str).unwrap_or_default(),
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
            "{:<14}  {:<18}  {:<22}  {:<20}  {:<9}  {}",
            "peer", "agent", "interests", "endpoint", "relay", "description"
        ),
        format!(
            "{:-<14}  {:-<18}  {:-<22}  {:-<20}  {:-<9}  {:-<1}",
            "", "", "", "", "", ""
        ),
    ];
    for peer in peers {
        let peer_id = peer.get("peer_id").and_then(Value::as_str).unwrap_or_default();
        let short_peer = &peer_id[..peer_id.len().min(12)];
        let agent = peer
            .get("agent_label")
            .and_then(Value::as_str)
            .or_else(|| peer.get("label").and_then(Value::as_str))
            .unwrap_or("<unknown>");
        let interests = peer
            .get("interests")
            .and_then(Value::as_array)
            .map(|values| values.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(","))
            .filter(|joined| !joined.is_empty())
            .unwrap_or_else(|| "-".to_string());
        let endpoint = format!(
            "{}:{}",
            peer.get("host").and_then(Value::as_str).unwrap_or("?"),
            peer.get("port").and_then(Value::as_u64).unwrap_or(0)
        );
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
            "{:<14}  {:<18}  {:<22}  {:<20}  {:<9}  {}",
            short_peer,
            truncate(agent, 18),
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
    let mut truncated = value.chars().take(width.saturating_sub(1)).collect::<String>();
    truncated.push('…');
    truncated
}

fn run_sidecar(home: Option<PathBuf>) -> Result<()> {
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
            Some("status") => client
                .get(format!("{base}/v1/status"))
                .send()?
                .error_for_status()?
                .json::<Value>()?,
            Some("profile") => {
                let cfg = AgentMeshConfig::load_or_create(&config_home)?;
                let mut profile = json!({
                    "agent_label": cfg.agent_label,
                    "agent_description": cfg.agent_description,
                    "interests": cfg.interests,
                    "control_url": cfg.control_url(),
                    "p2p_endpoint": cfg.p2p_endpoint(),
                    "public_api_url": cfg.public_api_url(),
                    "bootstrap_urls": cfg.bootstrap_urls,
                    "nat_status": "unknown",
                    "public_address": Value::Null,
                    "listen_addrs": [],
                    "external_addrs": [],
                    "upnp_mapped_addrs": [],
                });
                if let Ok(status) = client
                    .get(format!("{base}/v1/status"))
                    .send()?
                    .error_for_status()?
                    .json::<Value>()
                {
                    if let Some(reachability) = status.get("reachability").cloned() {
                        if let Some(map) = profile.as_object_mut() {
                            map.insert("nat_status".to_string(), reachability.get("nat_status").cloned().unwrap_or(Value::String("unknown".to_string())));
                            map.insert("public_address".to_string(), reachability.get("public_address").cloned().unwrap_or(Value::Null));
                            map.insert("listen_addrs".to_string(), reachability.get("listen_addrs").cloned().unwrap_or_else(|| json!([])));
                            map.insert("external_addrs".to_string(), reachability.get("external_addrs").cloned().unwrap_or_else(|| json!([])));
                            map.insert("upnp_mapped_addrs".to_string(), reachability.get("upnp_mapped_addrs").cloned().unwrap_or_else(|| json!([])));
                        }
                    }
                }
                profile
            }
            Some("list_peers") => {
                json!({"items": client.get(format!("{base}/v1/peers")).send()?.error_for_status()?.json::<Value>()?})
            }
            Some("browse") => {
                client
                    .post(format!("{base}/v1/discovery/announce"))
                    .json(&json!({}))
                    .send()?
                    .error_for_status()?;
                let mut items = client
                    .get(format!("{base}/v1/peers"))
                    .send()?
                    .error_for_status()?
                    .json::<Vec<Value>>()?;
                if let Some(interest) = value.get("interest").and_then(Value::as_str) {
                    let interest = interest.to_lowercase();
                    items.retain(|peer| {
                        peer.get("interests")
                            .and_then(Value::as_array)
                            .map(|values| {
                                values.iter().filter_map(Value::as_str).any(|item| item.to_lowercase().contains(&interest))
                            })
                            .unwrap_or(false)
                    });
                }
                if let Some(text) = value.get("text").and_then(Value::as_str) {
                    let text = text.to_lowercase();
                    items.retain(|peer| {
                        [
                            peer.get("peer_id").and_then(Value::as_str).unwrap_or_default(),
                            peer.get("label").and_then(Value::as_str).unwrap_or_default(),
                            peer.get("agent_label").and_then(Value::as_str).unwrap_or_default(),
                            peer.get("agent_description").and_then(Value::as_str).unwrap_or_default(),
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
                    items.retain(|peer| peer.get("discovered").and_then(Value::as_bool).unwrap_or(false));
                }
                json!({"items": items})
            }
            Some("add_peer") => client
                .post(format!("{base}/v1/peers"))
                .json(&value["payload"])
                .send()?
                .error_for_status()?
                .json::<Value>()?,
            Some("grant") => client
                .post(format!("{base}/v1/capabilities/grants"))
                .json(&value["payload"])
                .send()?
                .error_for_status()?
                .json::<Value>()?,
            Some("subscribe") => client
                .post(format!("{base}/v1/subscriptions"))
                .json(&value["payload"])
                .send()?
                .error_for_status()?
                .json::<Value>()?,
            Some("list_subscriptions") => {
                json!({"items": client.get(format!("{base}/v1/subscriptions")).send()?.error_for_status()?.json::<Value>()?})
            }
            Some("send") => client
                .post(format!("{base}/v1/messages/send"))
                .json(&value["payload"])
                .send()?
                .error_for_status()?
                .json::<Value>()?,
            Some("broadcast") => client
                .post(format!("{base}/v1/messages/broadcast"))
                .json(&value["payload"])
                .send()?
                .error_for_status()?
                .json::<Value>()?,
            Some("inbox") => {
                json!({"items": client.get(format!("{base}/v1/messages/inbox?limit={}", value.get("limit").and_then(Value::as_i64).unwrap_or(50))).send()?.error_for_status()?.json::<Value>()?})
            }
            Some("outbox") => {
                json!({"items": client.get(format!("{base}/v1/messages/outbox?limit={}", value.get("limit").and_then(Value::as_i64).unwrap_or(50))).send()?.error_for_status()?.json::<Value>()?})
            }
            Some("discover_now") => client
                .post(format!("{base}/v1/discovery/announce"))
                .json(&value["payload"])
                .send()?
                .error_for_status()?
                .json::<Value>()?,
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
        asset_root.join("plugin.yaml"),
        plugin_target.join("plugin.yaml"),
    )
    .context("copy plugin.yaml")?;
    fs::copy(asset_root.join("plugin.py"), plugin_target.join("plugin.py"))
        .context("copy plugin.py")?;
    copy_dir_all(&asset_root.join("agentmesh"), &plugin_target.join("agentmesh"))?;
    copy_dir_all(&asset_root.join("skill").join("wildmesh"), &skill_target)?;
    println!("installed plugin -> {}", plugin_target.display());
    println!("installed skill -> {}", skill_target.display());
    Ok(())
}

fn install_launch_agent(home: &Path) -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let uid = current_uid()?;
        let launch_agents = dirs::home_dir()
            .context("resolve home directory")?
            .join("Library")
            .join("LaunchAgents");
        fs::create_dir_all(&launch_agents)?;
        let plist_path = launch_agents.join("com.nativ3ai.wildmesh.plist");
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
  <string>com.nativ3ai.wildmesh</string>
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
        let _ = run_launchctl(
            ["kickstart", "-k", &format!("{domain}/com.nativ3ai.wildmesh")],
            true,
        );
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
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("run id -u")?;
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
