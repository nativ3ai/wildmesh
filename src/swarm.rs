use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use chrono::Utc;
use libp2p::autonat;
use libp2p::futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic, MessageAuthenticity, ValidationMode};
use libp2p::identify;
use libp2p::kad::{self, store::MemoryStore};
use libp2p::mdns;
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, OutboundRequestId, ProtocolSupport};
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::upnp;
use libp2p::{Multiaddr, PeerId, StreamProtocol, SwarmBuilder, identity, ping};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Digest;
use sqlx::SqlitePool;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use crate::artifact;
use crate::config::AgentMeshConfig;
use crate::crypto::IdentityMaterial;
use crate::executor;
use crate::models::{
    ArtifactFetchBody, ArtifactPayloadBody, BroadcastDelivery, BroadcastRequest, BroadcastResponse,
    DelegateDecisionResponse, DelegateRequestBody, DelegateResultBody, Envelope, LocalProfile,
    MeshDirectRequest, MeshDirectResponse, MeshProfileRecord, MeshPubsubMessage,
    MessageDirection, MessageKind, MessageStatus, PeerRecord, ReachabilityView,
    SendMessageResponse, StoredMessage,
    SubscriptionRecord,
};
use crate::storage;

const PROFILE_TOPIC: &str = "agentmesh.profile.v1";
const GLOBAL_DISCOVERY_KEY: &[u8] = b"agentmesh/discovery/global/v1";
const DIRECT_PROTOCOL: &str = "/agentmesh/direct/1";

#[derive(Debug)]
pub enum SwarmCommand {
    Subscribe {
        topic: String,
        respond_to: oneshot::Sender<Result<SubscriptionRecord>>,
    },
    Broadcast {
        request: BroadcastRequest,
        respond_to: oneshot::Sender<Result<BroadcastResponse>>,
    },
    SendEnvelope {
        peer_id: String,
        envelope: Envelope,
        body: Value,
        respond_to: oneshot::Sender<Result<SendMessageResponse>>,
    },
    SendLocalMessage {
        peer_id: String,
        kind: MessageKind,
        capability: Option<String>,
        body: Value,
        respond_to: Option<oneshot::Sender<Result<SendMessageResponse>>>,
    },
    Discover {
        respond_to: oneshot::Sender<Result<()>>,
    },
    DialTarget {
        host: String,
        port: u16,
        respond_to: oneshot::Sender<Result<()>>,
    },
    ApproveDelegateRequest {
        request: crate::models::DelegateDecisionRequest,
        respond_to: oneshot::Sender<Result<DelegateDecisionResponse>>,
    },
    DenyDelegateRequest {
        message_id: String,
        reason: Option<String>,
        respond_to: oneshot::Sender<Result<DelegateDecisionResponse>>,
    },
}

#[derive(Clone)]
pub struct SwarmHandle {
    sender: mpsc::Sender<SwarmCommand>,
    reachability: Arc<RwLock<ReachabilityRuntime>>,
}

impl SwarmHandle {
    pub async fn subscribe(&self, topic: String) -> Result<SubscriptionRecord> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SwarmCommand::Subscribe {
                topic,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow!("swarm command channel closed"))?;
        rx.await.map_err(|_| anyhow!("swarm response dropped"))?
    }

    pub async fn broadcast(&self, request: BroadcastRequest) -> Result<BroadcastResponse> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SwarmCommand::Broadcast {
                request,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow!("swarm command channel closed"))?;
        rx.await.map_err(|_| anyhow!("swarm response dropped"))?
    }

    pub async fn send_envelope(
        &self,
        peer_id: String,
        envelope: Envelope,
        body: Value,
    ) -> Result<SendMessageResponse> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SwarmCommand::SendEnvelope {
                peer_id,
                envelope,
                body,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow!("swarm command channel closed"))?;
        rx.await.map_err(|_| anyhow!("swarm response dropped"))?
    }

    pub async fn discover(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SwarmCommand::Discover { respond_to: tx })
            .await
            .map_err(|_| anyhow!("swarm command channel closed"))?;
        rx.await.map_err(|_| anyhow!("swarm response dropped"))?
    }

    pub async fn dial_target(&self, host: String, port: u16) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SwarmCommand::DialTarget {
                host,
                port,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow!("swarm command channel closed"))?;
        rx.await.map_err(|_| anyhow!("swarm response dropped"))?
    }

    pub async fn approve_delegate_request(
        &self,
        request: crate::models::DelegateDecisionRequest,
    ) -> Result<DelegateDecisionResponse> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SwarmCommand::ApproveDelegateRequest {
                request,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow!("swarm command channel closed"))?;
        rx.await.map_err(|_| anyhow!("swarm response dropped"))?
    }

    pub async fn deny_delegate_request(
        &self,
        message_id: String,
        reason: Option<String>,
    ) -> Result<DelegateDecisionResponse> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SwarmCommand::DenyDelegateRequest {
                message_id,
                reason,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow!("swarm command channel closed"))?;
        rx.await.map_err(|_| anyhow!("swarm response dropped"))?
    }

    pub fn reachability(&self) -> ReachabilityView {
        let snapshot = self
            .reachability
            .read()
            .map(|value| value.clone())
            .unwrap_or_default();
        snapshot.into_view()
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "MeshBehaviourEvent")]
struct MeshBehaviour {
    identify: identify::Behaviour,
    kad: kad::Behaviour<MemoryStore>,
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
    autonat: autonat::v1::Behaviour,
    upnp: upnp::tokio::Behaviour,
    request_response: request_response::cbor::Behaviour<MeshDirectRequest, MeshDirectResponse>,
    ping: ping::Behaviour,
}

#[derive(Debug)]
enum MeshBehaviourEvent {
    Identify(identify::Event),
    Kad(kad::Event),
    Gossipsub(gossipsub::Event),
    Mdns(mdns::Event),
    AutoNat(autonat::v1::Event),
    Upnp(upnp::Event),
    RequestResponse(request_response::Event<MeshDirectRequest, MeshDirectResponse>),
    Ping(()),
}

impl From<identify::Event> for MeshBehaviourEvent {
    fn from(value: identify::Event) -> Self {
        Self::Identify(value)
    }
}

impl From<kad::Event> for MeshBehaviourEvent {
    fn from(value: kad::Event) -> Self {
        Self::Kad(value)
    }
}

impl From<gossipsub::Event> for MeshBehaviourEvent {
    fn from(value: gossipsub::Event) -> Self {
        Self::Gossipsub(value)
    }
}

impl From<mdns::Event> for MeshBehaviourEvent {
    fn from(value: mdns::Event) -> Self {
        Self::Mdns(value)
    }
}

impl From<autonat::v1::Event> for MeshBehaviourEvent {
    fn from(value: autonat::v1::Event) -> Self {
        Self::AutoNat(value)
    }
}

impl From<upnp::Event> for MeshBehaviourEvent {
    fn from(value: upnp::Event) -> Self {
        Self::Upnp(value)
    }
}

impl From<request_response::Event<MeshDirectRequest, MeshDirectResponse>> for MeshBehaviourEvent {
    fn from(value: request_response::Event<MeshDirectRequest, MeshDirectResponse>) -> Self {
        Self::RequestResponse(value)
    }
}

impl From<ping::Event> for MeshBehaviourEvent {
    fn from(value: ping::Event) -> Self {
        let _ = value;
        Self::Ping(())
    }
}

#[derive(Clone)]
struct RuntimeIdentity {
    app_identity: IdentityMaterial,
    transport_keypair: identity::Keypair,
    transport_peer_id: PeerId,
}

