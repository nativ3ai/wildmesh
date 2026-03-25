use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Tabs, Wrap,
};
use ratatui::{Frame, Terminal};
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::config::AgentMeshConfig;
use crate::models::{
    CapabilityGrant, MessageKind, PendingDelegateRequest, PeerRecord, StatusResponse,
    StoredMessage, SubscriptionRecord, TopicView,
};

const REFRESH_INTERVAL: Duration = Duration::from_secs(3);
const EVENT_POLL: Duration = Duration::from_millis(120);
const TOAST_TTL: Duration = Duration::from_secs(4);

const LOGO: [&str; 5] = [
    "__        ___ _     ____  __  __ _____ ____  _   _",
    "\\ \\      / (_) |   |  _ \\|  \\/  | ____/ ___|| | | |",
    " \\ \\ /\\ / /| | |   | | | | |\\/| |  _| \\___ \\| |_| |",
    "  \\ V  V / | | |___| |_| | |  | | |___ ___) |  _  |",
    "   \\_/\\_/  |_|_____|____/|_|  |_|_____|____/|_| |_|",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabPage {
    Overview,
    Peers,
    Topics,
    Requests,
    Messages,
    Actions,
    Help,
}

impl TabPage {
    fn all() -> [Self; 7] {
        [
            Self::Overview,
            Self::Peers,
            Self::Topics,
            Self::Requests,
            Self::Messages,
            Self::Actions,
            Self::Help,
        ]
    }

    fn title(self) -> &'static str {
        match self {
            Self::Overview => "OVERVIEW",
            Self::Peers => "PEERS",
            Self::Topics => "TOPICS",
            Self::Requests => "REQUESTS",
            Self::Messages => "MESSAGES",
            Self::Actions => "ACTIONS",
            Self::Help => "HELP",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessagePane {
    Inbox,
    Outbox,
}

impl MessagePane {
    fn toggle(&mut self) {
        *self = match self {
            Self::Inbox => Self::Outbox,
            Self::Outbox => Self::Inbox,
        };
    }

    fn title(self) -> &'static str {
        match self {
            Self::Inbox => "INBOX",
            Self::Outbox => "OUTBOX",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionItem {
    DiscoverNow,
    CreateChannel,
    SubscribeTopic,
    BroadcastTopic,
    GrantSelectedPeer,
    SendNoteSelectedPeer,
    SendSummaryTask,
    ToggleMessagePane,
}

impl ActionItem {
    fn all() -> [Self; 8] {
        [
            Self::DiscoverNow,
            Self::CreateChannel,
            Self::SubscribeTopic,
            Self::BroadcastTopic,
            Self::GrantSelectedPeer,
            Self::SendNoteSelectedPeer,
            Self::SendSummaryTask,
            Self::ToggleMessagePane,
        ]
    }

    fn title(self) -> &'static str {
        match self {
            Self::DiscoverNow => "Discover the wilderness",
            Self::CreateChannel => "Create a public channel",
            Self::SubscribeTopic => "Subscribe to a public topic",
            Self::BroadcastTopic => "Broadcast an update",
            Self::GrantSelectedPeer => "Grant selected peer a capability",
            Self::SendNoteSelectedPeer => "Send selected peer a note",
            Self::SendSummaryTask => "Send selected peer a summary task",
            Self::ToggleMessagePane => "Toggle inbox / outbox",
        }
    }

    fn detail(self) -> &'static str {
        match self {
            Self::DiscoverNow => "Trigger a fresh discovery pass through the mesh bootstrap set.",
            Self::CreateChannel => {
                "Reserve a new exact-name public channel and join it locally."
            }
            Self::SubscribeTopic => {
                "Join an existing public channel so this node can read and publish there."
            }
            Self::BroadcastTopic => "Publish a public message to a topic. Good for open chatter.",
            Self::GrantSelectedPeer => "Grant the selected peer one narrow capability label.",
            Self::SendNoteSelectedPeer => "Send a plain note to the selected peer.",
            Self::SendSummaryTask => {
                "Send a task_offer using the summary capability to the selected peer."
            }
            Self::ToggleMessagePane => {
                "Flip the message view between inbound and outbound traffic."
            }
        }
    }
}

#[derive(Debug, Clone)]
enum ModalKind {
    PeerFilter,
    CreateChannel,
    SubscribeTopic,
    BroadcastTopic,
    BroadcastBody { topic: String },
    GrantCapability { peer_id: String },
    SendNote { peer_id: String },
    SendSummaryTask { peer_id: String },
    DenyRequest { message_id: String },
}

#[derive(Debug, Clone)]
struct ModalState {
    kind: ModalKind,
    title: String,
    prompt: String,
    input: String,
}

impl ModalState {
    fn new(
        kind: ModalKind,
        title: impl Into<String>,
        prompt: impl Into<String>,
        input: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            title: title.into(),
            prompt: prompt.into(),
            input: input.into(),
        }
    }
}

#[derive(Debug, Clone)]
struct Toast {
    message: String,
    created_at: Instant,
    color: Color,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DashboardProfile {
    agent_label: Option<String>,
    agent_description: Option<String>,
    interests: Vec<String>,
    control_url: String,
    p2p_endpoint: String,
    public_api_url: String,
    bootstrap_urls: Vec<String>,
    nat_status: String,
    public_address: Option<String>,
    listen_addrs: Vec<String>,
    external_addrs: Vec<String>,
    upnp_mapped_addrs: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct DashboardSnapshot {
    profile: Option<DashboardProfile>,
    status: Option<StatusResponse>,
    peers: Vec<PeerRecord>,
    grants: Vec<CapabilityGrant>,
    topics: Vec<TopicView>,
    subscriptions: Vec<SubscriptionRecord>,
    pending: Vec<PendingDelegateRequest>,
    inbox: Vec<StoredMessage>,
    outbox: Vec<StoredMessage>,
}

struct DashboardClient {
    base: String,
    http: Client,
}

impl DashboardClient {
    fn new(home: &PathBuf) -> Result<Self> {
        let cfg = AgentMeshConfig::load_or_create(home)?;
        Ok(Self {
            base: cfg.control_url(),
            http: Client::builder().build()?,
        })
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        Ok(self
            .http
            .get(format!("{}{}", self.base, path))
            .send()
            .context("send dashboard GET")?
            .error_for_status()
            .context("dashboard GET status")?
            .json()?)
    }

    fn post<T: DeserializeOwned>(&self, path: &str, payload: &Value) -> Result<T> {
        Ok(self
            .http
            .post(format!("{}{}", self.base, path))
            .json(payload)
            .send()
            .context("send dashboard POST")?
            .error_for_status()
            .context("dashboard POST status")?
            .json()?)
    }

    fn snapshot(&self) -> Result<DashboardSnapshot> {
        Ok(DashboardSnapshot {
            profile: None,
            status: self.get("/v1/status").ok(),
            peers: self.get("/v1/peers").unwrap_or_default(),
            grants: self.get("/v1/capabilities").unwrap_or_default(),
            topics: self.get("/v1/topics").unwrap_or_default(),
            subscriptions: self.get("/v1/subscriptions").unwrap_or_default(),
            pending: self.get("/v1/delegate/pending?limit=50").unwrap_or_default(),
            inbox: self.get("/v1/messages/inbox?limit=50").unwrap_or_default(),
            outbox: self.get("/v1/messages/outbox?limit=50").unwrap_or_default(),
        })
    }

    fn load_profile(&self, home: &PathBuf) -> Result<DashboardProfile> {
        let cfg = AgentMeshConfig::load_or_create(home)?;
        let status = self.get::<StatusResponse>("/v1/status").ok();
        Ok(DashboardProfile {
            agent_label: cfg.agent_label.clone(),
            agent_description: cfg.agent_description.clone(),
            interests: cfg.interests.clone(),
            control_url: cfg.control_url(),
            p2p_endpoint: cfg.p2p_endpoint(),
            public_api_url: cfg.public_api_url(),
            bootstrap_urls: cfg.bootstrap_urls.clone(),
            nat_status: status
                .as_ref()
                .map(|item| item.reachability.nat_status.clone())
                .unwrap_or_else(|| "unknown".to_string()),
            public_address: status
                .as_ref()
                .and_then(|item| item.reachability.public_address.clone()),
            listen_addrs: status
                .as_ref()
                .map(|item| item.reachability.listen_addrs.clone())
                .unwrap_or_default(),
            external_addrs: status
                .as_ref()
                .map(|item| item.reachability.external_addrs.clone())
                .unwrap_or_default(),
            upnp_mapped_addrs: status
                .as_ref()
                .map(|item| item.reachability.upnp_mapped_addrs.clone())
                .unwrap_or_default(),
        })
    }
}

struct DashboardApp {
    home: PathBuf,
    client: DashboardClient,
    tab: usize,
    peer_index: usize,
    request_index: usize,
    message_index: usize,
    action_index: usize,
    message_pane: MessagePane,
    peer_filter: String,
    snapshot: DashboardSnapshot,
    last_refresh: Instant,
    last_discover: Instant,
    splash_started: Instant,
    modal: Option<ModalState>,
    toast: Option<Toast>,
    last_error: Option<String>,
    known_inbox_ids: HashSet<String>,
    unread_inbox_ids: HashSet<String>,
}

impl DashboardApp {
    fn new(home: PathBuf) -> Result<Self> {
        let client = DashboardClient::new(&home)?;
        let mut snapshot = DashboardSnapshot::default();
        let mut last_error = None;
        match client.snapshot() {
            Ok(mut value) => {
                value.profile = Some(client.load_profile(&home)?);
                snapshot = value;
            }
            Err(err) => {
                last_error = Some(err.to_string());
                snapshot.profile = Some(client.load_profile(&home)?);
            }
        }
        let known_inbox_ids = snapshot
            .inbox
            .iter()
            .map(|message| message.id.clone())
            .collect::<HashSet<_>>();
        Ok(Self {
            home,
            client,
            tab: 0,
            peer_index: 0,
            request_index: 0,
            message_index: 0,
            action_index: 0,
            message_pane: MessagePane::Inbox,
            peer_filter: String::new(),
            snapshot,
            last_refresh: Instant::now(),
            last_discover: Instant::now() - Duration::from_secs(60),
            splash_started: Instant::now(),
            modal: None,
            toast: None,
            last_error,
            known_inbox_ids,
            unread_inbox_ids: HashSet::new(),
        })
    }

    fn current_tab(&self) -> TabPage {
        TabPage::all()[self.tab]
    }

    fn select_tab(&mut self, index: usize) {
        self.tab = index.min(TabPage::all().len().saturating_sub(1));
        self.sync_message_alerts();
    }

    fn selected_peer(&self) -> Option<&PeerRecord> {
        self.filtered_peers().get(self.peer_index).copied()
    }

    fn selected_request(&self) -> Option<&PendingDelegateRequest> {
        self.snapshot.pending.get(self.request_index)
    }

    fn filtered_peers(&self) -> Vec<&PeerRecord> {
        let term = self.peer_filter.trim().to_lowercase();
        self.snapshot
            .peers
            .iter()
            .filter(|peer| {
                if term.is_empty() {
                    return true;
                }
                let haystack = [
                    peer.peer_id.as_str(),
                    peer.label.as_deref().unwrap_or_default(),
                    peer.agent_label.as_deref().unwrap_or_default(),
                    peer.agent_description.as_deref().unwrap_or_default(),
                    peer.host.as_str(),
                    &peer.interests.join(" "),
                ]
                .join(" ")
                .to_lowercase();
                haystack.contains(&term)
            })
            .collect()
    }

    fn current_messages(&self) -> &[StoredMessage] {
        match self.message_pane {
            MessagePane::Inbox => &self.snapshot.inbox,
            MessagePane::Outbox => &self.snapshot.outbox,
        }
    }

    fn unread_inbox_count(&self) -> usize {
        self.unread_inbox_ids.len()
    }

    fn has_unread_inbox(&self) -> bool {
        self.unread_inbox_count() > 0
    }

    fn sync_message_alerts(&mut self) {
        if self.current_tab() == TabPage::Messages && self.message_pane == MessagePane::Inbox {
            self.unread_inbox_ids.clear();
        }
    }

    fn overview_preview_peers(&self) -> Vec<&PeerRecord> {
        self.filtered_peers().into_iter().take(5).collect()
    }

    fn action_items(&self) -> [ActionItem; 8] {
        ActionItem::all()
    }

    fn show_toast(&mut self, message: impl Into<String>, color: Color) {
        self.toast = Some(Toast {
            message: message.into(),
            created_at: Instant::now(),
            color,
        });
    }

    fn clear_expired_toast(&mut self) {
        if self
            .toast
            .as_ref()
            .map(|toast| toast.created_at.elapsed() > TOAST_TTL)
            .unwrap_or(false)
        {
            self.toast = None;
        }
    }

    fn refresh(&mut self) {
        match self.client.snapshot() {
            Ok(mut value) => {
                value.profile = self.client.load_profile(&self.home).ok().or(value.profile);
                for message in &value.inbox {
                    if !self.known_inbox_ids.contains(&message.id) {
                        self.unread_inbox_ids.insert(message.id.clone());
                    }
                }
                self.known_inbox_ids
                    .extend(value.inbox.iter().map(|message| message.id.clone()));
                self.snapshot = value;
                self.last_refresh = Instant::now();
                self.last_error = None;
                self.clamp_selection();
                self.sync_message_alerts();
            }
            Err(err) => {
                self.last_error = Some(err.to_string());
                self.show_toast(
                    "daemon unreachable; keeping last known state",
                    Color::Yellow,
                );
            }
        }
    }

    fn discover_now(&mut self) {
        match self
            .client
            .post::<Value>("/v1/discovery/announce", &json!({}))
        {
            Ok(_) => {
                self.last_discover = Instant::now();
                self.refresh();
                self.show_toast("mesh discovery pulse sent", Color::Cyan);
            }
            Err(err) => {
                self.last_error = Some(err.to_string());
                self.show_toast("discovery failed", Color::Red);
            }
        }
    }

    fn subscribe(&mut self, topic: &str) {
        match self
            .client
            .post::<SubscriptionRecord>("/v1/subscriptions", &json!({ "topic": topic }))
        {
            Ok(_) => {
                self.refresh();
                self.show_toast(format!("subscribed to {topic}"), Color::Green);
            }
            Err(err) => {
                self.last_error = Some(err.to_string());
                self.show_toast("subscription failed", Color::Red);
            }
        }
    }

    fn create_channel(&mut self, topic: &str) {
        match self
            .client
            .post::<crate::models::CreateChannelResponse>("/v1/topics", &json!({ "topic": topic }))
        {
            Ok(result) => {
                self.refresh();
                self.show_toast(
                    if result.created {
                        format!("created channel {}", topic)
                    } else {
                        format!("joined owned channel {}", topic)
                    },
                    Color::Green,
                );
            }
            Err(err) => {
                self.last_error = Some(err.to_string());
                self.show_toast("channel creation failed", Color::Red);
            }
        }
    }

    fn broadcast(&mut self, topic: &str, body: &str) {
        let payload = json!({
            "topic": topic,
            "body": parse_body(body),
        });
        match self
            .client
            .post::<crate::models::BroadcastResponse>("/v1/messages/broadcast", &payload)
        {
            Ok(result) => {
                self.refresh();
                self.show_toast(
                    format!("broadcast to {} peers on {}", result.attempted_peers, topic),
                    Color::Green,
                );
            }
            Err(err) => {
                self.last_error = Some(err.to_string());
                self.show_toast("broadcast failed", Color::Red);
            }
        }
    }

    fn grant(&mut self, peer_id: &str, capability: &str) {
        let payload = json!({
            "peer_id": peer_id,
            "capability": capability,
            "expires_at": Value::Null,
            "note": "granted from dashboard",
        });
        match self
            .client
            .post::<CapabilityGrant>("/v1/capabilities/grants", &payload)
        {
            Ok(_) => {
                self.refresh();
                self.show_toast(format!("granted {capability}"), Color::Green);
            }
            Err(err) => {
                self.last_error = Some(err.to_string());
                self.show_toast("grant failed", Color::Red);
            }
        }
    }

    fn send_note(&mut self, peer_id: &str, body: &str) {
        let payload = json!({
            "peer_id": peer_id,
            "kind": "note",
            "body": parse_body(body),
            "capability": Value::Null,
        });
        self.send_payload(payload, "note sent");
    }

    fn send_summary_task(&mut self, peer_id: &str, prompt: &str) {
        let payload = json!({
            "peer_id": peer_id,
            "kind": "task_offer",
            "body": { "prompt": prompt },
            "capability": "summary",
        });
        self.send_payload(payload, "summary task sent");
    }

    fn send_payload(&mut self, payload: Value, success_message: &str) {
        match self
            .client
            .post::<crate::models::SendMessageResponse>("/v1/messages/send", &payload)
        {
            Ok(result) => {
                self.refresh();
                self.show_toast(
                    format!("{} ({})", success_message, result.delivery_status),
                    Color::Green,
                );
            }
            Err(err) => {
                self.last_error = Some(err.to_string());
                self.show_toast("message send failed", Color::Red);
            }
        }
    }

    fn clamp_selection(&mut self) {
        let peer_len = self.filtered_peers().len();
        if peer_len == 0 {
            self.peer_index = 0;
        } else {
            self.peer_index = self.peer_index.min(peer_len.saturating_sub(1));
        }
        let message_len = self.current_messages().len();
        if message_len == 0 {
            self.message_index = 0;
        } else {
            self.message_index = self.message_index.min(message_len.saturating_sub(1));
        }
        let request_len = self.snapshot.pending.len();
        if request_len == 0 {
            self.request_index = 0;
        } else {
            self.request_index = self.request_index.min(request_len.saturating_sub(1));
        }
        self.action_index = self
            .action_index
            .min(self.action_items().len().saturating_sub(1));
    }

    fn open_modal(&mut self, modal: ModalState) {
        self.modal = Some(modal);
    }

    fn open_peer_filter(&mut self) {
        self.open_modal(ModalState::new(
            ModalKind::PeerFilter,
            "PEER FILTER",
            "Filter peers by label, description, host, or interests",
            self.peer_filter.clone(),
        ));
    }

    fn open_subscribe(&mut self) {
        self.open_modal(ModalState::new(
            ModalKind::SubscribeTopic,
            "JOIN CHANNEL",
            "Enter the exact name of an existing channel to join",
            "",
        ));
    }

    fn open_create_channel(&mut self) {
        self.open_modal(ModalState::new(
            ModalKind::CreateChannel,
            "CREATE CHANNEL",
            "Enter an exact public channel name. If it already exists globally, creation fails.",
            "",
        ));
    }

    fn open_broadcast_topic(&mut self) {
        self.open_modal(ModalState::new(
            ModalKind::BroadcastTopic,
            "BROADCAST",
            "Enter the topic to publish to",
            "",
        ));
    }

    fn open_grant(&mut self) {
        if let Some(peer) = self.selected_peer() {
            self.open_modal(ModalState::new(
                ModalKind::GrantCapability {
                    peer_id: peer.peer_id.clone(),
                },
                "GRANT CAPABILITY",
                &format!("Grant a capability to {}", short_peer(&peer.peer_id)),
                "summary",
            ));
        } else {
            self.show_toast("select a peer first", Color::Yellow);
        }
    }

    fn open_note(&mut self) {
        if let Some(peer) = self.selected_peer() {
            self.open_modal(ModalState::new(
                ModalKind::SendNote {
                    peer_id: peer.peer_id.clone(),
                },
                "SEND NOTE",
                &format!("Note for {}", short_peer(&peer.peer_id)),
                "",
            ));
        } else {
            self.show_toast("select a peer first", Color::Yellow);
        }
    }

    fn open_summary_task(&mut self) {
        if let Some(peer) = self.selected_peer() {
            self.open_modal(ModalState::new(
                ModalKind::SendSummaryTask {
                    peer_id: peer.peer_id.clone(),
                },
                "SEND SUMMARY TASK",
                &format!("Prompt for {}", short_peer(&peer.peer_id)),
                "",
            ));
        } else {
            self.show_toast("select a peer first", Color::Yellow);
        }
    }

    fn open_deny_request(&mut self) {
        if let Some(request) = self.selected_request() {
            self.open_modal(ModalState::new(
                ModalKind::DenyRequest {
                    message_id: request.message_id.clone(),
                },
                "DENY REQUEST",
                &format!("Deny task {} from {}", request.task_type, short_peer(&request.peer_id)),
                "denied by local operator",
            ));
        } else {
            self.show_toast("select a pending request first", Color::Yellow);
        }
    }

    fn accept_request(&mut self, always_allow: bool) {
        let Some(request) = self.selected_request() else {
            self.show_toast("select a pending request first", Color::Yellow);
            return;
        };
        let payload = json!({
            "message_id": request.message_id,
            "always_allow": always_allow,
            "grant_note": if always_allow { Some("trusted from dashboard".to_string()) } else { None },
        });
        match self
            .client
            .post::<Value>("/v1/delegate/accept", &payload)
        {
            Ok(_) => {
                self.refresh();
                self.show_toast(
                    if always_allow {
                        "request approved and peer trusted"
                    } else {
                        "request approved once"
                    },
                    Color::Green,
                );
            }
            Err(err) => {
                self.last_error = Some(err.to_string());
                self.show_toast("approval failed", Color::Red);
            }
        }
    }

    fn submit_modal(&mut self) {
        let Some(modal) = self.modal.clone() else {
            return;
        };
        let value = modal.input.trim().to_string();
        match modal.kind {
            ModalKind::PeerFilter => {
                self.peer_filter = value;
                self.peer_index = 0;
                self.modal = None;
            }
            ModalKind::CreateChannel => {
                if value.is_empty() {
                    self.show_toast("channel cannot be empty", Color::Yellow);
                } else {
                    self.create_channel(&value);
                    self.modal = None;
                }
            }
            ModalKind::SubscribeTopic => {
                if value.is_empty() {
                    self.show_toast("topic cannot be empty", Color::Yellow);
                } else {
                    self.subscribe(&value);
                    self.modal = None;
                }
            }
            ModalKind::BroadcastTopic => {
                if value.is_empty() {
                    self.show_toast("topic cannot be empty", Color::Yellow);
                } else {
                    self.open_modal(ModalState::new(
                        ModalKind::BroadcastBody {
                            topic: value.clone(),
                        },
                        "BROADCAST BODY",
                        "Enter JSON or plain text for the broadcast payload",
                        "{\"text\":\"\"}",
                    ));
                }
            }
            ModalKind::BroadcastBody { topic } => {
                if value.is_empty() {
                    self.show_toast("body cannot be empty", Color::Yellow);
                } else {
                    self.broadcast(&topic, &value);
                    self.modal = None;
                }
            }
            ModalKind::GrantCapability { peer_id } => {
                if value.is_empty() {
                    self.show_toast("capability cannot be empty", Color::Yellow);
                } else {
                    self.grant(&peer_id, &value);
                    self.modal = None;
                }
            }
            ModalKind::SendNote { peer_id } => {
                if value.is_empty() {
                    self.show_toast("note cannot be empty", Color::Yellow);
                } else {
                    self.send_note(&peer_id, &value);
                    self.modal = None;
                }
            }
            ModalKind::SendSummaryTask { peer_id } => {
                if value.is_empty() {
                    self.show_toast("task prompt cannot be empty", Color::Yellow);
                } else {
                    self.send_summary_task(&peer_id, &value);
                    self.modal = None;
                }
            }
            ModalKind::DenyRequest { message_id } => {
                let payload = json!({
                    "message_id": message_id,
                    "reason": if value.is_empty() { Value::Null } else { Value::String(value.clone()) },
                });
                match self.client.post::<Value>("/v1/delegate/deny", &payload) {
                    Ok(_) => {
                        self.refresh();
                        self.show_toast("request denied", Color::Yellow);
                        self.modal = None;
                    }
                    Err(err) => {
                        self.last_error = Some(err.to_string());
                        self.show_toast("deny failed", Color::Red);
                    }
                }
            }
        }
    }

    fn perform_action(&mut self) {
        match self.action_items()[self.action_index] {
            ActionItem::DiscoverNow => self.discover_now(),
            ActionItem::CreateChannel => self.open_create_channel(),
            ActionItem::SubscribeTopic => self.open_subscribe(),
            ActionItem::BroadcastTopic => self.open_broadcast_topic(),
            ActionItem::GrantSelectedPeer => self.open_grant(),
            ActionItem::SendNoteSelectedPeer => self.open_note(),
            ActionItem::SendSummaryTask => self.open_summary_task(),
            ActionItem::ToggleMessagePane => {
                self.message_pane.toggle();
                self.message_index = 0;
                self.sync_message_alerts();
            }
        }
    }
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub fn run(home: Option<PathBuf>) -> Result<()> {
    let home = home.unwrap_or_else(AgentMeshConfig::home_dir);
    let mut app = DashboardApp::new(home)?;
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    loop {
        if app.splash_started.elapsed() < Duration::from_millis(1400) {
            terminal.draw(|frame| render_splash(frame, &app))?;
        } else {
            if app.last_refresh.elapsed() > REFRESH_INTERVAL && app.modal.is_none() {
                app.refresh();
            }
            app.clear_expired_toast();
            terminal.draw(|frame| render_dashboard(frame, &app))?;
        }

        if !event::poll(EVENT_POLL)? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        if app.splash_started.elapsed() < Duration::from_millis(1400)
            && matches!(key.code, KeyCode::Enter | KeyCode::Char(' '))
        {
            app.splash_started = Instant::now() - Duration::from_millis(1400);
            continue;
        }

        if let Some(modal) = app.modal.as_mut() {
            match key.code {
                KeyCode::Esc => app.modal = None,
                KeyCode::Enter => app.submit_modal(),
                KeyCode::Backspace => {
                    modal.input.pop();
                }
                KeyCode::Char(c) => {
                    if !key.modifiers.contains(KeyModifiers::CONTROL) {
                        modal.input.push(c);
                    }
                }
                _ => {}
            }
            continue;
        }

        match key.code {
            KeyCode::Char('q') => break,
            KeyCode::Char('1') => app.select_tab(0),
            KeyCode::Char('2') => app.select_tab(1),
            KeyCode::Char('3') => app.select_tab(2),
            KeyCode::Char('4') => app.select_tab(3),
            KeyCode::Char('5') => app.select_tab(4),
            KeyCode::Char('6') => app.select_tab(5),
            KeyCode::Char('7') | KeyCode::Char('?') => app.select_tab(6),
            KeyCode::Tab => app.select_tab((app.tab + 1) % TabPage::all().len()),
            KeyCode::BackTab => {
                let next = if app.tab == 0 {
                    TabPage::all().len() - 1
                } else {
                    app.tab - 1
                };
                app.select_tab(next);
            }
            KeyCode::Char('r') => app.refresh(),
            KeyCode::Char('a') if app.current_tab() == TabPage::Requests => app.accept_request(false),
            KeyCode::Char('w') if app.current_tab() == TabPage::Requests => app.accept_request(true),
            KeyCode::Char('d') if app.current_tab() == TabPage::Requests => app.open_deny_request(),
            KeyCode::Char('d') => app.discover_now(),
            KeyCode::Char('/') => app.open_peer_filter(),
            KeyCode::Char('c') => app.open_create_channel(),
            KeyCode::Char('s') => app.open_subscribe(),
            KeyCode::Char('b') => app.open_broadcast_topic(),
            KeyCode::Char('g') => app.open_grant(),
            KeyCode::Char('n') => app.open_note(),
            KeyCode::Char('t') => app.open_summary_task(),
            KeyCode::Char('m') => {
                app.message_pane.toggle();
                app.message_index = 0;
                app.sync_message_alerts();
            }
            KeyCode::Enter if app.current_tab() == TabPage::Actions => app.perform_action(),
            KeyCode::Down | KeyCode::Char('j') => match app.current_tab() {
                TabPage::Overview | TabPage::Peers => {
                    let len = app.filtered_peers().len();
                    if len > 0 {
                        app.peer_index = (app.peer_index + 1).min(len - 1);
                    }
                }
                TabPage::Messages => {
                    let len = app.current_messages().len();
                    if len > 0 {
                        app.message_index = (app.message_index + 1).min(len - 1);
                    }
                }
                TabPage::Requests => {
                    let len = app.snapshot.pending.len();
                    if len > 0 {
                        app.request_index = (app.request_index + 1).min(len - 1);
                    }
                }
                TabPage::Actions => {
                    let len = app.action_items().len();
                    app.action_index = (app.action_index + 1).min(len - 1);
                }
                _ => {}
            },
            KeyCode::Up | KeyCode::Char('k') => match app.current_tab() {
                TabPage::Overview | TabPage::Peers => {
                    app.peer_index = app.peer_index.saturating_sub(1);
                }
                TabPage::Messages => {
                    app.message_index = app.message_index.saturating_sub(1);
                }
                TabPage::Requests => {
                    app.request_index = app.request_index.saturating_sub(1);
                }
                TabPage::Actions => {
                    app.action_index = app.action_index.saturating_sub(1);
                }
                _ => {}
            },
            _ => {}
        }
        app.clamp_selection();
    }

    Ok(())
}

fn render_splash(frame: &mut Frame, app: &DashboardApp) {
    let area = frame.area();
    let progress = (app.splash_started.elapsed().as_millis() as f64 / 1400.0).clamp(0.0, 1.0);
    let visible_lines = ((LOGO.len() as f64) * progress).ceil() as usize;
    let title = LOGO
        .iter()
        .take(visible_lines.max(1))
        .map(|line| Line::from(Span::styled(*line, Style::default().fg(Color::Cyan))))
        .collect::<Vec<_>>();
    let gauge = Gauge::default()
        .block(block("BOOT"))
        .gauge_style(Style::default().fg(Color::LightGreen).bg(Color::Black))
        .percent((progress * 100.0) as u16)
        .label("joining the mesh");
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Length(8),
            Constraint::Length(3),
            Constraint::Percentage(25),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(Text::from(title))
            .alignment(Alignment::Center)
            .block(block("WILDMESH")),
        layout[1],
    );
    frame.render_widget(gauge, layout[2]);
}

fn render_dashboard(frame: &mut Frame, app: &DashboardApp) {
    let area = frame.area();
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(area);

    render_header(frame, root[0], app);

    match app.current_tab() {
        TabPage::Overview => render_overview(frame, root[1], app),
        TabPage::Peers => render_peers(frame, root[1], app),
        TabPage::Topics => render_topics(frame, root[1], app),
        TabPage::Requests => render_requests(frame, root[1], app),
        TabPage::Messages => render_messages(frame, root[1], app),
        TabPage::Actions => render_actions(frame, root[1], app),
        TabPage::Help => render_help(frame, root[1], app),
    }

    render_footer(frame, root[2], app);

    if let Some(modal) = &app.modal {
        render_modal(frame, area, modal);
    }
}

fn render_header(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let header = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(32), Constraint::Min(10)])
        .split(area);

    let status = app
        .snapshot
        .status
        .as_ref()
        .map(|status| {
            format!(
                "peer {}  nat {}",
                short_peer(&status.identity.peer_id),
                status.reachability.nat_status
            )
        })
        .unwrap_or_else(|| "daemon offline".to_string());

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "WILDMESH",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled("P2P WILDERNESS", Style::default().fg(Color::LightMagenta)),
        ]))
        .block(block(&status)),
        header[0],
    );

    let titles = TabPage::all()
        .into_iter()
        .map(|tab| {
            let mut title = tab.title().to_string();
            if tab == TabPage::Requests && !app.snapshot.pending.is_empty() {
                title.push_str(" !");
            }
            if tab == TabPage::Messages && app.has_unread_inbox() {
                title.push_str(" *");
            }
            Line::from(Span::styled(title, Style::default().fg(Color::White)))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Tabs::new(titles)
            .select(app.tab)
            .block(block("MENU"))
            .style(Style::default().fg(Color::Gray))
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        header[1],
    );
}

