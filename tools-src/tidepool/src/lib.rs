wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../../../betterclaw/wit/tool.wit",
});

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const DEFAULT_BASE_URL: &str = "https://spacetimedb.com";
const DEFAULT_MESSAGE_LIMIT: u32 = 100;
const MAX_TEXT_LENGTH: usize = 65_536;

struct TidepoolTool;

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum TidepoolAction {
    Signup {
        database: String,
        handle: String,
        base_url: Option<String>,
    },
    Sql {
        database: String,
        sql: String,
        base_url: Option<String>,
    },
    CreateAccount {
        database: String,
        handle: String,
        base_url: Option<String>,
    },
    CreateDomain {
        database: String,
        kind: String,
        slug: String,
        title: String,
        message_char_limit: Option<u16>,
        base_url: Option<String>,
    },
    CreateDm {
        database: String,
        recipient_account_ids: Vec<u64>,
        title: String,
        base_url: Option<String>,
    },
    CreateDmWithDomainId {
        database: String,
        domain_id: u64,
        recipient_account_ids: Vec<u64>,
        title: String,
        base_url: Option<String>,
    },
    SubscribeDomain {
        database: String,
        domain_id: u64,
        batch_window_seconds: Option<u32>,
        base_url: Option<String>,
    },
    PostMessage {
        database: String,
        domain_id: u64,
        body: String,
        reply_to_message_id: Option<u64>,
        base_url: Option<String>,
    },
    MyDmDomains {
        database: String,
        base_url: Option<String>,
    },
    GetDomainMessages {
        database: String,
        domain_id: u64,
        after_sequence: Option<u64>,
        limit: Option<u32>,
        base_url: Option<String>,
    },
}

#[derive(Debug, Serialize)]
struct ToolOutput {
    ok: bool,
    action: &'static str,
    data: serde_json::Value,
}

impl exports::near::agent::tool::Guest for TidepoolTool {
    fn execute(req: exports::near::agent::tool::Request) -> exports::near::agent::tool::Response {
        match execute_inner(&req.params) {
            Ok(output) => exports::near::agent::tool::Response {
                output: Some(output),
                error: None,
            },
            Err(error) => exports::near::agent::tool::Response {
                output: None,
                error: Some(error),
            },
        }
    }

    fn schema() -> String {
        SCHEMA.to_string()
    }

    fn description() -> String {
        "Interact with Tidepool over the SpacetimeDB HTTP API. Supports account creation, domain creation, domain subscription, canonical DM creation, posting messages, reading domain messages, discovering the caller's DM domains, and issuing explicit SQL queries."
            .to_string()
    }
}