struct PendingEnvelopeSend {
    envelope: Envelope,
    body: Value,
    respond_to: oneshot::Sender<Result<SendMessageResponse>>,
}

struct RuntimeState {
    home: PathBuf,
    config: AgentMeshConfig,
    pool: SqlitePool,
    identity: RuntimeIdentity,
    known_transport_by_app: HashMap<String, PeerId>,
    known_profiles: HashMap<PeerId, MeshProfileRecord>,
    pending_sends: HashMap<OutboundRequestId, PendingEnvelopeSend>,
    reachability: Arc<RwLock<ReachabilityRuntime>>,
    command_sender: mpsc::Sender<SwarmCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedTransportIdentity {
    protobuf_keypair_b64: String,
}

#[derive(Debug, Clone, Default)]
struct ReachabilityRuntime {
    nat_status: String,
    public_address: Option<String>,
    listen_addrs: Vec<String>,
    external_addrs: Vec<String>,
    upnp_mapped_addrs: Vec<String>,
    mesh_worker_alive: bool,
    mesh_worker_error: Option<String>,
}

impl ReachabilityRuntime {
    fn into_view(self) -> ReachabilityView {
        ReachabilityView {
            nat_status: self.nat_status,
            public_address: self.public_address,
            listen_addrs: self.listen_addrs,
            external_addrs: self.external_addrs,
            upnp_mapped_addrs: self.upnp_mapped_addrs,
            mesh_worker_alive: self.mesh_worker_alive,
            mesh_worker_error: self.mesh_worker_error,
        }
    }
}

pub async fn spawn_swarm(
    home: &Path,
    pool: SqlitePool,
    config: AgentMeshConfig,
    app_identity: IdentityMaterial,
) -> Result<SwarmHandle> {
    let runtime_identity = load_or_create_transport_identity(home, app_identity)?;
    let reachability = Arc::new(RwLock::new(ReachabilityRuntime {
        nat_status: "unknown".to_string(),
        mesh_worker_alive: true,
        ..ReachabilityRuntime::default()
    }));
    let (tx, rx) = mpsc::channel(128);
    let task_pool = pool.clone();
    let task_config = config.clone();
    let task_reachability = reachability.clone();
    let task_reachability_error = reachability.clone();
    let task_home = home.to_path_buf();
    let task_tx = tx.clone();
    tokio::spawn(async move {
        if let Err(err) = run_swarm_loop(
            task_home,
            task_pool,
            task_config,
            runtime_identity,
            task_reachability,
            task_tx,
            rx,
        )
        .await
        {
            let mut runtime = task_reachability_error.write().expect("reachability lock");
            runtime.mesh_worker_alive = false;
            runtime.mesh_worker_error = Some(err.to_string());
            warn!(target: "agentmesh", error = %err, "libp2p swarm exited");
        }
    });
    Ok(SwarmHandle {
        sender: tx,
        reachability,
    })
}

fn load_or_create_transport_identity(
    home: &Path,
    app_identity: IdentityMaterial,
) -> Result<RuntimeIdentity> {
    let path = home.join("transport-keypair.json");
    let transport_keypair = if path.exists() {
        let raw =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let persisted: PersistedTransportIdentity =
            serde_json::from_str(&raw).context("parse transport identity")?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(persisted.protobuf_keypair_b64)
            .context("decode transport keypair")?;
        identity::Keypair::from_protobuf_encoding(&bytes).context("decode libp2p keypair")?
    } else {
        let keypair = identity::Keypair::generate_ed25519();
        let encoded = keypair
            .to_protobuf_encoding()
            .context("encode libp2p keypair")?;
        let persisted = PersistedTransportIdentity {
            protobuf_keypair_b64: base64::engine::general_purpose::STANDARD.encode(encoded),
        };
        std::fs::create_dir_all(home).with_context(|| format!("mkdir {}", home.display()))?;
        std::fs::write(&path, serde_json::to_string_pretty(&persisted)?)
            .with_context(|| format!("write {}", path.display()))?;
        keypair
    };
    let transport_peer_id = transport_keypair.public().to_peer_id();
    Ok(RuntimeIdentity {
        app_identity,
        transport_keypair,
        transport_peer_id,
    })
}

async fn run_swarm_loop(
    home: PathBuf,
    pool: SqlitePool,
    config: AgentMeshConfig,
    identity: RuntimeIdentity,
    reachability: Arc<RwLock<ReachabilityRuntime>>,
    command_sender: mpsc::Sender<SwarmCommand>,
    mut commands: mpsc::Receiver<SwarmCommand>,
) -> Result<()> {
    let mut swarm = build_swarm(&config, &identity)?;
    listen_on_default_addrs(&mut swarm, config.p2p_port)?;
    add_bootstrap_peers(&mut swarm, &config)?;
    let mut state = RuntimeState {
        home,
        config: config.clone(),
        pool,
        identity,
        known_transport_by_app: HashMap::new(),
        known_profiles: HashMap::new(),
        pending_sends: HashMap::new(),
        reachability,
        command_sender,
    };
    announce_profile(&mut swarm, &state).await?;
    trigger_discovery_queries(&mut swarm, &state);
    let mut announce_interval =
        tokio::time::interval(Duration::from_secs(config.announce_interval_secs));
    let mut discover_interval = tokio::time::interval(Duration::from_secs(
        config.peer_exchange_interval_secs.max(30),
    ));
    loop {
        tokio::select! {
            _ = announce_interval.tick() => {
                if let Err(err) = announce_profile(&mut swarm, &state).await {
                    warn!(target: "agentmesh", error = %err, "profile announce failed");
                }
            }
            _ = discover_interval.tick() => {
                trigger_discovery_queries(&mut swarm, &state);
            }
            Some(command) = commands.recv() => {
                handle_command(&mut swarm, &mut state, command).await;
            }
            event = swarm.select_next_some() => {
                handle_swarm_event(&mut swarm, &mut state, event).await;
            }
        }
    }
}

fn build_swarm(
    config: &AgentMeshConfig,
    identity: &RuntimeIdentity,
) -> Result<libp2p::Swarm<MeshBehaviour>> {
    let transport_keypair = identity.transport_keypair.clone();
    let app_peer_id = identity.app_identity.peer_id();
    let swarm = SwarmBuilder::with_existing_identity(transport_keypair)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default().nodelay(true),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )?
        .with_quic()
        .with_dns()?
        .with_behaviour(|key| {
            let local_peer_id = key.public().to_peer_id();
            let mut kad = kad::Behaviour::new(local_peer_id, MemoryStore::new(local_peer_id));
            kad.set_mode(Some(kad::Mode::Server));
            let gossipsub_config = gossipsub::ConfigBuilder::default()
                .validation_mode(ValidationMode::Strict)
                .build()
                .map_err(|err| anyhow!(err.to_string()))?;
            let mut gossipsub = gossipsub::Behaviour::new(
                MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            )?;
            gossipsub.subscribe(&IdentTopic::new(PROFILE_TOPIC))?;
            for topic in &config.interests {
                let _ = gossipsub.subscribe(&IdentTopic::new(topic_topic(topic)));
            }
            let identify = identify::Behaviour::new(identify::Config::new(
                "/agentmesh/1.0.0".to_string(),
                key.public(),
            ));
            let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)?;
            let autonat = autonat::v1::Behaviour::new(
                local_peer_id,
                autonat::v1::Config {
                    boot_delay: Duration::from_secs(3),
                    retry_interval: Duration::from_secs(45),
                    refresh_interval: Duration::from_secs(5 * 60),
                    only_global_ips: false,
                    ..Default::default()
                },
            );
            let upnp = upnp::tokio::Behaviour::default();
            let request_response = request_response::cbor::Behaviour::new(
                [(StreamProtocol::new(DIRECT_PROTOCOL), ProtocolSupport::Full)],
                request_response::Config::default(),
            );
            let ping = ping::Behaviour::default();
            Ok(MeshBehaviour {
                identify,
                kad,
                gossipsub,
                mdns,
                autonat,
                upnp,
                request_response,
                ping,
            })
        })?
        .build();
    info!(target: "agentmesh", transport_peer_id = %swarm.local_peer_id(), app_peer_id = %app_peer_id, "libp2p swarm ready");
    Ok(swarm)
}

