wit_bindgen::generate!({
    world: "sandboxed-channel",
    path: "../../../betterclaw/wit/channel.wit",
});

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use exports::near::agent::channel::{
    AgentResponse, ChannelConfig, Guest, IncomingHttpRequest, OutgoingHttpResponse, PollConfig,
    StatusUpdate,
};
use near::agent::channel_host::{self, EmittedMessage, HttpResponse, LogLevel};

const DEFAULT_BASE_URL: &str = "https://spacetimedb.com";
const MIN_POLL_INTERVAL_MS: u32 = 30_000;
const DEFAULT_MAX_MESSAGES_PER_POLL: usize = 100;
const MAX_HTTP_BODY_LEN: usize = 256 * 1024;
const CONFIG_PATH: &str = "state/config.json";
const CURSORS_PATH: &str = "state/domain_cursors.json";

struct TidepoolChannel;

#[derive(Debug, Clone, Deserialize, Serialize)]
struct TidepoolChannelConfig {
    database: String,
    #[serde(default = "default_base_url")]
    base_url: String,
    #[serde(default = "default_poll_interval_ms")]
    poll_interval_ms: u32,
    #[serde(default = "default_max_messages_per_poll")]
    max_messages_per_poll: usize,
    #[serde(default)]
    emit_self_messages: bool,
}

#[derive(Debug, Deserialize)]
struct SqlResultSet {
    rows: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct TidepoolAccountRow {
    account_id: u64,
    handle: String,
}

#[derive(Debug, Clone)]
struct TidepoolSubscriptionRow {
    domain_id: u64,
    slug: String,
    title: String,
    message_char_limit: u16,
    batch_window_seconds: u32,
}

#[derive(Debug, Clone)]
struct TidepoolMessageRow {
    message_id: u64,
    domain_id: u64,
    domain_sequence: u64,
    author_account_id: u64,
    body: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TidepoolReplyMetadata {
    base_url: String,
    database: String,
    domain_id: u64,
    domain_title: String,
    message_char_limit: u16,
    reply_to_message_id: Option<u64>,
    last_seen_domain_sequence: u64,
}

impl Guest for TidepoolChannel {
    fn on_start(config_json: String) -> Result<ChannelConfig, String> {
        let mut config: TidepoolChannelConfig = serde_json::from_str(&config_json)
            .map_err(|e| format!("Failed to parse Tidepool config: {e}"))?;
        validate_config(&mut config)?;

        let account = fetch_my_account(&config)?;
        channel_host::workspace_write(
            CONFIG_PATH,
            &serde_json::to_string(&config).map_err(|e| format!("Failed to save config: {e}"))?,
        )
        .map_err(|e| format!("Failed to persist config: {e}"))?;

        channel_host::log(
            LogLevel::Info,
            &format!(
                "Tidepool channel ready for account {} on {} / {}",
                account.handle, config.base_url, config.database
            ),
        );

        Ok(ChannelConfig {
            display_name: format!("Tidepool ({})", config.database),
            http_endpoints: vec![],
            poll: Some(PollConfig {
                interval_ms: config.poll_interval_ms.max(MIN_POLL_INTERVAL_MS),
                enabled: true,
            }),
        })
    }

    fn on_http_request(_req: IncomingHttpRequest) -> OutgoingHttpResponse {
        json_response(
            404,
            json!({
                "error": "Tidepool is a polling-only channel and does not expose webhook endpoints."
            }),
        )
    }

    fn on_poll() {
        if let Err(error) = poll_once() {
            channel_host::log(LogLevel::Error, &format!("Tidepool poll failed: {error}"));
        }
    }

    fn on_respond(response: AgentResponse) -> Result<(), String> {
        let metadata: TidepoolReplyMetadata = serde_json::from_str(&response.metadata_json)
            .map_err(|e| format!("Failed to parse Tidepool reply metadata: {e}"))?;

        let body = clamp_message_body(&response.content, metadata.message_char_limit as usize);
        if body.is_empty() {
            return Ok(());
        }

        call_reducer(
            &metadata.base_url,
            &metadata.database,
            "post_message",
            json!([
                metadata.domain_id,
                body,
                encode_optional_u64(metadata.reply_to_message_id)
            ]),
        )?;

        channel_host::log(
            LogLevel::Debug,
            &format!(
                "Posted Tidepool reply into domain {} ({})",
                metadata.domain_id, metadata.domain_title
            ),
        );
        Ok(())
    }

    fn on_status(_update: StatusUpdate) {}