fn render_overview(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(40),
            Constraint::Min(30),
            Constraint::Length(34),
        ])
        .split(area);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Min(6),
        ])
        .split(columns[0]);
    let middle = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(14), Constraint::Min(10)])
        .split(columns[1]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(12), Constraint::Min(10)])
        .split(columns[2]);

    let profile = app.snapshot.profile.clone().unwrap_or_default();
    let status = app.snapshot.status.clone();
    let worker_alive = status
        .as_ref()
        .map(|value| value.reachability.mesh_worker_alive)
        .unwrap_or(false);
    let worker_error = status
        .as_ref()
        .and_then(|value| value.reachability.mesh_worker_error.clone());
    let node_lines = vec![
        Line::from(vec![
            Span::styled("label ", neon(Color::LightGreen)),
            Span::raw(profile.agent_label.unwrap_or_else(|| "<unset>".to_string())),
        ]),
        Line::from(vec![
            Span::styled("desc  ", neon(Color::LightGreen)),
            Span::raw(
                profile
                    .agent_description
                    .unwrap_or_else(|| "<unset>".to_string()),
            ),
        ]),
        Line::from(vec![
            Span::styled("mesh  ", neon(Color::LightGreen)),
            Span::raw(profile.interests.join(", ")),
        ]),
        Line::from(vec![
            Span::styled("state ", neon(Color::LightGreen)),
            Span::raw(if worker_alive { "live" } else { "offline" }),
        ]),
        Line::from(vec![
            Span::styled("peer  ", neon(Color::LightGreen)),
            Span::raw(
                status
                    .as_ref()
                    .map(|value| short_peer(&value.identity.peer_id))
                    .unwrap_or_else(|| "<offline>".to_string()),
            ),
        ]),
        Line::from(vec![
            Span::styled("p2p   ", neon(Color::LightGreen)),
            Span::raw(
                status
                    .as_ref()
                    .map(|value| value.identity.p2p_endpoint.clone())
                    .unwrap_or_else(|| profile.p2p_endpoint.clone()),
            ),
        ]),
        Line::from(vec![
            Span::styled("ctrl  ", neon(Color::LightGreen)),
            Span::raw(profile.control_url),
        ]),
    ];
    let mut node_text = Text::from(node_lines);
    if let Some(err) = worker_error {
        node_text.extend([
            Line::default(),
            Line::from(vec![
                Span::styled("mesh  ", neon(Color::Red)),
                Span::raw(err),
            ]),
        ]);
    }
    frame.render_widget(
        Paragraph::new(node_text)
            .block(block("NODE"))
            .wrap(Wrap { trim: true }),
        left[0],
    );

    let reach = status.as_ref().map(|value| &value.reachability);
    let reach_lines = vec![
        Line::from(vec![
            Span::styled("nat     ", neon(Color::Yellow)),
            Span::raw(
                reach
                    .map(|item| item.nat_status.as_str())
                    .unwrap_or("unknown"),
            ),
        ]),
        Line::from(vec![
            Span::styled("public  ", neon(Color::Yellow)),
            Span::raw(
                reach
                    .and_then(|item| item.public_address.as_deref())
                    .unwrap_or("<none>"),
            ),
        ]),
        Line::from(vec![
            Span::styled("listen  ", neon(Color::Yellow)),
            Span::raw(
                reach
                    .map(|item| item.listen_addrs.len())
                    .unwrap_or(0)
                    .to_string(),
            ),
        ]),
        Line::from(vec![
            Span::styled("extern  ", neon(Color::Yellow)),
            Span::raw(
                reach
                    .map(|item| item.external_addrs.len())
                    .unwrap_or(0)
                    .to_string(),
            ),
        ]),
        Line::from(vec![
            Span::styled("upnp    ", neon(Color::Yellow)),
            Span::raw(
                reach
                    .map(|item| item.upnp_mapped_addrs.len())
                    .unwrap_or(0)
                    .to_string(),
            ),
        ]),
        Line::from(vec![
            Span::styled("last d  ", neon(Color::Yellow)),
            Span::raw(format!("{}s ago", app.last_discover.elapsed().as_secs())),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(reach_lines)
            .block(block("REACHABILITY"))
            .wrap(Wrap { trim: true }),
        left[1],
    );

    let counts = vec![
        gauge_line("PEERS", app.snapshot.peers.len() as u16, 50, Color::Cyan),
        gauge_line(
            "SUBS",
            app.snapshot.subscriptions.len() as u16,
            20,
            Color::LightMagenta,
        ),
        gauge_line("PENDING", app.snapshot.pending.len() as u16, 20, Color::LightRed),
        gauge_line("INBOX", app.snapshot.inbox.len() as u16, 50, Color::Green),
        gauge_line("UNREAD", app.unread_inbox_count() as u16, 20, Color::Red),
        gauge_line(
            "OUTBOX",
            app.snapshot.outbox.len() as u16,
            50,
            Color::Yellow,
        ),
    ];
    let counts_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3); 6])
        .split(left[2]);
    for (idx, gauge) in counts.into_iter().enumerate() {
        frame.render_widget(gauge, counts_layout[idx]);
    }

    let preview_peers = app.overview_preview_peers();
    let preview_items = if preview_peers.is_empty() {
        if worker_alive {
            vec![
                ListItem::new("No WildMesh peers discovered yet. Press d to pulse discovery."),
                ListItem::new("LAN peers appear quickly. Internet peers only show up when other live WildMesh nodes are advertising."),
            ]
        } else {
            vec![
                ListItem::new("Mesh worker is offline."),
                ListItem::new("Run `wildmesh setup ...` or `wildmesh run` for this home before expecting peers."),
            ]
        }
    } else {
        preview_peers
            .iter()
            .map(|peer| {
                let title = peer
                    .agent_label
                    .clone()
                    .or_else(|| peer.label.clone())
                    .unwrap_or_else(|| short_peer(&peer.peer_id));
                let detail = format!(
                    "[{}] {} {}",
                    peer.activity_state.as_deref().unwrap_or("unknown"),
                    peer.host,
                    if peer.interests.is_empty() {
                        String::from("-")
                    } else {
                        peer.interests.join(", ")
                    }
                );
                ListItem::new(vec![
                    Line::from(Span::styled(title, Style::default().fg(Color::White))),
                    Line::from(Span::styled(detail, Style::default().fg(Color::DarkGray))),
                ])
            })
            .collect()
    };
    let mut preview_state = ListState::default();
    if !preview_peers.is_empty() {
        preview_state.select(Some(app.peer_index.min(preview_peers.len() - 1)));
    }
    frame.render_stateful_widget(
        List::new(preview_items)
            .block(block("PEER PREVIEW"))
            .highlight_style(
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> "),
        middle[0],
        &mut preview_state,
    );

    let preview_detail = if let Some(peer) = app.selected_peer() {
        vec![
            Line::from(Span::styled("SELECTED PEER", neon(Color::Cyan))),
            Line::from(""),
            Line::from(vec![
                Span::styled("name  ", neon(Color::LightGreen)),
                Span::raw(
                    peer.agent_label
                        .clone()
                        .or_else(|| peer.label.clone())
                        .unwrap_or_else(|| short_peer(&peer.peer_id)),
                ),
            ]),
            Line::from(vec![
                Span::styled("state ", neon(Color::LightGreen)),
                Span::raw(
                    peer.activity_state
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                ),
            ]),
            Line::from(vec![
                Span::styled("host  ", neon(Color::LightGreen)),
                Span::raw(format!("{}:{}", peer.host, peer.port)),
            ]),
            Line::from(vec![
                Span::styled("tags  ", neon(Color::LightGreen)),
                Span::raw(if peer.interests.is_empty() {
                    "-".to_string()
                } else {
                    peer.interests.join(", ")
                }),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "j/k move  g grant  n note  t task",
                neon(Color::LightMagenta),
            )),
        ]
    } else {
        vec![
            Line::from(Span::styled("SELECTED PEER", neon(Color::Cyan))),
            Line::from(""),
            Line::from("No peer selected yet."),
            Line::from("Press d to pulse discovery."),
            Line::from("Use j/k to move the peer preview."),
        ]
    };
    frame.render_widget(
        Paragraph::new(Text::from(preview_detail))
            .block(block("LIVE INTERACTION"))
            .wrap(Wrap { trim: true }),
        middle[1],
    );

    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(Span::styled("QUICK START", neon(Color::Cyan))),
            Line::from(""),
            Line::from("1. press d to refresh discovery"),
            Line::from("2. use j/k to inspect peer preview"),
            Line::from("3. use g, n, or t to interact"),
            Line::from("4. review REQUESTS to approve or deny delegated work"),
            Line::from(""),
            Line::from(Span::styled("WILDERNESS RULES", neon(Color::LightMagenta))),
            Line::from("discovery is open; authority stays local"),
            Line::from("remote payloads are still untrusted"),
            Line::from("grants are narrow and explicit"),
            Line::from("broadcasts are for chatter, not power"),
        ]))
        .block(block("OPERATOR DECK"))
        .wrap(Wrap { trim: true }),
        right[0],
    );

    let latest_inbox = app.snapshot.inbox.first();
    let log_lines = if let Some(error) = &app.last_error {
        vec![
            Line::from(Span::styled(
                "LAST ERROR",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(error.clone()),
        ]
    } else {
        let mut lines = vec![
            Line::from(Span::styled("SIGNAL", neon(Color::Cyan))),
            Line::from("daemon reachable"),
            Line::from(format!(
                "refresh age {}s",
                app.last_refresh.elapsed().as_secs()
            )),
            Line::from(format!("bootstrap peers {}", profile.bootstrap_urls.len())),
            Line::from(""),
            Line::from(Span::styled(
                if app.has_unread_inbox() {
                    format!("NEW INBOX * {}", app.unread_inbox_count())
                } else {
                    "NEW INBOX none".to_string()
                },
                if app.has_unread_inbox() {
                    neon(Color::Red)
                } else {
                    neon(Color::LightGreen)
                },
            )),
            Line::from(Span::styled(
                if app.snapshot.pending.is_empty() {
                    "PENDING REQUESTS none".to_string()
                } else {
                    format!("PENDING REQUESTS ! {}", app.snapshot.pending.len())
                },
                if app.snapshot.pending.is_empty() {
                    neon(Color::LightGreen)
                } else {
                    neon(Color::LightRed)
                },
            )),
        ];
        if let Some(message) = latest_inbox {
            lines.push(Line::from(format!(
                "{} from {}",
                kind_label(&message.kind),
                short_peer(&message.peer_id)
            )));
            lines.push(Line::from(format_timestamp(message.created_at)));
        }
        lines
    };
    frame.render_widget(
        Paragraph::new(Text::from(log_lines))
            .block(block("SIGNAL"))
            .wrap(Wrap { trim: true }),
        right[1],
    );
}

fn render_peers(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);
    let peers = app.filtered_peers();
    let items = if peers.is_empty() {
        vec![ListItem::new("No peers matched the current filter.")]
    } else {
        peers
            .iter()
            .map(|peer| {
                let title = peer
                    .agent_label
                    .clone()
                    .or_else(|| peer.label.clone())
                    .unwrap_or_else(|| short_peer(&peer.peer_id));
                let detail = format!(
                    "{}  [{}]  {}  {}",
                    short_peer(&peer.peer_id),
                    peer.activity_state.as_deref().unwrap_or("unknown"),
                    peer.interests.join(", "),
                    peer.host
                );
                ListItem::new(vec![
                    Line::from(Span::styled(title, Style::default().fg(Color::White))),
                    Line::from(Span::styled(detail, Style::default().fg(Color::DarkGray))),
                ])
            })
            .collect()
    };
    let mut state = ListState::default();
    if !peers.is_empty() {
        state.select(Some(app.peer_index));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(block(&format!(
                "PEERS  [{}]  filter={}",
                peers.len(),
                if app.peer_filter.is_empty() {
                    "<none>"
                } else {
                    app.peer_filter.as_str()
                }
            )))
            .highlight_style(
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> "),
        columns[0],
        &mut state,
    );

    let detail = peers.get(app.peer_index).copied();
    let detail_text = if let Some(peer) = detail {
        let granted = app
            .snapshot
            .grants
            .iter()
            .filter(|grant| grant.peer_id == peer.peer_id)
            .map(|grant| grant.capability.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Text::from(vec![
            Line::from(vec![
                Span::styled("agent ", neon(Color::LightGreen)),
                Span::raw(
                    peer.agent_label
                        .clone()
                        .unwrap_or_else(|| "<unknown>".to_string()),
                ),
            ]),
            Line::from(vec![
                Span::styled("peer  ", neon(Color::LightGreen)),
                Span::raw(peer.peer_id.clone()),
            ]),
            Line::from(vec![
                Span::styled("desc  ", neon(Color::LightGreen)),
                Span::raw(
                    peer.agent_description
                        .clone()
                        .unwrap_or_else(|| "-".to_string()),
                ),
            ]),
            Line::from(vec![
                Span::styled("host  ", neon(Color::LightGreen)),
                Span::raw(format!("{}:{}", peer.host, peer.port)),
            ]),
            Line::from(vec![
                Span::styled("tags  ", neon(Color::LightGreen)),
                Span::raw(if peer.interests.is_empty() {
                    "-".to_string()
                } else {
                    peer.interests.join(", ")
                }),
            ]),
            Line::from(vec![
                Span::styled("route ", neon(Color::LightGreen)),
                Span::raw(if peer.relay_url.is_some() {
                    "relay-assisted"
                } else {
                    "direct-advertised"
                }),
            ]),
            Line::from(vec![
                Span::styled("state ", neon(Color::LightGreen)),
                Span::raw(
                    peer.activity_state
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                ),
            ]),
            Line::from(vec![
                Span::styled("seen  ", neon(Color::LightGreen)),
                Span::raw(match (peer.last_seen_age_secs, peer.last_seen_at) {
                    (Some(age), Some(timestamp)) => {
                        format!("{}s ago  ({})", age, format_timestamp(timestamp))
                    }
                    (_, Some(timestamp)) => format_timestamp(timestamp),
                    _ => "never".to_string(),
                }),
            ]),
            Line::from(vec![
                Span::styled("grant ", neon(Color::LightGreen)),
                Span::raw(if granted.is_empty() {
                    "<none>".to_string()
                } else {
                    granted
                }),
            ]),
            Line::from(""),
            Line::from(Span::styled("ACTIONS", neon(Color::LightMagenta))),
            Line::from("g grant summary"),
            Line::from("n send note"),
            Line::from("t send summary task"),
            Line::from("/ edit peer filter"),
        ])
    } else {
        Text::from("No peer selected.")
    };
    frame.render_widget(
        Paragraph::new(detail_text)
            .block(block("PEER DETAIL"))
            .wrap(Wrap { trim: true }),
        columns[1],
    );
}

fn render_topics(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);
    let channel_items = if app.snapshot.topics.is_empty() {
        vec![ListItem::new("No public channels visible yet. Press c to create one or d to pulse discovery.")]
    } else {
        app.snapshot
            .topics
            .iter()
            .map(|item| {
                let owner = item
                    .owner_agent_label
                    .clone()
                    .unwrap_or_else(|| short_peer(&item.owner_peer_id));
                let state = if item.local_subscribed {
                    "joined"
                } else {
                    "remote"
                };
                ListItem::new(vec![
                    Line::from(Span::styled(
                        item.topic.clone(),
                        Style::default().fg(Color::White),
                    )),
                    Line::from(Span::styled(
                        format!(
                            "{} by {} :: {} peers ({} active)",
                            state, owner, item.peer_count, item.active_peer_count
                        ),
                        Style::default().fg(Color::DarkGray),
                    )),
                ])
            })
            .collect()
    };
    frame.render_widget(
        List::new(channel_items).block(block("CHANNELS")),
        layout[0],
    );

    let mut lines = vec![
        Line::from(Span::styled("HOW TO USE CHANNELS", neon(Color::Cyan))),
        Line::from(""),
        Line::from("c  create a new public channel"),
        Line::from("s  join an existing public channel"),
        Line::from("b  broadcast a public update"),
        Line::from("d  force a discovery pulse"),
        Line::from(""),
        Line::from(Span::styled("NOTES", neon(Color::LightMagenta))),
        Line::from("Channels are global mesh lanes with exact names."),
        Line::from("Create reserves a name if no known peer already owns it."),
        Line::from("Join subscribes locally so this node can read and publish there."),
        Line::from("Broadcasts are public chatter, not local authority."),
        Line::from(""),
    ];
    if let Some(topic) = app.snapshot.topics.first() {
        lines.push(Line::from(Span::styled(
            "FIRST VISIBLE CHANNEL",
            neon(Color::Yellow),
        )));
        lines.push(Line::from(topic.topic.clone()));
        lines.push(Line::from(Span::styled(
            format!(
                "owner {} :: {} members",
                topic.owner_agent_label
                    .clone()
                    .unwrap_or_else(|| short_peer(&topic.owner_peer_id)),
                topic.peer_count
            ),
            Style::default().fg(Color::DarkGray),
        )));
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block("TOPIC CONSOLE"))
            .wrap(Wrap { trim: true }),
        layout[1],
    );
}