fn listen_on_default_addrs(swarm: &mut libp2p::Swarm<MeshBehaviour>, port: u16) -> Result<()> {
    swarm.listen_on(Multiaddr::from_str(&format!("/ip4/0.0.0.0/tcp/{port}"))?)?;
    swarm.listen_on(Multiaddr::from_str(&format!(
        "/ip4/0.0.0.0/udp/{port}/quic-v1"
    ))?)?;
    Ok(())
}

fn add_bootstrap_peers(
    swarm: &mut libp2p::Swarm<MeshBehaviour>,
    config: &AgentMeshConfig,
) -> Result<()> {
    for value in &config.bootstrap_urls {
        let addr =
            Multiaddr::from_str(value).with_context(|| format!("parse bootstrap peer {value}"))?;
        if let Some(PeerIdOrAddr::Peer(peer_id)) = peer_from_multiaddr(&addr) {
            swarm
                .behaviour_mut()
                .kad
                .add_address(&peer_id, addr.clone());
            swarm
                .behaviour_mut()
                .autonat
                .add_server(peer_id, Some(addr.clone()));
            let _ = swarm.dial(addr);
        }
    }
    Ok(())
}

async fn handle_command(
    swarm: &mut libp2p::Swarm<MeshBehaviour>,
    state: &mut RuntimeState,
    command: SwarmCommand,
) {
    match command {
        SwarmCommand::Subscribe { topic, respond_to } => {
            let response = async {
                let record = SubscriptionRecord {
                    topic: topic.clone(),
                    created_at: Utc::now(),
                };
                storage::upsert_subscription(&state.pool, &topic, record.created_at).await?;
                swarm
                    .behaviour_mut()
                    .gossipsub
                    .subscribe(&IdentTopic::new(topic_topic(&topic)))?;
                announce_profile(swarm, state).await?;
                Ok(record)
            }
            .await;
            let _ = respond_to.send(response);
        }
        SwarmCommand::Broadcast {
            request,
            respond_to,
        } => {
            let response = async {
                let topic = request.topic.trim();
                if topic.is_empty() {
                    bail!("topic must not be empty");
                }
                let payload = MeshPubsubMessage::Broadcast {
                    sender_peer_id: state.identity.app_identity.peer_id(),
                    sender_agent_label: state.config.agent_label.clone(),
                    topic: topic.to_string(),
                    body: request.body.clone(),
                    issued_at: Utc::now(),
                };
                let raw = serde_json::to_vec(&payload)?;
                swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(IdentTopic::new(topic_topic(topic)), raw)?;
                let peers = storage::list_peers_by_topic(&state.pool, topic).await?;
                let deliveries = peers
                    .into_iter()
                    .map(|peer| BroadcastDelivery {
                        peer_id: peer.peer_id,
                        delivery_status: "published".to_string(),
                        reason: Some("gossipsub publish accepted".to_string()),
                    })
                    .collect::<Vec<_>>();
                Ok(BroadcastResponse {
                    topic: topic.to_string(),
                    attempted_peers: deliveries.len(),
                    deliveries,
                })
            }
            .await;
            let _ = respond_to.send(response);
        }
        SwarmCommand::SendEnvelope {
            peer_id,
            envelope,
            body,
            respond_to,
        } => {
            let Some(transport_peer) = state.known_transport_by_app.get(&peer_id).cloned() else {
                let _ = respond_to.send(Err(anyhow!(
                    "peer not currently discoverable on the libp2p mesh"
                )));
                return;
            };
            let queued = StoredMessage {
                id: envelope.id.clone(),
                direction: MessageDirection::Outbound,
                peer_id: envelope.recipient_peer_id.clone(),
                kind: envelope.kind.clone(),
                capability: envelope.capability.clone(),
                body: body.clone(),
                status: MessageStatus::Queued,
                allowed: true,
                reason: Some("queued for mesh delivery".to_string()),
                created_at: envelope.issued_at,
                raw_envelope: serde_json::to_value(&envelope).unwrap_or_else(|_| json!({})),
            };
            let _ = storage::save_message(&state.pool, &queued).await;
            let request_id = swarm.behaviour_mut().request_response.send_request(
                &transport_peer,
                MeshDirectRequest::Envelope(envelope.clone()),
            );
            state.pending_sends.insert(
                request_id,
                PendingEnvelopeSend {
                    envelope,
                    body,
                    respond_to,
                },
            );
        }
        SwarmCommand::SendLocalMessage {
            peer_id,
            kind,
            capability,
            body,
            respond_to,
        } => {
            let peer = match storage::get_peer(&state.pool, &peer_id).await {
                Ok(Some(peer)) => peer,
                Ok(None) => {
                    if let Some(tx) = respond_to {
                        let _ = tx.send(Err(anyhow!("unknown peer: {peer_id}")));
                    }
                    return;
                }
                Err(err) => {
                    if let Some(tx) = respond_to {
                        let _ = tx.send(Err(err));
                    }
                    return;
                }
            };
            let envelope =
                match build_envelope_from_state(state, &peer, kind, body.clone(), capability) {
                    Ok(envelope) => envelope,
                    Err(err) => {
                        if let Some(tx) = respond_to {
                            let _ = tx.send(Err(err));
                        }
                        return;
                    }
                };
            let Some(transport_peer) = state.known_transport_by_app.get(&peer_id).cloned() else {
                if let Some(tx) = respond_to {
                    let _ = tx.send(Err(anyhow!(
                        "peer not currently discoverable on the libp2p mesh"
                    )));
                }
                return;
            };
            let request_id = swarm.behaviour_mut().request_response.send_request(
                &transport_peer,
                MeshDirectRequest::Envelope(envelope.clone()),
            );
            let queued = StoredMessage {
                id: envelope.id.clone(),
                direction: MessageDirection::Outbound,
                peer_id: envelope.recipient_peer_id.clone(),
                kind: envelope.kind.clone(),
                capability: envelope.capability.clone(),
                body: body.clone(),
                status: MessageStatus::Queued,
                allowed: true,
                reason: Some("queued for mesh delivery".to_string()),
                created_at: envelope.issued_at,
                raw_envelope: serde_json::to_value(&envelope).unwrap_or_else(|_| json!({})),
            };
            let _ = storage::save_message(&state.pool, &queued).await;
            if let Some(tx) = respond_to {
                state.pending_sends.insert(
                    request_id,
                    PendingEnvelopeSend {
                        envelope,
                        body,
                        respond_to: tx,
                    },
                );
            } else {
                let (noop_tx, _noop_rx) = oneshot::channel();
                state.pending_sends.insert(
                    request_id,
                    PendingEnvelopeSend {
                        envelope: envelope.clone(),
                        body: body.clone(),
                        respond_to: noop_tx,
                    },
                );
            }
        }
        SwarmCommand::Discover { respond_to } => {
            let _ = announce_profile(swarm, state).await;
            trigger_discovery_queries(swarm, state);
            let _ = respond_to.send(Ok(()));
        }
        SwarmCommand::DialTarget {
            host,
            port,
            respond_to,
        } => {
            let result = dial_target_addr(swarm, &host, port);
            let _ = respond_to.send(result);
        }
        SwarmCommand::ApproveDelegateRequest {
            request,
            respond_to,
        } => {
            let result = approve_pending_delegate_request(state, request).await;
            let _ = respond_to.send(result);
        }
        SwarmCommand::DenyDelegateRequest {
            message_id,
            reason,
            respond_to,
        } => {
            let result = deny_pending_delegate_request(state, &message_id, reason.as_deref()).await;
            let _ = respond_to.send(result);
        }
    }
}

