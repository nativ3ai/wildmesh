use std::path::PathBuf;

use agentmesh::config::AgentMeshConfig;
use agentmesh::models::{
    ArtifactFetchRequest, ArtifactOfferRequest, ContextCapsuleRequest, DelegateDecisionRequest,
    DelegateWorkRequest, MessageKind, MessageStatus, PeerRecord, StoredMessage,
};
use agentmesh::service::MeshService;
use agentmesh::storage;
use chrono::Utc;
use std::net::TcpListener;

use tempfile::tempdir;
use tokio::time::{Duration, sleep};

#[tokio::test]
async fn config_defaults_to_libp2p_bootstrap_peers() {
    let dir = tempdir().expect("tempdir");
    let cfg = AgentMeshConfig::load_or_create(dir.path()).expect("config");
    assert!(!cfg.bootstrap_urls.is_empty());
    assert!(
        cfg.bootstrap_urls
            .iter()
            .all(|value| value.starts_with("/"))
    );
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
        node_type: Some("agent".to_string()),
        runtime_name: Some("wildmesh".to_string()),
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
        accepts_context_capsules: true,
        accepts_artifact_exchange: true,
        accepts_delegate_work: true,
        activity_state: None,
        last_seen_age_secs: None,
    };
    storage::upsert_peer(&pool, &peer)
        .await
        .expect("upsert peer");
    let loaded = storage::get_peer(&pool, "peer-1")
        .await
        .expect("get peer")
        .expect("peer exists");
    assert_eq!(loaded.agent_label.as_deref(), Some("macro-scout"));
    assert_eq!(
        loaded.agent_description.as_deref(),
        Some("Tracks macro and rates")
    );
    assert_eq!(
        loaded.interests,
        vec!["macro".to_string(), "rates".to_string()]
    );
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