fn execute_inner(params: &str) -> Result<String, String> {
    validate_input_length(params, "params")?;
    let action: TidepoolAction =
        serde_json::from_str(params).map_err(|e| format!("Invalid parameters: {e}"))?;

    let output = match action {
        TidepoolAction::Sql {
            database,
            sql,
            base_url,
        } => ToolOutput {
            ok: true,
            action: "sql",
            data: sql_query(resolve_base_url(base_url)?, &database, &sql)?,
        },
        TidepoolAction::Signup {
            database,
            handle,
            base_url,
        } => {
            let response = call_reducer_response(
                resolve_base_url(base_url)?,
                &database,
                "create_account",
                json!([handle]),
            )?;
            let token = response_header(&response, "spacetime-identity-token")
                .ok_or_else(|| "Signup succeeded but no spacetime-identity-token header was returned".to_string())?;
            ToolOutput {
                ok: true,
                action: "signup",
                data: json!({
                    "database": database,
                    "token": token,
                    "secret_name": "tidepool_token",
                    "next_step": "Store this token with BetterClaw's secret_set tool under tidepool_token."
                }),
            }
        }
        TidepoolAction::CreateAccount {
            database,
            handle,
            base_url,
        } => {
            call_reducer(
                resolve_base_url(base_url)?,
                &database,
                "create_account",
                json!([handle]),
            )?;
            ToolOutput {
                ok: true,
                action: "create_account",
                data: json!({"database": database}),
            }
        }
        TidepoolAction::CreateDomain {
            database,
            kind,
            slug,
            title,
            message_char_limit,
            base_url,
        } => {
            let kind = encode_domain_kind(&kind)?;
            call_reducer(
                resolve_base_url(base_url)?,
                &database,
                "create_domain",
                json!([kind, slug, title, message_char_limit.unwrap_or(280)]),
            )?;
            ToolOutput {
                ok: true,
                action: "create_domain",
                data: json!({"database": database}),
            }
        }
        TidepoolAction::CreateDm {
            database,
            recipient_account_ids,
            title,
            base_url,
        } => {
            call_reducer(
                resolve_base_url(base_url)?,
                &database,
                "create_dm",
                json!([recipient_account_ids, title]),
            )?;
            ToolOutput {
                ok: true,
                action: "create_dm",
                data: json!({"database": database}),
            }
        }
        TidepoolAction::CreateDmWithDomainId {
            database,
            domain_id,
            recipient_account_ids,
            title,
            base_url,
        } => {
            call_reducer(
                resolve_base_url(base_url)?,
                &database,
                "create_dm_with_domain_id",
                json!([domain_id, recipient_account_ids, title]),
            )?;
            ToolOutput {
                ok: true,
                action: "create_dm_with_domain_id",
                data: json!({"database": database, "domain_id": domain_id}),
            }
        }
        TidepoolAction::SubscribeDomain {
            database,
            domain_id,
            batch_window_seconds,
            base_url,
        } => {
            call_reducer(
                resolve_base_url(base_url)?,
                &database,
                "subscribe_domain",
                json!([domain_id, batch_window_seconds.unwrap_or(30)]),
            )?;
            ToolOutput {
                ok: true,
                action: "subscribe_domain",
                data: json!({
                    "database": database,
                    "domain_id": domain_id,
                    "batch_window_seconds": batch_window_seconds.unwrap_or(30)
                }),
            }
        }
        TidepoolAction::PostMessage {
            database,
            domain_id,
            body,
            reply_to_message_id,
            base_url,
        } => {
            call_reducer(
                resolve_base_url(base_url)?,
                &database,
                "post_message",
                json!([domain_id, body, reply_to_message_id]),
            )?;
            ToolOutput {
                ok: true,
                action: "post_message",
                data: json!({"database": database, "domain_id": domain_id}),
            }
        }
        TidepoolAction::MyDmDomains { database, base_url } => ToolOutput {
            ok: true,
            action: "my_dm_domains",
            data: sql_query(resolve_base_url(base_url)?, &database, "SELECT domain_id, title, participant_account_ids FROM my_dm_domains")?,
        },
        TidepoolAction::GetDomainMessages {
            database,
            domain_id,
            after_sequence,
            limit,
            base_url,
        } => ToolOutput {
            ok: true,
            action: "get_domain_messages",
            data: filter_message_rows(
                sql_query(
                resolve_base_url(base_url)?,
                &database,
                "SELECT message_id, domain_id, domain_sequence, author_account_id, body, created_at, reply_to_message_id FROM message",
            )?,
                domain_id,
                after_sequence,
                limit.unwrap_or(DEFAULT_MESSAGE_LIMIT),
            )?,
        },
    };

    serde_json::to_string(&output).map_err(|e| format!("Failed to serialize tool output: {e}"))
}

fn resolve_base_url(base_url: Option<String>) -> Result<String, String> {
    let base_url = base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    validate_base_url(&base_url)?;
    Ok(base_url.trim_end_matches('/').to_string())
}

fn validate_base_url(base_url: &str) -> Result<(), String> {
    if base_url.is_empty() {
        return Err("base_url cannot be empty".to_string());
    }
    if !(base_url.starts_with("https://") || base_url.starts_with("http://")) {
        return Err("base_url must start with http:// or https://".to_string());
    }
    Ok(())
}

fn encode_domain_kind(kind: &str) -> Result<Value, String> {
    match kind {
        "public" | "Public" => Ok(json!([0, []])),
        "private" | "Private" => Ok(json!([1, []])),
        "dm" | "Dm" | "DM" => Ok(json!([2, []])),
        _ => Err("kind must be one of: public, private, dm".to_string()),
    }
}

fn sql_query(base_url: String, database: &str, sql: &str) -> Result<serde_json::Value, String> {
    validate_input_length(sql, "sql")?;
    let url = format!("{}/v1/database/{}/sql", base_url, database);
    let response = http_post_text(&url, sql, "text/plain")?;
    parse_json_response(&response)
}

fn call_reducer(
    base_url: String,
    database: &str,
    reducer: &str,
    args: serde_json::Value,
) -> Result<(), String> {
    let response = call_reducer_response(base_url, database, reducer, args)?;
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "Reducer call failed with status {}: {}",
            response.status,
            String::from_utf8_lossy(&response.body)
        ));
    }
    Ok(())
}

fn call_reducer_response(
    base_url: String,
    database: &str,
    reducer: &str,
    args: serde_json::Value,
) -> Result<near::agent::host::HttpResponse, String> {
    let url = format!("{}/v1/database/{}/call/{}", base_url, database, reducer);
    let response = http_post_json(&url, &args)?;
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "Reducer call failed with status {}: {}",
            response.status,
            String::from_utf8_lossy(&response.body)
        ));
    }
    Ok(response)
}