async fn handle_swarm_event(
    swarm: &mut libp2p::Swarm<MeshBehaviour>,
    state: &mut RuntimeState,
    event: SwarmEvent<MeshBehaviourEvent>,
) {
    match event {
        SwarmEvent::Behaviour(MeshBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
            for (peer_id, addr) in list {
                swarm
                    .behaviour_mut()
                    .kad
                    .add_address(&peer_id, addr.clone());
                swarm
                    .behaviour_mut()
                    .autonat
                    .add_server(peer_id, Some(addr.clone()));
                let _ = swarm.dial(addr);
            }
        }
        SwarmEvent::Behaviour(MeshBehaviourEvent::Identify(identify::Event::Received {
            peer_id,
            info,
            ..
        })) => {
            for addr in &info.listen_addrs {
                swarm
                    .behaviour_mut()
                    .kad
                    .add_address(&peer_id, addr.clone());
            }
            let _ = swarm
                .behaviour_mut()
                .request_response
                .send_request(&peer_id, MeshDirectRequest::Profile);
        }
        SwarmEvent::Behaviour(MeshBehaviourEvent::AutoNat(event)) => match event {
            autonat::v1::Event::StatusChanged { new, .. } => match new {
                autonat::v1::NatStatus::Public(addr) => {
                    set_nat_status(state, "public".to_string(), Some(addr.to_string()));
                    record_external_addr(state, &addr);
                }
                autonat::v1::NatStatus::Private => {
                    set_nat_status(state, "private".to_string(), None);
                }
                autonat::v1::NatStatus::Unknown => {
                    set_nat_status(state, "unknown".to_string(), None);
                }
            },
            autonat::v1::Event::OutboundProbe(event) => {
                info!(target: "agentmesh", outbound_probe = ?event, "autonat outbound probe");
            }
            autonat::v1::Event::InboundProbe(event) => {
                info!(target: "agentmesh", inbound_probe = ?event, "autonat inbound probe");
            }
        },
        SwarmEvent::Behaviour(MeshBehaviourEvent::Upnp(event)) => match event {
            upnp::Event::NewExternalAddr(addr) => {
                record_upnp_addr(state, &addr);
                info!(target: "agentmesh", upnp_addr = %addr, "upnp mapped external address");
            }
            upnp::Event::ExpiredExternalAddr(addr) => {
                remove_upnp_addr(state, &addr);
                info!(target: "agentmesh", upnp_addr = %addr, "upnp external address expired");
            }
            upnp::Event::GatewayNotFound => {
                info!(target: "agentmesh", "upnp gateway not found");
            }
            upnp::Event::NonRoutableGateway => {
                info!(target: "agentmesh", "upnp gateway is non-routable");
            }
        },
        SwarmEvent::Behaviour(MeshBehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
            result,
            ..
        })) => {
            if let kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders {
                providers,
                ..
            })) = result
            {
                for peer_id in providers {
                    let _ = swarm
                        .behaviour_mut()
                        .request_response
                        .send_request(&peer_id, MeshDirectRequest::Profile);
                }
            }
        }
        SwarmEvent::Behaviour(MeshBehaviourEvent::Gossipsub(gossipsub::Event::Message {
            message,
            ..
        })) => {
            if let Ok(payload) = serde_json::from_slice::<MeshPubsubMessage>(&message.data) {
                if let Err(err) = apply_pubsub_message(state, payload).await {
                    warn!(target: "agentmesh", error = %err, "pubsub payload rejected");
                }
            }
        }
        SwarmEvent::Behaviour(MeshBehaviourEvent::RequestResponse(
            request_response::Event::Message { peer, message, .. },
        )) => match message {
            request_response::Message::Request {
                request, channel, ..
            } => match handle_direct_request(state, &peer, request).await {
                Ok(response) => {
                    let _ = swarm
                        .behaviour_mut()
                        .request_response
                        .send_response(channel, response);
                }
                Err(err) => {
                    let _ = swarm.behaviour_mut().request_response.send_response(
                        channel,
                        MeshDirectResponse::Ack {
                            delivery_status: "failed".to_string(),
                            reason: Some(err.to_string()),
                        },
                    );
                }
            },
            request_response::Message::Response {
                request_id,
                response,
            } => {
                if let Some(pending) = state.pending_sends.remove(&request_id) {
                    let send_result = match response {
                        MeshDirectResponse::Ack {
                            delivery_status,
                            reason,
                        } => {
                            let status =
                                if delivery_status == "delivered" || delivery_status == "blocked" {
                                    MessageStatus::Delivered
                                } else {
                                    MessageStatus::Failed
                                };
                            let stored = StoredMessage {
                                id: pending.envelope.id.clone(),
                                direction: MessageDirection::Outbound,
                                peer_id: pending.envelope.recipient_peer_id.clone(),
                                kind: pending.envelope.kind.clone(),
                                capability: pending.envelope.capability.clone(),
                                body: pending.body,
                                status,
                                allowed: true,
                                reason: reason.clone(),
                                created_at: pending.envelope.issued_at,
                                raw_envelope: serde_json::to_value(&pending.envelope)
                                    .unwrap_or_else(|_| json!({})),
                            };
                            let _ = storage::save_message(&state.pool, &stored).await;
                            Ok(SendMessageResponse {
                                message_id: pending.envelope.id,
                                delivery_status,
                                peer_id: pending.envelope.recipient_peer_id,
                                reason,
                            })
                        }
                        MeshDirectResponse::Profile(profile) => {
                            let _ = upsert_profile(state, &peer, profile).await;
                            Err(anyhow!(
                                "peer returned a profile where an acknowledgement was expected"
                            ))
                        }
                    };
                    let _ = pending.respond_to.send(send_result);
                } else if let MeshDirectResponse::Profile(profile) = response {
                    let _ = upsert_profile(state, &peer, profile).await;
                }
            }
        },
        SwarmEvent::NewListenAddr { address, .. } => {
            record_listen_addr(state, &address);
            info!(target: "agentmesh", listen_addr = %address, "libp2p listener ready");
        }
        SwarmEvent::ExternalAddrConfirmed { address } => {
            record_external_addr(state, &address);
            info!(target: "agentmesh", external_addr = %address, "external address confirmed");
        }
        SwarmEvent::ExternalAddrExpired { address } => {
            remove_external_addr(state, &address);
            info!(target: "agentmesh", external_addr = %address, "external address expired");
        }
        _ => {}
    }
}

async fn handle_direct_request(
    state: &mut RuntimeState,
    transport_peer: &PeerId,
    request: MeshDirectRequest,
) -> Result<MeshDirectResponse> {
    match request {
        MeshDirectRequest::Profile => Ok(MeshDirectResponse::Profile(
            local_profile_record(state).await?,
        )),
        MeshDirectRequest::Envelope(envelope) => {
            state
                .known_transport_by_app
                .insert(envelope.sender_peer_id.clone(), *transport_peer);
            let response = accept_inbound_envelope(state, envelope.clone()).await?;
            maybe_handle_collaboration(state, envelope, response.stored_message).await;
            Ok(MeshDirectResponse::Ack {
                delivery_status: response.delivery_status,
                reason: response.reason,
            })
        }
    }
}

