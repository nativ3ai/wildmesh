use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use chrono::Utc;
use serde_json::Value;

use crate::artifact;
use crate::config::AgentMeshConfig;
use crate::crypto::{IdentityMaterial, encrypt_for_peer, sign_payload};
use crate::executor;
use crate::models::{
    ArtifactFetchBody, ArtifactFetchRequest, ArtifactOfferBody, ArtifactOfferRequest,
    ArtifactRecord, BroadcastRequest, BroadcastResponse, CapabilityGrant, ChannelRecord,
    CollaborationView, ContextCapsuleBody, ContextCapsuleRequest, CooperateConfigRequest,
    CreateChannelResponse, DelegateDecisionRequest, DelegateDecisionResponse, DelegateRequestBody,
    DelegateWorkRequest, Envelope, IdentityView, LocalProfile, MessageKind, PeerRecord,
    PendingDelegateRequest, SendMessageRequest, SendMessageResponse, StatusResponse, StoredMessage,
    SubscriptionRecord, TopicMember, TopicView,
};
use crate::payment;
use crate::storage::{self, IdentityRow};
use crate::swarm::{SwarmHandle, spawn_swarm};

#[derive(Clone)]
pub struct MeshService {
    pub home: PathBuf,
    pub config: AgentMeshConfig,
    pub pool: sqlx::SqlitePool,
    pub identity: Arc<IdentityMaterial>,
    pub swarm: SwarmHandle,
}

impl MeshService {
    pub fn collaboration_view(&self) -> CollaborationView {
        CollaborationView {
            cooperate_enabled: self.config.cooperate_enabled,
            executor_mode: self.config.executor_mode.clone(),
            accepts_context_capsules: true,
            accepts_artifact_exchange: true,
            accepts_delegate_work: executor::delegate_available(&self.config),
        }
    }

    pub fn local_peer_record(&self) -> PeerRecord {
        let peer_id = self.identity.peer_id();
        let default_label = format!("agent-{}", &peer_id[..12]);
        PeerRecord {
            peer_id,
            label: None,
            agent_label: Some(self.config.agent_label.clone().unwrap_or(default_label)),
            agent_description: self.config.agent_description.clone(),
            node_type: Some("agent".to_string()),
            runtime_name: Some("wildmesh".to_string()),
            payment_identity: payment::load_payment_identity(),
            interests: self.config.interests.clone(),
            host: self.config.advertise_host.clone(),
            port: self.config.p2p_port,
            public_key: self.identity.signing_public_b64(),
            encryption_public_key: self.identity.encryption_public_b64(),
            relay_url: None,
            notes: None,
            discovered: false,
            last_seen_at: Some(Utc::now()),
            created_at: Utc::now(),
            accepts_context_capsules: true,
            accepts_artifact_exchange: true,
            accepts_delegate_work: executor::delegate_available(&self.config),
            activity_state: None,
            last_seen_age_secs: None,
        }
    }

    pub fn local_profile(&self) -> LocalProfile {
        let reachability = self.swarm.reachability();
        let peer = self.local_peer_record();
        LocalProfile {
            peer_id: peer.peer_id,
            agent_label: peer.agent_label,
            agent_description: peer.agent_description,
            node_type: peer.node_type.unwrap_or_else(|| "agent".to_string()),
            runtime_name: peer.runtime_name.unwrap_or_else(|| "wildmesh".to_string()),
            interests: peer.interests,
            control_url: self.config.control_url(),
            p2p_endpoint: self.config.p2p_endpoint(),
            public_api_url: self.config.public_api_url(),
            local_only: self.config.local_only,
            network_scope: if self.config.local_only {
                "local_only".to_string()
            } else {
                "global".to_string()
            },
            bootstrap_urls: self.config.bootstrap_urls.clone(),
            nat_status: reachability.nat_status,
            public_address: reachability.public_address,
            payment_identity: payment::load_payment_identity(),
            collaboration: self.collaboration_view(),
        }
    }

