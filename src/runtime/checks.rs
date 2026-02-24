//! System Health Checks
//!
//! Built-in check functions for the heartbeat system.

use serde::{Deserialize, Serialize};
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::process::Command;

/// Check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    pub value: Option<f64>,
}

/// Check status levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CheckStatus {
    Ok,
    Warning,
    Critical,
}

/// Trait for health checks
#[async_trait]
pub trait Check: Send + Sync {
    fn name(&self) -> &str;
    async fn run(&self) -> CheckResult;
}

/// Parse percentage from df output
fn parse_percentage(output: &str, target: &str) -> Option<f64> {
    for line in output.lines() {
        if line.contains(target) {
            if let Some(pct) = line.split_whitespace()
                .nth(4)
                .and_then(|s| s.strip_suffix('%'))
            {
                return pct.parse().ok();
            }
        }
    }
    None
}

/// Disk space check
pub struct DiskSpaceCheck {
    mount_point: String,
    warn_threshold: u8,
}

impl DiskSpaceCheck {
    pub fn new(mount_point: &str, warn_threshold: u8) -> Self {
        Self {
            mount_point: mount_point.to_string(),
            warn_threshold,
        }
    }
}

#[async_trait]
impl Check for DiskSpaceCheck {
    fn name(&self) -> &str {
        "disk_space"
    }
    
    async fn run(&self) -> CheckResult {
        let output = match Command::new("df")
            .arg("-h")
            .arg(&self.mount_point)
            .output()
            .await
        {
            Ok(o) => String::from_utf8(o.stdout).unwrap_or_default(),
            Err(_) => return CheckResult {
                name: self.name().to_string(),
                status: CheckStatus::Critical,
                message: "Could not read disk space".to_string(),
                value: None,
            },
        };
        
        if let Some(pct) = parse_percentage(&output, &self.mount_point) {
            let status = if pct >= self.warn_threshold as f64 {
                CheckStatus::Critical
            } else if pct >= (self.warn_threshold - 10) as f64 {
                CheckStatus::Warning
            } else {
                CheckStatus::Ok
            };
            
            CheckResult {
                name: self.name().to_string(),
                status,
                message: format!("{}% used on {}", pct, self.mount_point),
                value: Some(pct),
            }
        } else {
            CheckResult {
                name: self.name().to_string(),
                status: CheckStatus::Critical,
                message: "Could not parse disk space output".to_string(),
                value: None,
            }
        }
    }
}

/// Memory check
pub struct MemoryCheck {
    warn_threshold: u8,
}

impl MemoryCheck {
    pub fn new(warn_threshold: u8) -> Self {
        Self { warn_threshold }
    }
}

#[async_trait]
impl Check for MemoryCheck {
    fn name(&self) -> &str {
        "memory"
    }
    
