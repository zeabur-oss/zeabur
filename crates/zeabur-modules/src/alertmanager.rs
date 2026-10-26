//! Notifications through Alertmanager, run by the VictoriaMetrics operator [vm-amcfg].
//!
//! # Where alerts come from
//!
//! Zeabur Catalog's `metrics/victoria-metrics` installs the rule set `zeabur-notifications`
//! (a `VMRule` in `observability`). Each alert names its event in the label `zeabur_event`
//! ([`AlertmanagerEvent`]). A project's alerts keep the namespace of the object they are about;
//! the cluster's set the namespace to `observability`.
//!
//! # Channels
//!
//! A channel is one `VMAlertmanagerConfig` with one Secret of the same name next to it. The
//! operator adds a `namespace` matcher to the config's top route [vm-amcfg], so a config gets
//! only its own namespace's alerts, and resolves the receiver's secret references in that
//! namespace. Zeabur's configs carry [`CHANNEL_LABEL`]; others are shown, not changed.
//!
//! The route takes the chosen events, plus test alerts for this channel: alerts without a
//! `zeabur_channel` label, or with this channel's name in it. Events that report one occurrence
//! (`zeabur_kind="event"`) go out once each; states, such as a crash loop, are grouped by alert
//! and say when they resolve.
//!
//! # Tests
//!
//! A test is a short-lived Job labelled [`TEST_LABEL`] with the channel's name. The rule
//! `ZeaburNotificationTest` fires while kube-state-metrics sees the Job, in the Job's namespace,
//! so a person can test only the channels of a namespace where they may create Jobs.
//!
//! [vm-amcfg]: https://docs.victoriametrics.com/operator/resources/vmalertmanagerconfig/

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_kube::DynamicObject;

use crate::{Module, Read, text};

/// The module.
pub static MODULE: Module = Module {
    id: "alertmanager",
    capability: "notifications",
    reads: &[CONFIGS],
    http: &[],
};

/// `operator.victoriametrics.com/v1beta1` `VMAlertmanagerConfig`, in the project's namespace or
/// in [`CLUSTER_NAMESPACE`].
pub const CONFIGS: Read = Read {
    group: "operator.victoriametrics.com",
    version: "v1beta1",
    resource: "vmalertmanagerconfigs",
    kind: "VMAlertmanagerConfig",
};

/// `operator.victoriametrics.com/v1beta1` `VMRule`: [`RULES_NAME`] in [`CLUSTER_NAMESPACE`].
pub const RULES: Read = Read {
    group: "operator.victoriametrics.com",
    version: "v1beta1",
    resource: "vmrules",
    kind: "VMRule",
};

/// Where the catalog installs Alertmanager, the rules and the cluster's channels.
pub const CLUSTER_NAMESPACE: &str = "observability";
/// The rule set that makes the alerts.
pub const RULES_NAME: &str = "zeabur-notifications";
/// The label of a channel Zeabur made; its value is the channel's name.
pub const CHANNEL_LABEL: &str = "zeabur.com/notification-channel";
/// What a channel sends to, without its secrets: an address, a chat, a server.
pub const TARGET_ANNOTATION: &str = "zeabur.com/notification-target";
/// Where a channel sends ([`AlertmanagerChannelKind`]); ntfy is a webhook to
/// Alertmanager.
pub const KIND_ANNOTATION: &str = "zeabur.com/notification-kind";
/// The label of a test Job; its value is the channel's name.
pub const TEST_LABEL: &str = "zeabur.com/notification-test";
/// The longest channel name: the test Job's name must stay within 63 characters.
pub const NAME_MAX: usize = 30;
const PREFIX: &str = "zeabur-notify-";
const EVENTS_MATCHER: &str = "zeabur_event=~";

/// The object name of channel `name`: the config's and the Secret's.
#[must_use]
pub fn object_name(name: &str) -> String {
    format!("{PREFIX}{name}")
}

/// An event a channel can receive.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum AlertmanagerEvent {
    /// A build failed.
    BuildFailed,
    /// A build succeeded.
    BuildSucceeded,
    /// A rollout passed its progress deadline.
    DeployFailed,
    /// A rollout finished.
    DeploySucceeded,
    /// A container keeps crashing.
    CrashLoop,
    /// A container used its memory limit and restarted.
    OutOfMemory,
    /// A database has not been healthy for a while.
    DatabaseUnhealthy,
    /// A backup failed.
    BackupFailed,
    /// A volume or a node's disk is almost full.
    VolumeAlmostFull,
    /// A certificate expires soon or is not issued.
    Certificate,
    /// A node is not ready.
    NodeNotReady,
    /// Falco saw suspicious behaviour in a container.
    RuntimeSecurity,
}

