use std::net::SocketAddr;

use anyhow::Result;
use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tracing::info;

use crate::models::{
    AddPeerRequest, ArtifactFetchRequest, ArtifactOfferRequest, ArtifactRecord, BroadcastRequest,
    BroadcastResponse, CapabilityGrant, ContextCapsuleRequest, DelegateDecisionRequest,
    CreateChannelRequest, CreateChannelResponse, DelegateDecisionResponse, DelegateWorkRequest,
    DiscoveryAnnounceRequest, GrantRequest, PeerRecord, PendingDelegateRequest,
    RevokeGrantRequest, SendMessageRequest, SubscribeRequest, SubscriptionRecord, TopicView,
};
use crate::service::MeshService;

#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    pub limit: Option<i64>,
}

pub fn router(service: MeshService) -> Router {
    Router::new()
        .route("/v1/status", get(status))
        .route("/v1/peers", get(list_peers).post(add_peer))
        .route("/v1/capabilities", get(list_grants))
        .route("/v1/capabilities/grants", post(grant))
        .route("/v1/capabilities/revoke", post(revoke_grant))
        .route("/v1/topics", get(list_topics).post(create_channel))
        .route("/v1/subscriptions", get(list_subscriptions).post(subscribe))
        .route("/v1/messages/send", post(send_message))
        .route("/v1/messages/broadcast", post(broadcast))
        .route("/v1/messages/inbox", get(inbox))
        .route("/v1/messages/outbox", get(outbox))
        .route("/v1/context/send", post(send_context))
        .route("/v1/artifacts", get(list_artifacts))
        .route("/v1/artifacts/offer", post(offer_artifact))
        .route("/v1/artifacts/fetch", post(fetch_artifact))
        .route("/v1/delegate", post(delegate_work))
        .route("/v1/delegate/pending", get(list_pending_delegate_requests))
        .route("/v1/delegate/accept", post(approve_delegate_request))
        .route("/v1/delegate/deny", post(deny_delegate_request))
        .route("/v1/discovery/announce", post(discovery_announce))
        .with_state(service)
}

pub async fn serve(service: MeshService) -> Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], service.config.control_port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(target: "agentmesh", control = %service.config.control_url(), "control api ready");
    axum::serve(listener, router(service)).await?;
    Ok(())
}

async fn status(
    State(service): State<MeshService>,
) -> Result<Json<crate::models::StatusResponse>, axum::http::StatusCode> {
    service
        .status()
        .await
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn list_peers(
    State(service): State<MeshService>,
) -> Result<Json<Vec<PeerRecord>>, axum::http::StatusCode> {
    service
        .list_peers()
        .await
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn add_peer(
    State(service): State<MeshService>,
    Json(payload): Json<AddPeerRequest>,
) -> Result<Json<PeerRecord>, axum::http::StatusCode> {
    let peer = PeerRecord {
        peer_id: payload.peer_id,
        label: payload.label,
        agent_label: None,
        agent_description: None,
        node_type: None,
        runtime_name: None,
        interests: Vec::new(),
        host: payload.host,
        port: payload.port,
        public_key: payload.public_key,
        encryption_public_key: payload.encryption_public_key,
        relay_url: None,
        notes: payload.notes,
        discovered: false,
        last_seen_at: None,
        created_at: chrono::Utc::now(),
        accepts_context_capsules: false,
        accepts_artifact_exchange: false,
        accepts_delegate_work: false,
        activity_state: None,
        last_seen_age_secs: None,
    };
    service
        .add_peer(peer)
        .await
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn list_grants(
    State(service): State<MeshService>,
) -> Result<Json<Vec<CapabilityGrant>>, axum::http::StatusCode> {
    service
        .list_grants()
        .await
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn grant(
    State(service): State<MeshService>,
    Json(payload): Json<GrantRequest>,
) -> Result<Json<CapabilityGrant>, axum::http::StatusCode> {
    let grant = CapabilityGrant {
        peer_id: payload.peer_id,
        capability: payload.capability,
        expires_at: payload.expires_at,
        note: payload.note,
        created_at: chrono::Utc::now(),
    };
    service
        .grant(grant)
        .await
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn revoke_grant(
    State(service): State<MeshService>,
    Json(payload): Json<RevokeGrantRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    service
        .revoke_grant(&payload.peer_id, &payload.capability)
        .await
        .map(|deleted| {
            Json(serde_json::json!({
                "peer_id": payload.peer_id,
                "capability": payload.capability,
                "revoked": deleted,
            }))
        })
        .map_err(|err| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

async fn send_message(
    State(service): State<MeshService>,
    Json(payload): Json<SendMessageRequest>,
) -> Result<Json<crate::models::SendMessageResponse>, (axum::http::StatusCode, String)> {
    service
        .send_message(payload)
        .await
        .map(Json)
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))
}

async fn list_subscriptions(
    State(service): State<MeshService>,
) -> Result<Json<Vec<SubscriptionRecord>>, axum::http::StatusCode> {
    service
        .list_subscriptions()
        .await
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn subscribe(
    State(service): State<MeshService>,
    Json(payload): Json<SubscribeRequest>,
) -> Result<Json<SubscriptionRecord>, (axum::http::StatusCode, String)> {
    service
        .subscribe(&payload.topic)
        .await
        .map(Json)
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))
}

async fn list_topics(
    State(service): State<MeshService>,
) -> Result<Json<Vec<TopicView>>, axum::http::StatusCode> {
    service
        .list_topics()
        .await
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn create_channel(
    State(service): State<MeshService>,
    Json(payload): Json<CreateChannelRequest>,
) -> Result<Json<CreateChannelResponse>, (axum::http::StatusCode, String)> {
    service
        .create_channel(&payload.topic)
        .await
        .map(Json)
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))
}

async fn broadcast(
    State(service): State<MeshService>,
    Json(payload): Json<BroadcastRequest>,
) -> Result<Json<BroadcastResponse>, (axum::http::StatusCode, String)> {
    service
        .broadcast(payload)
        .await
        .map(Json)
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))
}

async fn discovery_announce(
    State(service): State<MeshService>,
    payload: Option<Json<DiscoveryAnnounceRequest>>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let payload = payload
        .map(|Json(value)| value)
        .unwrap_or(DiscoveryAnnounceRequest {
            host: None,
            port: None,
        });
    service
        .announce_to(match (payload.host, payload.port) {
            (Some(host), Some(port)) => Some((host, port)),
            (None, None) => None,
            _ => {
                return Err((
                    axum::http::StatusCode::BAD_REQUEST,
                    "host and port must be supplied together".to_string(),
                ));
            }
        })
        .await
        .map(|_| Json(serde_json::json!({"status":"announced"})))
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))
}