async fn apply_pubsub_message(state: &mut RuntimeState, payload: MeshPubsubMessage) -> Result<()> {
    match payload {
        MeshPubsubMessage::Profile(profile) => {
            let transport_peer = PeerId::from_str(&profile.transport_peer_id).ok();
            if let Some(transport_peer) = transport_peer {
                upsert_profile(state, &transport_peer, profile).await?;
            } else {
                upsert_profile_without_transport(state, profile).await?;
            }
        }
        MeshPubsubMessage::Broadcast {
            sender_peer_id,
            sender_agent_label,
            topic,
            body,
            issued_at,
        } => {
            let message = StoredMessage {
                id: uuid::Uuid::new_v4().simple().to_string(),
                direction: MessageDirection::Inbound,
                peer_id: sender_peer_id,
                kind: MessageKind::Broadcast,
                capability: None,
                body: json!({
                    "topic": topic,
                    "payload": body,
                    "sender_agent_label": sender_agent_label,
                }),
                status: MessageStatus::Received,
                allowed: true,
                reason: Some("accepted from gossipsub topic".to_string()),
                created_at: issued_at,
                raw_envelope: json!({"kind":"broadcast"}),
            };
            storage::save_message(&state.pool, &message).await?;
        }
    }
    Ok(())
}

async fn upsert_profile(
    state: &mut RuntimeState,
    transport_peer: &PeerId,
    profile: MeshProfileRecord,
) -> Result<()> {
    let peer = discovered_peer_from_profile(&profile);
    state
        .known_transport_by_app
        .insert(peer.peer_id.clone(), *transport_peer);
    state
        .known_profiles
        .insert(*transport_peer, profile.clone());
    storage::upsert_peer(&state.pool, &peer).await?;
    storage::replace_peer_topics(
        &state.pool,
        &peer.peer_id,
        &profile.subscriptions,
        Utc::now(),
    )
    .await?;
    Ok(())
}

async fn upsert_profile_without_transport(
    state: &mut RuntimeState,
    profile: MeshProfileRecord,
) -> Result<()> {
    let peer = discovered_peer_from_profile(&profile);
    storage::upsert_peer(&state.pool, &peer).await?;
    storage::replace_peer_topics(
        &state.pool,
        &peer.peer_id,
        &profile.subscriptions,
        Utc::now(),
    )
    .await?;
    Ok(())
}

async fn local_profile_record(state: &RuntimeState) -> Result<MeshProfileRecord> {
    let subscriptions = storage::list_subscriptions(&state.pool)
        .await?
        .into_iter()
        .map(|subscription| subscription.topic)
        .collect::<Vec<_>>();
    let reachability = state
        .reachability
        .read()
        .map(|value| value.clone())
        .unwrap_or_default();
    let peer = PeerRecord {
        peer_id: state.identity.app_identity.peer_id(),
        label: None,
        agent_label: state.config.agent_label.clone(),
        agent_description: state.config.agent_description.clone(),
        node_type: Some("agent".to_string()),
        runtime_name: Some("wildmesh".to_string()),
        interests: state.config.interests.clone(),
        host: state.config.advertise_host.clone(),
        port: state.config.p2p_port,
        public_key: state.identity.app_identity.signing_public_b64(),
        encryption_public_key: state.identity.app_identity.encryption_public_b64(),
        relay_url: None,
        notes: None,
        discovered: false,
        last_seen_at: Some(Utc::now()),
        created_at: Utc::now(),
        accepts_context_capsules: true,
        accepts_artifact_exchange: true,
        accepts_delegate_work: executor::delegate_available(&state.config),
        activity_state: None,
        last_seen_age_secs: None,
    };
    Ok(MeshProfileRecord {
        transport_peer_id: state.identity.transport_peer_id.to_string(),
        peer,
        subscriptions,
        listen_addrs: if reachability.external_addrs.is_empty() {
            advertised_multiaddrs(&state.config)
        } else {
            reachability.external_addrs
        },
    })
}

async fn announce_profile(
    swarm: &mut libp2p::Swarm<MeshBehaviour>,
    state: &RuntimeState,
) -> Result<()> {
    let profile = local_profile_record(state).await?;
    let raw = serde_json::to_vec(&MeshPubsubMessage::Profile(profile))?;
    let _ = swarm
        .behaviour_mut()
        .gossipsub
        .publish(IdentTopic::new(PROFILE_TOPIC), raw);
    let key = kad::RecordKey::new(&GLOBAL_DISCOVERY_KEY.to_vec());
    let _ = swarm.behaviour_mut().kad.start_providing(key);
    for interest in &state.config.interests {
        let key = kad::RecordKey::new(&interest_key(interest));
        let _ = swarm.behaviour_mut().kad.start_providing(key);
    }
    Ok(())
}

fn trigger_discovery_queries(swarm: &mut libp2p::Swarm<MeshBehaviour>, state: &RuntimeState) {
    let _ = swarm.behaviour_mut().kad.bootstrap();
    let _ = swarm
        .behaviour_mut()
        .kad
        .get_providers(kad::RecordKey::new(&GLOBAL_DISCOVERY_KEY.to_vec()));
    for interest in &state.config.interests {
        let _ = swarm
            .behaviour_mut()
            .kad
            .get_providers(kad::RecordKey::new(&interest_key(interest)));
    }
}

fn advertised_multiaddrs(config: &AgentMeshConfig) -> Vec<String> {
    let mut addrs = Vec::new();
    let host = config.advertise_host.as_str();
    if host.parse::<std::net::Ipv4Addr>().is_ok() {
        addrs.push(format!("/ip4/{host}/tcp/{}", config.p2p_port));
        addrs.push(format!("/ip4/{host}/udp/{}/quic-v1", config.p2p_port));
    } else if host.parse::<std::net::Ipv6Addr>().is_ok() {
        addrs.push(format!("/ip6/{host}/tcp/{}", config.p2p_port));
        addrs.push(format!("/ip6/{host}/udp/{}/quic-v1", config.p2p_port));
    } else {
        addrs.push(format!("/dns/{host}/tcp/{}", config.p2p_port));
        addrs.push(format!("/dns/{host}/udp/{}/quic-v1", config.p2p_port));
    }
    addrs
}

fn dial_target_addr(swarm: &mut libp2p::Swarm<MeshBehaviour>, host: &str, port: u16) -> Result<()> {
    let multiaddr = if host.parse::<std::net::Ipv4Addr>().is_ok() {
        Multiaddr::from_str(&format!("/ip4/{host}/tcp/{port}"))?
    } else if host.parse::<std::net::Ipv6Addr>().is_ok() {
        Multiaddr::from_str(&format!("/ip6/{host}/tcp/{port}"))?
    } else {
        Multiaddr::from_str(&format!("/dns/{host}/tcp/{port}"))?
    };
    swarm.dial(multiaddr)?;
    Ok(())
}

fn discovered_peer_from_profile(profile: &MeshProfileRecord) -> PeerRecord {
    let mut peer = profile.peer.clone();
    if let Some((host, port)) = pick_best_addr(&profile.listen_addrs) {
        peer.host = host;
        peer.port = port;
    }
    peer.discovered = true;
    peer.last_seen_at = Some(Utc::now());
    peer
}