impl AlertmanagerEvent {
    /// The events a project's channel can receive.
    pub const PROJECT: [Self; 10] = [
        Self::BuildFailed,
        Self::BuildSucceeded,
        Self::DeployFailed,
        Self::DeploySucceeded,
        Self::CrashLoop,
        Self::OutOfMemory,
        Self::DatabaseUnhealthy,
        Self::BackupFailed,
        Self::VolumeAlmostFull,
        Self::Certificate,
    ];
    /// The events a cluster channel can receive.
    pub const CLUSTER: [Self; 6] = [
        Self::NodeNotReady,
        Self::RuntimeSecurity,
        Self::CrashLoop,
        Self::VolumeAlmostFull,
        Self::Certificate,
        Self::BackupFailed,
    ];

    /// The value of the alerts' `zeabur_event` label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::BuildFailed => "build-failed",
            Self::BuildSucceeded => "build-succeeded",
            Self::DeployFailed => "deploy-failed",
            Self::DeploySucceeded => "deploy-succeeded",
            Self::CrashLoop => "crash-loop",
            Self::OutOfMemory => "out-of-memory",
            Self::DatabaseUnhealthy => "database-unhealthy",
            Self::BackupFailed => "backup-failed",
            Self::VolumeAlmostFull => "volume-almost-full",
            Self::Certificate => "certificate",
            Self::NodeNotReady => "node-not-ready",
            Self::RuntimeSecurity => "runtime-security",
        }
    }

    fn from_label(label: &str) -> Option<Self> {
        Self::PROJECT
            .iter()
            .chain(Self::CLUSTER.iter())
            .copied()
            .find(|e| e.label() == label)
    }
}

/// Where a channel sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AlertmanagerChannelKind {
    /// A Slack incoming webhook.
    Slack,
    /// A Discord webhook.
    Discord,
    /// A Telegram bot and chat.
    Telegram,
    /// Email through an SMTP server.
    Email,
    /// Alertmanager's JSON, posted to a URL.
    Webhook,
    /// An ntfy topic.
    Ntfy,
    /// Something Zeabur does not set up.
    Other,
}

impl AlertmanagerChannelKind {
    const ALL: [Self; 6] = [
        Self::Slack,
        Self::Discord,
        Self::Telegram,
        Self::Email,
        Self::Webhook,
        Self::Ntfy,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Slack => "slack",
            Self::Discord => "discord",
            Self::Telegram => "telegram",
            Self::Email => "email",
            Self::Webhook => "webhook",
            Self::Ntfy => "ntfy",
            Self::Other => "other",
        }
    }
}

/// A channel's settings, secrets included. They go into the channel's Secret and spec; Zeabur
/// never shows them again.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AlertmanagerChannelSettings {
    /// A Slack incoming webhook.
    Slack {
        /// `https://hooks.slack.com/services/…`.
        #[serde(rename = "webhookUrl")]
        webhook_url: String,
    },
    /// A Discord webhook.
    Discord {
        /// `https://discord.com/api/webhooks/…`.
        #[serde(rename = "webhookUrl")]
        webhook_url: String,
    },
    /// A Telegram bot and chat.
    Telegram {
        /// The bot's token, from `@BotFather`.
        #[serde(rename = "botToken")]
        bot_token: String,
        /// The chat to post in.
        #[serde(rename = "chatId")]
        chat_id: i64,
    },
    /// Email through an SMTP server.
    Email {
        /// The address to send to.
        to: String,
        /// The sender's address.
        from: String,
        /// The SMTP server, `host:port`.
        smarthost: String,
        /// The SMTP user; empty for none.
        #[serde(default)]
        username: String,
        /// The SMTP password.
        #[serde(default)]
        password: String,
    },
    /// Alertmanager's JSON, posted to a URL.
    Webhook {
        /// The URL, `https://…` or `http://…`.
        url: String,
        /// Sent as `Authorization: Bearer …`; empty for none.
        #[serde(default, rename = "bearerToken")]
        bearer_token: String,
    },
    /// An ntfy topic.
    Ntfy {
        /// The ntfy server; `https://ntfy.sh` when empty.
        #[serde(default)]
        server: String,
        /// The topic.
        topic: String,
        /// An access token for a protected topic; empty for none.
        #[serde(default, rename = "accessToken")]
        access_token: String,
    },
}

impl AlertmanagerChannelSettings {
    /// Where these settings send.
    #[must_use]
    pub const fn kind(&self) -> AlertmanagerChannelKind {
        match self {
            Self::Slack { .. } => AlertmanagerChannelKind::Slack,
            Self::Discord { .. } => AlertmanagerChannelKind::Discord,
            Self::Telegram { .. } => AlertmanagerChannelKind::Telegram,
            Self::Email { .. } => AlertmanagerChannelKind::Email,
            Self::Webhook { .. } => AlertmanagerChannelKind::Webhook,
            Self::Ntfy { .. } => AlertmanagerChannelKind::Ntfy,
        }
    }
}

impl std::fmt::Debug for AlertmanagerChannelSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlertmanagerChannelSettings")
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}

/// Whether Alertmanager took a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AlertmanagerChannelStatus {
    /// Alertmanager uses it.
    Active,
    /// The operator refused it; `reason` says why.
    Rejected,
    /// No Alertmanager has taken it yet. When this lasts, none selects it.
    Waiting,
}