fn render_requests(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);
    let items = if app.snapshot.pending.is_empty() {
        vec![ListItem::new("No pending delegate requests.")]
    } else {
        app.snapshot
            .pending
            .iter()
            .map(|item| {
                let who = item
                    .peer_agent_label
                    .clone()
                    .or_else(|| item.peer_label.clone())
                    .unwrap_or_else(|| short_peer(&item.peer_id));
                let line1 = format!("{}  {}", who, item.task_type);
                let line2 = format!(
                    "{}  {}{}",
                    short_peer(&item.peer_id),
                    format_timestamp(item.created_at),
                    if item.peer_has_capability_grant {
                        "  [trusted]"
                    } else {
                        "  [review]"
                    }
                );
                ListItem::new(vec![
                    Line::from(Span::styled(line1, Style::default().fg(Color::White))),
                    Line::from(Span::styled(line2, Style::default().fg(Color::DarkGray))),
                ])
            })
            .collect()
    };
    let mut state = ListState::default();
    if !app.snapshot.pending.is_empty() {
        state.select(Some(app.request_index));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(block(&format!("PENDING [{}]", app.snapshot.pending.len())))
            .highlight_style(
                Style::default()
                    .bg(Color::LightRed)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> "),
        layout[0],
        &mut state,
    );

    let detail = if let Some(request) = app.selected_request() {
        let input = serde_json::to_string_pretty(&request.input)
            .unwrap_or_else(|_| "<input decode failed>".to_string());
        let context = request
            .context
            .as_ref()
            .map(|value| serde_json::to_string_pretty(value).unwrap_or_else(|_| "<context decode failed>".to_string()))
            .unwrap_or_else(|| "null".to_string());
        let mut lines = vec![
            Line::from(Span::styled("REQUEST DETAIL", neon(Color::Cyan))),
            Line::from(""),
            Line::from(vec![
                Span::styled("from   ", neon(Color::LightGreen)),
                Span::raw(
                    request
                        .peer_agent_label
                        .clone()
                        .or_else(|| request.peer_label.clone())
                        .unwrap_or_else(|| short_peer(&request.peer_id)),
                ),
            ]),
            Line::from(vec![
                Span::styled("peer   ", neon(Color::LightGreen)),
                Span::raw(request.peer_id.clone()),
            ]),
            Line::from(vec![
                Span::styled("desc   ", neon(Color::LightGreen)),
                Span::raw(
                    request
                        .peer_agent_description
                        .clone()
                        .unwrap_or_else(|| "<none>".to_string()),
                ),
            ]),
            Line::from(vec![
                Span::styled("task   ", neon(Color::LightGreen)),
                Span::raw(request.task_type.clone()),
            ]),
            Line::from(vec![
                Span::styled("cap    ", neon(Color::LightGreen)),
                Span::raw(request.capability.clone().unwrap_or_else(|| "<none>".to_string())),
            ]),
            Line::from(vec![
                Span::styled("trust  ", neon(Color::LightGreen)),
                Span::raw(if request.peer_has_capability_grant {
                    "delegate_work allowed".to_string()
                } else {
                    "review required".to_string()
                }),
            ]),
            Line::from(vec![
                Span::styled("time   ", neon(Color::LightGreen)),
                Span::raw(format_timestamp(request.created_at)),
            ]),
        ];
        if let Some(note) = request.grant_note.clone().filter(|value| !value.trim().is_empty()) {
            lines.push(Line::from(vec![
                Span::styled("note   ", neon(Color::LightGreen)),
                Span::raw(note),
            ]));
        }
        lines.extend([
            Line::from(""),
            Line::from(Span::styled("INSTRUCTION", neon(Color::Yellow))),
            Line::from(request.instruction.clone()),
            Line::from(""),
            Line::from(Span::styled("INPUT", neon(Color::LightMagenta))),
        ]);
        for line in input.lines() {
            lines.push(Line::from(line.to_string()));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("CONTEXT", neon(Color::LightMagenta))));
        for line in context.lines() {
            lines.push(Line::from(line.to_string()));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "a accept once   w trust + accept   d deny   j/k move",
            neon(Color::Cyan),
        )));
        Text::from(lines)
    } else {
        Text::from("No pending request selected.")
    };
    frame.render_widget(
        Paragraph::new(detail)
            .block(block("APPROVAL CONSOLE"))
            .wrap(Wrap { trim: false }),
        layout[1],
    );
}

