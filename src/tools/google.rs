//! Google Tool - Calendar, Sheets, and Gmail via gog CLI
//!
//! Requires gog CLI authenticated for crustaison@gmail.com.
//! Set GOG_KEYRING_PASSWORD=crustaison when invoking gog.

use crate::tools::tool::{Tool, ToolResult};
use async_trait::async_trait;

const GOG_BIN: &str = "/home/sean/.npm-global/bin/gog";
const DEFAULT_ACCOUNT: &str = "crustaison@gmail.com";
const DEFAULT_CALENDAR: &str = "crustaison@gmail.com";
const KEYRING_PASSWORD: &str = "crustaison";

pub struct GoogleTool;

impl GoogleTool {
    pub fn new() -> Self { Self }

    async fn gog(&self, args: Vec<String>) -> Result<String, String> {
        let output = tokio::process::Command::new(GOG_BIN)
            .args(&args)
            .env("GOG_KEYRING_PASSWORD", KEYRING_PASSWORD)
            .output()
            .await
            .map_err(|e| format!("gog not found at {}: {}", GOG_BIN, e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(stdout)
        } else {
            Err(if !stderr.is_empty() { stderr } else { stdout })
        }
    }
}

#[async_trait]
impl Tool for GoogleTool {
    fn name(&self) -> &str { "google" }

    fn description(&self) -> &str {
        "Access Google Calendar, Sheets, and Gmail for crustaison@gmail.com. \
         Actions: \
         'calendar_list' (list upcoming events), \
         'calendar_search' (search events by query), \
         'calendar_create' (create a new event), \
         'calendar_delete' (delete an event by ID), \
         'sheets_read' (read values from a spreadsheet range), \
         'sheets_write' (update values in a spreadsheet range), \
         'sheets_append' (append rows to a spreadsheet), \
         'sheets_create' (create a new spreadsheet), \
         'sheets_info' (get spreadsheet metadata), \
         'gmail_search' (search emails with Gmail query syntax e.g. in:sent, in:inbox, from:email, subject:text), \
         'gmail_send' (send an email). \
         Default account is crustaison@gmail.com."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "calendar_list", "calendar_search", "calendar_create", "calendar_delete",
                        "sheets_read", "sheets_write", "sheets_append", "sheets_create", "sheets_info",
                        "gmail_search", "gmail_send"
                    ],
                    "description": "Action to perform"
                },
                "account": {
                    "type": "string",
                    "description": "Google account email (default: crustaison@gmail.com)"
                },
                "calendar_id": {
                    "type": "string",
                    "description": "Calendar ID (default: crustaison@gmail.com). Use 'calendar_list' to find IDs."
                },
                "query": {
                    "type": "string",
                    "description": "Search query (for calendar_search or gmail_search)"
                },
                "from": {
                    "type": "string",
                    "description": "Start date/time. For all-day events: date only like 2026-03-05. For timed events: RFC3339 with timezone like 2026-03-05T10:00:00-06:00 (Central time is -06:00 winter, -05:00 summer). Date-only input auto-enables all-day mode."
                },
                "to": {
                    "type": "string",
                    "description": "End date/time. Same formats as 'from'."
                },
                "days": {
                    "type": "integer",
                    "description": "For calendar_list: number of upcoming days to show (default: 7)"
                },
                "summary": {
                    "type": "string",
                    "description": "REQUIRED for calendar_create: event title/summary"
                },
                "description": {
                    "type": "string",
                    "description": "Event description or email body"
                },
                "location": {
                    "type": "string",
                    "description": "Event location (for calendar_create)"
                },
                "attendees": {
                    "type": "string",
                    "description": "Comma-separated attendee emails (for calendar_create)"
                },
                "all_day": {
                    "type": "boolean",
                    "description": "Create an all-day event (use date-only in from/to)"
                },
                "event_id": {
                    "type": "string",
                    "description": "Event ID (for calendar_delete)"
                },
                "spreadsheet_id": {
                    "type": "string",
                    "description": "REQUIRED for sheets_read/write/append/info: Google Sheets spreadsheet ID (from URL)"
                },
                "range": {
                    "type": "string",
                    "description": "REQUIRED for sheets_read/write/append: A1 notation range (e.g. 'Sheet1!A1:D10' or 'A1:B5')"
                },
                "values": {
                    "type": "string",
                    "description": "For sheets_write/append: tab-separated values, rows separated by newlines. Example: 'Alice\t30\nBob\t25'"
                },
                "title": {
                    "type": "string",
                    "description": "Title for sheets_create"
                },
                "to_email": {
                    "type": "string",
                    "description": "REQUIRED for gmail_send: recipient email address"
                },
                "subject": {
                    "type": "string",
                    "description": "REQUIRED for gmail_send: email subject"
                },
                "body": {
                    "type": "string",
                    "description": "REQUIRED for gmail_send: email body text"
                },
                "max": {
                    "type": "integer",
                    "description": "Max results to return (default: 10)"
                }
            },
            "required": ["action"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a.to_string(),
            None => return ToolResult::err("Missing 'action' parameter"),
        };

        let account = args.get("account").and_then(|v| v.as_str()).unwrap_or(DEFAULT_ACCOUNT);

        match action.as_str() {
            "calendar_list"   => self.calendar_list(&args, account).await,
            "calendar_search" => self.calendar_search(&args, account).await,
            "calendar_create" => self.calendar_create(&args, account).await,
            "calendar_delete" => self.calendar_delete(&args, account).await,
            "sheets_read"     => self.sheets_read(&args, account).await,
            "sheets_write"    => self.sheets_write(&args, account).await,
            "sheets_append"   => self.sheets_append(&args, account).await,
            "sheets_create"   => self.sheets_create(&args, account).await,
            "sheets_info"     => self.sheets_info(&args, account).await,
            "gmail_search"    => self.gmail_search(&args, account).await,
            "gmail_send"      => self.gmail_send(&args, account).await,
            _ => ToolResult::err(format!("Unknown action: {}", action)),
        }
    }
}