    pub async fn bootstrap(home: &Path, config: AgentMeshConfig) -> Result<Self> {
        std::fs::create_dir_all(home)?;
        config.persist(home)?;
        artifact::ensure_dirs(home)?;
        let pool = storage::open_pool(&AgentMeshConfig::db_path(home)).await?;
        let identity = if let Some(row) = storage::load_identity(&pool).await? {
            IdentityMaterial::from_b64(&row.signing_secret_key, &row.encryption_secret_key)?
        } else {
            let identity = IdentityMaterial::generate();
            let row = IdentityRow {
                peer_id: identity.peer_id(),
                public_key: identity.signing_public_b64(),
                signing_secret_key: identity.signing_secret_b64(),
                encryption_public_key: identity.encryption_public_b64(),
                encryption_secret_key: identity.encryption_secret_b64(),
            };
            storage::ensure_identity(&pool, &row).await?;
            identity
        };
        let swarm = spawn_swarm(home, pool.clone(), config.clone(), identity.clone()).await?;
        Ok(Self {
            home: home.to_path_buf(),
            config,
            pool,
            identity: Arc::new(identity),
            swarm,
        })
    }

    pub fn identity_view(&self) -> IdentityView {
        IdentityView {
            peer_id: self.identity.peer_id(),
            public_key: self.identity.signing_public_b64(),
            encryption_public_key: self.identity.encryption_public_b64(),
            control_url: self.config.control_url(),
            p2p_endpoint: self.config.p2p_endpoint(),
        }
    }

    pub async fn status(&self) -> Result<StatusResponse> {
        let visible_peer_count = self.list_peers().await?.len() as i64;
        let (_, grant_count, subscription_count, inbox_count, outbox_count) =
            storage::counts(&self.pool).await?;
        Ok(StatusResponse {
            identity: self.identity_view(),
            reachability: self.swarm.reachability(),
            peer_count: visible_peer_count,
            grant_count,
            subscription_count,
            inbox_count,
            outbox_count,
        })
    }

    pub async fn add_peer(&self, peer: PeerRecord) -> Result<PeerRecord> {
        storage::upsert_peer(&self.pool, &peer).await?;
        Ok(
            present_peer(&self.config, peer.clone(), Utc::now()).unwrap_or_else(|| {
                let mut peer = peer;
                peer.activity_state = Some("manual".to_string());
                peer
            }),
        )
    }

    pub async fn list_peers(&self) -> Result<Vec<PeerRecord>> {
        Ok(storage::list_peers(&self.pool)
            .await?
            .into_iter()
            .filter_map(|peer| present_peer(&self.config, peer, Utc::now()))
            .collect())
    }

    pub async fn grant(&self, grant: CapabilityGrant) -> Result<CapabilityGrant> {
        storage::upsert_grant(&self.pool, &grant).await?;
        Ok(grant)
    }

    pub async fn list_grants(&self) -> Result<Vec<CapabilityGrant>> {
        storage::list_grants(&self.pool).await
    }

    pub async fn revoke_grant(&self, peer_id: &str, capability: &str) -> Result<bool> {
        storage::delete_grant(&self.pool, peer_id, capability).await
    }

    pub async fn subscribe(&self, topic: &str) -> Result<SubscriptionRecord> {
        let topic = topic.trim();
        if topic.is_empty() {
            bail!("topic must not be empty");
        }
        self.swarm.subscribe(topic.to_string()).await
    }

    pub async fn list_subscriptions(&self) -> Result<Vec<SubscriptionRecord>> {
        storage::list_subscriptions(&self.pool).await
    }