fn render_messages(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);
    let messages = app.current_messages();
    let items = if messages.is_empty() {
        vec![ListItem::new("No messages in this lane yet.")]
    } else {
        messages
            .iter()
            .map(|item| {
                let line1 = format!(
                    "{}  {}  {}",
                    short_peer(&item.peer_id),
                    kind_label(&item.kind),
                    status_label(&item.status)
                );
                let line2 = format_timestamp(item.created_at);
                ListItem::new(vec![
                    Line::from(Span::styled(line1, Style::default().fg(Color::White))),
                    Line::from(Span::styled(line2, Style::default().fg(Color::DarkGray))),
                ])
            })
            .collect()
    };
    let mut state = ListState::default();
    if !messages.is_empty() {
        state.select(Some(app.message_index));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(block(&format!(
                "{}  [{}]",
                app.message_pane.title(),
                messages.len()
            )))
            .highlight_style(
                Style::default()
                    .bg(Color::LightMagenta)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> "),
        layout[0],
        &mut state,
    );

    let detail = messages.get(app.message_index);
    let body_text = detail
        .map(|message| {
            serde_json::to_string_pretty(&message.body)
                .unwrap_or_else(|_| "<body decode failed>".to_string())
        })
        .unwrap_or_else(|| "No message selected.".to_string());
    let meta_lines = if let Some(message) = detail {
        vec![
            Line::from(vec![
                Span::styled("peer   ", neon(Color::LightGreen)),
                Span::raw(message.peer_id.clone()),
            ]),
            Line::from(vec![
                Span::styled("kind   ", neon(Color::LightGreen)),
                Span::raw(kind_label(&message.kind)),
            ]),
            Line::from(vec![
                Span::styled("status ", neon(Color::LightGreen)),
                Span::raw(status_label(&message.status)),
            ]),
            Line::from(vec![
                Span::styled("time   ", neon(Color::LightGreen)),
                Span::raw(format_timestamp(message.created_at)),
            ]),
            Line::from(""),
            Line::from(Span::styled("BODY", neon(Color::Cyan))),
        ]
    } else {
        vec![Line::from("No message selected.")]
    };
    let mut text = meta_lines;
    for line in body_text.lines() {
        text.push(Line::from(line.to_string()));
    }
    frame.render_widget(
        Paragraph::new(Text::from(text))
            .block(block("MESSAGE DETAIL"))
            .wrap(Wrap { trim: false }),
        layout[1],
    );
}

