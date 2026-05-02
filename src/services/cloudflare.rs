use anyhow::{Context, anyhow};
use chrono::Utc;
use reqwest::{Client, header};
use serde::{Deserialize, Serialize};

use crate::models::Record;

#[derive(Clone)]
pub struct CloudflareClient {
    client: Client,
    email: String,
    token: String,
}

impl CloudflareClient {
    pub fn new(email: String, token: String) -> anyhow::Result<Self> {
        let client = Client::builder()
            .build()
            .context("failed to build cloudflare client")?;
        Ok(Self {
            client,
            email,
            token,
        })
    }

    pub async fn upsert_dns_record(&self, record: &Record, ip: &str) -> anyhow::Result<bool> {
        let zone_name = apex_zone(&record.fqdn);
        let zone_id = self.get_zone_id(&zone_name).await?;
        let dns_record = self
            .get_dns_record(&zone_id, &record.fqdn, &record.record_type)
            .await?;

        if dns_record.content == ip {
            return Ok(false);
        }

        let comment = format!("modified since {}", Utc::now().to_rfc3339());
        let payload = UpdateDnsRecordRequest {
            record_type: record.record_type.clone(),
            name: record.fqdn.clone(),
            content: ip.to_string(),
            ttl: record.ttl,
            proxied: record.proxied,
            comment,
        };

        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
            zone_id, dns_record.id
        );

        let response = self
            .client
            .put(url)
            .headers(self.auth_headers()?)
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;

        let body: ApiEnvelope<serde_json::Value> = response.json().await?;
        if !body.success {
            return Err(anyhow!("cloudflare update failed"));
        }

        Ok(true)
    }

    async fn get_zone_id(&self, zone_name: &str) -> anyhow::Result<String> {
        let url = format!("https://api.cloudflare.com/client/v4/zones?name={zone_name}");
        let response = self
            .client
            .get(url)
            .headers(self.auth_headers()?)
            .send()
            .await?
            .error_for_status()?;

        let body: ApiEnvelope<Vec<Zone>> = response.json().await?;
        let zone = body
            .result
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("cloudflare zone not found: {zone_name}"))?;
        Ok(zone.id)
    }

    async fn get_dns_record(
        &self,
        zone_id: &str,
        fqdn: &str,
        record_type: &str,
    ) -> anyhow::Result<DnsRecord> {
        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records?name={fqdn}&type={record_type}"
        );
        let response = self
            .client
            .get(url)
            .headers(self.auth_headers()?)
            .send()
            .await?
            .error_for_status()?;

        let body: ApiEnvelope<Vec<DnsRecord>> = response.json().await?;
        body.result
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("cloudflare record not found: {fqdn} ({record_type})"))
    }

    fn auth_headers(&self) -> anyhow::Result<header::HeaderMap> {
        let mut headers = header::HeaderMap::new();
        headers.insert("X-Auth-Email", self.email.parse()?);
        headers.insert("Authorization", format!("Bearer {}", self.token).parse()?);
        headers.insert(header::CONTENT_TYPE, "application/json".parse()?);
        Ok(headers)
    }
}

fn apex_zone(fqdn: &str) -> String {
    let parts: Vec<&str> = fqdn.split('.').collect();
    if parts.len() >= 2 {
        format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1])
    } else {
        fqdn.to_string()
    }
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    success: bool,
    result: T,
}

#[derive(Debug, Deserialize)]
struct Zone {
    id: String,
}

#[derive(Debug, Deserialize)]
struct DnsRecord {
    id: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct UpdateDnsRecordRequest {
    #[serde(rename = "type")]
    record_type: String,
    name: String,
    content: String,
    ttl: u32,
    proxied: bool,
    comment: String,
}
