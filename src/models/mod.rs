use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub id: i64,
    pub fqdn: String,
    pub record_type: String,
    pub proxied: bool,
    pub ttl: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSnapshot {
    pub last_public_ip: Option<String>,
    pub last_sync_at: Option<String>,
    pub last_sync_status: Option<String>,
    pub last_sync_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRecordRequest {
    pub fqdn: String,
    pub record_type: String,
    pub proxied: bool,
    pub ttl: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRecordRequest {
    pub fqdn: Option<String>,
    pub record_type: Option<String>,
    pub proxied: Option<bool>,
    pub ttl: Option<u32>,
}