fn render_actions(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);
    let actions = app.action_items();
    let items = actions
        .iter()
        .map(|item| ListItem::new(item.title()))
        .collect::<Vec<_>>();
    let mut state = ListState::default();
    state.select(Some(app.action_index));
    frame.render_stateful_widget(
        List::new(items)
            .block(block("ACTION MENU"))
            .highlight_style(
                Style::default()
                    .bg(Color::Green)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> "),
        layout[0],
        &mut state,
    );

    let selected = actions[app.action_index];
    let peer_text = app
        .selected_peer()
        .map(|peer| {
            format!(
                "{} ({})",
                peer.agent_label
                    .clone()
                    .unwrap_or_else(|| short_peer(&peer.peer_id)),
                short_peer(&peer.peer_id)
            )
        })
        .unwrap_or_else(|| "<none>".to_string());
    let detail = Text::from(vec![
        Line::from(Span::styled(selected.title(), neon(Color::Cyan))),
        Line::from(""),
        Line::from(selected.detail()),
        Line::from(""),
        Line::from(vec![
            Span::styled("selected peer ", neon(Color::Yellow)),
            Span::raw(peer_text),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "PRESS ENTER TO RUN",
            neon(Color::LightMagenta),
        )),
        Line::from(""),
        Line::from("Direct shortcuts work from any tab:"),
        Line::from("d discover   c create      s join       b broadcast"),
        Line::from("g grant      n note        t summary task"),
        Line::from("m toggle inbox/outbox"),
        Line::from("Requests tab: a accept once   w trust + accept   d deny"),
    ]);
    frame.render_widget(
        Paragraph::new(detail)
            .block(block("ACTION DETAIL"))
            .wrap(Wrap { trim: true }),
        layout[1],
    );
}