fn response_header(response: &near::agent::host::HttpResponse, name: &str) -> Option<String> {
    let headers: serde_json::Value = serde_json::from_str(&response.headers_json).ok()?;
    let object = headers.as_object()?;
    object.iter().find_map(|(key, value)| {
        if key.eq_ignore_ascii_case(name) {
            value.as_str().map(ToString::to_string)
        } else {
            None
        }
    })
}

fn http_post_json(
    url: &str,
    body: &serde_json::Value,
) -> Result<near::agent::host::HttpResponse, String> {
    let body_bytes =
        serde_json::to_vec(body).map_err(|e| format!("Failed to encode JSON body: {e}"))?;
    http_post_bytes(url, &body_bytes, "{\"content-type\":\"application/json\"}")
}

fn http_post_text(
    url: &str,
    body: &str,
    content_type: &str,
) -> Result<near::agent::host::HttpResponse, String> {
    validate_input_length(body, "http body")?;
    http_post_bytes(
        url,
        body.as_bytes(),
        &format!("{{\"content-type\":\"{}\"}}", content_type),
    )
}

fn http_post_bytes(
    url: &str,
    body: &[u8],
    headers_json: &str,
) -> Result<near::agent::host::HttpResponse, String> {
    Ok(
        near::agent::host::http_request("POST", url, headers_json, Some(body), None)
            .map_err(|e| format!("HTTP request failed: {e}"))?,
    )
}

fn filter_message_rows(
    sql_result: Value,
    domain_id: u64,
    after_sequence: Option<u64>,
    limit: u32,
) -> Result<Value, String> {
    let limit = limit.min(DEFAULT_MESSAGE_LIMIT) as usize;
    let Some(queries) = sql_result.as_array() else {
        return Err("Unexpected SQL response shape: expected top-level array".to_string());
    };
    if queries.is_empty() {
        return Ok(json!([]));
    }

    let mut filtered_rows: Vec<Value> = queries[0]
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|row| row_matches_domain_message(row, domain_id, after_sequence))
        .collect();

    filtered_rows.sort_by_key(|row| row.get(2).and_then(Value::as_u64).unwrap_or(u64::MAX));
    filtered_rows.truncate(limit);

    let schema = queries[0].get("schema").cloned().unwrap_or(Value::Null);
    Ok(json!({
        "schema": schema,
        "rows": filtered_rows
    }))
}

fn row_matches_domain_message(row: &Value, domain_id: u64, after_sequence: Option<u64>) -> bool {
    let Some(cols) = row.as_array() else {
        return false;
    };
    let row_domain_id = cols.get(1).and_then(Value::as_u64);
    let row_sequence = cols.get(2).and_then(Value::as_u64);

    match (row_domain_id, row_sequence) {
        (Some(row_domain_id), Some(row_sequence)) if row_domain_id == domain_id => after_sequence
            .map(|after| row_sequence > after)
            .unwrap_or(true),
        _ => false,
    }
}

fn parse_json_response(
    response: &near::agent::host::HttpResponse,
) -> Result<serde_json::Value, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "HTTP request failed with status {}: {}",
            response.status,
            String::from_utf8_lossy(&response.body)
        ));
    }

    serde_json::from_slice(&response.body)
        .map_err(|e| format!("Failed to parse JSON response: {e}"))
}

fn validate_input_length(value: &str, field_name: &str) -> Result<(), String> {
    if value.len() > MAX_TEXT_LENGTH {
        return Err(format!(
            "Input '{}' exceeds maximum length of {} characters",
            field_name, MAX_TEXT_LENGTH
        ));
    }
    Ok(())
}

const SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "action": {
      "type": "string",
      "enum": [
        "signup",
        "sql",
        "create_account",
        "create_domain",
        "create_dm",
        "create_dm_with_domain_id",
        "subscribe_domain",
        "post_message",
        "my_dm_domains",
        "get_domain_messages"
      ]
    },
    "database": { "type": "string" },
    "base_url": { "type": "string" },
    "handle": { "type": "string" },
    "sql": { "type": "string" },
    "kind": { "type": "string", "enum": ["public", "private", "dm"] },
    "slug": { "type": "string" },
    "title": { "type": "string" },
    "message_char_limit": { "type": "integer", "minimum": 32, "maximum": 1024 },
    "recipient_account_ids": { "type": "array", "items": { "type": "integer" } },
    "domain_id": { "type": "integer" },
    "batch_window_seconds": { "type": "integer", "minimum": 1, "maximum": 3600 },
    "body": { "type": "string" },
    "reply_to_message_id": { "type": ["integer", "null"] },
    "after_sequence": { "type": "integer" },
    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
  },
  "required": ["action", "database"],
  "additionalProperties": false
}"#;

export!(TidepoolTool);