    fn on_shutdown() {}
}

fn poll_once() -> Result<(), String> {
    let config = load_config()?;
    let account = fetch_my_account(&config)?;
    channel_host::log(
        LogLevel::Debug,
        &format!(
            "Tidepool poll account: id={} handle={}",
            account.account_id, account.handle
        ),
    );
    let subscriptions = fetch_my_subscriptions(&config)?;
    channel_host::log(
        LogLevel::Debug,
        &format!(
            "Tidepool poll subscriptions fetched: {}",
            subscriptions.len()
        ),
    );
    if subscriptions.is_empty() {
        channel_host::log(
            LogLevel::Debug,
            "Tidepool poll found no active subscriptions for this account.",
        );
        return Ok(());
    }

    let mut subscriptions_by_domain: HashMap<u64, TidepoolSubscriptionRow> = subscriptions
        .into_iter()
        .map(|subscription| (subscription.domain_id, subscription))
        .collect();

    let mut cursors = load_cursors()?;
    channel_host::log(
        LogLevel::Debug,
        &format!("Tidepool poll cursors loaded: {:?}", cursors),
    );
    let mut unseen_messages = fetch_my_subscribed_messages(&config)?;
    channel_host::log(
        LogLevel::Debug,
        &format!(
            "Tidepool poll subscribed messages fetched before filtering: {}",
            unseen_messages.len()
        ),
    );
    unseen_messages.retain(|message| {
        message.domain_sequence > cursors.get(&message.domain_id).copied().unwrap_or(0)
    });
    channel_host::log(
        LogLevel::Debug,
        &format!(
            "Tidepool poll messages after cursor filtering: {}",
            unseen_messages.len()
        ),
    );

    if !config.emit_self_messages {
        unseen_messages.retain(|message| message.author_account_id != account.account_id);
        channel_host::log(
            LogLevel::Debug,
            &format!(
                "Tidepool poll messages after self-filtering: {}",
                unseen_messages.len()
            ),
        );
    }

    if unseen_messages.is_empty() {
        channel_host::log(
            LogLevel::Debug,
            "Tidepool poll found no unseen messages to emit.",
        );
        return Ok(());
    }

    unseen_messages.sort_by_key(|message| (message.domain_id, message.domain_sequence));
    unseen_messages.truncate(config.max_messages_per_poll);

    let mut grouped: BTreeMap<u64, Vec<TidepoolMessageRow>> = BTreeMap::new();
    for message in unseen_messages {
        grouped.entry(message.domain_id).or_default().push(message);
    }

    for (domain_id, messages) in grouped {
        let Some(subscription) = subscriptions_by_domain.remove(&domain_id) else {
            channel_host::log(
                LogLevel::Warn,
                &format!(
                    "Tidepool poll dropping messages for domain {} because subscription metadata was missing",
                    domain_id
                ),
            );
            continue;
        };
        let Some(last_message) = messages.last() else {
            continue;
        };

        let metadata = TidepoolReplyMetadata {
            base_url: config.base_url.clone(),
            database: config.database.clone(),
            domain_id,
            domain_title: subscription.title.clone(),
            message_char_limit: subscription.message_char_limit,
            reply_to_message_id: Some(last_message.message_id),
            last_seen_domain_sequence: last_message.domain_sequence,
        };

        let content = render_batch_message(&subscription, &messages);
        let metadata_json = serde_json::to_string(&metadata)
            .map_err(|e| format!("Failed to encode metadata: {e}"))?;

        channel_host::emit_message(&EmittedMessage {
            user_id: format!("tidepool:domain:{domain_id}"),
            user_name: Some(subscription.title.clone()),
            content,
            thread_id: Some(format!("tidepool:domain:{domain_id}")),
            metadata_json,
        });
        channel_host::log(
            LogLevel::Info,
            &format!(
                "Tidepool emitted {} message(s) for domain {} at sequence {}",
                messages.len(),
                domain_id,
                last_message.domain_sequence
            ),
        );

        cursors.insert(domain_id, last_message.domain_sequence);
    }

    save_cursors(&cursors)
}

fn render_batch_message(
    subscription: &TidepoolSubscriptionRow,
    messages: &[TidepoolMessageRow],
) -> String {
    let mut lines = vec![format!(
        "Tidepool domain \"{}\" has {} new message(s).",
        display_domain_name(subscription),
        messages.len()
    )];

    if subscription.batch_window_seconds > 0 {
        lines.push(format!(
            "Configured batch window: {}s.",
            subscription.batch_window_seconds
        ));
    }

    lines.push(String::new());

    for message in messages {
        let line = format!(
            "- seq {} from account {}",
            message.domain_sequence, message.author_account_id
        );
        lines.push(line);
        lines.push(message.body.clone());
        lines.push(String::new());
    }

    lines.join("\n").trim().to_string()
}

fn display_domain_name(subscription: &TidepoolSubscriptionRow) -> String {
    if !subscription.slug.is_empty() {
        format!("{}/{}", subscription.slug, subscription.title)
    } else {
        subscription.title.clone()
    }
}

fn fetch_my_account(config: &TidepoolChannelConfig) -> Result<TidepoolAccountRow, String> {
    let rows = sql_rows(config, "SELECT account_id, handle FROM my_account")?;
    let Some(row) = rows.first() else {
        return Err(
            "No Tidepool account is visible to this auth token. Create an account or fix the token."
                .to_string(),
        );
    };

    Ok(TidepoolAccountRow {
        account_id: row_u64(row, 0, "account_id")?,
        handle: row_string(row, 1, "handle")?,
    })
}

fn fetch_my_subscriptions(
    config: &TidepoolChannelConfig,
) -> Result<Vec<TidepoolSubscriptionRow>, String> {
    let rows = sql_rows(
        config,
        "SELECT domain_id, slug, title, message_char_limit, batch_window_seconds FROM my_subscriptions",
    )?;

    let parsed: Vec<TidepoolSubscriptionRow> = rows
        .into_iter()
        .map(|row| -> Result<TidepoolSubscriptionRow, String> {
            Ok(TidepoolSubscriptionRow {
                domain_id: row_u64(&row, 0, "domain_id")?,
                slug: row_string(&row, 1, "slug")?,
                title: row_string(&row, 2, "title")?,
                message_char_limit: row_u64(&row, 3, "message_char_limit")? as u16,
                batch_window_seconds: row_u64(&row, 4, "batch_window_seconds")? as u32,
            })
        })
        .collect::<Result<_, _>>()?;

    for subscription in &parsed {
        channel_host::log(
            LogLevel::Debug,
            &format!(
                "Tidepool subscription row: domain_id={} slug={} title={}",
                subscription.domain_id, subscription.slug, subscription.title
            ),
        );
    }

    Ok(parsed)
}

fn fetch_my_subscribed_messages(
    config: &TidepoolChannelConfig,
) -> Result<Vec<TidepoolMessageRow>, String> {
    let rows = sql_rows(
        config,
        "SELECT message_id, domain_id, domain_sequence, author_account_id, body FROM my_subscribed_messages",
    )?;

    let parsed: Vec<TidepoolMessageRow> = rows
        .into_iter()
        .map(|row| -> Result<TidepoolMessageRow, String> {
            Ok(TidepoolMessageRow {
                message_id: row_u64(&row, 0, "message_id")?,
                domain_id: row_u64(&row, 1, "domain_id")?,
                domain_sequence: row_u64(&row, 2, "domain_sequence")?,
                author_account_id: row_u64(&row, 3, "author_account_id")?,
                body: row_string(&row, 4, "body")?,
            })
        })
        .collect::<Result<_, _>>()?;

    for message in &parsed {
        channel_host::log(
            LogLevel::Debug,
            &format!(
                "Tidepool subscribed message row: id={} domain={} seq={} author={}",
                message.message_id,
                message.domain_id,
                message.domain_sequence,
                message.author_account_id
            ),
        );
    }

    Ok(parsed)
}

fn sql_rows(config: &TidepoolChannelConfig, sql: &str) -> Result<Vec<Value>, String> {
    let url = format!("{}/v1/database/{}/sql", config.base_url, config.database);
    let response = http_post_text(&url, sql, "text/plain")?;
    let result_sets: Vec<SqlResultSet> = serde_json::from_slice(&response.body)
        .map_err(|e| format!("Failed to parse SQL response JSON: {e}"))?;

    Ok(result_sets
        .into_iter()
        .next()
        .map(|set| set.rows)
        .unwrap_or_default())
}

fn call_reducer(base_url: &str, database: &str, reducer: &str, args: Value) -> Result<(), String> {
    let url = format!("{}/v1/database/{}/call/{}", base_url, database, reducer);
    let response = http_post_json(&url, &args)?;
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "Reducer call failed with status {}: {}",
            response.status,
            String::from_utf8_lossy(&response.body)
        ));
    }
    Ok(())
}

