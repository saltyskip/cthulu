//! Native Rust Slack client for fetching messages.
//!
//! Replaces the Python sidecar (`scripts/slack_messages.py`) with direct
//! `reqwest` calls to the Slack Web API. Supports pagination, rate-limit
//! retries, thread fetching, and message filtering.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

const SLACK_API_BASE: &str = "https://slack.com/api";
const MAX_RETRIES: usize = 5;
const PAGE_LIMIT: usize = 200;

/// A Slack client for fetching messages from channels.
pub struct SlackClient {
    http: reqwest::Client,
    token: String,
    my_user_id: String,
    users: HashMap<String, String>,
}

/// A fetched channel with its messages.
#[derive(Debug, Serialize)]
pub struct ChannelMessages {
    pub channel: String,
    pub count: usize,
    pub messages: Vec<MessageEntry>,
}

/// A single message entry.
#[derive(Debug, Serialize)]
pub struct MessageEntry {
    pub time: String,
    pub user: String,
    pub text: String,
    pub ts: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_ts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replies: Option<Vec<MessageEntry>>,
}

/// Read filter for messages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReadFilter {
    Unread,
    Read,
    All,
}

impl SlackClient {
    /// Create a new SlackClient, authenticating and building the user map.
    pub async fn new(http: reqwest::Client, token: String) -> Result<Self> {
        let auth = slack_api_get(&http, &token, "auth.test", &[]).await?;
        let my_user_id = auth["user_id"]
            .as_str()
            .context("auth.test missing user_id")?
            .to_string();

        tracing::info!(user = %auth["user"].as_str().unwrap_or("?"), "Slack authenticated");

        let users = build_user_map(&http, &token).await?;

        Ok(Self {
            http,
            token,
            my_user_id,
            users,
        })
    }

    /// Fetch messages from configured channels within a time range.
    pub async fn fetch_messages(
        &self,
        channels: &[String],
        oldest: f64,
        latest: f64,
        read_filter: ReadFilter,
        with_threads: bool,
    ) -> Result<Vec<ChannelMessages>> {
        let convos = self.list_conversations(Some(channels)).await?;
        let mut results = Vec::new();

        for conv in convos {
            let cid = conv["id"].as_str().unwrap_or("");
            let name = self.conv_label(&conv);
            let last_read: f64 = conv["last_read"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);

            let msgs_result = slack_api_get(
                &self.http,
                &self.token,
                "conversations.history",
                &[
                    ("channel", cid),
                    ("oldest", &oldest.to_string()),
                    ("latest", &latest.to_string()),
                    ("limit", &PAGE_LIMIT.to_string()),
                ],
            )
            .await;

            let resp = match msgs_result {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(channel = %name, error = %e, "skipping channel");
                    continue;
                }
            };

            let raw_msgs = resp["messages"].as_array().cloned().unwrap_or_default();
            if raw_msgs.is_empty() {
                continue;
            }

            // Apply read filter
            let filtered: Vec<&Value> = raw_msgs
                .iter()
                .filter(|m| {
                    let ts: f64 = m["ts"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    let is_me = m["user"].as_str() == Some(&self.my_user_id);
                    match read_filter {
                        ReadFilter::All => true,
                        ReadFilter::Unread => ts > last_read && !is_me,
                        ReadFilter::Read => ts <= last_read,
                    }
                })
                .collect();

            if filtered.is_empty() {
                continue;
            }

            let mut entries: Vec<MessageEntry> = Vec::new();
            for m in &filtered {
                let ts_str = m["ts"].as_str().unwrap_or("0");
                let ts_f64: f64 = ts_str.parse().unwrap_or(0.0);
                let dt = Utc.timestamp_opt(ts_f64 as i64, 0).single();

                let mut entry = MessageEntry {
                    time: dt.map(|d| d.to_rfc3339()).unwrap_or_default(),
                    user: self.resolve_user(m["user"].as_str().unwrap_or("")),
                    text: m["text"].as_str().unwrap_or("").to_string(),
                    ts: ts_str.to_string(),
                    thread_ts: None,
                    reply_count: None,
                    replies: None,
                };

                let reply_count = m["reply_count"].as_u64().unwrap_or(0);
                if with_threads && reply_count > 0 {
                    entry.thread_ts = m["thread_ts"]
                        .as_str()
                        .or(Some(ts_str))
                        .map(String::from);
                    entry.reply_count = Some(reply_count);

                    match self.fetch_thread_replies(cid, ts_str).await {
                        Ok(replies) => entry.replies = Some(replies),
                        Err(e) => {
                            tracing::warn!(thread_ts = %ts_str, error = %e, "failed to fetch thread");
                        }
                    }
                }

                entries.push(entry);
            }

            // Sort by timestamp ascending
            entries.sort_by(|a, b| a.ts.partial_cmp(&b.ts).unwrap_or(std::cmp::Ordering::Equal));

            results.push(ChannelMessages {
                channel: name,
                count: entries.len(),
                messages: entries,
            });
        }

        Ok(results)
    }