#[tokio::test]
async fn two_nodes_can_share_context_delegate_and_exchange_artifacts() {
    let dir = tempdir().expect("tempdir");
    let home_a = dir.path().join("node-a");
    let home_b = dir.path().join("node-b");
    let control_port_a = free_port();
    let control_port_b = free_port();
    let p2p_port_a = free_port();
    let p2p_port_b = free_port();
    let config_a = AgentMeshConfig {
        control_port: control_port_a,
        p2p_port: p2p_port_a,
        advertise_host: "127.0.0.1".to_string(),
        agent_label: Some("alpha".to_string()),
        agent_description: Some("delegator".to_string()),
        interests: vec!["mesh".to_string(), "macro".to_string()],
        bootstrap_urls: Vec::new(),
        ..AgentMeshConfig::default()
    };
    let config_b = AgentMeshConfig {
        control_port: control_port_b,
        p2p_port: p2p_port_b,
        advertise_host: "127.0.0.1".to_string(),
        agent_label: Some("beta".to_string()),
        agent_description: Some("worker".to_string()),
        interests: vec!["mesh".to_string(), "research".to_string()],
        bootstrap_urls: Vec::new(),
        cooperate_enabled: true,
        executor_mode: "builtin".to_string(),
        ..AgentMeshConfig::default()
    };
    let service_a = MeshService::bootstrap(&home_a, config_a)
        .await
        .expect("bootstrap a");
    let service_b = MeshService::bootstrap(&home_b, config_b)
        .await
        .expect("bootstrap b");
    service_a
        .announce_to(Some(("127.0.0.1".to_string(), p2p_port_b)))
        .await
        .expect("dial b");
    service_b
        .announce_to(Some(("127.0.0.1".to_string(), p2p_port_a)))
        .await
        .expect("dial a");

    wait_for("peer discovery", || async {
        let peers_a = service_a.list_peers().await.expect("list peers a");
        let peers_b = service_b.list_peers().await.expect("list peers b");
        peers_a
            .iter()
            .any(|peer| peer.agent_label.as_deref() == Some("beta"))
            && peers_b
                .iter()
                .any(|peer| peer.agent_label.as_deref() == Some("alpha"))
    })
    .await;

    let peer_b = service_a
        .list_peers()
        .await
        .expect("list peers a")
        .into_iter()
        .find(|peer| peer.agent_label.as_deref() == Some("beta"))
        .expect("beta visible");
    let peer_a = service_b
        .list_peers()
        .await
        .expect("list peers b")
        .into_iter()
        .find(|peer| peer.agent_label.as_deref() == Some("alpha"))
        .expect("alpha visible");

    service_b
        .grant(agentmesh::models::CapabilityGrant {
            peer_id: peer_a.peer_id.clone(),
            capability: "delegate_work".to_string(),
            expires_at: None,
            note: Some("test delegate".to_string()),
            created_at: Utc::now(),
        })
        .await
        .expect("grant delegate");
    service_b
        .grant(agentmesh::models::CapabilityGrant {
            peer_id: peer_a.peer_id.clone(),
            capability: "artifact_exchange".to_string(),
            expires_at: None,
            note: Some("test artifact".to_string()),
            created_at: Utc::now(),
        })
        .await
        .expect("grant artifact");
    service_b
        .grant(agentmesh::models::CapabilityGrant {
            peer_id: peer_a.peer_id.clone(),
            capability: "context_share".to_string(),
            expires_at: None,
            note: Some("test context".to_string()),
            created_at: Utc::now(),
        })
        .await
        .expect("grant context");
    service_a
        .grant(agentmesh::models::CapabilityGrant {
            peer_id: peer_b.peer_id.clone(),
            capability: "artifact_exchange".to_string(),
            expires_at: None,
            note: Some("allow offers".to_string()),
            created_at: Utc::now(),
        })
        .await
        .expect("grant reverse artifact");

    service_a
        .send_context_capsule(ContextCapsuleRequest {
            peer_id: peer_b.peer_id.clone(),
            capability: None,
            title: Some("macro-state".to_string()),
            tags: vec!["macro".to_string(), "fed".to_string()],
            ttl_secs: Some(600),
            context: serde_json::json!({"headline":"rates higher for longer"}),
        })
        .await
        .expect("send context");
    wait_for("context capsule", || async {
        service_b
            .list_inbox(20)
            .await
            .expect("inbox b")
            .iter()
            .any(|message| matches!(message.kind, MessageKind::ContextCapsule))
    })
    .await;

    service_a
        .delegate_work(DelegateWorkRequest {
            peer_id: peer_b.peer_id.clone(),
            capability: None,
            task_type: "summary".to_string(),
            instruction: "Summarize the macro headline".to_string(),
            input: serde_json::json!({"headline":"rates higher for longer"}),
            context: Some(serde_json::json!({"region":"US"})),
            max_output_chars: Some(220),
        })
        .await
        .expect("delegate work");
    wait_for("delegate result", || async {
        service_a
            .list_inbox(50)
            .await
            .expect("inbox a")
            .iter()
            .any(|message| matches!(message.kind, MessageKind::DelegateResult))
    })
    .await;

    let sample_path = home_b.join("notes.txt");
    std::fs::create_dir_all(&home_b).expect("mkdir home b");
    std::fs::write(&sample_path, "wildmesh artifact payload").expect("write sample");
    service_b
        .offer_artifact(ArtifactOfferRequest {
            peer_id: peer_a.peer_id.clone(),
            capability: None,
            path: sample_path.to_string_lossy().to_string(),
            name: None,
            mime_type: None,
            note: Some("test artifact".to_string()),
        })
        .await
        .expect("offer artifact");
    wait_for("artifact offer", || async {
        service_a
            .list_inbox(50)
            .await
            .expect("inbox a")
            .iter()
            .any(|message| matches!(message.kind, MessageKind::ArtifactOffer))
    })
    .await;
    let offer = service_a
        .list_inbox(50)
        .await
        .expect("inbox a")
        .into_iter()
        .find(|message| matches!(message.kind, MessageKind::ArtifactOffer))
        .expect("offer exists");
    let artifact_id = offer
        .body
        .get("artifact_id")
        .and_then(serde_json::Value::as_str)
        .expect("artifact id");
    service_a
        .fetch_artifact(ArtifactFetchRequest {
            peer_id: peer_b.peer_id,
            capability: None,
            artifact_id: artifact_id.to_string(),
        })
        .await
        .expect("fetch artifact");
    wait_for("artifact fetch", || async {
        service_a
            .list_artifacts()
            .await
            .expect("artifacts a")
            .iter()
            .any(|artifact| artifact.direction == "incoming")
    })
    .await;
}

