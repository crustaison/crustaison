//! Web Tool - Web Search, Weather, and HTTP Requests
//!
//! Provides web search via DuckDuckGo, weather via wttr.in/Open-Meteo,
//! and full HTTP requests (GET/POST/PUT/DELETE) with custom headers.

use crate::tools::{Tool, ToolResult};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Configuration for web tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    /// User agent for HTTP requests
    pub user_agent: String,
    /// Request timeout (seconds)
    pub timeout: u64,
    /// Enable search
    pub search_enabled: bool,
    /// Enable weather
    pub weather_enabled: bool,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            user_agent: "Crustaison/1.0".to_string(),
            timeout: 10,
            search_enabled: true,
            weather_enabled: true,
        }
    }
}

/// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Web tool - search, weather, fetch, and full HTTP requests
pub struct WebTool {
    client: reqwest::Client,
    config: WebConfig,
}

impl WebTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::limited(5))
                .build()
                .unwrap_or_default(),
            config: WebConfig::default(),
        }
    }

    pub fn with_config(config: WebConfig) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(config.timeout))
                .redirect(reqwest::redirect::Policy::limited(5))
                .build()
                .unwrap_or_default(),
            config,
        }
    }
}

impl Default for WebTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for WebTool {
    fn name(&self) -> &str {
        "web"
    }

    fn description(&self) -> &str {
        "Web tool with multiple actions:\n\
         - search: Search the web via DuckDuckGo. Params: query\n\
         - weather: Get weather for a location. Params: location\n\
         - fetch: GET a URL and return content. Params: url\n\
         - http_request: Full HTTP request with method, headers, body. Params: url, method (GET/POST/PUT/DELETE), headers (object), body (string)"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["search", "weather", "fetch", "http_request"],
                    "description": "The action to perform"
                },
                "query": {
                    "type": "string",
                    "description": "Search query or location"
                },
                "location": {
                    "type": "string",
                    "description": "Location for weather (city name or lat,lon)"
                },
                "url": {
                    "type": "string",
                    "description": "URL to fetch or send request to"
                },
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE"],
                    "description": "HTTP method for http_request (default: GET)"
                },
                "headers": {
                    "type": "object",
                    "description": "Custom HTTP headers as key-value pairs, e.g. {\"x-api-key\": \"abc123\", \"Content-Type\": \"application/json\"}"
                },
                "body": {
                    "type": "string",
                    "description": "Request body for POST/PUT (typically JSON string)"
                }
            },
            "required": ["action"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::err("Missing 'action' parameter"),
        };

        match action {
            "search" => {
                if !self.config.search_enabled {
                    return ToolResult::err("Search is disabled");
                }
                let query = match args.get("query").and_then(|v| v.as_str()) {
                    Some(q) => q,
                    None => return ToolResult::err("Missing 'query' for search"),
                };
                self.search(query).await
            }
            "weather" => {
                if !self.config.weather_enabled {
                    return ToolResult::err("Weather is disabled");
                }
                let location = match args.get("location").or(args.get("query")).and_then(|v| v.as_str()) {
                    Some(l) => l,
                    None => return ToolResult::err("Missing location for weather"),
                };
                self.weather(location).await
            }
            "fetch" => {
                let url = match args.get("url").and_then(|v| v.as_str()) {
                    Some(u) => u,
                    None => return ToolResult::err("Missing 'url' for fetch"),
                };
                self.fetch(url).await
            }
            "http_request" => {
                let url = match args.get("url").and_then(|v| v.as_str()) {
                    Some(u) => u,
                    None => return ToolResult::err("Missing 'url' for http_request"),
                };
                let method = args.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
                let headers = args.get("headers").and_then(|v| v.as_object());
                let body = args.get("body").and_then(|v| v.as_str());
                self.http_request(url, method, headers, body).await
            }
            _ => ToolResult::err(format!("Unknown action: {}. Available: search, weather, fetch, http_request", action)),
        }
    }
}

impl WebTool {
    async fn search(&self, query: &str) -> ToolResult {
        // Use DuckDuckGo instant answer API (no CAPTCHA)
        let url = format!(
            "https://api.duckduckgo.com/?q={}&format=json&no_html=1",
            urlencoding::encode(query)
        );

        let response = match self.client.get(&url)
            .header("User-Agent", &self.config.user_agent)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(format!("Search request failed: {}", e)),
        };

        if !response.status().is_success() {
            return ToolResult::err(format!("Search failed: {}", response.status()));
        }