    /// List conversations the user is a member of, optionally filtered by name.
    async fn list_conversations(
        &self,
        channel_filter: Option<&[String]>,
    ) -> Result<Vec<Value>> {
        let types = "public_channel,private_channel";
        let mut all_convos = Vec::new();
        let mut cursor = String::new();

        loop {
            let mut params = vec![
                ("types", types),
                ("exclude_archived", "true"),
                ("limit", "200"),
            ];
            if !cursor.is_empty() {
                params.push(("cursor", &cursor));
            }

            let resp = slack_api_get(&self.http, &self.token, "conversations.list", &params).await?;

            if let Some(channels) = resp["channels"].as_array() {
                all_convos.extend(channels.iter().cloned());
            }

            cursor = resp["response_metadata"]["next_cursor"]
                .as_str()
                .unwrap_or("")
                .to_string();
            if cursor.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Filter to channels the user is a member of
        let mut filtered: Vec<Value> = all_convos
            .into_iter()
            .filter(|c| {
                c["is_im"].as_bool() == Some(true)
                    || c["is_mpim"].as_bool() == Some(true)
                    || c["is_member"].as_bool() == Some(true)
            })
            .collect();

        // Filter by channel name if provided
        if let Some(names) = channel_filter {
            let name_set: std::collections::HashSet<String> = names
                .iter()
                .map(|n| n.trim().trim_start_matches('#').to_lowercase())
                .collect();
            filtered.retain(|c| {
                c["name"]
                    .as_str()
                    .map(|n| name_set.contains(&n.to_lowercase()))
                    .unwrap_or(false)
            });
        }

        Ok(filtered)
    }

    /// Fetch thread replies for a message.
    async fn fetch_thread_replies(
        &self,
        channel_id: &str,
        thread_ts: &str,
    ) -> Result<Vec<MessageEntry>> {
        let resp = slack_api_get(
            &self.http,
            &self.token,
            "conversations.replies",
            &[
                ("channel", channel_id),
                ("ts", thread_ts),
                ("limit", &PAGE_LIMIT.to_string()),
            ],
        )
        .await?;

        let messages = resp["messages"].as_array().cloned().unwrap_or_default();
        // First message is the parent — skip it
        let replies = if messages.len() > 1 {
            &messages[1..]
        } else {
            &[]
        };

        Ok(replies
            .iter()
            .map(|r| {
                let ts_str = r["ts"].as_str().unwrap_or("0");
                let ts_f64: f64 = ts_str.parse().unwrap_or(0.0);
                let dt = Utc.timestamp_opt(ts_f64 as i64, 0).single();

                MessageEntry {
                    time: dt.map(|d| d.to_rfc3339()).unwrap_or_default(),
                    user: self.resolve_user(r["user"].as_str().unwrap_or("")),
                    text: r["text"].as_str().unwrap_or("").to_string(),
                    ts: ts_str.to_string(),
                    thread_ts: None,
                    reply_count: None,
                    replies: None,
                }
            })
            .collect())
    }

    /// Build a human-readable label for a conversation.
    fn conv_label(&self, conv: &Value) -> String {
        if conv["is_im"].as_bool() == Some(true) {
            let uid = conv["user"].as_str().unwrap_or("");
            return format!("DM: {}", self.resolve_user(uid));
        }
        if conv["is_mpim"].as_bool() == Some(true) {
            let name = conv["name"].as_str().unwrap_or("");
            return format!("Group DM: {name}");
        }
        let prefix = if conv["is_private"].as_bool() == Some(true) {
            "\u{1f512}"
        } else {
            "#"
        };
        let name = conv["name"].as_str().unwrap_or("");
        format!("{prefix}{name}")
    }

    /// Resolve a Slack user ID to a display name.
    fn resolve_user(&self, user_id: &str) -> String {
        self.users
            .get(user_id)
            .cloned()
            .unwrap_or_else(|| "unknown".to_string())
    }
}

/// Build a map of user_id -> display_name.
async fn build_user_map(
    http: &reqwest::Client,
    token: &str,
) -> Result<HashMap<String, String>> {
    let mut users = HashMap::new();
    let mut cursor = String::new();

    loop {
        let mut params: Vec<(&str, &str)> = vec![("limit", "200")];
        if !cursor.is_empty() {
            params.push(("cursor", &cursor));
        }

        let resp = slack_api_get(http, token, "users.list", &params).await?;

        if let Some(members) = resp["members"].as_array() {
            for u in members {
                let id = u["id"].as_str().unwrap_or("");
                let profile = &u["profile"];
                let name = profile["display_name"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .or_else(|| profile["real_name"].as_str().filter(|s| !s.is_empty()))
                    .or_else(|| u["name"].as_str())
                    .unwrap_or(id);
                users.insert(id.to_string(), name.to_string());
            }
        }

        cursor = resp["response_metadata"]["next_cursor"]
            .as_str()
            .unwrap_or("")
            .to_string();
        if cursor.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    Ok(users)
}

/// Make a GET request to a Slack Web API method with automatic rate-limit retry.
async fn slack_api_get(
    http: &reqwest::Client,
    token: &str,
    method: &str,
    params: &[(&str, &str)],
) -> Result<Value> {
    let url = format!("{SLACK_API_BASE}/{method}");

    for attempt in 0..MAX_RETRIES {
        let resp = http
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .query(params)
            .send()
            .await
            .with_context(|| format!("failed to call Slack {method}"))?;

        if resp.status() == 429 {
            let retry_after: u64 = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .unwrap_or(5)
                + attempt as u64;
            tracing::warn!(
                method,
                retry_after,
                attempt,
                "Slack rate limited, retrying"
            );
            tokio::time::sleep(Duration::from_secs(retry_after)).await;
            continue;
        }

        let body: Value = resp
            .json()
            .await
            .with_context(|| format!("failed to parse Slack {method} response"))?;

        if body["ok"].as_bool() != Some(true) {
            let err = body["error"].as_str().unwrap_or("unknown");
            bail!("Slack {method} failed: {err}");
        }

        return Ok(body);
    }

    bail!("Slack {method} still rate-limited after {MAX_RETRIES} retries")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_filter_variants() {
        assert_eq!(ReadFilter::All, ReadFilter::All);
        assert_ne!(ReadFilter::Unread, ReadFilter::Read);
    }
}