async fn inbox(
    State(service): State<MeshService>,
    Query(limit): Query<LimitQuery>,
) -> Result<Json<Vec<crate::models::StoredMessage>>, axum::http::StatusCode> {
    service
        .list_inbox(limit.limit.unwrap_or(50).clamp(1, 200))
        .await
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn outbox(
    State(service): State<MeshService>,
    Query(limit): Query<LimitQuery>,
) -> Result<Json<Vec<crate::models::StoredMessage>>, axum::http::StatusCode> {
    service
        .list_outbox(limit.limit.unwrap_or(50).clamp(1, 200))
        .await
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn send_context(
    State(service): State<MeshService>,
    Json(payload): Json<ContextCapsuleRequest>,
) -> Result<Json<crate::models::SendMessageResponse>, (axum::http::StatusCode, String)> {
    service
        .send_context_capsule(payload)
        .await
        .map(Json)
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))
}

async fn list_artifacts(
    State(service): State<MeshService>,
) -> Result<Json<Vec<ArtifactRecord>>, (axum::http::StatusCode, String)> {
    service.list_artifacts().await.map(Json).map_err(|err| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            err.to_string(),
        )
    })
}

async fn offer_artifact(
    State(service): State<MeshService>,
    Json(payload): Json<ArtifactOfferRequest>,
) -> Result<Json<crate::models::SendMessageResponse>, (axum::http::StatusCode, String)> {
    service
        .offer_artifact(payload)
        .await
        .map(Json)
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))
}

async fn fetch_artifact(
    State(service): State<MeshService>,
    Json(payload): Json<ArtifactFetchRequest>,
) -> Result<Json<crate::models::SendMessageResponse>, (axum::http::StatusCode, String)> {
    service
        .fetch_artifact(payload)
        .await
        .map(Json)
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))
}

async fn delegate_work(
    State(service): State<MeshService>,
    Json(payload): Json<DelegateWorkRequest>,
) -> Result<Json<crate::models::SendMessageResponse>, (axum::http::StatusCode, String)> {
    service
        .delegate_work(payload)
        .await
        .map(Json)
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))
}

async fn list_pending_delegate_requests(
    State(service): State<MeshService>,
    Query(limit): Query<LimitQuery>,
) -> Result<Json<Vec<PendingDelegateRequest>>, axum::http::StatusCode> {
    service
        .list_pending_delegate_requests(limit.limit.unwrap_or(50).clamp(1, 200))
        .await
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn approve_delegate_request(
    State(service): State<MeshService>,
    Json(payload): Json<DelegateDecisionRequest>,
) -> Result<Json<DelegateDecisionResponse>, (axum::http::StatusCode, String)> {
    service
        .approve_delegate_request(payload)
        .await
        .map(Json)
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))
}

async fn deny_delegate_request(
    State(service): State<MeshService>,
    Json(payload): Json<DelegateDecisionRequest>,
) -> Result<Json<DelegateDecisionResponse>, (axum::http::StatusCode, String)> {
    service
        .deny_delegate_request(payload)
        .await
        .map(Json)
        .map_err(|err| (axum::http::StatusCode::BAD_REQUEST, err.to_string()))
}