fn render_help(frame: &mut Frame, area: Rect, _app: &DashboardApp) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
        .split(area);
    let left = Text::from(vec![
        Line::from(Span::styled("COMMAND DECK", neon(Color::Cyan))),
        Line::from(""),
        Line::from("1-7 switch tabs"),
        Line::from("Tab / Shift+Tab cycle tabs"),
        Line::from("j/k or arrows move the current list"),
        Line::from("r refresh local snapshot"),
        Line::from("d pulse discovery now"),
        Line::from("a accept selected pending request once"),
        Line::from("w trust peer and accept selected request"),
        Line::from("/ filter peers"),
        Line::from("c create a public channel"),
        Line::from("s join an existing channel"),
        Line::from("b broadcast to a channel"),
        Line::from("g grant selected peer"),
        Line::from("n send note to selected peer"),
        Line::from("t send summary task"),
        Line::from("d deny selected pending request (on Requests tab)"),
        Line::from("m toggle inbox/outbox"),
        Line::from("? jump to this help tab"),
        Line::from("q quit"),
    ]);
    frame.render_widget(
        Paragraph::new(left)
            .block(block("HELP"))
            .wrap(Wrap { trim: true }),
        layout[0],
    );

    let right = Text::from(vec![
        Line::from(Span::styled(
            "WHAT YOU ARE LOOKING AT",
            neon(Color::LightMagenta),
        )),
        Line::from(""),
        Line::from("Overview: node, reachability, peer preview, and alerts"),
        Line::from("Peers: full list and selected peer detail"),
        Line::from("Topics: global public channels, owners, and member counts"),
        Line::from("Requests: pending delegate approvals"),
        Line::from("Messages: inbox / outbox with body detail"),
        Line::from("Actions: guided operator actions"),
        Line::from("Help: this quick reference"),
        Line::from(""),
        Line::from(Span::styled("INBOX ALERTS", neon(Color::Yellow))),
        Line::from("A * on the Messages tab means new inbound mail arrived."),
        Line::from("Opening the Messages tab while viewing Inbox clears that alert."),
        Line::from("A ! on the Requests tab means delegated work is awaiting approval."),
        Line::from(""),
        Line::from(Span::styled("PEER STATES", neon(Color::LightGreen))),
        Line::from("active: recently seen on the mesh"),
        Line::from("quiet: older, still inside the visibility window"),
        Line::from("hidden: aged out of normal views"),
    ]);
    frame.render_widget(
        Paragraph::new(right)
            .block(block("REFERENCE"))
            .wrap(Wrap { trim: true }),
        layout[1],
    );
}

