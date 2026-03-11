wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../../../betterclaw/wit/tool.wit",
});

use serde::{Deserialize, Serialize};
use serde_json::json;

const DEFAULT_BASE_URL: &str = "https://spacetimedb.com";
const DEFAULT_MESSAGE_LIMIT: u32 = 100;
const MAX_TEXT_LENGTH: usize = 65_536;

struct TidepoolTool;

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum TidepoolAction {
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
        "Interact with Tidepool over the SpacetimeDB HTTP API. Supports account creation, domain creation, canonical DM creation, posting messages, reading domain messages, discovering the caller's DM domains, and issuing explicit SQL queries."
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
            let kind = normalize_domain_kind(&kind)?;
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
            data: sql_query(
                resolve_base_url(base_url)?,
                &database,
                "SELECT domain_id, title, participant_account_ids FROM my_dm_domains ORDER BY domain_id",
            )?,
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
            data: sql_query(
                resolve_base_url(base_url)?,
                &database,
                &build_messages_sql(domain_id, after_sequence, limit.unwrap_or(DEFAULT_MESSAGE_LIMIT)),
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

fn normalize_domain_kind(kind: &str) -> Result<&'static str, String> {
    match kind {
        "public" | "Public" => Ok("Public"),
        "private" | "Private" => Ok("Private"),
        "dm" | "Dm" | "DM" => Ok("Dm"),
        _ => Err("kind must be one of: public, private, dm".to_string()),
    }
}

fn build_messages_sql(domain_id: u64, after_sequence: Option<u64>, limit: u32) -> String {
    let limit = limit.min(DEFAULT_MESSAGE_LIMIT);
    match after_sequence {
        Some(after_sequence) => format!(
            "SELECT message_id, domain_id, domain_sequence, author_account_id, body, created_at, reply_to_message_id \
             FROM message \
             WHERE domain_id = {} AND domain_sequence > {} \
             ORDER BY domain_sequence ASC \
             LIMIT {}",
            domain_id, after_sequence, limit
        ),
        None => format!(
            "SELECT message_id, domain_id, domain_sequence, author_account_id, body, created_at, reply_to_message_id \
             FROM message \
             WHERE domain_id = {} \
             ORDER BY domain_sequence ASC \
             LIMIT {}",
            domain_id, limit
        ),
    }
}

fn sql_query(base_url: String, database: &str, sql: &str) -> Result<serde_json::Value, String> {
    validate_input_length(sql, "sql")?;
    let url = format!("{}/v1/database/{}/sql", base_url, database);
    let body = json!({ "query": sql });
    let response = http_post_json(&url, &body)?;
    parse_json_response(&response)
}

fn call_reducer(
    base_url: String,
    database: &str,
    reducer: &str,
    args: serde_json::Value,
) -> Result<(), String> {
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

fn http_post_json(url: &str, body: &serde_json::Value) -> Result<near::agent::host::HttpResponse, String> {
    let body_bytes = serde_json::to_vec(body).map_err(|e| format!("Failed to encode JSON body: {e}"))?;
    Ok(near::agent::host::http_request(
        "POST",
        url,
        "{\"content-type\":\"application/json\"}",
        Some(&body_bytes),
        None,
    )
    .map_err(|e| format!("HTTP request failed: {e}"))?)
}

fn parse_json_response(response: &near::agent::host::HttpResponse) -> Result<serde_json::Value, String> {
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
  "oneOf": [
    {
      "properties": {
        "action": { "const": "sql" },
        "database": { "type": "string" },
        "sql": { "type": "string" },
        "base_url": { "type": "string" }
      },
      "required": ["action", "database", "sql"]
    },
    {
      "properties": {
        "action": { "const": "create_account" },
        "database": { "type": "string" },
        "handle": { "type": "string" },
        "base_url": { "type": "string" }
      },
      "required": ["action", "database", "handle"]
    },
    {
      "properties": {
        "action": { "const": "create_domain" },
        "database": { "type": "string" },
        "kind": { "type": "string", "enum": ["public", "private", "dm"] },
        "slug": { "type": "string" },
        "title": { "type": "string" },
        "message_char_limit": { "type": "integer", "minimum": 32, "maximum": 1024 },
        "base_url": { "type": "string" }
      },
      "required": ["action", "database", "kind", "slug", "title"]
    },
    {
      "properties": {
        "action": { "const": "create_dm" },
        "database": { "type": "string" },
        "recipient_account_ids": { "type": "array", "items": { "type": "integer" } },
        "title": { "type": "string" },
        "base_url": { "type": "string" }
      },
      "required": ["action", "database", "recipient_account_ids", "title"]
    },
    {
      "properties": {
        "action": { "const": "create_dm_with_domain_id" },
        "database": { "type": "string" },
        "domain_id": { "type": "integer" },
        "recipient_account_ids": { "type": "array", "items": { "type": "integer" } },
        "title": { "type": "string" },
        "base_url": { "type": "string" }
      },
      "required": ["action", "database", "domain_id", "recipient_account_ids", "title"]
    },
    {
      "properties": {
        "action": { "const": "post_message" },
        "database": { "type": "string" },
        "domain_id": { "type": "integer" },
        "body": { "type": "string" },
        "reply_to_message_id": { "type": ["integer", "null"] },
        "base_url": { "type": "string" }
      },
      "required": ["action", "database", "domain_id", "body"]
    },
    {
      "properties": {
        "action": { "const": "my_dm_domains" },
        "database": { "type": "string" },
        "base_url": { "type": "string" }
      },
      "required": ["action", "database"]
    },
    {
      "properties": {
        "action": { "const": "get_domain_messages" },
        "database": { "type": "string" },
        "domain_id": { "type": "integer" },
        "after_sequence": { "type": "integer" },
        "limit": { "type": "integer", "minimum": 1, "maximum": 100 },
        "base_url": { "type": "string" }
      },
      "required": ["action", "database", "domain_id"]
    }
  ]
}"#;

export!(TidepoolTool);