impl GoogleTool {
    async fn calendar_list(&self, args: &serde_json::Value, account: &str) -> ToolResult {
        let cal_id = args.get("calendar_id").and_then(|v| v.as_str()).unwrap_or(DEFAULT_CALENDAR);
        let days = args.get("days").and_then(|v| v.as_i64()).unwrap_or(7);
        let max = args.get("max").and_then(|v| v.as_i64()).unwrap_or(20);

        let mut gog_args = vec![
            "calendar".to_string(), "events".to_string(), cal_id.to_string(),
            "-a".to_string(), account.to_string(),
            "-p".to_string(),
            "--days".to_string(), days.to_string(),
            "--max".to_string(), max.to_string(),
        ];

        if let Some(from) = args.get("from").and_then(|v| v.as_str()) {
            gog_args.extend(["--from".to_string(), from.to_string()]);
        }
        if let Some(to) = args.get("to").and_then(|v| v.as_str()) {
            gog_args.extend(["--to".to_string(), to.to_string()]);
        }

        match self.gog(gog_args).await {
            Ok(out) => {
                if out.trim().is_empty() {
                    ToolResult::ok("No upcoming events found.")
                } else {
                    ToolResult::ok(format!("Upcoming calendar events:\n{}", out))
                }
            }
            Err(e) => ToolResult::err(format!("Calendar list failed: {}", e)),
        }
    }

