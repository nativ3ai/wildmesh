use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityView {
    pub peer_id: String,
    pub public_key: String,
    pub encryption_public_key: String,
    pub control_url: String,
    pub p2p_endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReachabilityView {
    pub nat_status: String,
    pub public_address: Option<String>,
    pub listen_addrs: Vec<String>,
    pub external_addrs: Vec<String>,
    pub upnp_mapped_addrs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub identity: IdentityView,
    pub reachability: ReachabilityView,
    pub peer_count: i64,
    pub grant_count: i64,
    pub subscription_count: i64,
    pub inbox_count: i64,
    pub outbox_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerRecord {
    pub peer_id: String,
    pub label: Option<String>,
    pub agent_label: Option<String>,
    pub agent_description: Option<String>,
    #[serde(default)]
    pub node_type: Option<String>,
    #[serde(default)]
    pub runtime_name: Option<String>,
    pub interests: Vec<String>,
    pub host: String,
    pub port: u16,
    pub public_key: String,
    pub encryption_public_key: String,
    pub relay_url: Option<String>,
    pub notes: Option<String>,
    pub discovered: bool,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub accepts_context_capsules: bool,
    #[serde(default)]
    pub accepts_artifact_exchange: bool,
    #[serde(default)]
    pub accepts_delegate_work: bool,
    #[serde(default)]
    pub activity_state: Option<String>,
    #[serde(default)]
    pub last_seen_age_secs: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityGrant {
    pub peer_id: String,
    pub capability: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Hello,
    Broadcast,
    PeerExchange,
    TaskOffer,
    TaskResult,
    ContextCapsule,
    ArtifactOffer,
    ArtifactFetch,
    ArtifactPayload,
    DelegateRequest,
    DelegateResult,
    Note,
    Receipt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageDirection {
    Inbound,
    Outbound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Received,
    Blocked,
    Queued,
    Delivered,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub id: String,
    pub kind: MessageKind,
    pub sender_peer_id: String,
    pub sender_public_key: String,
    pub sender_encryption_public_key: String,
    pub sender_endpoint: String,
    pub recipient_peer_id: String,
    pub capability: Option<String>,
    pub issued_at: DateTime<Utc>,
    pub body_ciphertext: String,
    pub body_nonce: String,
    pub body_ephemeral_public_key: String,
    pub body_sha256: String,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub direction: MessageDirection,
    pub peer_id: String,
    pub kind: MessageKind,
    pub capability: Option<String>,
    pub body: serde_json::Value,
    pub status: MessageStatus,
    pub allowed: bool,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub raw_envelope: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddPeerRequest {
    pub peer_id: String,
    pub host: String,
    pub port: u16,
    pub public_key: String,
    pub encryption_public_key: String,
    pub label: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantRequest {
    pub peer_id: String,
    pub capability: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub peer_id: String,
    pub kind: MessageKind,
    #[serde(default)]
    pub body: serde_json::Value,
    pub capability: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageResponse {
    pub message_id: String,
    pub delivery_status: String,
    pub peer_id: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionRecord {
    pub topic: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationView {
    pub cooperate_enabled: bool,
    pub executor_mode: String,
    pub accepts_context_capsules: bool,
    pub accepts_artifact_exchange: bool,
    pub accepts_delegate_work: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeRequest {
    pub topic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastRequest {
    pub topic: String,
    #[serde(default)]
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastDelivery {
    pub peer_id: String,
    pub delivery_status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastResponse {
    pub topic: String,
    pub attempted_peers: usize,
    pub deliveries: Vec<BroadcastDelivery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryPacket {
    pub sender_peer_id: String,
    pub sender_public_key: String,
    pub sender_encryption_public_key: String,
    pub agent_label: Option<String>,
    pub agent_description: Option<String>,
    pub interests: Vec<String>,
    pub sender_endpoint: String,
    pub control_url: String,
    pub topics: Vec<String>,
    pub issued_at: DateTime<Utc>,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryAnnounceRequest {
    pub host: Option<String>,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubAnnouncement {
    pub sender_peer_id: String,
    pub sender_public_key: String,
    pub sender_encryption_public_key: String,
    pub agent_label: Option<String>,
    pub agent_description: Option<String>,
    pub interests: Vec<String>,
    pub sender_endpoint: String,
    pub control_url: String,
    pub topics: Vec<String>,
    pub issued_at: DateTime<Utc>,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubPeerRecord {
    pub peer_id: String,
    pub agent_label: Option<String>,
    pub agent_description: Option<String>,
    pub interests: Vec<String>,
    pub host: String,
    pub port: u16,
    pub public_key: String,
    pub encryption_public_key: String,
    pub relay_url: Option<String>,
    pub control_url: String,
    pub topics: Vec<String>,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayPublishRequest {
    pub envelope: Envelope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayPullRequest {
    pub peer_id: String,
    pub public_key: String,
    pub issued_at: DateTime<Utc>,
    pub nonce: String,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayPullResponse {
    pub envelopes: Vec<Envelope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalProfile {
    pub peer_id: String,
    pub agent_label: Option<String>,
    pub agent_description: Option<String>,
    pub node_type: String,
    pub runtime_name: String,
    pub interests: Vec<String>,
    pub control_url: String,
    pub p2p_endpoint: String,
    pub public_api_url: String,
    pub bootstrap_urls: Vec<String>,
    pub nat_status: String,
    pub public_address: Option<String>,
    pub collaboration: CollaborationView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshProfileRecord {
    pub transport_peer_id: String,
    pub peer: PeerRecord,
    pub subscriptions: Vec<String>,
    pub listen_addrs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshDirectRequest {
    Profile,
    Envelope(Envelope),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshDirectResponse {
    Profile(MeshProfileRecord),
    Ack {
        delivery_status: String,
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshPubsubMessage {
    Profile(MeshProfileRecord),
    Broadcast {
        sender_peer_id: String,
        sender_agent_label: Option<String>,
        topic: String,
        body: serde_json::Value,
        issued_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextCapsuleBody {
    pub title: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub ttl_secs: Option<u64>,
    #[serde(default)]
    pub context: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactOfferBody {
    pub artifact_id: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub inline_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactFetchBody {
    pub artifact_id: String,
    #[serde(default)]
    pub reply_to_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactPayloadBody {
    pub artifact_id: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub content_base64: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reply_to_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateRequestBody {
    pub task_id: String,
    pub task_type: String,
    pub instruction: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
    #[serde(default)]
    pub max_output_chars: Option<usize>,
    #[serde(default)]
    pub reply_to_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateResultBody {
    pub task_id: String,
    pub task_type: String,
    pub status: String,
    pub handled_by: String,
    #[serde(default)]
    pub output: serde_json::Value,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub reply_to_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextCapsuleRequest {
    pub peer_id: String,
    #[serde(default)]
    pub capability: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub ttl_secs: Option<u64>,
    #[serde(default)]
    pub context: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactOfferRequest {
    pub peer_id: String,
    #[serde(default)]
    pub capability: Option<String>,
    pub path: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactFetchRequest {
    pub peer_id: String,
    #[serde(default)]
    pub capability: Option<String>,
    pub artifact_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub artifact_id: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub direction: String,
    #[serde(default)]
    pub peer_id: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    pub saved_path: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateWorkRequest {
    pub peer_id: String,
    #[serde(default)]
    pub capability: Option<String>,
    pub task_type: String,
    pub instruction: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
    #[serde(default)]
    pub max_output_chars: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooperateConfigRequest {
    #[serde(default)]
    pub cooperate_enabled: Option<bool>,
    #[serde(default)]
    pub executor_mode: Option<String>,
    #[serde(default)]
    pub executor_url: Option<String>,
    #[serde(default)]
    pub executor_model: Option<String>,
    #[serde(default)]
    pub executor_api_key_env: Option<String>,
}