#[tokio::test]
async fn delegated_work_can_wait_for_manual_approval() {
    let dir = tempdir().expect("tempdir");
    let home_a = dir.path().join("manual-a");
    let home_b = dir.path().join("manual-b");
    let control_port_a = free_port();
    let control_port_b = free_port();
    let p2p_port_a = free_port();
    let p2p_port_b = free_port();
    let config_a = AgentMeshConfig {
        control_port: control_port_a,
        p2p_port: p2p_port_a,
        advertise_host: "127.0.0.1".to_string(),
        agent_label: Some("alpha-manual".to_string()),
        agent_description: Some("delegator".to_string()),
        interests: vec!["mesh".to_string()],
        bootstrap_urls: Vec::new(),
        ..AgentMeshConfig::default()
    };
    let config_b = AgentMeshConfig {
        control_port: control_port_b,
        p2p_port: p2p_port_b,
        advertise_host: "127.0.0.1".to_string(),
        agent_label: Some("beta-manual".to_string()),
        agent_description: Some("worker".to_string()),
        interests: vec!["mesh".to_string()],
        bootstrap_urls: Vec::new(),
        cooperate_enabled: false,
        executor_mode: "builtin".to_string(),
        ..AgentMeshConfig::default()
    };
    let service_a = MeshService::bootstrap(&home_a, config_a)
        .await
        .expect("bootstrap a");
    let service_b = MeshService::bootstrap(&home_b, config_b)
        .await
        .expect("bootstrap b");
    service_a
        .announce_to(Some(("127.0.0.1".to_string(), p2p_port_b)))
        .await
        .expect("dial b");
    service_b
        .announce_to(Some(("127.0.0.1".to_string(), p2p_port_a)))
        .await
        .expect("dial a");

    wait_for("peer discovery", || async {
        let peers_a = service_a.list_peers().await.expect("list peers a");
        let peers_b = service_b.list_peers().await.expect("list peers b");
        !peers_a.is_empty() && !peers_b.is_empty()
    })
    .await;

    let peer_b = service_a
        .list_peers()
        .await
        .expect("list peers a")
        .into_iter()
        .find(|peer| peer.agent_label.as_deref() == Some("beta-manual"))
        .expect("beta visible");
    let peer_a = service_b
        .list_peers()
        .await
        .expect("list peers b")
        .into_iter()
        .find(|peer| peer.agent_label.as_deref() == Some("alpha-manual"))
        .expect("alpha visible");

    service_a
        .delegate_work(DelegateWorkRequest {
            peer_id: peer_b.peer_id.clone(),
            capability: None,
            task_type: "summary".to_string(),
            instruction: "Summarize this manually approved request".to_string(),
            input: serde_json::json!({"headline":"manual approval path"}),
            context: Some(serde_json::json!({"mode":"manual"})),
            max_output_chars: Some(220),
        })
        .await
        .expect("delegate work");

    wait_for("pending request", || async {
        service_b
            .list_pending_delegate_requests(20)
            .await
            .expect("pending")
            .iter()
            .any(|item| item.peer_id == peer_a.peer_id)
    })
    .await;

    assert!(
        service_a
            .list_inbox(20)
            .await
            .expect("inbox a")
            .iter()
            .all(|message| !matches!(message.kind, MessageKind::DelegateResult))
    );

    let pending = service_b
        .list_pending_delegate_requests(20)
        .await
        .expect("pending list")
        .into_iter()
        .find(|item| item.peer_id == peer_a.peer_id)
        .expect("pending request exists");

    assert!(!pending.peer_has_capability_grant);

    service_b
        .approve_delegate_request(DelegateDecisionRequest {
            message_id: pending.message_id,
            reason: None,
            always_allow: false,
            grant_capability: None,
            grant_note: None,
        })
        .await
        .expect("approve pending");

    let grants = service_b.list_grants().await.expect("list grants");
    assert!(
        !grants
            .iter()
            .any(|grant| grant.peer_id == peer_a.peer_id && grant.capability == "delegate_work")
    );

    wait_for("delegate result", || async {
        service_a
            .list_inbox(50)
            .await
            .expect("inbox a")
            .iter()
            .any(|message| matches!(message.kind, MessageKind::DelegateResult))
    })
    .await;
}

