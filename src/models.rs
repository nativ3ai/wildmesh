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
    pub interests: Vec<String>,
    pub control_url: String,
    pub p2p_endpoint: String,
    pub public_api_url: String,
    pub bootstrap_urls: Vec<String>,
    pub nat_status: String,
    pub public_address: Option<String>,
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