fn pick_best_addr(addrs: &[String]) -> Option<(String, u16)> {
    let mut fallback: Option<(String, u16)> = None;
    for raw in addrs {
        let Ok(addr) = Multiaddr::from_str(raw) else {
            continue;
        };
        let mut host: Option<String> = None;
        let mut port: Option<u16> = None;
        for protocol in addr.iter() {
            match protocol {
                Protocol::Ip4(ip) => host = Some(ip.to_string()),
                Protocol::Ip6(ip) => host = Some(ip.to_string()),
                Protocol::Dns(name)
                | Protocol::Dns4(name)
                | Protocol::Dns6(name)
                | Protocol::Dnsaddr(name) => host = Some(name.to_string()),
                Protocol::Tcp(value) | Protocol::Udp(value) => {
                    if port.is_none() {
                        port = Some(value);
                    }
                }
                _ => {}
            }
        }
        let Some(host) = host else {
            continue;
        };
        let Some(port) = port else {
            continue;
        };
        let is_loopback = host
            .parse::<std::net::IpAddr>()
            .map(|value| value.is_loopback())
            .unwrap_or(false);
        if !is_loopback {
            return Some((host, port));
        }
        if fallback.is_none() {
            fallback = Some((host, port));
        }
    }
    fallback
}

fn topic_topic(topic: &str) -> String {
    format!("agentmesh.topic.v1.{}", topic)
}

fn interest_key(interest: &str) -> Vec<u8> {
    format!("agentmesh/discovery/interest/v1/{}", interest).into_bytes()
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|entry| entry == &value) {
        values.push(value);
    }
}

fn set_nat_status(state: &RuntimeState, value: String, public_address: Option<String>) {
    if let Ok(mut reachability) = state.reachability.write() {
        reachability.nat_status = value;
        if public_address.is_some() {
            reachability.public_address = public_address;
        }
    }
}

fn record_listen_addr(state: &RuntimeState, addr: &Multiaddr) {
    if let Ok(mut reachability) = state.reachability.write() {
        push_unique(&mut reachability.listen_addrs, addr.to_string());
    }
}

fn record_external_addr(state: &RuntimeState, addr: &Multiaddr) {
    if let Ok(mut reachability) = state.reachability.write() {
        let value = addr.to_string();
        push_unique(&mut reachability.external_addrs, value.clone());
        reachability.public_address = Some(value);
    }
}

fn remove_external_addr(state: &RuntimeState, addr: &Multiaddr) {
    if let Ok(mut reachability) = state.reachability.write() {
        let value = addr.to_string();
        reachability.external_addrs.retain(|entry| entry != &value);
        if reachability.public_address.as_deref() == Some(value.as_str()) {
            reachability.public_address = reachability.external_addrs.first().cloned();
        }
    }
}

fn record_upnp_addr(state: &RuntimeState, addr: &Multiaddr) {
    if let Ok(mut reachability) = state.reachability.write() {
        let value = addr.to_string();
        push_unique(&mut reachability.upnp_mapped_addrs, value.clone());
        push_unique(&mut reachability.external_addrs, value.clone());
        reachability.public_address = Some(value);
    }
}