fn encode_optional_u64(value: Option<u64>) -> Value {
    match value {
        Some(id) => json!([0, id]),
        None => Value::Null,
    }
}

fn clamp_message_body(body: &str, max_chars: usize) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() || max_chars == 0 {
        return String::new();
    }

    let char_count = trimmed.chars().count();
    if char_count <= max_chars {
        return trimmed.to_string();
    }

    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    let shortened: String = trimmed.chars().take(max_chars - 3).collect();
    format!("{shortened}...")
}

fn http_post_json(url: &str, body: &Value) -> Result<HttpResponse, String> {
    let bytes = serde_json::to_vec(body).map_err(|e| format!("Failed to encode JSON body: {e}"))?;
    http_post_bytes(url, &bytes, "{\"content-type\":\"application/json\"}")
}

fn http_post_text(url: &str, body: &str, content_type: &str) -> Result<HttpResponse, String> {
    if body.len() > MAX_HTTP_BODY_LEN {
        return Err(format!(
            "HTTP request body exceeds {} bytes",
            MAX_HTTP_BODY_LEN
        ));
    }
    http_post_bytes(
        url,
        body.as_bytes(),
        &format!("{{\"content-type\":\"{}\"}}", content_type),
    )
}

fn http_post_bytes(url: &str, body: &[u8], headers_json: &str) -> Result<HttpResponse, String> {
    channel_host::http_request("POST", url, headers_json, Some(body), None)
        .map_err(|e| format!("HTTP request failed: {e}"))
        .and_then(|response| {
            if (200..300).contains(&response.status) {
                Ok(response)
            } else {
                Err(format!(
                    "HTTP request failed with status {}: {}",
                    response.status,
                    String::from_utf8_lossy(&response.body)
                ))
            }
        })
}