/// One channel, without its secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AlertmanagerChannel {
    /// The channel's name; the config's for one Zeabur did not make.
    pub name: String,
    /// The `VMAlertmanagerConfig`.
    pub config: String,
    /// Where it sends.
    pub kind: AlertmanagerChannelKind,
    /// The address, chat or server it sends to, when it is not a secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The events it receives; empty for one Zeabur did not make.
    pub events: Vec<AlertmanagerEvent>,
    /// Whether Zeabur made it, and so may change it.
    pub managed: bool,
    /// Whether Alertmanager took it.
    pub status: AlertmanagerChannelStatus,
    /// Why the operator refused it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Its settings that are not secret, by the request's field names (`chatId`, `to`, `from`,
    /// `smarthost`, `username`); empty for one Zeabur did not make.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
}

/// What `name`, `events` and `settings` need before Zeabur writes them; `None` when nothing.
#[must_use]
pub fn problem(
    name: &str,
    events: &[AlertmanagerEvent],
    offered: &[AlertmanagerEvent],
    settings: Option<&AlertmanagerChannelSettings>,
) -> Option<String> {
    if let Some(problem) = name_problem(name) {
        return Some(problem);
    }
    if events.is_empty() {
        return Some("Choose at least one event.".to_owned());
    }
    if let Some(event) = events.iter().find(|e| !offered.contains(e)) {
        return Some(format!(
            "This channel cannot receive {}; choose from the events listed.",
            event.label()
        ));
    }
    settings.and_then(settings_problem)
}

fn name_problem(name: &str) -> Option<String> {
    let valid = !name.is_empty()
        && name.len() <= NAME_MAX
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && !name.ends_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    (!valid).then(|| {
        format!(
            "Name the channel with up to {NAME_MAX} lower-case letters, digits and hyphens, \
             starting with a letter."
        )
    })
}

fn https(url: &str) -> bool {
    url.strip_prefix("https://")
        .is_some_and(|rest| !rest.is_empty() && !rest.contains(char::is_whitespace))
}

fn http_or_https(url: &str) -> bool {
    https(url)
        || url
            .strip_prefix("http://")
            .is_some_and(|rest| !rest.is_empty() && !rest.contains(char::is_whitespace))
}

fn token_like(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'))
}

fn settings_problem(settings: &AlertmanagerChannelSettings) -> Option<String> {
    let fail = |s: &str| Some(s.to_owned());
    match settings {
        AlertmanagerChannelSettings::Slack { webhook_url }
        | AlertmanagerChannelSettings::Discord { webhook_url } => (!https(webhook_url.trim()))
            .then(|| "Paste the webhook URL; it starts with https://.".to_owned()),
        AlertmanagerChannelSettings::Telegram { bot_token, chat_id } => {
            if !token_like(bot_token.trim()) {
                fail("Paste the bot token BotFather gave you.")
            } else if *chat_id == 0 {
                fail("Enter the chat ID, a number.")
            } else {
                None
            }
        }
        AlertmanagerChannelSettings::Email {
            to,
            from,
            smarthost,
            ..
        } => {
            if !to.contains('@') || !from.contains('@') {
                fail("Enter both email addresses.")
            } else if !smarthost
                .rsplit_once(':')
                .is_some_and(|(host, port)| !host.is_empty() && port.parse::<u16>().is_ok())
            {
                fail("Enter the SMTP server as host:port, such as smtp.example.com:587.")
            } else {
                None
            }
        }
        AlertmanagerChannelSettings::Webhook { url, .. } => (!http_or_https(url.trim()))
            .then(|| "Enter the URL; it starts with https:// or http://.".to_owned()),
        AlertmanagerChannelSettings::Ntfy { server, topic, .. } => {
            if !server.trim().is_empty() && !http_or_https(server.trim()) {
                fail("Enter the ntfy server's address; it starts with https:// or http://.")
            } else if !token_like(topic.trim()) {
                fail("Enter the topic: letters, digits, hyphens and underscores.")
            } else {
                None
            }
        }
    }
}

/// The Secret's values: what the config's secret references point at.
fn secret_values(settings: &AlertmanagerChannelSettings) -> Vec<(&'static str, String)> {
    match settings {
        AlertmanagerChannelSettings::Slack { webhook_url }
        | AlertmanagerChannelSettings::Discord { webhook_url } => {
            vec![("url", webhook_url.trim().to_owned())]
        }
        AlertmanagerChannelSettings::Telegram { bot_token, .. } => {
            vec![("token", bot_token.trim().to_owned())]
        }
        AlertmanagerChannelSettings::Email { password, .. } => {
            vec![("password", password.clone())]
        }
        AlertmanagerChannelSettings::Webhook { url, bearer_token } => {
            let mut out = vec![("url", url.trim().to_owned())];
            if !bearer_token.trim().is_empty() {
                out.push(("token", bearer_token.trim().to_owned()));
            }
            out
        }
        AlertmanagerChannelSettings::Ntfy {
            server,
            topic,
            access_token,
        } => {
            let server = match server.trim().trim_end_matches('/') {
                "" => "https://ntfy.sh",
                other => other,
            };
            let mut out = vec![(
                "url",
                format!("{server}/{}?template=alertmanager", topic.trim()),
            )];
            if !access_token.trim().is_empty() {
                out.push(("token", access_token.trim().to_owned()));
            }
            out
        }
    }
}