        let text = match response.text().await {
            Ok(t) => t,
            Err(e) => return ToolResult::err(format!("Failed to read response: {}", e)),
        };

        // Parse JSON response
        let json: serde_json::Value = match serde_json::from_str(&text) {
            Ok(j) => j,
            Err(e) => return ToolResult::err(format!("Failed to parse response: {}", e)),
        };

        // Extract Abstract (Wikipedia-style answer)
        if let Some(abstract_text) = json.get("AbstractText").and_then(|v| v.as_str()) {
            if !abstract_text.is_empty() {
                let source = json.get("AbstractSource").and_then(|v| v.as_str()).unwrap_or("DuckDuckGo");
                return ToolResult::ok(format!(
                    "[UNTRUSTED_EXTERNAL_CONTENT]\n{}\nSource: {}\nURL: {}\n[/UNTRUSTED_EXTERNAL_CONTENT]",
                    abstract_text,
                    source,
                    json.get("AbstractURL").and_then(|v| v.as_str()).unwrap_or("")
                ));
            }
        }

        // If no abstract, try related topics
        let mut results_text = String::new();
        if let Some(topics) = json.get("RelatedTopics").and_then(|v| v.as_array()) {
            for topic in topics.iter().take(5) {
                if let (Some(text), Some(url)) = (
                    topic.get("Text").and_then(|v| v.as_str()),
                    topic.get("FirstURL").and_then(|v| v.as_str())
                ) {
                    results_text.push_str(&format!("- {}\n  {}\n\n", text, url));
                }
            }
        }

