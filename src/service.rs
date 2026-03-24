use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use chrono::Utc;
use serde_json::Value;

use crate::config::AgentMeshConfig;
use crate::crypto::{IdentityMaterial, encrypt_for_peer, sign_payload};
use crate::models::{
    BroadcastRequest, BroadcastResponse, CapabilityGrant, Envelope, IdentityView, LocalProfile,
    MessageKind, PeerRecord, SendMessageRequest, SendMessageResponse, StatusResponse,
    StoredMessage, SubscriptionRecord,
};
use crate::storage::{self, IdentityRow};
use crate::swarm::{SwarmHandle, spawn_swarm};

#[derive(Clone)]
pub struct MeshService {
    pub config: AgentMeshConfig,
    pub pool: sqlx::SqlitePool,
    pub identity: Arc<IdentityMaterial>,
    pub swarm: SwarmHandle,
}

impl MeshService {
    pub fn local_profile(&self) -> LocalProfile {
        let peer_id = self.identity.peer_id();
        let default_label = format!("agent-{}", &peer_id[..12]);
        let reachability = self.swarm.reachability();
        LocalProfile {
            peer_id,
            agent_label: Some(self.config.agent_label.clone().unwrap_or(default_label)),
            agent_description: self.config.agent_description.clone(),
            interests: self.config.interests.clone(),
            control_url: self.config.control_url(),
            p2p_endpoint: self.config.p2p_endpoint(),
            public_api_url: self.config.public_api_url(),
            bootstrap_urls: self.config.bootstrap_urls.clone(),
            nat_status: reachability.nat_status,
            public_address: reachability.public_address,
        }
    }

    pub async fn bootstrap(home: &Path, config: AgentMeshConfig) -> Result<Self> {
        std::fs::create_dir_all(home)?;
        config.persist(home)?;
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
        let (peer_count, grant_count, subscription_count, inbox_count, outbox_count) =
            storage::counts(&self.pool).await?;
        Ok(StatusResponse {
            identity: self.identity_view(),
            reachability: self.swarm.reachability(),
            peer_count,
            grant_count,
            subscription_count,
            inbox_count,
            outbox_count,
        })
    }

    pub async fn add_peer(&self, peer: PeerRecord) -> Result<PeerRecord> {
        storage::upsert_peer(&self.pool, &peer).await?;
        Ok(peer)
    }

    pub async fn list_peers(&self) -> Result<Vec<PeerRecord>> {
        storage::list_peers(&self.pool).await
    }

    pub async fn grant(&self, grant: CapabilityGrant) -> Result<CapabilityGrant> {
        storage::upsert_grant(&self.pool, &grant).await?;
        Ok(grant)
    }

    pub async fn list_grants(&self) -> Result<Vec<CapabilityGrant>> {
        storage::list_grants(&self.pool).await
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

    pub async fn list_inbox(&self, limit: i64) -> Result<Vec<StoredMessage>> {
        storage::list_messages(&self.pool, crate::models::MessageDirection::Inbound, limit).await
    }

    pub async fn list_outbox(&self, limit: i64) -> Result<Vec<StoredMessage>> {
        storage::list_messages(&self.pool, crate::models::MessageDirection::Outbound, limit).await
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
        if explicit_target.is_some() {
            bail!("targeted discovery is not supported in libp2p mode");
        }
        self.swarm.discover().await
    }
}