/// The Secret of channel `name`: its values, or `shown` in place of each to show it.
#[must_use]
pub fn secret(
    namespace: &str,
    name: &str,
    settings: &AlertmanagerChannelSettings,
    shown: Option<&str>,
) -> Value {
    let data: serde_json::Map<String, Value> = secret_values(settings)
        .into_iter()
        .map(|(key, value)| {
            (
                key.to_owned(),
                Value::String(shown.map_or(value, str::to_owned)),
            )
        })
        .collect();
    json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {
            "name": object_name(name),
            "namespace": namespace,
            "labels": {CHANNEL_LABEL: name},
        },
        "type": "Opaque",
        "stringData": data,
    })
}

fn key(name: &str, key: &str) -> Value {
    json!({"name": object_name(name), "key": key})
}

fn bearer(name: &str, settings: &AlertmanagerChannelSettings) -> Option<Value> {
    secret_values(settings)
        .iter()
        .any(|(k, _)| *k == "token")
        .then(|| json!({"authorization": {"type": "Bearer", "credentials": key(name, "token")}}))
}

const TITLE: &str = r#"{{ if eq .Status "resolved" }}Resolved: {{ end }}{{ (index .Alerts 0).Annotations.summary }}"#;
const TEXT: &str = "{{ range .Alerts }}{{ .Annotations.description }}\n{{ end }}";

fn set(object: &mut Value, key: &str, value: Value) {
    if let Some(map) = object.as_object_mut() {
        map.insert(key.to_owned(), value);
    }
}

/// One receiver of channel `name`.
fn receiver(
    receiver: &str,
    name: &str,
    settings: &AlertmanagerChannelSettings,
    send_resolved: bool,
) -> Value {
    let (field, mut config) = match settings {
        AlertmanagerChannelSettings::Slack { .. } => (
            "slack_configs",
            json!({"api_url": key(name, "url"), "title": TITLE, "text": TEXT}),
        ),
        AlertmanagerChannelSettings::Discord { .. } => (
            "discord_configs",
            json!({"webhook_url_secret": key(name, "url"), "title": TITLE, "message": TEXT}),
        ),
        AlertmanagerChannelSettings::Telegram { chat_id, .. } => (
            "telegram_configs",
            json!({
                "bot_token": key(name, "token"),
                "chat_id": chat_id,
                "message": format!("{TITLE}\n\n{TEXT}"),
            }),
        ),
        AlertmanagerChannelSettings::Email {
            to,
            from,
            smarthost,
            username,
            password,
        } => {
            let mut config = json!({
                "to": to.trim(),
                "from": from.trim(),
                "smarthost": smarthost.trim(),
                "require_tls": true,
            });
            if !username.trim().is_empty() {
                set(&mut config, "auth_username", json!(username.trim()));
            }
            if !password.is_empty() {
                set(&mut config, "auth_password", key(name, "password"));
            }
            ("email_configs", config)
        }
        AlertmanagerChannelSettings::Webhook { .. } | AlertmanagerChannelSettings::Ntfy { .. } => {
            let mut config = json!({"url_secret": key(name, "url")});
            if let Some(http) = bearer(name, settings) {
                set(&mut config, "http_config", http);
            }
            ("webhook_configs", config)
        }
    };
    set(&mut config, "send_resolved", json!(send_resolved));
    json!({"name": receiver, field: [config]})
}

fn events_matcher(events: &[AlertmanagerEvent]) -> String {
    let mut labels: Vec<&str> = events.iter().map(|e| e.label()).collect();
    labels.sort_unstable();
    labels.dedup();
    labels.push("test");
    format!("{EVENTS_MATCHER}\"{}\"", labels.join("|"))
}

fn matchers(name: &str, events: &[AlertmanagerEvent]) -> Value {
    json!([
        events_matcher(events),
        format!("zeabur_channel=~\"|{name}\"")
    ])
}

fn target(settings: &AlertmanagerChannelSettings) -> Option<String> {
    match settings {
        AlertmanagerChannelSettings::Telegram { chat_id, .. } => Some(chat_id.to_string()),
        AlertmanagerChannelSettings::Email { to, .. } => Some(to.trim().to_owned()),
        AlertmanagerChannelSettings::Webhook { url, .. } => host(url),
        AlertmanagerChannelSettings::Ntfy { server, .. } => {
            host(server).or_else(|| Some("ntfy.sh".to_owned()))
        }
        AlertmanagerChannelSettings::Slack { .. } | AlertmanagerChannelSettings::Discord { .. } => {
            None
        }
    }
}

