use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use chrono::Utc;
use libp2p::autonat;
use libp2p::upnp;
use libp2p::futures::StreamExt;
use libp2p::gossipsub::{
    self, IdentTopic, MessageAuthenticity, ValidationMode,
};
use libp2p::identify;
use libp2p::kad::{self, store::MemoryStore};
use libp2p::mdns;
use libp2p::request_response::{self, OutboundRequestId, ProtocolSupport};
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, StreamProtocol, SwarmBuilder, identity, ping};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Digest;
use sqlx::SqlitePool;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use crate::config::AgentMeshConfig;
use crate::crypto::IdentityMaterial;
use crate::models::{
    BroadcastDelivery, BroadcastRequest, BroadcastResponse, Envelope, MeshDirectRequest,
    MeshDirectResponse, MeshProfileRecord, MeshPubsubMessage, MessageDirection, MessageKind,
    MessageStatus, PeerRecord, ReachabilityView, SendMessageResponse, StoredMessage,
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
    Discover {
        respond_to: oneshot::Sender<Result<()>>,
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
    request_response:
        request_response::cbor::Behaviour<MeshDirectRequest, MeshDirectResponse>,
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
    config: AgentMeshConfig,
    pool: SqlitePool,
    identity: RuntimeIdentity,
    known_transport_by_app: HashMap<String, PeerId>,
    known_profiles: HashMap<PeerId, MeshProfileRecord>,
    pending_sends: HashMap<OutboundRequestId, PendingEnvelopeSend>,
    reachability: Arc<RwLock<ReachabilityRuntime>>,
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
}

impl ReachabilityRuntime {
    fn into_view(self) -> ReachabilityView {
        ReachabilityView {
            nat_status: self.nat_status,
            public_address: self.public_address,
            listen_addrs: self.listen_addrs,
            external_addrs: self.external_addrs,
            upnp_mapped_addrs: self.upnp_mapped_addrs,
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
        ..ReachabilityRuntime::default()
    }));
    let (tx, rx) = mpsc::channel(128);
    let task_pool = pool.clone();
    let task_config = config.clone();
    let task_reachability = reachability.clone();
    tokio::spawn(async move {
        if let Err(err) =
            run_swarm_loop(task_pool, task_config, runtime_identity, task_reachability, rx).await
        {
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
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("read {}", path.display()))?;
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
    pool: SqlitePool,
    config: AgentMeshConfig,
    identity: RuntimeIdentity,
    reachability: Arc<RwLock<ReachabilityRuntime>>,
    mut commands: mpsc::Receiver<SwarmCommand>,
) -> Result<()> {
    let mut swarm = build_swarm(&config, &identity)?;
    listen_on_default_addrs(&mut swarm, config.p2p_port)?;
    add_bootstrap_peers(&mut swarm, &config)?;
    let mut state = RuntimeState {
        config: config.clone(),
        pool,
        identity,
        known_transport_by_app: HashMap::new(),
        known_profiles: HashMap::new(),
        pending_sends: HashMap::new(),
        reachability,
    };
    announce_profile(&mut swarm, &state).await?;
    trigger_discovery_queries(&mut swarm, &state);
    let mut announce_interval = tokio::time::interval(Duration::from_secs(config.announce_interval_secs));
    let mut discover_interval =
        tokio::time::interval(Duration::from_secs(config.peer_exchange_interval_secs.max(30)));
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

fn listen_on_default_addrs(
    swarm: &mut libp2p::Swarm<MeshBehaviour>,
    port: u16,
) -> Result<()> {
    swarm.listen_on(Multiaddr::from_str(&format!("/ip4/0.0.0.0/tcp/{port}"))?)?;
    swarm.listen_on(Multiaddr::from_str(&format!("/ip4/0.0.0.0/udp/{port}/quic-v1"))?)?;
    Ok(())
}

fn add_bootstrap_peers(
    swarm: &mut libp2p::Swarm<MeshBehaviour>,
    config: &AgentMeshConfig,
) -> Result<()> {
    for value in &config.bootstrap_urls {
        let addr = Multiaddr::from_str(value)
            .with_context(|| format!("parse bootstrap peer {value}"))?;
        if let Some(PeerIdOrAddr::Peer(peer_id)) = peer_from_multiaddr(&addr) {
            swarm.behaviour_mut().kad.add_address(&peer_id, addr.clone());
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
        SwarmCommand::Broadcast { request, respond_to } => {
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
                let _ = respond_to.send(Err(anyhow!("peer not currently discoverable on the libp2p mesh")));
                return;
            };
            let request_id = swarm
                .behaviour_mut()
                .request_response
                .send_request(&transport_peer, MeshDirectRequest::Envelope(envelope.clone()));
            state.pending_sends.insert(
                request_id,
                PendingEnvelopeSend {
                    envelope,
                    body,
                    respond_to,
                },
            );
        }
        SwarmCommand::Discover { respond_to } => {
            trigger_discovery_queries(swarm, state);
            let _ = respond_to.send(Ok(()));
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
                swarm.behaviour_mut().kad.add_address(&peer_id, addr.clone());
                swarm
                    .behaviour_mut()
                    .autonat
                    .add_server(peer_id, Some(addr.clone()));
                let _ = swarm.dial(addr);
            }
        }
        SwarmEvent::Behaviour(MeshBehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. })) => {
            for addr in &info.listen_addrs {
                swarm.behaviour_mut().kad.add_address(&peer_id, addr.clone());
            }
            let _ = swarm
                .behaviour_mut()
                .request_response
                .send_request(&peer_id, MeshDirectRequest::Profile);
        }
        SwarmEvent::Behaviour(MeshBehaviourEvent::AutoNat(event)) => {
            match event {
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
            }
        }
        SwarmEvent::Behaviour(MeshBehaviourEvent::Upnp(event)) => {
            match event {
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
            }
        }
        SwarmEvent::Behaviour(MeshBehaviourEvent::Kad(kad::Event::OutboundQueryProgressed { result, .. })) => {
            if let kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders { providers, .. })) = result {
                for peer_id in providers {
                    let _ = swarm
                        .behaviour_mut()
                        .request_response
                        .send_request(&peer_id, MeshDirectRequest::Profile);
                }
            }
        }
        SwarmEvent::Behaviour(MeshBehaviourEvent::Gossipsub(gossipsub::Event::Message { message, .. })) => {
            if let Ok(payload) = serde_json::from_slice::<MeshPubsubMessage>(&message.data) {
                if let Err(err) = apply_pubsub_message(state, payload).await {
                    warn!(target: "agentmesh", error = %err, "pubsub payload rejected");
                }
            }
        }
        SwarmEvent::Behaviour(MeshBehaviourEvent::RequestResponse(request_response::Event::Message { peer, message, .. })) => {
            match message {
                request_response::Message::Request { request, channel, .. } => {
                    match handle_direct_request(state, request).await {
                        Ok(response) => {
                            let _ = swarm.behaviour_mut().request_response.send_response(channel, response);
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
                    }
                }
                request_response::Message::Response { request_id, response } => {
                    if let Some(pending) = state.pending_sends.remove(&request_id) {
                        let send_result = match response {
                            MeshDirectResponse::Ack { delivery_status, reason } => {
                                let status = if delivery_status == "delivered" || delivery_status == "blocked" {
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
                                    raw_envelope: serde_json::to_value(&pending.envelope).unwrap_or_else(|_| json!({})),
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
                                Err(anyhow!("peer returned a profile where an acknowledgement was expected"))
                            }
                        };
                        let _ = pending.respond_to.send(send_result);
                    } else if let MeshDirectResponse::Profile(profile) = response {
                        let _ = upsert_profile(state, &peer, profile).await;
                    }
                }
            }
        }
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
    request: MeshDirectRequest,
) -> Result<MeshDirectResponse> {
    match request {
        MeshDirectRequest::Profile => Ok(MeshDirectResponse::Profile(local_profile_record(state).await?)),
        MeshDirectRequest::Envelope(envelope) => {
            let response = accept_inbound_envelope(&state.pool, &state.identity.app_identity, envelope).await?;
            Ok(MeshDirectResponse::Ack {
                delivery_status: response.0,
                reason: response.1,
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
    state
        .known_transport_by_app
        .insert(profile.peer.peer_id.clone(), *transport_peer);
    state.known_profiles.insert(*transport_peer, profile.clone());
    storage::upsert_peer(&state.pool, &profile.peer).await?;
    storage::replace_peer_topics(
        &state.pool,
        &profile.peer.peer_id,
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
    storage::upsert_peer(&state.pool, &profile.peer).await?;
    storage::replace_peer_topics(
        &state.pool,
        &profile.peer.peer_id,
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
    let reachability = state.reachability.read().map(|value| value.clone()).unwrap_or_default();
    let peer = PeerRecord {
        peer_id: state.identity.app_identity.peer_id(),
        label: None,
        agent_label: state.config.agent_label.clone(),
        agent_description: state.config.agent_description.clone(),
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

fn trigger_discovery_queries(
    swarm: &mut libp2p::Swarm<MeshBehaviour>,
    state: &RuntimeState,
) {
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
        reachability.upnp_mapped_addrs.retain(|entry| entry != &value);
        reachability.external_addrs.retain(|entry| entry != &value);
        if reachability.public_address.as_deref() == Some(value.as_str()) {
            reachability.public_address = reachability.external_addrs.first().cloned();
        }
    }
}

async fn accept_inbound_envelope(
    pool: &SqlitePool,
    identity: &IdentityMaterial,
    envelope: Envelope,
) -> Result<(String, Option<String>)> {
    if storage::message_exists(pool, &envelope.id).await? {
        return Ok(("delivered".to_string(), Some("duplicate ignored".to_string())));
    }
    let derived_sender = crate::crypto::derive_peer_id(&envelope.sender_public_key)?;
    if derived_sender != envelope.sender_peer_id {
        bail!("sender peer_id does not match sender public key");
    }
    if envelope.recipient_peer_id != identity.peer_id() {
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
    let body_bytes = identity.decrypt(
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
        pool,
        &envelope.sender_peer_id,
        envelope.capability.as_deref(),
    )
    .await?;
    let allowed = match envelope.kind {
        MessageKind::Hello | MessageKind::Broadcast | MessageKind::PeerExchange => true,
        _ => grant_allowed,
    };
    let reason = if allowed {
        None
    } else {
        Some(format!(
            "missing local capability grant for {}",
            envelope.capability.as_deref().unwrap_or("<none>")
        ))
    };
    storage::save_message(
        pool,
        &StoredMessage {
            id: envelope.id.clone(),
            direction: MessageDirection::Inbound,
            peer_id: envelope.sender_peer_id.clone(),
            kind: envelope.kind.clone(),
            capability: envelope.capability.clone(),
            body,
            status: if allowed {
                MessageStatus::Received
            } else {
                MessageStatus::Blocked
            },
            allowed,
            reason: reason.clone(),
            created_at: envelope.issued_at,
            raw_envelope: serde_json::to_value(&envelope)?,
        },
    )
    .await?;
    Ok((
        if allowed {
            "delivered".to_string()
        } else {
            "blocked".to_string()
        },
        Some(reason.unwrap_or_else(|| "accepted".to_string())),
    ))
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
