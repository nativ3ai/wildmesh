use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::Path;
use std::str::FromStr;

use crate::models::{
    CapabilityGrant, ChannelRecord, Envelope, HubAnnouncement, HubPeerRecord, MessageDirection,
    MessageKind, MessageStatus, OwnedChannelRecord, PeerRecord, PendingDelegateRequest,
    StoredMessage, SubscriptionRecord,
};

pub async fn open_pool(db_path: &Path) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))?
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    init_schema(&pool).await?;
    Ok(pool)
}

async fn init_schema(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS identity (
            singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
            peer_id TEXT NOT NULL,
            public_key TEXT NOT NULL,
            signing_secret_key TEXT NOT NULL,
            encryption_public_key TEXT NOT NULL,
            encryption_secret_key TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS peers (
            peer_id TEXT PRIMARY KEY,
            label TEXT,
            agent_label TEXT,
            agent_description TEXT,
            node_type TEXT,
            runtime_name TEXT,
            payment_identity_json TEXT,
            interests_json TEXT NOT NULL DEFAULT '[]',
            host TEXT NOT NULL,
            port INTEGER NOT NULL,
            public_key TEXT NOT NULL,
            encryption_public_key TEXT NOT NULL,
            relay_url TEXT,
            notes TEXT,
            discovered INTEGER NOT NULL DEFAULT 0,
            last_seen_at TEXT,
            created_at TEXT NOT NULL,
            accepts_context_capsules INTEGER NOT NULL DEFAULT 0,
            accepts_artifact_exchange INTEGER NOT NULL DEFAULT 0,
            accepts_delegate_work INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS grants (
            peer_id TEXT NOT NULL,
            capability TEXT NOT NULL,
            expires_at TEXT,
            note TEXT,
            created_at TEXT NOT NULL,
            PRIMARY KEY (peer_id, capability)
        );

        CREATE TABLE IF NOT EXISTS subscriptions (
            topic TEXT PRIMARY KEY,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS channels (
            topic TEXT PRIMARY KEY,
            owner_peer_id TEXT NOT NULL,
            owner_agent_label TEXT,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS peer_topics (
            peer_id TEXT NOT NULL,
            topic TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (peer_id, topic)
        );

        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            direction TEXT NOT NULL,
            peer_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            capability TEXT,
            body_json TEXT NOT NULL,
            status TEXT NOT NULL,
            allowed INTEGER NOT NULL,
            reason TEXT,
            created_at TEXT NOT NULL,
            raw_envelope_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS hub_registry (
            peer_id TEXT PRIMARY KEY,
            agent_label TEXT,
            agent_description TEXT,
            interests_json TEXT NOT NULL DEFAULT '[]',
            host TEXT NOT NULL,
            port INTEGER NOT NULL,
            public_key TEXT NOT NULL,
            encryption_public_key TEXT NOT NULL,
            control_url TEXT NOT NULL,
            announced_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS hub_registry_topics (
            peer_id TEXT NOT NULL,
            topic TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (peer_id, topic)
        );

        CREATE TABLE IF NOT EXISTS relay_queue (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            recipient_peer_id TEXT NOT NULL,
            envelope_json TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;
    let _ = sqlx::query("ALTER TABLE peers ADD COLUMN relay_url TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE peers ADD COLUMN agent_label TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE peers ADD COLUMN agent_description TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE peers ADD COLUMN node_type TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE peers ADD COLUMN runtime_name TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE peers ADD COLUMN payment_identity_json TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE peers ADD COLUMN interests_json TEXT NOT NULL DEFAULT '[]'")
        .execute(pool)
        .await;
    let _ = sqlx::query(
        "ALTER TABLE peers ADD COLUMN accepts_context_capsules INTEGER NOT NULL DEFAULT 0",
    )
    .execute(pool)
    .await;
    let _ = sqlx::query(
        "ALTER TABLE peers ADD COLUMN accepts_artifact_exchange INTEGER NOT NULL DEFAULT 0",
    )
    .execute(pool)
    .await;
    let _ = sqlx::query(
        "ALTER TABLE peers ADD COLUMN accepts_delegate_work INTEGER NOT NULL DEFAULT 0",
    )
    .execute(pool)
    .await;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct IdentityRow {
    pub peer_id: String,
    pub public_key: String,
    pub signing_secret_key: String,
    pub encryption_public_key: String,
    pub encryption_secret_key: String,
}

pub async fn load_identity(pool: &SqlitePool) -> Result<Option<IdentityRow>> {
    let row = sqlx::query(
        "SELECT peer_id, public_key, signing_secret_key, encryption_public_key, encryption_secret_key FROM identity WHERE singleton = 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| IdentityRow {
        peer_id: row.get("peer_id"),
        public_key: row.get("public_key"),
        signing_secret_key: row.get("signing_secret_key"),
        encryption_public_key: row.get("encryption_public_key"),
        encryption_secret_key: row.get("encryption_secret_key"),
    }))
}

pub async fn ensure_identity(pool: &SqlitePool, row: &IdentityRow) -> Result<()> {
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO identity (
            singleton, peer_id, public_key, signing_secret_key, encryption_public_key, encryption_secret_key, created_at
        ) VALUES (1, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&row.peer_id)
    .bind(&row.public_key)
    .bind(&row.signing_secret_key)
    .bind(&row.encryption_public_key)
    .bind(&row.encryption_secret_key)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn upsert_peer(pool: &SqlitePool, peer: &PeerRecord) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO peers (
            peer_id, label, agent_label, agent_description, node_type, runtime_name, payment_identity_json, interests_json,
            host, port, public_key, encryption_public_key, relay_url, notes, discovered, last_seen_at, created_at,
            accepts_context_capsules, accepts_artifact_exchange, accepts_delegate_work
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(peer_id) DO UPDATE SET
            label=excluded.label,
            agent_label=excluded.agent_label,
            agent_description=excluded.agent_description,
            node_type=excluded.node_type,
            runtime_name=excluded.runtime_name,
            payment_identity_json=excluded.payment_identity_json,
            interests_json=excluded.interests_json,
            host=excluded.host,
            port=excluded.port,
            public_key=excluded.public_key,
            encryption_public_key=excluded.encryption_public_key,
            relay_url=excluded.relay_url,
            notes=excluded.notes,
            discovered=excluded.discovered,
            last_seen_at=excluded.last_seen_at,
            accepts_context_capsules=excluded.accepts_context_capsules,
            accepts_artifact_exchange=excluded.accepts_artifact_exchange,
            accepts_delegate_work=excluded.accepts_delegate_work
        "#,
    )
    .bind(&peer.peer_id)
    .bind(&peer.label)
    .bind(&peer.agent_label)
    .bind(&peer.agent_description)
    .bind(&peer.node_type)
    .bind(&peer.runtime_name)
    .bind(match &peer.payment_identity {
        Some(value) => Some(serde_json::to_string(value)?),
        None => None,
    })
    .bind(serde_json::to_string(&peer.interests)?)
    .bind(&peer.host)
    .bind(i64::from(peer.port))
    .bind(&peer.public_key)
    .bind(&peer.encryption_public_key)
    .bind(&peer.relay_url)
    .bind(&peer.notes)
    .bind(if peer.discovered { 1 } else { 0 })
    .bind(peer.last_seen_at.map(|v| v.to_rfc3339()))
    .bind(peer.created_at.to_rfc3339())
    .bind(if peer.accepts_context_capsules { 1 } else { 0 })
    .bind(if peer.accepts_artifact_exchange { 1 } else { 0 })
    .bind(if peer.accepts_delegate_work { 1 } else { 0 })
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_peer(pool: &SqlitePool, peer_id: &str) -> Result<Option<PeerRecord>> {
    let row = sqlx::query("SELECT * FROM peers WHERE peer_id = ?")
        .bind(peer_id)
        .fetch_optional(pool)
        .await?;
    row.map(|row| row_to_peer(&row)).transpose()
}

pub async fn list_peers(pool: &SqlitePool) -> Result<Vec<PeerRecord>> {
    let rows = sqlx::query(
        "SELECT * FROM peers ORDER BY COALESCE(last_seen_at, created_at) DESC, peer_id",
    )
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_peer).collect()
}

pub async fn upsert_grant(pool: &SqlitePool, grant: &CapabilityGrant) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO grants (peer_id, capability, expires_at, note, created_at)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(peer_id, capability) DO UPDATE SET
            expires_at=excluded.expires_at,
            note=excluded.note,
            created_at=excluded.created_at
        "#,
    )
    .bind(&grant.peer_id)
    .bind(&grant.capability)
    .bind(grant.expires_at.map(|v| v.to_rfc3339()))
    .bind(&grant.note)
    .bind(grant.created_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_grants(pool: &SqlitePool) -> Result<Vec<CapabilityGrant>> {
    let rows = sqlx::query("SELECT * FROM grants ORDER BY created_at DESC")
        .fetch_all(pool)
        .await?;
    rows.iter().map(row_to_grant).collect()
}

pub async fn delete_grant(pool: &SqlitePool, peer_id: &str, capability: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM grants WHERE peer_id = ? AND capability = ?")
        .bind(peer_id)
        .bind(capability)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn upsert_subscription(
    pool: &SqlitePool,
    topic: &str,
    created_at: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO subscriptions (topic, created_at)
        VALUES (?, ?)
        ON CONFLICT(topic) DO UPDATE SET created_at=excluded.created_at
        "#,
    )
    .bind(topic)
    .bind(created_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_subscriptions(pool: &SqlitePool) -> Result<Vec<SubscriptionRecord>> {
    let rows = sqlx::query("SELECT topic, created_at FROM subscriptions ORDER BY topic")
        .fetch_all(pool)
        .await?;
    rows.iter()
        .map(|row| {
            Ok(SubscriptionRecord {
                topic: row.get("topic"),
                created_at: parse_datetime(&row.get::<String, _>("created_at"))?,
            })
        })
        .collect()
}

pub async fn get_channel(pool: &SqlitePool, topic: &str) -> Result<Option<ChannelRecord>> {
    let row = sqlx::query(
        "SELECT topic, owner_peer_id, owner_agent_label, created_at FROM channels WHERE topic = ?",
    )
    .bind(topic)
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        Ok(ChannelRecord {
            topic: row.get("topic"),
            owner_peer_id: row.get("owner_peer_id"),
            owner_agent_label: row.get("owner_agent_label"),
            created_at: parse_datetime(&row.get::<String, _>("created_at"))?,
        })
    })
    .transpose()
}

pub async fn upsert_channel(pool: &SqlitePool, channel: &ChannelRecord) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO channels (topic, owner_peer_id, owner_agent_label, created_at)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(topic) DO UPDATE SET
            owner_agent_label=excluded.owner_agent_label,
            created_at=excluded.created_at
        WHERE channels.owner_peer_id = excluded.owner_peer_id
        "#,
    )
    .bind(&channel.topic)
    .bind(&channel.owner_peer_id)
    .bind(&channel.owner_agent_label)
    .bind(channel.created_at.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn replace_owned_channels(
    pool: &SqlitePool,
    owner_peer_id: &str,
    owner_agent_label: Option<&str>,
    channels: &[OwnedChannelRecord],
) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM channels WHERE owner_peer_id = ?")
        .bind(owner_peer_id)
        .execute(&mut *tx)
        .await?;
    for channel in channels {
        sqlx::query(
            r#"
            INSERT INTO channels (topic, owner_peer_id, owner_agent_label, created_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(topic) DO UPDATE SET
                owner_agent_label=excluded.owner_agent_label,
                created_at=excluded.created_at
            WHERE channels.owner_peer_id = excluded.owner_peer_id
            "#,
        )
        .bind(&channel.topic)
        .bind(owner_peer_id)
        .bind(owner_agent_label)
        .bind(channel.created_at.to_rfc3339())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn list_channels(pool: &SqlitePool) -> Result<Vec<ChannelRecord>> {
    let rows = sqlx::query(
        "SELECT topic, owner_peer_id, owner_agent_label, created_at FROM channels ORDER BY created_at ASC, topic",
    )
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(ChannelRecord {
                topic: row.get("topic"),
                owner_peer_id: row.get("owner_peer_id"),
                owner_agent_label: row.get("owner_agent_label"),
                created_at: parse_datetime(&row.get::<String, _>("created_at"))?,
            })
        })
        .collect()
}

pub async fn replace_peer_topics(
    pool: &SqlitePool,
    peer_id: &str,
    topics: &[String],
    updated_at: DateTime<Utc>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM peer_topics WHERE peer_id = ?")
        .bind(peer_id)
        .execute(&mut *tx)
        .await?;
    for topic in topics {
        sqlx::query("INSERT INTO peer_topics (peer_id, topic, updated_at) VALUES (?, ?, ?)")
            .bind(peer_id)
            .bind(topic)
            .bind(updated_at.to_rfc3339())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn list_peer_topic_links(pool: &SqlitePool) -> Result<Vec<(String, String)>> {
    let rows = sqlx::query("SELECT peer_id, topic FROM peer_topics ORDER BY topic, peer_id")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get("peer_id"), row.get("topic")))
        .collect())
}

pub async fn list_peers_by_topic(pool: &SqlitePool, topic: &str) -> Result<Vec<PeerRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT p.* FROM peers p
        INNER JOIN peer_topics pt ON pt.peer_id = p.peer_id
        WHERE pt.topic = ?
        ORDER BY COALESCE(p.last_seen_at, p.created_at) DESC, p.peer_id
        "#,
    )
    .bind(topic)
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_peer).collect()
}

pub async fn has_grant(pool: &SqlitePool, peer_id: &str, capability: Option<&str>) -> Result<bool> {
    let Some(capability) = capability else {
        return Ok(true);
    };
    let row = sqlx::query("SELECT 1 FROM grants WHERE peer_id = ? AND capability = ? LIMIT 1")
        .bind(peer_id)
        .bind(capability)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

pub async fn message_exists(pool: &SqlitePool, message_id: &str) -> Result<bool> {
    let row = sqlx::query("SELECT 1 FROM messages WHERE id = ? LIMIT 1")
        .bind(message_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

pub async fn outbound_message_exists_for_peer(
    pool: &SqlitePool,
    peer_id: &str,
    message_id: &str,
) -> Result<bool> {
    let row = sqlx::query(
        "SELECT 1 FROM messages WHERE id = ? AND direction = 'outbound' AND peer_id = ? LIMIT 1",
    )
    .bind(message_id)
    .bind(peer_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

pub async fn save_message(pool: &SqlitePool, message: &StoredMessage) -> Result<()> {
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO messages (
            id, direction, peer_id, kind, capability, body_json, status, allowed, reason, created_at, raw_envelope_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&message.id)
    .bind(direction_to_str(&message.direction))
    .bind(&message.peer_id)
    .bind(kind_to_str(&message.kind))
    .bind(&message.capability)
    .bind(serde_json::to_string(&message.body)?)
    .bind(status_to_str(&message.status))
    .bind(if message.allowed { 1 } else { 0 })
    .bind(&message.reason)
    .bind(message.created_at.to_rfc3339())
    .bind(serde_json::to_string(&message.raw_envelope)?)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_message(pool: &SqlitePool, message_id: &str) -> Result<Option<StoredMessage>> {
    let row = sqlx::query("SELECT * FROM messages WHERE id = ?")
        .bind(message_id)
        .fetch_optional(pool)
        .await?;
    row.map(|row| row_to_message(&row)).transpose()
}

pub async fn update_message_status(
    pool: &SqlitePool,
    message_id: &str,
    status: MessageStatus,
    reason: Option<&str>,
) -> Result<()> {
    sqlx::query("UPDATE messages SET status = ?, reason = ? WHERE id = ?")
        .bind(status_to_str(&status))
        .bind(reason)
        .bind(message_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_messages(
    pool: &SqlitePool,
    direction: MessageDirection,
    limit: i64,
) -> Result<Vec<StoredMessage>> {
    let rows =
        sqlx::query("SELECT * FROM messages WHERE direction = ? ORDER BY created_at DESC LIMIT ?")
            .bind(direction_to_str(&direction))
            .bind(limit)
            .fetch_all(pool)
            .await?;
    rows.iter().map(row_to_message).collect()
}

pub async fn list_pending_delegate_requests(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<PendingDelegateRequest>> {
    let rows = sqlx::query(
        r#"
        SELECT
            m.id,
            m.peer_id,
            m.capability,
            m.body_json,
            m.created_at,
            p.label,
            p.agent_label,
            p.agent_description,
            g.note AS grant_note,
            CASE WHEN g.peer_id IS NULL THEN 0 ELSE 1 END AS peer_has_capability_grant
        FROM messages m
        LEFT JOIN peers p ON p.peer_id = m.peer_id
        LEFT JOIN grants g ON g.peer_id = m.peer_id AND g.capability = m.capability
        WHERE m.direction = 'inbound'
          AND m.kind = 'delegate_request'
          AND m.status = 'pending'
        ORDER BY m.created_at DESC
        LIMIT ?
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let body = serde_json::from_str::<crate::models::DelegateRequestBody>(
                &row.get::<String, _>("body_json"),
            )?;
            Ok(PendingDelegateRequest {
                message_id: row.get("id"),
                peer_id: row.get("peer_id"),
                peer_label: row.get("label"),
                peer_agent_label: row.get("agent_label"),
                peer_agent_description: row.get("agent_description"),
                task_id: body.task_id,
                task_type: body.task_type,
                instruction: body.instruction,
                input: body.input,
                context: body.context,
                max_output_chars: body.max_output_chars,
                capability: row.get("capability"),
                peer_has_capability_grant: row.get::<i64, _>("peer_has_capability_grant") != 0,
                grant_note: row.get("grant_note"),
                created_at: parse_datetime(&row.get::<String, _>("created_at"))?,
            })
        })
        .collect()
}

pub async fn counts(pool: &SqlitePool) -> Result<(i64, i64, i64, i64, i64)> {
    let peer_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM peers")
        .fetch_one(pool)
        .await?;
    let grant_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM grants")
        .fetch_one(pool)
        .await?;
    let subscription_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM subscriptions")
        .fetch_one(pool)
        .await?;
    let inbox_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE direction = 'inbound'")
            .fetch_one(pool)
            .await?;
    let outbox_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE direction = 'outbound'")
            .fetch_one(pool)
            .await?;
    Ok((
        peer_count,
        grant_count,
        subscription_count,
        inbox_count,
        outbox_count,
    ))
}

pub async fn upsert_hub_announcement(
    pool: &SqlitePool,
    announcement: &HubAnnouncement,
) -> Result<()> {
    let (host, port) = announcement
        .sender_endpoint
        .rsplit_once(':')
        .context("parse sender endpoint")?;
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"
        INSERT INTO hub_registry (
            peer_id, agent_label, agent_description, interests_json, host, port, public_key, encryption_public_key, control_url, announced_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(peer_id) DO UPDATE SET
            agent_label=excluded.agent_label,
            agent_description=excluded.agent_description,
            interests_json=excluded.interests_json,
            host=excluded.host,
            port=excluded.port,
            public_key=excluded.public_key,
            encryption_public_key=excluded.encryption_public_key,
            control_url=excluded.control_url,
            announced_at=excluded.announced_at
        "#,
    )
    .bind(&announcement.sender_peer_id)
    .bind(&announcement.agent_label)
    .bind(&announcement.agent_description)
    .bind(serde_json::to_string(&announcement.interests)?)
    .bind(host)
    .bind(port.parse::<u16>()? as i64)
    .bind(&announcement.sender_public_key)
    .bind(&announcement.sender_encryption_public_key)
    .bind(&announcement.control_url)
    .bind(announcement.issued_at.to_rfc3339())
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM hub_registry_topics WHERE peer_id = ?")
        .bind(&announcement.sender_peer_id)
        .execute(&mut *tx)
        .await?;
    for topic in &announcement.topics {
        sqlx::query(
            "INSERT INTO hub_registry_topics (peer_id, topic, updated_at) VALUES (?, ?, ?)",
        )
        .bind(&announcement.sender_peer_id)
        .bind(topic)
        .bind(announcement.issued_at.to_rfc3339())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn list_hub_peers(pool: &SqlitePool, topic: Option<&str>) -> Result<Vec<HubPeerRecord>> {
    let rows = if let Some(topic) = topic {
        sqlx::query(
            r#"
            SELECT hr.* FROM hub_registry hr
            INNER JOIN hub_registry_topics hrt ON hrt.peer_id = hr.peer_id
            WHERE hrt.topic = ?
            ORDER BY hr.announced_at DESC, hr.peer_id
            "#,
        )
        .bind(topic)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query("SELECT * FROM hub_registry ORDER BY announced_at DESC, peer_id")
            .fetch_all(pool)
            .await?
    };
    let mut peers = Vec::with_capacity(rows.len());
    for row in rows {
        let peer_id: String = row.get("peer_id");
        let topic_rows =
            sqlx::query("SELECT topic FROM hub_registry_topics WHERE peer_id = ? ORDER BY topic")
                .bind(&peer_id)
                .fetch_all(pool)
                .await?;
        peers.push(HubPeerRecord {
            peer_id,
            agent_label: row.get("agent_label"),
            agent_description: row.get("agent_description"),
            interests: serde_json::from_str(&row.get::<String, _>("interests_json"))?,
            host: row.get("host"),
            port: row.get::<i64, _>("port") as u16,
            public_key: row.get("public_key"),
            encryption_public_key: row.get("encryption_public_key"),
            relay_url: Some(row.get("control_url")),
            control_url: row.get("control_url"),
            topics: topic_rows
                .into_iter()
                .map(|topic_row| topic_row.get("topic"))
                .collect(),
            last_seen_at: parse_datetime(&row.get::<String, _>("announced_at"))?,
        });
    }
    Ok(peers)
}

pub async fn enqueue_relay_envelope(
    pool: &SqlitePool,
    recipient_peer_id: &str,
    envelope: &Envelope,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO relay_queue (recipient_peer_id, envelope_json, created_at) VALUES (?, ?, ?)",
    )
    .bind(recipient_peer_id)
    .bind(serde_json::to_string(envelope)?)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn pull_relay_envelopes(
    pool: &SqlitePool,
    recipient_peer_id: &str,
    limit: i64,
) -> Result<Vec<Envelope>> {
    let mut tx = pool.begin().await?;
    let rows = sqlx::query(
        "SELECT id, envelope_json FROM relay_queue WHERE recipient_peer_id = ? ORDER BY id LIMIT ?",
    )
    .bind(recipient_peer_id)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;
    let mut ids = Vec::with_capacity(rows.len());
    let mut envelopes = Vec::with_capacity(rows.len());
    for row in rows {
        ids.push(row.get::<i64, _>("id"));
        envelopes.push(serde_json::from_str::<Envelope>(
            &row.get::<String, _>("envelope_json"),
        )?);
    }
    for id in ids {
        sqlx::query("DELETE FROM relay_queue WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(envelopes)
}

fn row_to_peer(row: &sqlx::sqlite::SqliteRow) -> Result<PeerRecord> {
    Ok(PeerRecord {
        peer_id: row.get("peer_id"),
        label: row.get("label"),
        agent_label: row.get("agent_label"),
        agent_description: row.get("agent_description"),
        node_type: row.try_get::<Option<String>, _>("node_type").ok().flatten(),
        runtime_name: row
            .try_get::<Option<String>, _>("runtime_name")
            .ok()
            .flatten(),
        payment_identity: row
            .try_get::<Option<String>, _>("payment_identity_json")
            .ok()
            .flatten()
            .map(|raw| serde_json::from_str(&raw))
            .transpose()?,
        interests: serde_json::from_str(&row.get::<String, _>("interests_json"))?,
        host: row.get("host"),
        port: row.get::<i64, _>("port") as u16,
        public_key: row.get("public_key"),
        encryption_public_key: row.get("encryption_public_key"),
        relay_url: row.try_get::<Option<String>, _>("relay_url").ok().flatten(),
        notes: row.get("notes"),
        discovered: row.get::<i64, _>("discovered") != 0,
        last_seen_at: opt_datetime(row.try_get::<Option<String>, _>("last_seen_at")?)?,
        created_at: parse_datetime(&row.get::<String, _>("created_at"))?,
        accepts_context_capsules: row
            .try_get::<i64, _>("accepts_context_capsules")
            .map(|value| value != 0)
            .unwrap_or(false),
        accepts_artifact_exchange: row
            .try_get::<i64, _>("accepts_artifact_exchange")
            .map(|value| value != 0)
            .unwrap_or(false),
        accepts_delegate_work: row
            .try_get::<i64, _>("accepts_delegate_work")
            .map(|value| value != 0)
            .unwrap_or(false),
        activity_state: None,
        last_seen_age_secs: None,
    })
}

fn row_to_grant(row: &sqlx::sqlite::SqliteRow) -> Result<CapabilityGrant> {
    Ok(CapabilityGrant {
        peer_id: row.get("peer_id"),
        capability: row.get("capability"),
        expires_at: opt_datetime(row.try_get::<Option<String>, _>("expires_at")?)?,
        note: row.get("note"),
        created_at: parse_datetime(&row.get::<String, _>("created_at"))?,
    })
}

fn row_to_message(row: &sqlx::sqlite::SqliteRow) -> Result<StoredMessage> {
    Ok(StoredMessage {
        id: row.get("id"),
        direction: str_to_direction(&row.get::<String, _>("direction")),
        peer_id: row.get("peer_id"),
        kind: str_to_kind(&row.get::<String, _>("kind")),
        capability: row.get("capability"),
        body: serde_json::from_str(&row.get::<String, _>("body_json"))?,
        status: str_to_status(&row.get::<String, _>("status")),
        allowed: row.get::<i64, _>("allowed") != 0,
        reason: row.get("reason"),
        created_at: parse_datetime(&row.get::<String, _>("created_at"))?,
        raw_envelope: serde_json::from_str(&row.get::<String, _>("raw_envelope_json"))?,
    })
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("parse datetime {value}"))?
        .with_timezone(&Utc))
}

fn opt_datetime(value: Option<String>) -> Result<Option<DateTime<Utc>>> {
    value.map(|v| parse_datetime(&v)).transpose()
}

fn direction_to_str(value: &MessageDirection) -> &'static str {
    match value {
        MessageDirection::Inbound => "inbound",
        MessageDirection::Outbound => "outbound",
    }
}

fn status_to_str(value: &MessageStatus) -> &'static str {
    match value {
        MessageStatus::Received => "received",
        MessageStatus::Pending => "pending",
        MessageStatus::Approved => "approved",
        MessageStatus::Denied => "denied",
        MessageStatus::Blocked => "blocked",
        MessageStatus::Queued => "queued",
        MessageStatus::Delivered => "delivered",
        MessageStatus::Failed => "failed",
    }
}

fn kind_to_str(value: &MessageKind) -> &'static str {
    match value {
        MessageKind::Hello => "hello",
        MessageKind::Broadcast => "broadcast",
        MessageKind::PeerExchange => "peer_exchange",
        MessageKind::TaskOffer => "task_offer",
        MessageKind::TaskResult => "task_result",
        MessageKind::ContextCapsule => "context_capsule",
        MessageKind::ArtifactOffer => "artifact_offer",
        MessageKind::ArtifactFetch => "artifact_fetch",
        MessageKind::ArtifactPayload => "artifact_payload",
        MessageKind::DelegateRequest => "delegate_request",
        MessageKind::DelegateResult => "delegate_result",
        MessageKind::Note => "note",
        MessageKind::Receipt => "receipt",
    }
}

fn str_to_direction(value: &str) -> MessageDirection {
    match value {
        "inbound" => MessageDirection::Inbound,
        _ => MessageDirection::Outbound,
    }
}

fn str_to_status(value: &str) -> MessageStatus {
    match value {
        "received" => MessageStatus::Received,
        "pending" => MessageStatus::Pending,
        "approved" => MessageStatus::Approved,
        "denied" => MessageStatus::Denied,
        "blocked" => MessageStatus::Blocked,
        "queued" => MessageStatus::Queued,
        "delivered" => MessageStatus::Delivered,
        "failed" => MessageStatus::Failed,
        _ => MessageStatus::Failed,
    }
}

fn str_to_kind(value: &str) -> MessageKind {
    match value {
        "hello" => MessageKind::Hello,
        "broadcast" => MessageKind::Broadcast,
        "peer_exchange" => MessageKind::PeerExchange,
        "task_result" => MessageKind::TaskResult,
        "context_capsule" => MessageKind::ContextCapsule,
        "artifact_offer" => MessageKind::ArtifactOffer,
        "artifact_fetch" => MessageKind::ArtifactFetch,
        "artifact_payload" => MessageKind::ArtifactPayload,
        "delegate_request" => MessageKind::DelegateRequest,
        "delegate_result" => MessageKind::DelegateResult,
        "note" => MessageKind::Note,
        "receipt" => MessageKind::Receipt,
        _ => MessageKind::TaskOffer,
    }
}
