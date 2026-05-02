use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub scheduler: SchedulerConfig,
    #[serde(default)]
    pub ddns: DdnsConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub retry: RetryConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub webhooks: WebhookConfig,
}

impl AppConfig {
    pub fn load(config_path: &str) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(config_path)
            .with_context(|| format!("unable to read config file: {config_path}"))?;
        let expanded = shellexpand::env(&raw)
            .with_context(|| format!("failed to expand env variables in: {config_path}"))?
            .into_owned();

        let cfg: AppConfig = serde_yaml::from_str(&expanded)
            .with_context(|| format!("invalid yaml in: {config_path}"))?;

        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> anyhow::Result<()> {
        if self.server.port == 0 {
            bail!("server.port must be a valid TCP port");
        }

        if self.auth.token_ttl_secs < 60 {
            bail!("auth.token_ttl_secs must be at least 60 seconds");
        }

        if self.ddns.enabled {
            if self.ddns.providers.is_empty() {
                bail!("ddns.providers cannot be empty while ddns.enabled=true");
            }

            for provider in &self.ddns.providers {
                if provider.records.is_empty() {
                    bail!("ddns provider records cannot be empty");
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(default = "default_admin_username")]
    pub admin_username: String,
    #[serde(default = "default_admin_password")]
    pub admin_password: String,
    #[serde(default = "default_token_ttl")]
    pub token_ttl_secs: i64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            admin_username: default_admin_username(),
            admin_password: default_admin_password(),
            token_ttl_secs: default_token_ttl(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_query_api")]
    pub query_api: String,
    #[serde(default = "default_fetch_timeout")]
    pub fetch_timeout_secs: u64,
    #[serde(default = "default_state_path")]
    pub state_path: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            query_api: default_query_api(),
            fetch_timeout_secs: default_fetch_timeout(),
            state_path: default_state_path(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    #[serde(default = "default_scheduler_mode")]
    pub mode: SchedulerMode,
    #[serde(default = "default_interval_secs")]
    pub interval_secs: u64,
    #[serde(default = "default_unix_time")]
    pub unix_time_utc: String,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            mode: default_scheduler_mode(),
            interval_secs: default_interval_secs(),
            unix_time_utc: default_unix_time(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerMode {
    Interval,
    UnixEpoch,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DdnsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub providers: Vec<DdnsProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdnsProvider {
    pub kind: String,
    pub email: String,
    pub token: String,
    #[serde(default = "default_cache_timeout")]
    pub cache_timeout_secs: u64,
    #[serde(default)]
    pub records: Vec<DdnsRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdnsRecord {
    pub fqdn: String,
    pub record_type: DnsRecordType,
    #[serde(default)]
    pub proxied: bool,
    #[serde(default = "default_record_ttl")]
    pub ttl: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DnsRecordType {
    A,
    Aaaa,
}

impl std::fmt::Display for DnsRecordType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::A => write!(f, "A"),
            Self::Aaaa => write!(f, "AAAA"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_sqlite_path")]
    pub sqlite_path: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            sqlite_path: default_sqlite_path(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_metrics_path")]
    pub path: String,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            path: default_metrics_path(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    #[serde(default = "default_retry_attempts")]
    pub max_attempts: u32,
    #[serde(default = "default_retry_base")]
    pub base_delay_ms: u64,
    #[serde(default = "default_retry_max")]
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_retry_attempts(),
            base_delay_ms: default_retry_base(),
            max_delay_ms: default_retry_max(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_rate_limit")]
    pub requests_per_minute: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: default_rate_limit(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub endpoints: Vec<String>,
    #[serde(default)]
    pub shared_secret: Option<String>,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoints: Vec::new(),
            shared_secret: None,
        }
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_admin_username() -> String {
    "admin".to_string()
}

fn default_admin_password() -> String {
    "change-me".to_string()
}

fn default_token_ttl() -> i64 {
    86_400
}

fn default_query_api() -> String {
    "ipify".to_string()
}

fn default_fetch_timeout() -> u64 {
    8
}

fn default_state_path() -> String {
    ".dumps/state.json".to_string()
}

fn default_scheduler_mode() -> SchedulerMode {
    SchedulerMode::Interval
}

fn default_interval_secs() -> u64 {
    300
}

fn default_unix_time() -> String {
    "03:00:00".to_string()
}

fn default_true() -> bool {
    true
}

fn default_cache_timeout() -> u64 {
    172_800
}

fn default_record_ttl() -> u32 {
    120
}

fn default_sqlite_path() -> String {
    ".dumps/meshops.db".to_string()
}

fn default_metrics_path() -> String {
    "/metrics".to_string()
}

fn default_retry_attempts() -> u32 {
    3
}

fn default_retry_base() -> u64 {
    300
}

fn default_retry_max() -> u64 {
    5_000
}

fn default_rate_limit() -> u32 {
    120
}