        if results_text.is_empty() {
            // Try Wikipedia as fallback
            return self.search_wikipedia(query).await;
        } else {
            ToolResult::ok(format!(
                "Search results for '{}':\n\n[UNTRUSTED_EXTERNAL_CONTENT]\n{}\n[/UNTRUSTED_EXTERNAL_CONTENT]",
                query, results_text
            ))
        }
    }

    async fn search_wikipedia(&self, query: &str) -> ToolResult {
        let url = format!(
            "https://en.wikipedia.org/w/api.php?action=query&list=search&srsearch={}&format=json&limit=5",
            urlencoding::encode(query)
        );

        let response = match self.client.get(&url)
            .header("User-Agent", &self.config.user_agent)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::ok(format!("No results found for: {}", query)),
        };

        let text = match response.text().await {
            Ok(t) => t,
            Err(_) => return ToolResult::ok(format!("No results found for: {}", query)),
        };

        let json: serde_json::Value = match serde_json::from_str(&text) {
            Ok(j) => j,
            Err(_) => return ToolResult::ok(format!("No results found for: {}", query)),
        };

        let results = json.get("query")
            .and_then(|q| q.get("search"))
            .and_then(|s| s.as_array());

        if let Some(results) = results {
            if results.is_empty() {
                return ToolResult::ok(format!("No results found for: {}", query));
            }

            let mut output = String::from("Wikipedia results:\n\n[UNTRUSTED_EXTERNAL_CONTENT]\n");
            for r in results.iter().take(5) {
                let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let snippet = r.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
                let pageid = r.get("pageid").and_then(|v| v.as_i64()).unwrap_or(0);
                let url = format!("https://en.wikipedia.org/?curid={}", pageid);
                output.push_str(&format!("- {}\n  {}\n  {}\n\n", title, snippet.replace("\"", ""), url));
            }
            output.push_str("[/UNTRUSTED_EXTERNAL_CONTENT]");

            return ToolResult::ok(output);
        }

        ToolResult::ok(format!("No results found for: {}", query))
    }

    async fn weather(&self, location: &str) -> ToolResult {
        // Clean location for geocoding: "Eldon, Missouri" -> "Eldon"
        let clean_loc = location.split(',').next().unwrap_or(location).trim();

        // Primary: Open-Meteo API (reliable, structured data)
        let geocode_url = format!(
            "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1",
            urlencoding::encode(clean_loc)
        );

        let geocode_result = self.client.get(&geocode_url).send().await;
        if let Ok(resp) = geocode_result {
            if let Ok(geocode_resp) = resp.json::<serde_json::Value>().await {
                if let Some(lat) = geocode_resp.get("results")
                    .and_then(|r| r.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|r| r.get("latitude"))
                    .and_then(|v| v.as_f64())
                {
                    let lon = geocode_resp["results"][0]["longitude"].as_f64().unwrap_or(0.0);
                    let location_name = geocode_resp["results"][0]["name"]
                        .as_str()
                        .unwrap_or(location);
                    let state = geocode_resp["results"][0]["admin1"]
                        .as_str()
                        .unwrap_or("");

                    let weather_url = format!(
                        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m,apparent_temperature&temperature_unit=fahrenheit&wind_speed_unit=mph",
                        lat, lon
                    );

                    if let Ok(weather_resp) = self.client.get(&weather_url).send().await {
                        if let Ok(data) = weather_resp.json::<serde_json::Value>().await {
                            let temp = data["current"]["temperature_2m"].as_f64().unwrap_or(0.0);
                            let feels_like = data["current"]["apparent_temperature"].as_f64().unwrap_or(temp);
                            let wind = data["current"]["wind_speed_10m"].as_f64().unwrap_or(0.0);
                            let humidity = data["current"]["relative_humidity_2m"].as_f64().unwrap_or(0.0);
                            let code = data["current"]["weather_code"].as_i64().unwrap_or(0);
                            let conditions = self.weather_code_to_string(code);

                            let display_loc = if state.is_empty() {
                                location_name.to_string()
                            } else {
                                format!("{}, {}", location_name, state)
                            };

                            return ToolResult::ok(format!(
                                "Weather for {}:\n{} | {:.0}°F (feels like {:.0}°F) | Wind: {:.0} mph | Humidity: {:.0}%",
                                display_loc, conditions, temp, feels_like, wind, humidity
                            ));
                        }
                    }
                }
            }
        }

        // Fallback: wttr.in
        let wttr_url = format!(
            "https://wttr.in/{}?format=%l:+%C+%t+%w+%h+humidity",
            urlencoding::encode(location)
        );

        if let Ok(resp) = self.client.get(&wttr_url)
            .header("User-Agent", "curl/7.0")
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(text) = resp.text().await {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() && !trimmed.contains("Unknown location") && !trimmed.contains("Sorry") {
                        return ToolResult::ok(format!("Weather for {}:\n{}", location, trimmed));
                    }
                }
            }
        }

        ToolResult::err(format!("Could not get weather for: {}", location))
    }

    fn weather_code_to_string(&self, code: i64) -> String {
        // WMO Weather interpretation codes
        match code {
            0 => "Clear sky",
            1 => "Mainly clear",
            2 => "Partly cloudy",
            3 => "Overcast",
            45 | 48 => "Fog",
            51 | 53 | 55 => "Drizzle",
            56 | 57 => "Freezing drizzle",
            61 | 63 | 65 => "Rain",
            66 | 67 => "Freezing rain",
            71 | 73 | 75 => "Snow",
            77 => "Snow grains",
            80 | 81 | 82 => "Rain showers",
            85 | 86 => "Snow showers",
            95 => "Thunderstorm",
            96 | 99 => "Thunderstorm with hail",
            _ => "Unknown",
        }.to_string()
    }

    async fn fetch(&self, url: &str) -> ToolResult {
        // Basic URL validation
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return ToolResult::err("URL must start with http:// or https://");
        }

        let response = match self.client.get(url)
            .header("User-Agent", &self.config.user_agent)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(format!("Fetch failed: {}", e)),
        };

        let status = response.status();
        if !status.is_success() {
            return ToolResult::err(format!("Fetch failed: HTTP {}", status));
        }

        // Clone content_type before consuming response
        let content_type = response.headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "".to_string());

        // For HTML, extract readable text
        if content_type.contains("text/html") {
            let html = match response.text().await {
                Ok(t) => t,
                Err(e) => return ToolResult::err(format!("Failed to read HTML: {}", e)),
            };

            // Simple HTML to text extraction
            let text = self.html_to_text(&html);
            let truncated = if text.len() > 5000 {
                &text[..5000]
            } else {
                &text
            };

            let result = format!(
                "URL: {}\nContent-Type: {}\n\n[UNTRUSTED_EXTERNAL_CONTENT]\n{}\n[/UNTRUSTED_EXTERNAL_CONTENT]\n\n{}",
                url,
                content_type,
                truncated,
                if text.len() > 5000 { "... [truncated]" } else { "" }
            );

            ToolResult::ok(result)
        } else {
            // For text content, return it directly
            if content_type.contains("text/") || content_type.contains("application/json") {
                let text = match response.text().await {
                    Ok(t) => t,
                    Err(e) => return ToolResult::err(format!("Failed to read response: {}", e)),
                };
                let truncated = if text.len() > 5000 { &text[..5000] } else { &text };
                ToolResult::ok(format!(
                    "URL: {}\nContent-Type: {}\n\n[UNTRUSTED_EXTERNAL_CONTENT]\n{}\n[/UNTRUSTED_EXTERNAL_CONTENT]{}",
                    url, content_type, truncated,
                    if text.len() > 5000 { "\n... [truncated]" } else { "" }
                ))
            } else {
                ToolResult::ok(format!(
                    "URL: {}\nContent-Type: {}\n\n[Binary content - {} bytes]",
                    url,
                    content_type,
                    response.content_length().unwrap_or(0)
                ))
            }
        }
    }

    /// Full HTTP request with method, custom headers, and body
    async fn http_request(
        &self,
        url: &str,
        method: &str,
        headers: Option<&serde_json::Map<String, serde_json::Value>>,
        body: Option<&str>,
    ) -> ToolResult {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return ToolResult::err("URL must start with http:// or https://");
        }

        let method_parsed = match method.to_uppercase().as_str() {
            "GET" => reqwest::Method::GET,
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "DELETE" => reqwest::Method::DELETE,
            "PATCH" => reqwest::Method::PATCH,
            other => return ToolResult::err(format!("Unsupported method: {}. Use GET, POST, PUT, DELETE, or PATCH.", other)),
        };

        let mut request = self.client.request(method_parsed.clone(), url);

        // Set default user-agent
        request = request.header("User-Agent", &self.config.user_agent);

        // Apply custom headers
        if let Some(hdrs) = headers {
            for (key, value) in hdrs {
                if let Some(val_str) = value.as_str() {
                    match reqwest::header::HeaderName::from_bytes(key.as_bytes()) {
                        Ok(header_name) => {
                            match reqwest::header::HeaderValue::from_str(val_str) {
                                Ok(header_value) => {
                                    request = request.header(header_name, header_value);
                                }
                                Err(e) => {
                                    return ToolResult::err(format!("Invalid header value for '{}': {}", key, e));
                                }
                            }
                        }
                        Err(e) => {
                            return ToolResult::err(format!("Invalid header name '{}': {}", key, e));
                        }
                    }
                }
            }
        }

        // Set body for POST/PUT/PATCH
        if let Some(body_str) = body {
            request = request.body(body_str.to_string());
        }

        tracing::info!("HTTP {} {}", method, url);

        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => return ToolResult::err(format!("HTTP request failed: {}", e)),
        };

        let status = response.status();
        let status_code = status.as_u16();
        let response_headers: Vec<String> = response.headers().iter()
            .take(15)  // limit header output
            .map(|(k, v)| format!("{}: {}", k, v.to_str().unwrap_or("<binary>")))
            .collect();

        let content_type = response.headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_default();

        let body_text = match response.text().await {
            Ok(t) => t,
            Err(e) => return ToolResult::err(format!("Failed to read response body: {}", e)),
        };

        let truncated_body = if body_text.len() > 5000 {
            format!("{}... [truncated, {} bytes total]", &body_text[..5000], body_text.len())
        } else {
            body_text
        };

        let result = format!(
            "HTTP {} {} => {}\n\nResponse Headers:\n{}\n\n[UNTRUSTED_EXTERNAL_CONTENT]\n{}\n[/UNTRUSTED_EXTERNAL_CONTENT]",
            method,
            url,
            status_code,
            response_headers.join("\n"),
            truncated_body
        );

        if status.is_success() {
            ToolResult::ok(result)
        } else {
            // Return the full response even on error status — the caller needs to see it
            ToolResult {
                success: false,
                output: result,
                error: Some(format!("HTTP {} returned status {}", method, status_code)),
                metadata: None,
            }
        }
    }

    fn html_to_text(&self, html: &str) -> String {
        // Simple HTML to text conversion
        let document = scraper::Html::parse_document(html);
        let text_sel = scraper::Selector::parse("p, h1, h2, h3, h4, h5, h6, li, td, th, div, span, article, section").unwrap();

        let mut text = String::new();

        for element in document.select(&text_sel) {
            let element_text: String = element.text().collect();
            let trimmed = element_text.trim();
            if !trimmed.is_empty() && trimmed.len() > 20 {
                text.push_str(trimmed);
                text.push('\n');
            }
        }

        text
    }
}
