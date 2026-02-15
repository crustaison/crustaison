//! Web Tool - Web Search and Weather
//!
//! Provides web search via DuckDuckGo and weather via Open-Meteo.

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

/// Web tool - search and weather
pub struct WebTool {
    client: reqwest::Client,
    config: WebConfig,
}

impl WebTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            config: WebConfig::default(),
        }
    }
    
    pub fn with_config(config: WebConfig) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(config.timeout))
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
        "Search the web with DuckDuckGo or get weather from Open-Meteo. \
         Use search for current information. Use weather with a location."
    }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["search", "weather", "fetch"],
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
                    "description": "URL to fetch content from"
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
            _ => ToolResult::err(format!("Unknown action: {}", action)),
        }
    }
}

impl WebTool {
    async fn search(&self, query: &str) -> ToolResult {
        // DuckDuckGo HTML search (no API key required)
        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
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
        
        let html = match response.text().await {
            Ok(t) => t,
            Err(e) => return ToolResult::err(format!("Failed to read response: {}", e)),
        };
        
        // Parse results from HTML
        let results = self.parse_search_results(&html);
        
        if results.is_empty() {
            ToolResult::ok(format!("No results found for: {}", query))
        } else {
            let formatted: Vec<String> = results.into_iter().take(5).map(|r| {
                format!("- {}\n  {}\n  {}", r.title, r.url, r.snippet)
            }).collect();
            
            ToolResult::ok(format!(
                "Search results for '{}':\n\n{}",
                query,
                formatted.join("\n\n")
            ))
        }
    }
    
    fn parse_search_results(&self, html: &str) -> Vec<SearchResult> {
        let mut results = Vec::new();
        
        // DuckDuckGo HTML structure
        let document = scraper::Html::parse_document(html);
        let selector = scraper::Selector::parse("div.result").unwrap();
        
        for element in document.select(&selector).take(5) {
            let title_sel = scraper::Selector::parse("a.result__a").unwrap();
            let snippet_sel = scraper::Selector::parse("a.result__snippet").unwrap();
            let url_sel = scraper::Selector::parse("a.result__url").unwrap();
            
            let title = element.select(&title_sel)
                .next()
                .map(|e| e.text().collect::<String>())
                .unwrap_or_default();
                
            let snippet = element.select(&snippet_sel)
                .next()
                .map(|e| e.text().collect::<String>())
                .unwrap_or_default();
                
            let url = element.select(&url_sel)
                .next()
                .map(|e| e.value().attr("href").unwrap_or("").to_string())
                .unwrap_or_default();
            
            if !title.is_empty() {
                results.push(SearchResult { title, url, snippet });
            }
        }
        
        results
    }
    
    async fn weather(&self, location: &str) -> ToolResult {
        // Primary: wttr.in (simple, reliable, handles any location string)
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
                    if !text.contains("Unknown location") && !text.contains("Sorry") && !text.is_empty() {
                        return ToolResult::ok(format!("Weather for {}:\n{}", location, text.trim()));
                    }
                }
            }
        }

        // Fallback: Open-Meteo API (geocode then fetch)
        let geocode_url = format!(
            "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1",
            urlencoding::encode(location)
        );

        let geocode_resp: serde_json::Value = match self.client.get(&geocode_url)
            .send()
            .await
        {
            Ok(r) => match r.json().await {
                Ok(j) => j,
                Err(e) => return ToolResult::err(format!("Failed to parse geocoding response: {}", e)),
            },
            Err(e) => return ToolResult::err(format!("Geocoding failed: {}", e)),
        };

        let lat = match geocode_resp.get("results")
            .and_then(|r| r.as_array())
            .and_then(|arr| arr.first())
            .and_then(|r| r.get("latitude"))
            .and_then(|v| v.as_f64()) {
            Some(l) => l,
            None => return ToolResult::err(format!("Location not found: {}", location)),
        };

        let lon = geocode_resp["results"][0]["longitude"].as_f64().unwrap();
        let location_name = geocode_resp["results"][0]["name"]
            .as_str()
            .unwrap_or(location);

        let weather_url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,weather_code,wind_speed_10m&temperature_unit=fahrenheit&wind_speed_unit=mph",
            lat, lon
        );

        let weather_resp: serde_json::Value = match self.client.get(&weather_url)
            .send()
            .await
        {
            Ok(r) => match r.json().await {
                Ok(j) => j,
                Err(e) => return ToolResult::err(format!("Failed to parse weather response: {}", e)),
            },
            Err(e) => return ToolResult::err(format!("Weather request failed: {}", e)),
        };

        let temp = weather_resp["current"]["temperature_2m"].as_f64().unwrap_or(0.0);
        let wind = weather_resp["current"]["wind_speed_10m"].as_f64().unwrap_or(0.0);
        let code = weather_resp["current"]["weather_code"].as_i64().unwrap_or(0);

        let weather_desc = self.weather_code_to_string(code);

        let result = serde_json::json!({
            "location": location_name,
            "temperature": format!("{:.1}\u{00b0}F", temp),
            "wind_speed": format!("{:.1} mph", wind),
            "conditions": weather_desc,
        });

        ToolResult::ok(serde_json::to_string_pretty(&result).unwrap_or_default())
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
        
        if !response.status().is_success() {
            return ToolResult::err(format!("Fetch failed: {}", response.status()));
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
                "URL: {}\nContent-Type: {}\n\n{}\n\n{}",
                url,
                content_type,
                truncated,
                if text.len() > 5000 { "... [truncated]" } else { "" }
            );
            
            ToolResult::ok(result)
        } else {
            // For non-HTML, just report success
            ToolResult::ok(format!(
                "URL: {}\nContent-Type: {}\n\n[Binary content - {} bytes]",
                url,
                content_type,
                response.content_length().unwrap_or(0)
            ))
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
