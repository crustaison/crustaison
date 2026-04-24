//! Lake Tool - Lake of the Ozarks current conditions
//!
//! Fetches level, surface water temp, and weather from conditions.json
//! and formats a human-readable report for Telegram.

use crate::tools::{Tool, ToolResult};

pub struct LakeTool;

#[async_trait::async_trait]
impl Tool for LakeTool {
    fn name(&self) -> &str {
        "lake"
    }

    fn description(&self) -> &str {
        "Get current Lake of the Ozarks conditions: water level, surface water temperature, air temperature, wind speed and direction, and 3-day forecast. Use for ANY question about the lake conditions, lake level, water temp, or lake weather."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn call(&self, _args: serde_json::Value) -> ToolResult {
        let url = "https://crustaison.github.io/lake-ozarks-conditions/conditions.json";

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
        {
            Ok(c) => c,
            Err(e) => return ToolResult::err(format!("HTTP client error: {}", e)),
        };

        let resp = match client.get(url).send().await {
            Ok(r) => r,
            Err(e) => return ToolResult::err(format!("Fetch failed: {}", e)),
        };

        let text = match resp.text().await {
            Ok(t) => t,
            Err(e) => return ToolResult::err(format!("Read failed: {}", e)),
        };

        let data: serde_json::Value = match serde_json::from_str(&text) {
            Ok(d) => d,
            Err(e) => return ToolResult::err(format!("Parse failed: {}", e)),
        };

        let updated = data["updated"].as_str().unwrap_or("unknown");
        let level = data["lake_level"].as_f64().unwrap_or(0.0);
        let below = data["below_full_pool"].as_f64().unwrap_or(0.0);
        let water_temp = data["water_temp_f"].as_i64();
        let air_temp = data["air_temp_f"].as_i64();
        let feels_like = data["feels_like_f"].as_i64();
        let humidity = data["humidity"].as_i64();
        let wind_speed = data["wind_speed_mph"].as_i64();
        let wind_gusts = data["wind_gusts_mph"].as_i64();
        let wind_dir = data["wind_dir"].as_str().unwrap_or("—");

        let level_status = if below <= 1.0 {
            "at full pool"
        } else if below <= 3.0 {
            "normal"
        } else if below <= 6.0 {
            "low"
        } else {
            "very low"
        };

        let wind_advisory = if wind_speed.unwrap_or(0) >= 20 {
            " ⚠️ Rough conditions"
        } else if wind_speed.unwrap_or(0) >= 10 {
            " — moderate chop"
        } else {
            " — calm"
        };

        let water_temp_note = match water_temp {
            Some(t) if t < 50 => " (cold — hypothermia risk)",
            Some(t) if t < 60 => " (cool — limit exposure)",
            Some(t) if t < 70 => " (comfortable)",
            Some(t) if t < 80 => " (warm — ideal)",
            Some(_) => " (very warm)",
            None => "",
        };

        let mut lines = vec![
            format!("⛵ Lake of the Ozarks Conditions"),
            format!("Updated: {}", &updated[..10]),
            String::new(),
            format!("💧 Lake Level: {} ft MSL ({} — {:.1} ft below full pool)",
                level, level_status, below),
        ];

        if let Some(wt) = water_temp {
            lines.push(format!("🌡️ Surface Water Temp: {}°F{}", wt, water_temp_note));
        }
        let osage_temp = data["osage_temp_f"].as_f64();
        if let Some(ot) = osage_temp {
            lines.push(format!("🌊 Osage Below Dam: {}°F (deeper lake temp)", ot));
        }

        if let Some(at) = air_temp {
            let fl = feels_like.map(|f| format!(", feels like {}°F", f)).unwrap_or_default();
            let hum = humidity.map(|h| format!(", {}% humidity", h)).unwrap_or_default();
            lines.push(format!("☀️ Air Temp: {}°F{}{}", at, fl, hum));
        }

        if let Some(ws) = wind_speed {
            let gusts = wind_gusts.map(|g| format!(", gusts {} mph", g)).unwrap_or_default();
            lines.push(format!("💨 Wind: {} mph {}{}{}", ws, wind_dir, gusts, wind_advisory));
        }

        // Forecast
        if let Some(forecast) = data["forecast"].as_array() {
            if !forecast.is_empty() {
                lines.push(String::new());
                lines.push("📅 3-Day Forecast:".to_string());
                for day in forecast {
                    let date = day["date"].as_str().unwrap_or("?");
                    let high = day["high_f"].as_i64().unwrap_or(0);
                    let low = day["low_f"].as_i64().unwrap_or(0);
                    let wind_max = day["wind_max_mph"].as_i64().unwrap_or(0);
                    lines.push(format!("  {} — High {}° / Low {}°, wind {} mph",
                        date, high, low, wind_max));
                }
            }
        }

        ToolResult::ok(lines.join("\n"))
    }
}
