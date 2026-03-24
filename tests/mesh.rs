use std::path::PathBuf;

use agentmesh::config::AgentMeshConfig;
use agentmesh::models::{MessageKind, MessageStatus, PeerRecord, StoredMessage};
use agentmesh::storage;
use chrono::Utc;
use tempfile::tempdir;

#[tokio::test]
async fn config_defaults_to_libp2p_bootstrap_peers() {
    let dir = tempdir().expect("tempdir");
    let cfg = AgentMeshConfig::load_or_create(dir.path()).expect("config");
    assert!(!cfg.bootstrap_urls.is_empty());
    assert!(cfg
        .bootstrap_urls
        .iter()
        .all(|value| value.starts_with("/")));
}

#[tokio::test]
async fn peer_storage_roundtrip_preserves_profile_metadata() {
    let dir = tempdir().expect("tempdir");
    let db_path = PathBuf::from(dir.path()).join("state.db");
    let pool = storage::open_pool(&db_path).await.expect("pool");
    let peer = PeerRecord {
        peer_id: "peer-1".to_string(),
        label: Some("transport-peer".to_string()),
        agent_label: Some("macro-scout".to_string()),
        agent_description: Some("Tracks macro and rates".to_string()),
        interests: vec!["macro".to_string(), "rates".to_string()],
        host: "127.0.0.1".to_string(),
        port: 4500,
        public_key: "signing-pub".to_string(),
        encryption_public_key: "enc-pub".to_string(),
        relay_url: None,
        notes: None,
        discovered: true,
        last_seen_at: Some(Utc::now()),
        created_at: Utc::now(),
    };
    storage::upsert_peer(&pool, &peer).await.expect("upsert peer");
    let loaded = storage::get_peer(&pool, "peer-1")
        .await
        .expect("get peer")
        .expect("peer exists");
    assert_eq!(loaded.agent_label.as_deref(), Some("macro-scout"));
    assert_eq!(loaded.agent_description.as_deref(), Some("Tracks macro and rates"));
    assert_eq!(loaded.interests, vec!["macro".to_string(), "rates".to_string()]);
}

#[tokio::test]
async fn message_storage_roundtrip_keeps_status_and_kind() {
    let dir = tempdir().expect("tempdir");
    let db_path = PathBuf::from(dir.path()).join("state.db");
    let pool = storage::open_pool(&db_path).await.expect("pool");
    let message = StoredMessage {
        id: "msg-1".to_string(),
        direction: agentmesh::models::MessageDirection::Inbound,
        peer_id: "peer-1".to_string(),
        kind: MessageKind::Broadcast,
        capability: None,
        body: serde_json::json!({"topic":"market.alerts","payload":{"headline":"mesh-live"}}),
        status: MessageStatus::Received,
        allowed: true,
        reason: Some("accepted".to_string()),
        created_at: Utc::now(),
        raw_envelope: serde_json::json!({"kind":"broadcast"}),
    };
    storage::save_message(&pool, &message)
        .await
        .expect("save message");
    let inbox = storage::list_messages(&pool, agentmesh::models::MessageDirection::Inbound, 10)
        .await
        .expect("list messages");
    assert_eq!(inbox.len(), 1);
    assert!(matches!(inbox[0].kind, MessageKind::Broadcast));
    assert!(matches!(inbox[0].status, MessageStatus::Received));
}