    pub async fn list_topics(&self) -> Result<Vec<TopicView>> {
        let local_subscriptions = storage::list_subscriptions(&self.pool).await?;
        let channels = storage::list_channels(&self.pool).await?;
        let topic_links = storage::list_peer_topic_links(&self.pool).await?;
        let visible_peers = self
            .list_peers()
            .await?
            .into_iter()
            .map(|peer| (peer.peer_id.clone(), peer))
            .collect::<std::collections::HashMap<_, _>>();
        let local_index = local_subscriptions
            .iter()
            .map(|subscription| (subscription.topic.clone(), subscription.created_at))
            .collect::<std::collections::HashMap<_, _>>();

        let mut views = channels
            .into_iter()
            .map(|channel| {
                let joined_at = local_index.get(&channel.topic).copied();
                (
                    channel.topic.clone(),
                    TopicView {
                        topic: channel.topic,
                        owner_peer_id: channel.owner_peer_id,
                        owner_agent_label: channel.owner_agent_label,
                        created_at: channel.created_at,
                        local_subscribed: joined_at.is_some(),
                        local_joined_at: joined_at,
                        peer_count: 0,
                        active_peer_count: 0,
                        peers: Vec::new(),
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();

        for (peer_id, topic) in topic_links {
            let Some(channel) = views.get_mut(&topic) else {
                continue;
            };
            let Some(peer) = visible_peers.get(&peer_id) else {
                continue;
            };
            channel.peer_count += 1;
            if peer.activity_state.as_deref() == Some("active") {
                channel.active_peer_count += 1;
            }
            channel.peers.push(TopicMember {
                peer_id: peer.peer_id.clone(),
                peer_label: peer.label.clone(),
                agent_label: peer.agent_label.clone(),
                agent_description: peer.agent_description.clone(),
                activity_state: peer.activity_state.clone(),
                host: peer.host.clone(),
                port: peer.port,
            });
        }

        for channel in views.values_mut() {
            if channel.local_subscribed {
                channel.peer_count += 1;
                channel.active_peer_count += 1;
                channel.peers.push(TopicMember {
                    peer_id: self.identity.peer_id(),
                    peer_label: None,
                    agent_label: self.config.agent_label.clone(),
                    agent_description: self.config.agent_description.clone(),
                    activity_state: Some("active".to_string()),
                    host: self.config.advertise_host.clone(),
                    port: self.config.p2p_port,
                });
            }
        }

        let mut items = views
            .into_values()
            .filter(|item| item.local_subscribed || item.peer_count > 0)
            .collect::<Vec<_>>();
        items.sort_by(|left, right| left.topic.cmp(&right.topic));
        Ok(items)
    }

    pub async fn create_channel(&self, topic: &str) -> Result<CreateChannelResponse> {
        let topic = topic.trim();
        if topic.is_empty() {
            bail!("topic must not be empty");
        }
        let visible_topics = self.list_topics().await?;
        let existing_visible = visible_topics.into_iter().find(|item| item.topic == topic);
        let existing = storage::get_channel(&self.pool, topic).await?;
        let created = if let Some(channel) = existing_visible {
            if channel.owner_peer_id != self.identity.peer_id() {
                bail!(
                    "channel already exists: {} (owner {})",
                    channel.topic,
                    channel
                        .owner_agent_label
                        .clone()
                        .unwrap_or_else(|| channel.owner_peer_id.clone())
                );
            }
            false
        } else if existing
            .as_ref()
            .is_some_and(|channel| channel.owner_peer_id == self.identity.peer_id())
        {
            false
        } else {
            storage::upsert_channel(
                &self.pool,
                &ChannelRecord {
                    topic: topic.to_string(),
                    owner_peer_id: self.identity.peer_id(),
                    owner_agent_label: self.config.agent_label.clone(),
                    created_at: Utc::now(),
                },
            )
            .await?;
            true
        };
        let joined = self.subscribe(topic).await?;
        let channel = self
            .list_topics()
            .await?
            .into_iter()
            .find(|item| item.topic == topic)
            .ok_or_else(|| anyhow!("channel created but not visible in local topic registry"))?;
        Ok(CreateChannelResponse {
            created,
            joined,
            channel,
        })
    }

    pub async fn list_inbox(&self, limit: i64) -> Result<Vec<StoredMessage>> {
        storage::list_messages(&self.pool, crate::models::MessageDirection::Inbound, limit).await
    }

    pub async fn list_outbox(&self, limit: i64) -> Result<Vec<StoredMessage>> {
        storage::list_messages(&self.pool, crate::models::MessageDirection::Outbound, limit).await
    }

    pub async fn list_pending_delegate_requests(
        &self,
        limit: i64,
    ) -> Result<Vec<PendingDelegateRequest>> {
        storage::list_pending_delegate_requests(&self.pool, limit).await
    }

    pub async fn list_artifacts(&self) -> Result<Vec<ArtifactRecord>> {
        artifact::list_artifacts(&self.home)
    }

    pub async fn send_message(&self, request: SendMessageRequest) -> Result<SendMessageResponse> {
        let peer = storage::get_peer(&self.pool, &request.peer_id)
            .await?
            .ok_or_else(|| anyhow!("unknown peer: {}", request.peer_id))?;
        let envelope = self.build_envelope(
            &peer,
            request.kind,
            request.body.clone(),
            request.capability.clone(),
        )?;
        self.swarm
            .send_envelope(peer.peer_id.clone(), envelope, request.body)
            .await
    }

    pub async fn broadcast(&self, request: BroadcastRequest) -> Result<BroadcastResponse> {
        self.swarm.broadcast(request).await
    }

    pub async fn send_context_capsule(
        &self,
        request: ContextCapsuleRequest,
    ) -> Result<SendMessageResponse> {
        self.send_message(SendMessageRequest {
            peer_id: request.peer_id,
            kind: MessageKind::ContextCapsule,
            capability: Some(
                request
                    .capability
                    .unwrap_or_else(|| "context_share".to_string()),
            ),
            body: serde_json::to_value(ContextCapsuleBody {
                title: request.title,
                tags: request.tags,
                ttl_secs: request.ttl_secs,
                context: request.context,
            })?,
        })
        .await
    }

    pub async fn offer_artifact(
        &self,
        request: ArtifactOfferRequest,
    ) -> Result<SendMessageResponse> {
        let record = artifact::store_artifact_from_path(
            &self.home,
            Path::new(&request.path),
            request.name.as_deref(),
            request.mime_type.as_deref(),
            "outgoing",
            Some(&request.peer_id),
            request.note.as_deref(),
        )?;
        let bytes = std::fs::read(&record.saved_path)?;
        let preview = if bytes.len() <= 256 && record.mime_type.starts_with("text/") {
            Some(
                String::from_utf8_lossy(&bytes)
                    .chars()
                    .take(160)
                    .collect::<String>(),
            )
        } else {
            None
        };
        self.send_message(SendMessageRequest {
            peer_id: request.peer_id,
            kind: MessageKind::ArtifactOffer,
            capability: Some(
                request
                    .capability
                    .unwrap_or_else(|| "artifact_exchange".to_string()),
            ),
            body: serde_json::to_value(ArtifactOfferBody {
                artifact_id: record.artifact_id,
                name: record.name,
                mime_type: record.mime_type,
                size_bytes: record.size_bytes,
                sha256: record.sha256,
                note: record.note,
                inline_preview: preview,
            })?,
        })
        .await
    }

    pub async fn fetch_artifact(
        &self,
        request: ArtifactFetchRequest,
    ) -> Result<SendMessageResponse> {
        self.send_message(SendMessageRequest {
            peer_id: request.peer_id,
            kind: MessageKind::ArtifactFetch,
            capability: Some(
                request
                    .capability
                    .unwrap_or_else(|| "artifact_exchange".to_string()),
            ),
            body: serde_json::to_value(ArtifactFetchBody {
                artifact_id: request.artifact_id,
                reply_to_message_id: None,
            })?,
        })
        .await
    }

    pub async fn delegate_work(&self, request: DelegateWorkRequest) -> Result<SendMessageResponse> {
        let max_output_chars = request.max_output_chars.or(Some(480));
        self.send_message(SendMessageRequest {
            peer_id: request.peer_id,
            kind: MessageKind::DelegateRequest,
            capability: Some(
                request
                    .capability
                    .unwrap_or_else(|| "delegate_work".to_string()),
            ),
            body: serde_json::to_value(DelegateRequestBody {
                task_id: uuid::Uuid::new_v4().simple().to_string(),
                task_type: request.task_type,
                instruction: request.instruction,
                input: request.input,
                context: request.context,
                max_output_chars,
                reply_to_message_id: None,
            })?,
        })
        .await
    }

    pub async fn approve_delegate_request(
        &self,
        request: DelegateDecisionRequest,
    ) -> Result<DelegateDecisionResponse> {
        self.swarm.approve_delegate_request(request).await
    }

    pub async fn deny_delegate_request(
        &self,
        request: DelegateDecisionRequest,
    ) -> Result<DelegateDecisionResponse> {
        self.swarm
            .deny_delegate_request(request.message_id, request.reason)
            .await
    }

    pub async fn configure_cooperation(
        &mut self,
        request: CooperateConfigRequest,
    ) -> Result<LocalProfile> {
        if let Some(enabled) = request.cooperate_enabled {
            self.config.cooperate_enabled = enabled;
        }
        if let Some(executor_mode) = request.executor_mode {
            self.config.executor_mode = executor_mode;
        }
        if let Some(executor_url) = request.executor_url {
            self.config.executor_url = Some(executor_url);
        }
        if let Some(executor_model) = request.executor_model {
            self.config.executor_model = Some(executor_model);
        }
        if let Some(executor_api_key_env) = request.executor_api_key_env {
            self.config.executor_api_key_env = Some(executor_api_key_env);
        }
        self.config.persist(&self.home)?;
        Ok(self.local_profile())
    }

    pub fn build_envelope(
        &self,
        peer: &PeerRecord,
        kind: MessageKind,
        body: Value,
        capability: Option<String>,
    ) -> Result<Envelope> {
        let (ciphertext, nonce, eph_public, body_sha256) =
            encrypt_for_peer(&body, &peer.encryption_public_key)?;
        let mut envelope = Envelope {
            id: uuid::Uuid::new_v4().simple().to_string(),
            kind,
            sender_peer_id: self.identity.peer_id(),
            sender_public_key: self.identity.signing_public_b64(),
            sender_encryption_public_key: self.identity.encryption_public_b64(),
            sender_endpoint: self.config.p2p_endpoint(),
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
        envelope.signature = Some(sign_payload(&self.identity.signing_key, &unsigned)?);
        Ok(envelope)
    }

    pub async fn announce_once(&self) -> Result<()> {
        self.swarm.discover().await
    }

    pub async fn announce_to(&self, explicit_target: Option<(String, u16)>) -> Result<()> {
        if let Some((host, port)) = explicit_target {
            self.swarm.dial_target(host, port).await?;
            return Ok(());
        }
        self.swarm.discover().await
    }
}

pub async fn initialize_home(home: &Path, config: &AgentMeshConfig) -> Result<LocalProfile> {
    std::fs::create_dir_all(home)?;
    config.persist(home)?;
    artifact::ensure_dirs(home)?;
    let pool = storage::open_pool(&AgentMeshConfig::db_path(home)).await?;
    let identity = if let Some(row) = storage::load_identity(&pool).await? {
        IdentityMaterial::from_b64(&row.signing_secret_key, &row.encryption_secret_key)?
    } else {
        let identity = IdentityMaterial::generate();
        let row = IdentityRow {
            peer_id: identity.peer_id(),
            public_key: identity.signing_public_b64(),
            signing_secret_key: identity.signing_secret_b64(),
            encryption_public_key: identity.encryption_public_b64(),
            encryption_secret_key: identity.encryption_secret_b64(),
        };
        storage::ensure_identity(&pool, &row).await?;
        identity
    };

    Ok(LocalProfile {
        peer_id: identity.peer_id(),
        agent_label: config.agent_label.clone(),
        agent_description: config.agent_description.clone(),
        node_type: "agent".to_string(),
        runtime_name: "wildmesh".to_string(),
        interests: config.interests.clone(),
        control_url: config.control_url(),
        p2p_endpoint: config.p2p_endpoint(),
        public_api_url: config.public_api_url(),
        local_only: config.local_only,
        network_scope: if config.local_only {
            "local_only".to_string()
        } else {
            "global".to_string()
        },
        bootstrap_urls: config.bootstrap_urls.clone(),
        nat_status: "unknown".to_string(),
        public_address: None,
        payment_identity: payment::load_payment_identity(),
        collaboration: CollaborationView {
            cooperate_enabled: config.cooperate_enabled,
            executor_mode: config.executor_mode.clone(),
            accepts_context_capsules: true,
            accepts_artifact_exchange: true,
            accepts_delegate_work: executor::delegate_available(config),
        },
    })
}

fn present_peer(
    config: &AgentMeshConfig,
    mut peer: PeerRecord,
    now: chrono::DateTime<Utc>,
) -> Option<PeerRecord> {
    match peer.last_seen_at {
        Some(last_seen_at) => {
            let age = (now - last_seen_at).num_seconds().max(0);
            peer.last_seen_age_secs = Some(age);
            if age as u64 <= config.peer_active_window_secs() {
                peer.activity_state = Some("active".to_string());
                Some(peer)
            } else if age as u64 <= config.peer_visible_window_secs() {
                peer.activity_state = Some("quiet".to_string());
                Some(peer)
            } else {
                None
            }
        }
        None => {
            peer.activity_state = Some("manual".to_string());
            Some(peer)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::present_peer;
    use crate::config::AgentMeshConfig;
    use crate::models::PeerRecord;
    use chrono::{Duration, Utc};

    fn sample_peer(last_seen_at: Option<chrono::DateTime<Utc>>) -> PeerRecord {
        PeerRecord {
            peer_id: "peer-1".to_string(),
            label: None,
            agent_label: Some("peer".to_string()),
            agent_description: Some("peer".to_string()),
            node_type: Some("agent".to_string()),
            runtime_name: Some("wildmesh".to_string()),
            payment_identity: None,
            interests: vec!["mesh".to_string()],
            host: "192.168.1.10".to_string(),
            port: 4500,
            public_key: "pub".to_string(),
            encryption_public_key: "enc".to_string(),
            relay_url: None,
            notes: None,
            discovered: true,
            last_seen_at,
            created_at: Utc::now(),
            accepts_context_capsules: true,
            accepts_artifact_exchange: true,
            accepts_delegate_work: true,
            activity_state: None,
            last_seen_age_secs: None,
        }
    }

    #[test]
    fn present_peer_marks_recent_peers_active() {
        let config = AgentMeshConfig::default();
        let now = Utc::now();
        let peer = sample_peer(Some(now - Duration::seconds(20)));
        let peer = present_peer(&config, peer, now).expect("peer visible");
        assert_eq!(peer.activity_state.as_deref(), Some("active"));
        assert_eq!(peer.last_seen_age_secs, Some(20));
    }

    #[test]
    fn present_peer_marks_mid_age_peers_quiet() {
        let config = AgentMeshConfig::default();
        let now = Utc::now();
        let peer = sample_peer(Some(now - Duration::seconds(150)));
        let peer = present_peer(&config, peer, now).expect("peer visible");
        assert_eq!(peer.activity_state.as_deref(), Some("quiet"));
    }

    #[test]
    fn present_peer_hides_expired_peers() {
        let config = AgentMeshConfig::default();
        let now = Utc::now();
        let peer = sample_peer(Some(now - Duration::seconds(600)));
        assert!(present_peer(&config, peer, now).is_none());
    }

    #[test]
    fn present_peer_keeps_manual_peers_visible() {
        let config = AgentMeshConfig::default();
        let now = Utc::now();
        let peer = sample_peer(None);
        let peer = present_peer(&config, peer, now).expect("peer visible");
        assert_eq!(peer.activity_state.as_deref(), Some("manual"));
        assert_eq!(peer.last_seen_age_secs, None);
    }
}