#[tokio::test]
async fn delegated_work_can_be_trusted_from_the_pending_queue() {
    let dir = tempdir().expect("tempdir");
    let home_a = dir.path().join("trust-a");
    let home_b = dir.path().join("trust-b");
    let control_port_a = free_port();
    let control_port_b = free_port();
    let p2p_port_a = free_port();
    let p2p_port_b = free_port();
    let config_a = AgentMeshConfig {
        control_port: control_port_a,
        p2p_port: p2p_port_a,
        advertise_host: "127.0.0.1".to_string(),
        agent_label: Some("alpha-trust".to_string()),
        agent_description: Some("delegator".to_string()),
        interests: vec!["mesh".to_string()],
        bootstrap_urls: Vec::new(),
        ..AgentMeshConfig::default()
    };
    let config_b = AgentMeshConfig {
        control_port: control_port_b,
        p2p_port: p2p_port_b,
        advertise_host: "127.0.0.1".to_string(),
        agent_label: Some("beta-trust".to_string()),
        agent_description: Some("worker".to_string()),
        interests: vec!["mesh".to_string()],
        bootstrap_urls: Vec::new(),
        cooperate_enabled: false,
        executor_mode: "builtin".to_string(),
        ..AgentMeshConfig::default()
    };
    let service_a = MeshService::bootstrap(&home_a, config_a)
        .await
        .expect("bootstrap a");
    let service_b = MeshService::bootstrap(&home_b, config_b)
        .await
        .expect("bootstrap b");
    service_a
        .announce_to(Some(("127.0.0.1".to_string(), p2p_port_b)))
        .await
        .expect("dial b");
    service_b
        .announce_to(Some(("127.0.0.1".to_string(), p2p_port_a)))
        .await
        .expect("dial a");

    wait_for("peer discovery", || async {
        let peers_a = service_a.list_peers().await.expect("list peers a");
        let peers_b = service_b.list_peers().await.expect("list peers b");
        !peers_a.is_empty() && !peers_b.is_empty()
    })
    .await;

    let peer_b = service_a
        .list_peers()
        .await
        .expect("list peers a")
        .into_iter()
        .find(|peer| peer.agent_label.as_deref() == Some("beta-trust"))
        .expect("beta visible");
    let peer_a = service_b
        .list_peers()
        .await
        .expect("list peers b")
        .into_iter()
        .find(|peer| peer.agent_label.as_deref() == Some("alpha-trust"))
        .expect("alpha visible");

    service_a
        .delegate_work(DelegateWorkRequest {
            peer_id: peer_b.peer_id.clone(),
            capability: None,
            task_type: "summary".to_string(),
            instruction: "Trust this peer and execute".to_string(),
            input: serde_json::json!({"headline":"whitelist path"}),
            context: None,
            max_output_chars: Some(200),
        })
        .await
        .expect("delegate work");

    let pending = loop {
        let items = service_b
            .list_pending_delegate_requests(20)
            .await
            .expect("pending list");
        if let Some(item) = items.into_iter().find(|item| item.peer_id == peer_a.peer_id) {
            break item;
        }
        sleep(Duration::from_millis(300)).await;
    };
    assert!(!pending.peer_has_capability_grant);

    let decision = service_b
        .approve_delegate_request(DelegateDecisionRequest {
            message_id: pending.message_id,
            reason: None,
            always_allow: true,
            grant_capability: None,
            grant_note: Some("always allow alpha-trust".to_string()),
        })
        .await
        .expect("approve pending with trust");

    assert!(decision.grant_created);
    assert_eq!(decision.granted_capability.as_deref(), Some("delegate_work"));

    let grants = service_b.list_grants().await.expect("list grants");
    assert!(grants.iter().any(|grant| {
        grant.peer_id == peer_a.peer_id
            && grant.capability == "delegate_work"
            && grant.note.as_deref() == Some("always allow alpha-trust")
    }));
}

async fn wait_for<F, Fut>(label: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..40 {
        if check().await {
            return;
        }
        sleep(Duration::from_millis(300)).await;
    }
    panic!("timed out waiting for {label}");
}

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("bind free port")
        .local_addr()
        .expect("local addr")
        .port()
}