    async fn run(&self) -> CheckResult {
        let output = match Command::new("cat")
            .arg("/proc/meminfo")
            .output()
            .await
        {
            Ok(o) => String::from_utf8(o.stdout).unwrap_or_default(),
            Err(_) => return CheckResult {
                name: self.name().to_string(),
                status: CheckStatus::Critical,
                message: "Could not read memory info".to_string(),
                value: None,
            },
        };
        
        let total_mb: u64 = output.lines()
            .find(|l| l.starts_with("MemTotal:"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        
        let available_mb: u64 = output.lines()
            .find(|l| l.starts_with("MemAvailable:"))
            .or(output.lines().find(|l| l.starts_with("MemFree:")))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        
        if total_mb > 0 {
            let used_pct = 100.0 - (available_mb as f64 / total_mb as f64 * 100.0);
            let used_gb = (total_mb - available_mb) as f64 / 1024.0;
            
            let status = if used_pct >= self.warn_threshold as f64 {
                CheckStatus::Critical
            } else if used_pct >= (self.warn_threshold - 10) as f64 {
                CheckStatus::Warning
            } else {
                CheckStatus::Ok
            };
            
            CheckResult {
                name: self.name().to_string(),
                status,
                message: format!("{:.1}GB used ({:.1}%)", used_gb, used_pct),
                value: Some(used_pct),
            }
        } else {
            CheckResult {
                name: self.name().to_string(),
                status: CheckStatus::Critical,
                message: "Could not parse memory info".to_string(),
                value: None,
            }
        }
    }
}

/// Service check
pub struct ServiceCheck {
    services: Vec<(String, String, u16)>,
}

impl ServiceCheck {
    pub fn new(services: Vec<(String, String, u16)>) -> Self {
        Self { services }
    }
}

#[async_trait]
impl Check for ServiceCheck {
    fn name(&self) -> &str {
        "services"
    }
    
    async fn run(&self) -> CheckResult {
        let mut ok_count = 0;
        let mut issues = Vec::new();
        
        for (name, host, port) in &self.services {
            let result = tokio::net::TcpStream::connect(format!("{}:{}", host, port)).await;
            
            match result {
                Ok(_) => ok_count += 1,
                Err(_) => issues.push(format!("{}:{}", name, port)),
            }
        }
        
        let status = if !issues.is_empty() {
            if issues.len() == self.services.len() {
                CheckStatus::Critical
            } else {
                CheckStatus::Warning
            }
        } else {
            CheckStatus::Ok
        };
        
        CheckResult {
            name: self.name().to_string(),
            status,
            message: if issues.is_empty() {
                format!("{}/{} services OK", ok_count, self.services.len())
            } else {
                format!("Issues: {}", issues.join(", "))
            },
            value: Some(ok_count as f64),
        }
    }
}

/// System load check
pub struct LoadCheck {
    warn_threshold: f64,
}

impl LoadCheck {
    pub fn new(warn_threshold: f64) -> Self {
        Self { warn_threshold }
    }
}

#[async_trait]
impl Check for LoadCheck {
    fn name(&self) -> &str {
        "load"
    }
    
    async fn run(&self) -> CheckResult {
        let output = match Command::new("cat")
            .arg("/proc/loadavg")
            .output()
            .await
        {
            Ok(o) => String::from_utf8(o.stdout).unwrap_or_default(),
            Err(_) => return CheckResult {
                name: self.name().to_string(),
                status: CheckStatus::Critical,
                message: "Could not read load average".to_string(),
                value: None,
            },
        };
        
        let load: f64 = output.split_whitespace()
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        
        let status = if load >= self.warn_threshold {
            CheckStatus::Critical
        } else if load >= self.warn_threshold * 0.75 {
            CheckStatus::Warning
        } else {
            CheckStatus::Ok
        };
        
        CheckResult {
            name: self.name().to_string(),
            status,
            message: format!("Load average: {:.2}", load),
            value: Some(load),
        }
    }
}

/// Uptime check
pub struct UptimeCheck;

impl UptimeCheck {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Check for UptimeCheck {
    fn name(&self) -> &str {
        "uptime"
    }
    
    async fn run(&self) -> CheckResult {
        let output = match Command::new("cat")
            .arg("/proc/uptime")
            .output()
            .await
        {
            Ok(o) => String::from_utf8(o.stdout).unwrap_or_default(),
            Err(_) => return CheckResult {
                name: self.name().to_string(),
                status: CheckStatus::Critical,
                message: "Could not read uptime".to_string(),
                value: None,
            },
        };
        
        let uptime_secs: f64 = output.split_whitespace()
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        
        let days = uptime_secs / 86400.0;
        
        CheckResult {
            name: self.name().to_string(),
            status: CheckStatus::Ok,
            message: format!("Uptime: {:.1} days", days),
            value: Some(days),
        }
    }
}

/// Docker containers check
pub struct DockerCheck {
    expected_containers: Vec<String>,
}

impl DockerCheck {
    pub fn new(expected_containers: Vec<String>) -> Self {
        Self { expected_containers }
    }
}

#[async_trait]
impl Check for DockerCheck {
    fn name(&self) -> &str {
        "docker"
    }
    
    async fn run(&self) -> CheckResult {
        let output = match Command::new("docker")
            .arg("ps")
            .arg("--format")
            .arg("{{.Names}}")
            .output()
            .await
        {
            Ok(o) => String::from_utf8(o.stdout).unwrap_or_default(),
            Err(_) => return CheckResult {
                name: self.name().to_string(),
                status: CheckStatus::Critical,
                message: "Docker not available".to_string(),
                value: None,
            },
        };
        
        let running: Vec<String> = output.lines()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .collect();
        
        let mut missing = Vec::new();
        for expected in &self.expected_containers {
            if !running.contains(expected) {
                missing.push(expected);
            }
        }
        
        let status = if !missing.is_empty() {
            CheckStatus::Critical
        } else {
            CheckStatus::Ok
        };
        
        CheckResult {
            name: self.name().to_string(),
            status,
            message: if missing.is_empty() {
                format!("{}/{} containers running", running.len(), self.expected_containers.len())
            } else {
                format!("Missing: {}", missing.iter().map(|s| s.as_str()).collect::<Vec<&str>>().join(", "))
            },
            value: Some(running.len() as f64),
        }
    }
}

/// Build default set of checks
pub fn default_checks() -> Vec<Box<dyn Check>> {
    vec![
        Box::new(DiskSpaceCheck::new("/", 85)),
        Box::new(MemoryCheck::new(90)),
        Box::new(ServiceCheck::new(vec![
            ("Nexa".to_string(), "localhost".to_string(), 18181),
        ])),
        Box::new(LoadCheck::new(4.0)),
        Box::new(UptimeCheck::new()),
    ]
}