fn render_footer(frame: &mut Frame, area: Rect, app: &DashboardApp) {
    let mut spans = vec![
        Span::styled("1-7", neon(Color::Cyan)),
        Span::raw(" tabs  "),
        Span::styled("j/k", neon(Color::Cyan)),
        Span::raw(" move  "),
        Span::styled("r", neon(Color::Cyan)),
        Span::raw(" refresh  "),
        Span::styled("d", neon(Color::Cyan)),
        Span::raw(if app.current_tab() == TabPage::Requests { " deny  " } else { " discover  " }),
        Span::styled("a", neon(Color::Cyan)),
        Span::raw(if app.current_tab() == TabPage::Requests {
            " accept once  "
        } else {
            " accept  "
        }),
        Span::styled("w", neon(Color::Cyan)),
        Span::raw(if app.current_tab() == TabPage::Requests {
            " trust+accept  "
        } else {
            " trust  "
        }),
        Span::styled("/", neon(Color::Cyan)),
        Span::raw(" filter  "),
        Span::styled("?", neon(Color::Cyan)),
        Span::raw(" help  "),
        Span::styled("q", neon(Color::Cyan)),
        Span::raw(" quit"),
    ];
    if let Some(toast) = &app.toast {
        spans.push(Span::raw("  |  "));
        spans.push(Span::styled(
            &toast.message,
            Style::default().fg(toast.color),
        ));
    } else if let Some(error) = &app.last_error {
        spans.push(Span::raw("  |  "));
        spans.push(Span::styled(
            truncate(error, 80),
            Style::default().fg(Color::Red),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .alignment(Alignment::Left)
            .block(block("CONSOLE")),
        area,
    );
}

fn render_modal(frame: &mut Frame, area: Rect, modal: &ModalState) {
    let popup = centered_rect(68, 30, area);
    frame.render_widget(Clear, popup);
    let lines = vec![
        Line::from(Span::styled(modal.prompt.clone(), neon(Color::Cyan))),
        Line::from(""),
        Line::from(modal.input.clone()),
        Line::from(""),
        Line::from(Span::styled(
            "ENTER submit  ESC cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block(&modal.title))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn gauge_line<'a>(title: &'a str, value: u16, max: u16, color: Color) -> Gauge<'a> {
    let pct = if max == 0 {
        0
    } else {
        (((value as f64 / max as f64) * 100.0).min(100.0)) as u16
    };
    Gauge::default()
        .block(block(title))
        .gauge_style(Style::default().fg(color).bg(Color::Black))
        .percent(pct)
        .label(format!("{value}"))
}

fn block(title: &str) -> Block<'static> {
    Block::default()
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(Color::LightYellow)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_set(border::DOUBLE)
        .border_style(Style::default().fg(Color::DarkGray))
}

fn neon(color: Color) -> Style {
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn short_peer(peer_id: &str) -> String {
    peer_id.chars().take(12).collect()
}

fn format_timestamp(value: DateTime<Utc>) -> String {
    value.with_timezone(&Local).format("%H:%M:%S").to_string()
}

fn kind_label(kind: &MessageKind) -> &'static str {
    match kind {
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

fn status_label(status: &crate::models::MessageStatus) -> &'static str {
    match status {
        crate::models::MessageStatus::Received => "received",
        crate::models::MessageStatus::Pending => "pending",
        crate::models::MessageStatus::Approved => "approved",
        crate::models::MessageStatus::Denied => "denied",
        crate::models::MessageStatus::Blocked => "blocked",
        crate::models::MessageStatus::Queued => "queued",
        crate::models::MessageStatus::Delivered => "delivered",
        crate::models::MessageStatus::Failed => "failed",
    }
}

fn parse_body(input: &str) -> Value {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return json!({});
    }
    serde_json::from_str(trimmed).unwrap_or_else(|_| json!({ "text": trimmed }))
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    let mut truncated = value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::parse_body;
    use serde_json::json;

    #[test]
    fn parse_body_wraps_plain_text() {
        assert_eq!(parse_body("hello"), json!({"text":"hello"}));
    }

    #[test]
    fn parse_body_keeps_json() {
        assert_eq!(
            parse_body("{\"headline\":\"ok\"}"),
            json!({"headline":"ok"})
        );
    }
}