    async fn calendar_search(&self, args: &serde_json::Value, account: &str) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::err("'query' is required for calendar_search"),
        };

        let mut gog_args = vec![
            "calendar".to_string(), "search".to_string(), query.to_string(),
            "-a".to_string(), account.to_string(),
            "-p".to_string(),
        ];

        if let Some(max) = args.get("max").and_then(|v| v.as_i64()) {
            gog_args.extend(["--max".to_string(), max.to_string()]);
        }

        match self.gog(gog_args).await {
            Ok(out) => {
                if out.trim().is_empty() {
                    ToolResult::ok(format!("No events found matching '{}'", query))
                } else {
                    ToolResult::ok(out)
                }
            }
            Err(e) => ToolResult::err(format!("Calendar search failed: {}", e)),
        }
    }

    async fn calendar_create(&self, args: &serde_json::Value, account: &str) -> ToolResult {
        // Accept canonical names and common LLM aliases
        let summary = match args.get("summary").or_else(|| args.get("title")).and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::err("'summary' (or 'title') is required for calendar_create"),
        };
        let from_raw = match args.get("from").or_else(|| args.get("start_time")).or_else(|| args.get("start")).or_else(|| args.get("startDate")).or_else(|| args.get("start_date")).or_else(|| args.get("date")).and_then(|v| v.as_str()) {
            Some(f) => f,
            None => return ToolResult::err("'from' (or 'start_time') is required for calendar_create"),
        };
        let cal_id = args.get("calendar_id").and_then(|v| v.as_str()).unwrap_or(DEFAULT_CALENDAR);

        // Detect all-day: date-only string (YYYY-MM-DD = 10 chars, no 'T')
        let explicit_all_day = args.get("all_day").and_then(|v| v.as_bool()).unwrap_or(false);
        let from_is_date_only = from_raw.len() == 10 && !from_raw.contains('T');
        let is_all_day = explicit_all_day || from_is_date_only;

        // For timed events without timezone, append Central time offset (America/Chicago)
        // Timezone-aware if: ends with Z, or char at pos 19 is '+' or '-'
        let tz_offset = "-06:00";
        let needs_tz = |s: &str| -> bool {
            !s.contains('T') || s.len() < 19 || (!s.ends_with('Z') && {
                let b = s.as_bytes();
                !(b.len() > 19 && (b[19] == b'+' || b[19] == b'-'))
            })
        };
        let from = if is_all_day {
            from_raw.to_string()
        } else if from_raw.len() == 19 && needs_tz(from_raw) {
            format!("{}{}", from_raw, tz_offset)
        } else {
            from_raw.to_string()
        };

        let mut gog_args = vec![
            "calendar".to_string(), "create".to_string(), cal_id.to_string(),
            "-a".to_string(), account.to_string(),
            "-p".to_string(),
            "--summary".to_string(), summary.to_string(),
            "--from".to_string(), from,
            "--no-input".to_string(),
        ];

        if let Some(to_raw) = args.get("to").or_else(|| args.get("end_time")).or_else(|| args.get("end")).or_else(|| args.get("endDate")).or_else(|| args.get("end_date")).and_then(|v| v.as_str()) {
            let to = if is_all_day {
                to_raw.to_string()
            } else if to_raw.len() == 19 && needs_tz(to_raw) {
                format!("{}{}", to_raw, tz_offset)
            } else {
                to_raw.to_string()
            };
            gog_args.extend(["--to".to_string(), to]);
        } else if is_all_day {
            // For all-day with no end date, use from date + 1 day as end
            // (gog requires --to for create)
            // Just duplicate from date as end — gog will handle it
        }
        if let Some(desc) = args.get("description").and_then(|v| v.as_str()) {
            gog_args.extend(["--description".to_string(), desc.to_string()]);
        }
        if let Some(loc) = args.get("location").and_then(|v| v.as_str()) {
            gog_args.extend(["--location".to_string(), loc.to_string()]);
        }
        if let Some(att) = args.get("attendees").and_then(|v| v.as_str()) {
            gog_args.extend(["--attendees".to_string(), att.to_string()]);
        }
        if is_all_day {
            gog_args.push("--all-day".to_string());
        }

        match self.gog(gog_args).await {
            Ok(out) => ToolResult::ok(format!("Event created: {}", out.trim())),
            Err(e) => ToolResult::err(format!("Calendar create failed: {}", e)),
        }
    }

    async fn calendar_delete(&self, args: &serde_json::Value, account: &str) -> ToolResult {
        let event_id = match args.get("event_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return ToolResult::err("'event_id' is required for calendar_delete"),
        };
        let cal_id = args.get("calendar_id").and_then(|v| v.as_str()).unwrap_or(DEFAULT_CALENDAR);

        let gog_args = vec![
            "calendar".to_string(), "delete".to_string(), cal_id.to_string(), event_id.to_string(),
            "-a".to_string(), account.to_string(),
            "-y".to_string(),
        ];

        match self.gog(gog_args).await {
            Ok(_) => ToolResult::ok(format!("Event {} deleted.", event_id)),
            Err(e) => ToolResult::err(format!("Calendar delete failed: {}", e)),
        }
    }

    async fn sheets_read(&self, args: &serde_json::Value, account: &str) -> ToolResult {
        let spreadsheet_id = match args.get("spreadsheet_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return ToolResult::err("'spreadsheet_id' is required for sheets_read"),
        };
        let range = match args.get("range").and_then(|v| v.as_str()) {
            Some(r) => r,
            None => return ToolResult::err("'range' is required for sheets_read"),
        };

        let gog_args = vec![
            "sheets".to_string(), "get".to_string(), spreadsheet_id.to_string(), range.to_string(),
            "-a".to_string(), account.to_string(),
            "-p".to_string(),
        ];

        match self.gog(gog_args).await {
            Ok(out) => {
                if out.trim().is_empty() {
                    ToolResult::ok("Range is empty.")
                } else {
                    ToolResult::ok(out)
                }
            }
            Err(e) => ToolResult::err(format!("Sheets read failed: {}", e)),
        }
    }

    async fn sheets_write(&self, args: &serde_json::Value, account: &str) -> ToolResult {
        let spreadsheet_id = match args.get("spreadsheet_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return ToolResult::err("'spreadsheet_id' is required for sheets_write"),
        };
        let range = match args.get("range").and_then(|v| v.as_str()) {
            Some(r) => r,
            None => return ToolResult::err("'range' is required for sheets_write"),
        };
        let values_raw = match args.get("values") {
            Some(v) => v,
            None => return ToolResult::err("'values' is required for sheets_write"),
        };
        // Accept values as either a JSON array or a string
        let values_json_str: String;
        let values: &str = if let Some(s) = values_raw.as_str() {
            s
        } else {
            values_json_str = values_raw.to_string();
            &values_json_str
        };

        // If values looks like a JSON array, use --values-json (handles any size)
        // Otherwise fall back to positional args (newline-delimited, tab-separated rows)
        let gog_args = if values.trim_start().starts_with('[') {
            vec![
                "sheets".to_string(), "update".to_string(),
                spreadsheet_id.to_string(), range.to_string(),
                "-a".to_string(), account.to_string(),
                format!("--values-json={}", values),
            ]
        } else {
            let mut a = vec![
                "sheets".to_string(), "update".to_string(),
                spreadsheet_id.to_string(), range.to_string(),
                "-a".to_string(), account.to_string(),
            ];
            for row in values.split('\n') {
                if !row.trim().is_empty() {
                    a.push(row.to_string());
                }
            }
            a
        };

        match self.gog(gog_args).await {
            Ok(out) => ToolResult::ok(format!("Sheets updated. {}", out.trim())),
            Err(e) => ToolResult::err(format!("Sheets write failed: {}", e)),
        }
    }

    async fn sheets_append(&self, args: &serde_json::Value, account: &str) -> ToolResult {
        let spreadsheet_id = match args.get("spreadsheet_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return ToolResult::err("'spreadsheet_id' is required for sheets_append"),
        };
        let range = match args.get("range").and_then(|v| v.as_str()) {
            Some(r) => r,
            None => return ToolResult::err("'range' is required for sheets_append"),
        };
        let values_raw = match args.get("values") {
            Some(v) => v,
            None => return ToolResult::err("'values' is required for sheets_append"),
        };
        let values_json_str: String;
        let values: &str = if let Some(s) = values_raw.as_str() {
            s
        } else {
            values_json_str = values_raw.to_string();
            &values_json_str
        };

        let gog_args = if values.trim_start().starts_with('[') {
            vec![
                "sheets".to_string(), "append".to_string(),
                spreadsheet_id.to_string(), range.to_string(),
                "-a".to_string(), account.to_string(),
                format!("--values-json={}", values),
            ]
        } else {
            let mut a = vec![
                "sheets".to_string(), "append".to_string(),
                spreadsheet_id.to_string(), range.to_string(),
                "-a".to_string(), account.to_string(),
            ];
            for row in values.split('\n') {
                if !row.trim().is_empty() {
                    a.push(row.to_string());
                }
            }
            a
        };

        match self.gog(gog_args).await {
            Ok(out) => ToolResult::ok(format!("Rows appended. {}", out.trim())),
            Err(e) => ToolResult::err(format!("Sheets append failed: {}", e)),
        }
    }

    async fn sheets_create(&self, args: &serde_json::Value, account: &str) -> ToolResult {
        let title = match args.get("title").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::err("'title' is required for sheets_create"),
        };

        let gog_args = vec![
            "sheets".to_string(), "create".to_string(), title.to_string(),
            "-a".to_string(), account.to_string(),
            "-p".to_string(),
        ];

        match self.gog(gog_args).await {
            Ok(out) => ToolResult::ok(format!("Spreadsheet created:\n{}", out.trim())),
            Err(e) => ToolResult::err(format!("Sheets create failed: {}", e)),
        }
    }

    async fn sheets_info(&self, args: &serde_json::Value, account: &str) -> ToolResult {
        let spreadsheet_id = match args.get("spreadsheet_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return ToolResult::err("'spreadsheet_id' is required for sheets_info"),
        };

        let gog_args = vec![
            "sheets".to_string(), "metadata".to_string(), spreadsheet_id.to_string(),
            "-a".to_string(), account.to_string(),
            "-p".to_string(),
        ];

        match self.gog(gog_args).await {
            Ok(out) => ToolResult::ok(out),
            Err(e) => ToolResult::err(format!("Sheets info failed: {}", e)),
        }
    }

    async fn gmail_search(&self, args: &serde_json::Value, account: &str) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::err("'query' is required for gmail_search"),
        };
        let max = args.get("max").and_then(|v| v.as_i64()).unwrap_or(10);

        let gog_args = vec![
            "gmail".to_string(), "search".to_string(), query.to_string(),
            "-a".to_string(), account.to_string(),
            "-p".to_string(),
            "--max".to_string(), max.to_string(),
        ];

        match self.gog(gog_args).await {
            Ok(out) => {
                if out.trim().is_empty() {
                    ToolResult::ok(format!("No emails found matching '{}'", query))
                } else {
                    ToolResult::ok(out)
                }
            }
            Err(e) => ToolResult::err(format!("Gmail search failed: {}", e)),
        }
    }

    async fn gmail_send(&self, args: &serde_json::Value, account: &str) -> ToolResult {
        let to = match args.get("to_email").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::err("'to_email' is required for gmail_send"),
        };
        let subject = match args.get("subject").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::err("'subject' is required for gmail_send"),
        };
        let body = match args.get("body").and_then(|v| v.as_str()) {
            Some(b) => b,
            None => return ToolResult::err("'body' is required for gmail_send"),
        };

        let gog_args = vec![
            "gmail".to_string(), "send".to_string(),
            "-a".to_string(), account.to_string(),
            "--to".to_string(), to.to_string(),
            "--subject".to_string(), subject.to_string(),
            "--body".to_string(), body.to_string(),
        ];

        match self.gog(gog_args).await {
            Ok(out) => ToolResult::ok(format!("Email sent to {}. {}", to, out.trim())),
            Err(e) => ToolResult::err(format!("Gmail send failed: {}", e)),
        }
    }
}