fn host(url: &str) -> Option<String> {
    let rest = url
        .trim()
        .strip_prefix("https://")
        .or_else(|| url.trim().strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    (!host.is_empty()).then(|| host.to_owned())
}

/// The `VMAlertmanagerConfig` of channel `name`. Its secrets stay in the Secret
/// ([`secret`]); the config only names their keys.
#[must_use]
pub fn config(
    namespace: &str,
    name: &str,
    settings: &AlertmanagerChannelSettings,
    events: &[AlertmanagerEvent],
) -> Value {
    let mut annotations = json!({KIND_ANNOTATION: settings.kind().label()});
    if let Some(target) = target(settings) {
        set(&mut annotations, TARGET_ANNOTATION, json!(target));
    }
    json!({
        "apiVersion": CONFIGS.resource().api_version(),
        "kind": CONFIGS.kind,
        "metadata": {
            "name": object_name(name),
            "namespace": namespace,
            "labels": {CHANNEL_LABEL: name},
            "annotations": annotations,
        },
        "spec": {
            "route": {
                "receiver": "events",
                "matchers": matchers(name, events),
                "group_by": ["..."],
                "group_wait": "30s",
                "group_interval": "5m",
                "repeat_interval": "12h",
                "routes": [{
                    "receiver": "states",
                    "matchers": ["zeabur_kind=\"state\""],
                    "group_by": ["alertname"],
                }],
            },
            "receivers": [
                receiver("events", name, settings, false),
                receiver("states", name, settings, true),
            ],
        },
    })
}

/// `config` with its route taking `events` instead.
#[must_use]
pub fn with_events(config: &Value, name: &str, events: &[AlertmanagerEvent]) -> Value {
    let mut out = config.clone();
    if let Some(route) = out.pointer_mut("/spec/route") {
        route["matchers"] = matchers(name, events);
    }
    out
}

/// The Job that tests channel `name`: it only has to exist for a minute or two. Its pod does
/// nothing and meets the restricted Pod Security Standard.
#[must_use]
pub fn test_job(namespace: &str, name: &str, job: &str) -> Value {
    json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {
            "name": job,
            "namespace": namespace,
            "labels": {TEST_LABEL: name},
        },
        "spec": {
            "activeDeadlineSeconds": 120,
            "ttlSecondsAfterFinished": 300,
            "backoffLimit": 0,
            "template": {
                "metadata": {"labels": {TEST_LABEL: name}},
                "spec": {
                    "restartPolicy": "Never",
                    "automountServiceAccountToken": false,
                    "enableServiceLinks": false,
                    "securityContext": {
                        "runAsNonRoot": true,
                        "runAsUser": 65535,
                        "seccompProfile": {"type": "RuntimeDefault"},
                    },
                    "containers": [{
                        "name": "wait",
                        "image": "registry.k8s.io/pause:3.10",
                        "resources": {
                            "requests": {"cpu": "1m", "memory": "4Mi"},
                            "limits": {"cpu": "10m", "memory": "8Mi"},
                        },
                        "securityContext": {
                            "allowPrivilegeEscalation": false,
                            "readOnlyRootFilesystem": true,
                            "capabilities": {"drop": ["ALL"]},
                        },
                    }],
                },
            },
        },
    })
}

/// The name of a test Job made at `unix_seconds`.
#[must_use]
pub fn test_job_name(name: &str, unix_seconds: i64) -> String {
    format!("{PREFIX}test-{name}-{unix_seconds}")
}

fn kind(spec: &Value) -> AlertmanagerChannelKind {
    let receivers = spec
        .get("receivers")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let has = |field: &str| {
        receivers.iter().any(|r| {
            r.get(field)
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty())
        })
    };
    [
        ("slack_configs", AlertmanagerChannelKind::Slack),
        ("discord_configs", AlertmanagerChannelKind::Discord),
        ("telegram_configs", AlertmanagerChannelKind::Telegram),
        ("email_configs", AlertmanagerChannelKind::Email),
        ("webhook_configs", AlertmanagerChannelKind::Webhook),
    ]
    .into_iter()
    .find(|(field, _)| has(field))
    .map_or(AlertmanagerChannelKind::Other, |(_, kind)| kind)
}