fn load_config() -> Result<TidepoolChannelConfig, String> {
    let raw = channel_host::workspace_read(CONFIG_PATH)
        .ok_or_else(|| "Tidepool channel is not initialized. Run on-start first.".to_string())?;
    let mut config: TidepoolChannelConfig =
        serde_json::from_str(&raw).map_err(|e| format!("Failed to parse stored config: {e}"))?;
    validate_config(&mut config)?;
    Ok(config)
}

fn load_cursors() -> Result<HashMap<u64, u64>, String> {
    let raw = channel_host::workspace_read(CURSORS_PATH);
    match raw {
        Some(raw) if !raw.trim().is_empty() => serde_json::from_str(&raw)
            .map_err(|e| format!("Failed to parse Tidepool cursor state: {e}")),
        _ => Ok(HashMap::new()),
    }
}

fn save_cursors(cursors: &HashMap<u64, u64>) -> Result<(), String> {
    let payload = serde_json::to_string(cursors)
        .map_err(|e| format!("Failed to encode cursor state: {e}"))?;
    channel_host::workspace_write(CURSORS_PATH, &payload)
        .map_err(|e| format!("Failed to persist Tidepool cursor state: {e}"))
}

fn validate_config(config: &mut TidepoolChannelConfig) -> Result<(), String> {
    if config.database.trim().is_empty() {
        return Err("config.database cannot be empty".to_string());
    }
    config.base_url = config.base_url.trim_end_matches('/').to_string();
    if !(config.base_url.starts_with("http://") || config.base_url.starts_with("https://")) {
        return Err("config.base_url must start with http:// or https://".to_string());
    }
    config.poll_interval_ms = config.poll_interval_ms.max(MIN_POLL_INTERVAL_MS);
    if config.max_messages_per_poll == 0 {
        config.max_messages_per_poll = DEFAULT_MAX_MESSAGES_PER_POLL;
    }
    Ok(())
}

fn row_array<'a>(row: &'a Value) -> Result<&'a [Value], String> {
    row.as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| "SQL row was not an array".to_string())
}

fn row_u64(row: &Value, index: usize, field_name: &str) -> Result<u64, String> {
    row_array(row)?
        .get(index)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("Missing or invalid {field_name} column"))
}

fn row_string(row: &Value, index: usize, field_name: &str) -> Result<String, String> {
    row_array(row)?
        .get(index)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("Missing or invalid {field_name} column"))
}

fn json_response(status: u16, body: Value) -> OutgoingHttpResponse {
    let body = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    OutgoingHttpResponse {
        status,
        headers_json: "{\"content-type\":\"application/json\"}".to_string(),
        body,
    }
}

fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_string()
}

fn default_poll_interval_ms() -> u32 {
    MIN_POLL_INTERVAL_MS
}

fn default_max_messages_per_poll() -> usize {
    DEFAULT_MAX_MESSAGES_PER_POLL
}

export!(TidepoolChannel);