fn remove_upnp_addr(state: &RuntimeState, addr: &Multiaddr) {
    if let Ok(mut reachability) = state.reachability.write() {
        let value = addr.to_string();
        reachability
            .upnp_mapped_addrs
            .retain(|entry| entry != &value);
        reachability.external_addrs.retain(|entry| entry != &value);
        if reachability.public_address.as_deref() == Some(value.as_str()) {
            reachability.public_address = reachability.external_addrs.first().cloned();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{discovered_peer_from_profile, pick_best_addr};
    use crate::models::{MeshProfileRecord, PeerRecord};
    use chrono::Utc;

    #[test]
    fn pick_best_addr_prefers_non_loopback() {
        let value = pick_best_addr(&[
            "/ip4/127.0.0.1/tcp/4500".to_string(),
            "/ip4/192.168.1.50/tcp/4500".to_string(),
        ]);
        assert_eq!(value, Some(("192.168.1.50".to_string(), 4500)));
    }

    #[test]
    fn discovered_profile_marks_peer_visible() {
        let peer = PeerRecord {
            peer_id: "peer-1".to_string(),
            label: None,
            agent_label: Some("peer".to_string()),
            agent_description: Some("peer".to_string()),
            node_type: Some("agent".to_string()),
            runtime_name: Some("wildmesh".to_string()),
            interests: vec!["local".to_string()],
            host: "127.0.0.1".to_string(),
            port: 4500,
            public_key: "pub".to_string(),
            encryption_public_key: "enc".to_string(),
            relay_url: None,
            notes: None,
            discovered: false,
            last_seen_at: None,
            created_at: Utc::now(),
            accepts_context_capsules: true,
            accepts_artifact_exchange: true,
            accepts_delegate_work: true,
            activity_state: None,
            last_seen_age_secs: None,
        };
        let profile = MeshProfileRecord {
            transport_peer_id: "transport-1".to_string(),
            peer,
            subscriptions: vec!["market.alerts".to_string()],
            listen_addrs: vec!["/ip4/192.168.1.77/tcp/4509".to_string()],
        };
        let discovered = discovered_peer_from_profile(&profile);
        assert!(discovered.discovered);
        assert_eq!(discovered.host, "192.168.1.77");
        assert_eq!(discovered.port, 4509);
        assert!(discovered.last_seen_at.is_some());
    }
}

struct InboundAcceptOutcome {
    delivery_status: String,
    reason: Option<String>,
    stored_message: StoredMessage,
}

async fn accept_inbound_envelope(
    state: &RuntimeState,
    envelope: Envelope,
) -> Result<InboundAcceptOutcome> {
    if storage::message_exists(&state.pool, &envelope.id).await? {
        let raw_envelope = serde_json::to_value(&envelope)?;
        return Ok(InboundAcceptOutcome {
            delivery_status: "delivered".to_string(),
            reason: Some("duplicate ignored".to_string()),
            stored_message: StoredMessage {
                id: envelope.id.clone(),
                direction: MessageDirection::Inbound,
                peer_id: envelope.sender_peer_id.clone(),
                kind: envelope.kind.clone(),
                capability: envelope.capability.clone(),
                body: json!({"duplicate": true}),
                status: MessageStatus::Delivered,
                allowed: true,
                reason: Some("duplicate ignored".to_string()),
                created_at: envelope.issued_at,
                raw_envelope,
            },
        });
    }
    let derived_sender = crate::crypto::derive_peer_id(&envelope.sender_public_key)?;
    if derived_sender != envelope.sender_peer_id {
        bail!("sender peer_id does not match sender public key");
    }
    if envelope.recipient_peer_id != state.identity.app_identity.peer_id() {
        bail!("envelope addressed to another peer");
    }
    let unsigned = Envelope {
        signature: None,
        ..envelope.clone()
    };
    crate::crypto::verify_signature(
        &envelope.sender_public_key,
        envelope
            .signature
            .as_deref()
            .ok_or_else(|| anyhow!("missing signature"))?,
        &unsigned,
    )?;
    let body_bytes = state.identity.app_identity.decrypt(
        &envelope.body_ciphertext,
        &envelope.body_nonce,
        &envelope.body_ephemeral_public_key,
    )?;
    let actual_sha = hex::encode(sha2::Sha256::digest(&body_bytes));
    if actual_sha != envelope.body_sha256 {
        bail!("body digest mismatch");
    }
    let body: Value = serde_json::from_slice(&body_bytes)?;
    let grant_allowed = storage::has_grant(
        &state.pool,
        &envelope.sender_peer_id,
        envelope.capability.as_deref(),
    )
    .await?;
    let is_delegate_request = matches!(envelope.kind, MessageKind::DelegateRequest);
    let allowed = match envelope.kind {
        MessageKind::Hello | MessageKind::Broadcast | MessageKind::PeerExchange => true,
        MessageKind::TaskResult | MessageKind::DelegateResult | MessageKind::ArtifactPayload => {
            matches_reply_context(&state.pool, &envelope.sender_peer_id, &body).await? || grant_allowed
        }
        _ => grant_allowed,
    };
    let auto_delegate = state.config.cooperate_enabled && executor::delegate_available(&state.config);
    let (status, reason, delivery_status) = if is_delegate_request && (!grant_allowed || !auto_delegate) {
        let reason = if grant_allowed {
            "pending local approval".to_string()
        } else {
            "awaiting local approval or trust grant".to_string()
        };
        (MessageStatus::Pending, Some(reason), "pending".to_string())
    } else if !allowed {
        (
            MessageStatus::Blocked,
            Some(format!(
                "missing local capability grant for {}",
                envelope.capability.as_deref().unwrap_or("<none>")
            )),
            "blocked".to_string(),
        )
    } else {
        (MessageStatus::Received, Some("accepted".to_string()), "delivered".to_string())
    };
    let stored_message = StoredMessage {
        id: envelope.id.clone(),
        direction: MessageDirection::Inbound,
        peer_id: envelope.sender_peer_id.clone(),
        kind: envelope.kind.clone(),
        capability: envelope.capability.clone(),
        body,
        status,
        allowed,
        reason: reason.clone(),
        created_at: envelope.issued_at,
        raw_envelope: serde_json::to_value(&envelope)?,
    };
    storage::save_message(&state.pool, &stored_message).await?;
    Ok(InboundAcceptOutcome {
        delivery_status,
        reason,
        stored_message,
    })
}

async fn matches_reply_context(pool: &SqlitePool, peer_id: &str, body: &Value) -> Result<bool> {
    let Some(reply_to_message_id) = body
        .get("reply_to_message_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(false);
    };
    storage::outbound_message_exists_for_peer(pool, peer_id, reply_to_message_id).await
}

async fn maybe_handle_collaboration(
    state: &RuntimeState,
    envelope: Envelope,
    stored_message: StoredMessage,
) {
    if !stored_message.allowed {
        return;
    }
    match stored_message.kind {
        MessageKind::ArtifactFetch => {
            if let Ok(fetch) =
                serde_json::from_value::<ArtifactFetchBody>(stored_message.body.clone())
            {
                maybe_reply_with_artifact(state, &stored_message.peer_id, &envelope.id, fetch)
                    .await;
            }
        }
        MessageKind::ArtifactPayload => {
            if let Ok(payload) =
                serde_json::from_value::<ArtifactPayloadBody>(stored_message.body.clone())
            {
                if let Err(err) = artifact::store_artifact_payload(
                    &state.home,
                    &payload,
                    "incoming",
                    Some(&stored_message.peer_id),
                ) {
                    warn!(target: "agentmesh", error = %err, peer_id = %stored_message.peer_id, "artifact payload store failed");
                }
            }
        }
        MessageKind::DelegateRequest => {
            if !matches!(stored_message.status, MessageStatus::Received) {
                return;
            }
            if let Ok(request) =
                serde_json::from_value::<DelegateRequestBody>(stored_message.body.clone())
            {
                if let Err(err) = execute_delegate_and_queue_result(
                    state,
                    &stored_message.peer_id,
                    &envelope.id,
                    request.clone(),
                )
                .await
                {
                    let err_text = err.to_string();
                    let _ = queue_delegate_failure(
                        state,
                        &stored_message.peer_id,
                        &envelope.id,
                        &request,
                        &err_text,
                    )
                    .await;
                    warn!(target: "agentmesh", error = %err, peer_id = %stored_message.peer_id, "delegate execution failed");
                }
            }
        }
        _ => {}
    }
}

async fn maybe_reply_with_artifact(
    state: &RuntimeState,
    peer_id: &str,
    reply_to_message_id: &str,
    fetch: ArtifactFetchBody,
) {
    let capability = Some("artifact_exchange".to_string());
    let outcome = (|| -> Result<ArtifactPayloadBody> {
        let (record, bytes) = artifact::load_artifact_bytes(&state.home, &fetch.artifact_id)?;
        if bytes.len() > state.config.artifact_inline_limit_bytes {
            bail!(
                "artifact {} exceeds inline limit {} bytes",
                record.artifact_id,
                state.config.artifact_inline_limit_bytes
            );
        }
        Ok(ArtifactPayloadBody {
            artifact_id: record.artifact_id,
            name: record.name,
            mime_type: record.mime_type,
            size_bytes: record.size_bytes,
            sha256: record.sha256,
            content_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            note: record.note,
            reply_to_message_id: Some(reply_to_message_id.to_string()),
        })
    })();
    match outcome {
        Ok(payload) => {
            let _ = state
                .command_sender
                .send(SwarmCommand::SendLocalMessage {
                    peer_id: peer_id.to_string(),
                    kind: MessageKind::ArtifactPayload,
                    capability,
                    body: serde_json::to_value(payload).unwrap_or_else(|_| json!({})),
                    respond_to: None,
                })
                .await;
        }
        Err(err) => {
            warn!(target: "agentmesh", error = %err, peer_id = %peer_id, "artifact fetch handling failed")
        }
    }
}

async fn execute_delegate_and_queue_result(
    state: &RuntimeState,
    peer_id: &str,
    reply_to_message_id: &str,
    request: DelegateRequestBody,
) -> Result<()> {
    if !executor::delegate_available(&state.config) {
        bail!("local executor is not enabled");
    }
    let profile = LocalProfile {
        peer_id: state.identity.app_identity.peer_id(),
        agent_label: state.config.agent_label.clone(),
        agent_description: state.config.agent_description.clone(),
        node_type: "agent".to_string(),
        runtime_name: "wildmesh".to_string(),
        interests: state.config.interests.clone(),
        control_url: state.config.control_url(),
        p2p_endpoint: state.config.p2p_endpoint(),
        public_api_url: state.config.public_api_url(),
        bootstrap_urls: state.config.bootstrap_urls.clone(),
        nat_status: state
            .reachability
            .read()
            .map(|value| value.nat_status.clone())
            .unwrap_or_else(|_| "unknown".to_string()),
        public_address: state
            .reachability
            .read()
            .ok()
            .and_then(|value| value.public_address.clone()),
        collaboration: crate::models::CollaborationView {
            cooperate_enabled: state.config.cooperate_enabled,
            executor_mode: state.config.executor_mode.clone(),
            accepts_context_capsules: true,
            accepts_artifact_exchange: true,
            accepts_delegate_work: executor::delegate_available(&state.config),
        },
    };
    let output = executor::execute_delegate(&state.config, &profile, &request).await?;
    let summary = output
        .get("summary")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let payload = DelegateResultBody {
        task_id: request.task_id,
        task_type: request.task_type,
        status: "completed".to_string(),
        handled_by: profile
            .agent_label
            .clone()
            .unwrap_or_else(|| profile.peer_id.clone()),
        output,
        summary,
        reply_to_message_id: Some(reply_to_message_id.to_string()),
    };
    state
        .command_sender
        .try_send(SwarmCommand::SendLocalMessage {
            peer_id: peer_id.to_string(),
            kind: MessageKind::DelegateResult,
            capability: Some("delegate_work".to_string()),
            body: serde_json::to_value(payload).unwrap_or_else(|_| json!({})),
            respond_to: None,
        })
        .map_err(|_| anyhow!("delegate result queue is full"))?;
    Ok(())
}

async fn queue_delegate_failure(
    state: &RuntimeState,
    peer_id: &str,
    reply_to_message_id: &str,
    request: &DelegateRequestBody,
    error: &str,
) -> Result<()> {
    let handled_by = state
        .config
        .agent_label
        .clone()
        .unwrap_or_else(|| state.identity.app_identity.peer_id());
    let payload = DelegateResultBody {
        task_id: request.task_id.clone(),
        task_type: request.task_type.clone(),
        status: "failed".to_string(),
        handled_by: handled_by.clone(),
        output: json!({
            "reason": error,
            "kind": "executor_failure",
        }),
        summary: Some(format!(
            "delegate execution failed on {}: {}",
            handled_by,
            error.trim()
        )),
        reply_to_message_id: Some(reply_to_message_id.to_string()),
    };
    state
        .command_sender
        .try_send(SwarmCommand::SendLocalMessage {
            peer_id: peer_id.to_string(),
            kind: MessageKind::DelegateResult,
            capability: Some("delegate_work".to_string()),
            body: serde_json::to_value(payload).unwrap_or_else(|_| json!({})),
            respond_to: None,
        })
        .map_err(|_| anyhow!("delegate failure queue is full"))?;
    Ok(())
}

async fn queue_delegate_denial(
    state: &RuntimeState,
    peer_id: &str,
    reply_to_message_id: &str,
    request: &DelegateRequestBody,
    reason: &str,
) -> Result<()> {
    let payload = DelegateResultBody {
        task_id: request.task_id.clone(),
        task_type: request.task_type.clone(),
        status: "denied".to_string(),
        handled_by: state
            .config
            .agent_label
            .clone()
            .unwrap_or_else(|| state.identity.app_identity.peer_id()),
        output: json!({ "reason": reason }),
        summary: Some(reason.to_string()),
        reply_to_message_id: Some(reply_to_message_id.to_string()),
    };
    state
        .command_sender
        .try_send(SwarmCommand::SendLocalMessage {
            peer_id: peer_id.to_string(),
            kind: MessageKind::DelegateResult,
            capability: Some("delegate_work".to_string()),
            body: serde_json::to_value(payload).unwrap_or_else(|_| json!({})),
            respond_to: None,
        })
        .map_err(|_| anyhow!("delegate denial queue is full"))?;
    Ok(())
}

async fn approve_pending_delegate_request(
    state: &RuntimeState,
    decision: crate::models::DelegateDecisionRequest,
) -> Result<DelegateDecisionResponse> {
    let message_id = decision.message_id.as_str();
    let stored = storage::get_message(&state.pool, message_id)
        .await?
        .ok_or_else(|| anyhow!("pending request not found: {message_id}"))?;
    if !matches!(stored.direction, MessageDirection::Inbound)
        || !matches!(stored.kind, MessageKind::DelegateRequest)
    {
        bail!("message is not an inbound delegate request");
    }
    if !matches!(stored.status, MessageStatus::Pending) {
        bail!("delegate request is not pending approval");
    }
    let delegate_request = serde_json::from_value::<DelegateRequestBody>(stored.body.clone())?;
    let mut grant_created = false;
    let granted_capability = if decision.always_allow {
        let capability = decision
            .grant_capability
            .clone()
            .or_else(|| stored.capability.clone())
            .unwrap_or_else(|| "delegate_work".to_string());
        storage::upsert_grant(
            &state.pool,
            &crate::models::CapabilityGrant {
                peer_id: stored.peer_id.clone(),
                capability: capability.clone(),
                expires_at: None,
                note: decision
                    .grant_note
                    .clone()
                    .or_else(|| Some("approved from pending request".to_string())),
                created_at: Utc::now(),
            },
        )
        .await?;
        grant_created = true;
        Some(capability)
    } else {
        None
    };
    execute_delegate_and_queue_result(state, &stored.peer_id, &stored.id, delegate_request).await?;
    storage::update_message_status(
        &state.pool,
        &stored.id,
        MessageStatus::Approved,
        Some("approved for local execution"),
    )
    .await?;
    Ok(DelegateDecisionResponse {
        message_id: stored.id,
        peer_id: stored.peer_id,
        action: "accept".to_string(),
        status: "approved".to_string(),
        reply_message_id: None,
        reason: Some(if grant_created {
            "delegate request approved and peer trusted for future delegated work".to_string()
        } else {
            "delegate request approved once".to_string()
        }),
        grant_created,
        granted_capability,
    })
}

async fn deny_pending_delegate_request(
    state: &RuntimeState,
    message_id: &str,
    reason: Option<&str>,
) -> Result<DelegateDecisionResponse> {
    let stored = storage::get_message(&state.pool, message_id)
        .await?
        .ok_or_else(|| anyhow!("pending request not found: {message_id}"))?;
    if !matches!(stored.direction, MessageDirection::Inbound)
        || !matches!(stored.kind, MessageKind::DelegateRequest)
    {
        bail!("message is not an inbound delegate request");
    }
    if !matches!(stored.status, MessageStatus::Pending) {
        bail!("delegate request is not pending approval");
    }
    let request = serde_json::from_value::<DelegateRequestBody>(stored.body.clone())?;
    let reason = reason.unwrap_or("denied by local operator");
    queue_delegate_denial(state, &stored.peer_id, &stored.id, &request, reason).await?;
    storage::update_message_status(&state.pool, &stored.id, MessageStatus::Denied, Some(reason))
        .await?;
    Ok(DelegateDecisionResponse {
        message_id: stored.id,
        peer_id: stored.peer_id,
        action: "deny".to_string(),
        status: "denied".to_string(),
        reply_message_id: None,
        reason: Some(reason.to_string()),
        grant_created: false,
        granted_capability: None,
    })
}

fn build_envelope_from_state(
    state: &RuntimeState,
    peer: &PeerRecord,
    kind: MessageKind,
    body: Value,
    capability: Option<String>,
) -> Result<Envelope> {
    let (ciphertext, nonce, eph_public, body_sha256) =
        crate::crypto::encrypt_for_peer(&body, &peer.encryption_public_key)?;
    let mut envelope = Envelope {
        id: uuid::Uuid::new_v4().simple().to_string(),
        kind,
        sender_peer_id: state.identity.app_identity.peer_id(),
        sender_public_key: state.identity.app_identity.signing_public_b64(),
        sender_encryption_public_key: state.identity.app_identity.encryption_public_b64(),
        sender_endpoint: state.config.p2p_endpoint(),
        recipient_peer_id: peer.peer_id.clone(),
        capability,
        issued_at: Utc::now(),
        body_ciphertext: ciphertext,
        body_nonce: nonce,
        body_ephemeral_public_key: eph_public,
        body_sha256,
        signature: None,
    };
    let unsigned = envelope.clone();
    envelope.signature = Some(crate::crypto::sign_payload(
        &state.identity.app_identity.signing_key,
        &unsigned,
    )?);
    Ok(envelope)
}

enum PeerIdOrAddr {
    Peer(PeerId),
}

fn peer_from_multiaddr(addr: &Multiaddr) -> Option<PeerIdOrAddr> {
    addr.iter().find_map(|protocol| match protocol {
        libp2p::multiaddr::Protocol::P2p(peer_id) => Some(PeerIdOrAddr::Peer(peer_id)),
        _ => None,
    })
}