/// The channel a `VMAlertmanagerConfig` is, as far as Zeabur can tell without its Secret.
#[must_use]
pub fn channel(object: &DynamicObject) -> AlertmanagerChannel {
    let config = object.metadata.name.clone().unwrap_or_default();
    let label = object
        .metadata
        .labels
        .as_ref()
        .and_then(|l| l.get(CHANNEL_LABEL))
        .cloned();
    let spec = object.data.get("spec").cloned().unwrap_or(Value::Null);
    let events: Vec<AlertmanagerEvent> = label
        .as_ref()
        .and_then(|_| spec.pointer("/route/matchers"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|m| m.strip_prefix(EVENTS_MATCHER))
        .flat_map(|m| m.trim_matches('"').split('|'))
        .filter_map(AlertmanagerEvent::from_label)
        .collect();
    let annotation = |key: &str| {
        object
            .metadata
            .annotations
            .as_ref()
            .and_then(|a| a.get(key))
            .cloned()
    };
    let kind = annotation(KIND_ANNOTATION)
        .filter(|_| label.is_some())
        .map_or_else(
            || kind(&spec),
            |k| {
                AlertmanagerChannelKind::ALL
                    .into_iter()
                    .find(|c| c.label() == k)
                    .unwrap_or(AlertmanagerChannelKind::Other)
            },
        );
    let (status, reason) = match text(object, "/status/updateStatus").as_deref() {
        Some("operational") => (AlertmanagerChannelStatus::Active, None),
        Some("failed") => (
            AlertmanagerChannelStatus::Rejected,
            text(object, "/status/reason").map(hide_url),
        ),
        _ => (AlertmanagerChannelStatus::Waiting, None),
    };
    AlertmanagerChannel {
        name: label.clone().unwrap_or_else(|| config.clone()),
        config,
        kind,
        target: annotation(TARGET_ANNOTATION),
        events,
        managed: label.is_some(),
        status,
        reason,
        fields: if label.is_some() {
            shown_fields(&spec)
        } else {
            BTreeMap::new()
        },
    }
}

/// The first `field` receiver config of `spec`, where Zeabur's channels keep their settings.
fn receiver_config<'a>(spec: &'a Value, field: &str) -> Option<&'a Value> {
    spec.pointer(&format!("/receivers/0/{field}/0"))
}

fn shown_fields(spec: &Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(chat) = receiver_config(spec, "telegram_configs")
        .and_then(|c| c.get("chat_id"))
        .and_then(Value::as_i64)
    {
        out.insert("chatId".to_owned(), chat.to_string());
    }
    if let Some(email) = receiver_config(spec, "email_configs") {
        for (field, key) in [
            ("to", "to"),
            ("from", "from"),
            ("smarthost", "smarthost"),
            ("username", "auth_username"),
        ] {
            let value = email.get(key).and_then(Value::as_str).unwrap_or_default();
            out.insert(field.to_owned(), value.to_owned());
        }
    }
    out
}

/// A change to a channel, as [`edit`] works it out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    /// The settings to write the config from. A setting kept in the Secret stands in with a
    /// valid value of its kind, empty when the Secret has no key for it.
    pub settings: AlertmanagerChannelSettings,
    /// The Secret's values to replace, by key; the other keys keep theirs.
    pub secret: BTreeMap<String, String>,
    /// The config's target annotation after the change, when the change says what it is.
    pub target: Option<String>,
}

const KEPT_URL: &str = "https://kept.invalid";
const KEPT: &str = "kept";

/// The Secret's keys, and those an edit types again.
struct Kept<'a> {
    stored: &'a BTreeSet<String>,
    fresh: BTreeSet<&'static str>,
}

impl Kept<'_> {
    /// `value` when typed; else a stand-in when the Secret has `key`, else empty.
    fn or(&mut self, value: &str, key: &'static str, stand_in: &str) -> String {
        if !value.trim().is_empty() {
            self.fresh.insert(key);
            value.to_owned()
        } else if self.stored.contains(key) {
            stand_in.to_owned()
        } else {
            String::new()
        }
    }
}

/// What changing a channel to `typed` writes, without reading its Secret's values.
///
/// A setting kept in the Secret (a URL, token, password, or ntfy's server and topic) left empty
/// keeps its value, and `stored` names the Secret's keys. ntfy's server and topic are one value,
/// so the topic is typed again to change either.
///
/// # Errors
///
/// What to fix when the settings are not valid.
pub fn edit(
    typed: &AlertmanagerChannelSettings,
    stored: &BTreeSet<String>,
) -> Result<Edited, String> {
    use AlertmanagerChannelSettings as S;
    let mut kept = Kept {
        stored,
        fresh: BTreeSet::new(),
    };
    let settings = match typed {
        S::Slack { webhook_url } => S::Slack {
            webhook_url: kept.or(webhook_url, "url", KEPT_URL),
        },
        S::Discord { webhook_url } => S::Discord {
            webhook_url: kept.or(webhook_url, "url", KEPT_URL),
        },
        S::Telegram { bot_token, chat_id } => S::Telegram {
            bot_token: kept.or(bot_token, "token", KEPT),
            chat_id: *chat_id,
        },
        S::Email {
            to,
            from,
            smarthost,
            username,
            password,
        } => S::Email {
            to: to.clone(),
            from: from.clone(),
            smarthost: smarthost.clone(),
            username: username.clone(),
            password: kept.or(password, "password", KEPT),
        },
        S::Webhook { url, bearer_token } => S::Webhook {
            url: kept.or(url, "url", KEPT_URL),
            bearer_token: kept.or(bearer_token, "token", KEPT),
        },
        S::Ntfy {
            server,
            topic,
            access_token,
        } => {
            if topic.trim().is_empty() && !server.trim().is_empty() {
                return Err(
                    "Enter the topic too: the server and topic are stored together.".to_owned(),
                );
            }
            let topic = kept.or(topic, "url", KEPT);
            S::Ntfy {
                server: if kept.fresh.contains("url") {
                    server.clone()
                } else {
                    String::new()
                },
                topic,
                access_token: kept.or(access_token, "token", KEPT),
            }
        }
    };
    let fresh = kept.fresh;
    if let Some(problem) = settings_problem(&settings) {
        return Err(problem);
    }
    let secret = secret_data(&settings)
        .into_iter()
        .filter(|(key, _)| fresh.contains(key.as_str()))
        .collect();
    let target = match &settings {
        S::Webhook { .. } | S::Ntfy { .. } if !fresh.contains("url") => None,
        S::Slack { .. }
        | S::Discord { .. }
        | S::Telegram { .. }
        | S::Email { .. }
        | S::Webhook { .. }
        | S::Ntfy { .. } => target(&settings),
    };
    Ok(Edited {
        settings,
        secret,
        target,
    })
}

/// The values of a channel's Secret with `settings`, by key.
#[must_use]
pub fn secret_data(settings: &AlertmanagerChannelSettings) -> BTreeMap<String, String> {
    secret_values(settings)
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

/// The operator's reason, without the URL it read from a Secret: that URL can hold a token.
fn hide_url(reason: String) -> String {
    let Some((head, rest)) = reason.split_once("invalid URL ") else {
        return reason;
    };
    let Some((_, tail)) = rest.split_once(" in key ") else {
        return reason;
    };
    format!("{head}invalid URL (hidden) in key {tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_leaves_the_secrets_out() {
        let settings = AlertmanagerChannelSettings::Webhook {
            url: "https://hooks.example.com/T0K3N".to_owned(),
            bearer_token: "TOKEN-SECRET".to_owned(),
        };
        let shown = format!("{settings:?}");
        assert!(
            !shown.contains("T0K3N") && !shown.contains("TOKEN-SECRET"),
            "{shown}"
        );
    }

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    const EVENTS: [AlertmanagerEvent; 2] = [
        AlertmanagerEvent::DeployFailed,
        AlertmanagerEvent::CrashLoop,
    ];

    #[test]
    fn a_config_names_its_secrets_but_never_holds_them() {
        let settings = AlertmanagerChannelSettings::Webhook {
            url: "https://hooks.example.com/zeabur?key=URL-SECRET".to_owned(),
            bearer_token: "TOKEN-SECRET".to_owned(),
        };
        let config = config("shop", "ops", &settings, &EVENTS);
        let text = config.to_string();
        assert!(
            !text.contains("URL-SECRET") && !text.contains("TOKEN-SECRET"),
            "{text}"
        );
        assert_eq!(
            config.pointer("/spec/receivers/0/webhook_configs/0/url_secret"),
            Some(&json!({"name": "zeabur-notify-ops", "key": "url"}))
        );
        let written = secret("shop", "ops", &settings, None);
        assert_eq!(written["stringData"]["token"], "TOKEN-SECRET");
        let shown = secret("shop", "ops", &settings, Some("(hidden)")).to_string();
        assert!(!shown.contains("SECRET"), "{shown}");
    }

    #[test]
    fn a_channel_reads_back_as_it_was_made() {
        let settings = AlertmanagerChannelSettings::Ntfy {
            server: String::new(),
            topic: "shop-alerts".to_owned(),
            access_token: String::new(),
        };
        let mut made = config("shop", "phone", &settings, &EVENTS);
        made["status"] = json!({"updateStatus": "failed", "reason": "bad matcher"});
        let channel = channel(&object(made.clone()));
        assert_eq!(channel.name, "phone");
        assert_eq!(channel.kind, AlertmanagerChannelKind::Ntfy);
        assert_eq!(channel.target.as_deref(), Some("ntfy.sh"));
        assert_eq!(
            channel.events,
            vec![
                AlertmanagerEvent::CrashLoop,
                AlertmanagerEvent::DeployFailed
            ]
        );
        assert_eq!(channel.status, AlertmanagerChannelStatus::Rejected);
        assert_eq!(channel.reason.as_deref(), Some("bad matcher"));

        let changed = with_events(&made, "phone", &[AlertmanagerEvent::BuildFailed]);
        assert_eq!(
            super::channel(&object(changed)).events,
            vec![AlertmanagerEvent::BuildFailed]
        );
    }

    #[test]
    fn an_edit_replaces_only_the_secrets_typed_again() {
        let stored = BTreeSet::from(["url".to_owned(), "token".to_owned()]);
        let typed = AlertmanagerChannelSettings::Webhook {
            url: String::new(),
            bearer_token: "new-token".to_owned(),
        };
        let edited = edit(&typed, &stored).unwrap();
        assert_eq!(
            edited.secret,
            BTreeMap::from([("token".to_owned(), "new-token".to_owned())])
        );
        assert_eq!(edited.target, None);
        let made = config("shop", "ops", &edited.settings, &EVENTS)["spec"].to_string();
        assert!(made.contains("authorization"), "{made}");
        assert!(!made.contains("kept"), "{made}");

        let without_token = BTreeSet::from(["url".to_owned()]);
        let kept = edit(
            &AlertmanagerChannelSettings::Webhook {
                url: String::new(),
                bearer_token: String::new(),
            },
            &without_token,
        )
        .unwrap();
        assert!(kept.secret.is_empty());
        assert!(
            !config("shop", "ops", &kept.settings, &EVENTS)
                .to_string()
                .contains("authorization")
        );

        let nothing = BTreeSet::new();
        assert!(edit(&typed, &nothing).is_err(), "a URL is required");
        let server_only = AlertmanagerChannelSettings::Ntfy {
            server: "https://ntfy.example.com".to_owned(),
            topic: String::new(),
            access_token: String::new(),
        };
        assert!(edit(&server_only, &stored).is_err());
        let moved = AlertmanagerChannelSettings::Ntfy {
            server: "https://ntfy.example.com".to_owned(),
            topic: "shop".to_owned(),
            access_token: String::new(),
        };
        let moved = edit(&moved, &stored).unwrap();
        assert_eq!(
            moved.secret["url"],
            "https://ntfy.example.com/shop?template=alertmanager"
        );
        assert_eq!(moved.target.as_deref(), Some("ntfy.example.com"));
    }

    #[test]
    fn a_reason_hides_the_url_from_the_secret() {
        assert_eq!(
            hide_url(
                "invalid URL ftp://x/T0K3N in key url from secret zeabur-notify-ops: \
                 unsupported scheme \"ftp\" for URL"
                    .to_owned()
            ),
            "invalid URL (hidden) in key url from secret zeabur-notify-ops: \
             unsupported scheme \"ftp\" for URL"
        );
        assert_eq!(hide_url("bad matcher".to_owned()), "bad matcher");
    }

    #[test]
    fn a_route_takes_the_chosen_events_and_only_its_own_tests() {
        let settings = AlertmanagerChannelSettings::Slack {
            webhook_url: "https://hooks.slack.com/services/x".to_owned(),
        };
        let config = config("shop", "team", &settings, &EVENTS);
        assert_eq!(
            config.pointer("/spec/route/matchers"),
            Some(&json!([
                "zeabur_event=~\"crash-loop|deploy-failed|test\"",
                "zeabur_channel=~\"|team\""
            ]))
        );
    }

    #[test]
    fn a_config_zeabur_did_not_make_is_shown_without_events() {
        let other = object(json!({
            "apiVersion": "operator.victoriametrics.com/v1beta1",
            "kind": "VMAlertmanagerConfig",
            "metadata": {"name": "pager", "namespace": "shop"},
            "spec": {"route": {"receiver": "p"}, "receivers": [{"name": "p", "pagerduty_configs": [{}]}]},
            "status": {"updateStatus": "operational"}
        }));
        let channel = channel(&other);
        assert!(!channel.managed);
        assert_eq!(channel.kind, AlertmanagerChannelKind::Other);
        assert_eq!(channel.events, Vec::new());
        assert_eq!(channel.status, AlertmanagerChannelStatus::Active);
    }

    #[test]
    fn a_channel_of_an_unknown_kind_is_shown_as_other() {
        let unknown = object(json!({
            "apiVersion": "operator.victoriametrics.com/v1beta1",
            "kind": "VMAlertmanagerConfig",
            "metadata": {"name": "zeabur-notify-pager", "namespace": "shop",
                "labels": {CHANNEL_LABEL: "pager"},
                "annotations": {KIND_ANNOTATION: "pigeon"}},
            "spec": {"route": {"receiver": "p"}, "receivers": [{"name": "p", "webhook_configs": [{}]}]},
            "status": {"updateStatus": "operational"}
        }));
        assert_eq!(channel(&unknown).kind, AlertmanagerChannelKind::Other);
    }

    #[test]
    fn requests_are_checked_before_anything_is_written() {
        let slack = |url: &str| AlertmanagerChannelSettings::Slack {
            webhook_url: url.to_owned(),
        };
        let ok = slack("https://hooks.slack.com/services/x");
        let project = &AlertmanagerEvent::PROJECT;
        assert_eq!(problem("team", &EVENTS, project, Some(&ok)), None);
        assert!(problem("Team", &EVENTS, project, Some(&ok)).is_some());
        assert!(problem(&"a".repeat(NAME_MAX + 1), &EVENTS, project, Some(&ok)).is_some());
        assert!(problem("team", &[], project, Some(&ok)).is_some());
        assert!(
            problem(
                "team",
                &[AlertmanagerEvent::NodeNotReady],
                project,
                Some(&ok)
            )
            .is_some()
        );
        assert!(problem("team", &EVENTS, project, Some(&slack("http://x"))).is_some());
        assert!(
            test_job_name(&"a".repeat(NAME_MAX), 9_999_999_999).len() <= 63,
            "a Job name must fit in a label value"
        );
    }
}
