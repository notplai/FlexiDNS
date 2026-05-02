use std::time::Duration;

use anyhow::{Context, bail};
use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize)]
struct IpifyResponse {
    ip: String,
}

pub async fn fetch_public_ip(
    query_api: &str,
    record_type: &str,
    timeout_secs: u64,
) -> anyhow::Result<String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .context("failed to build ip resolver client")?;

    let api = query_api.to_ascii_lowercase();
    match (api.as_str(), record_type) {
        ("ipify", "A") => {
            let res = client
                .get("https://api4.ipify.org?format=json")
                .send()
                .await?
                .error_for_status()?;
            let body: IpifyResponse = res.json().await?;
            Ok(body.ip)
        }
        ("ipify", "AAAA") => {
            let res = client
                .get("https://api6.ipify.org?format=json")
                .send()
                .await?
                .error_for_status()?;
            let body: IpifyResponse = res.json().await?;
            Ok(body.ip)
        }
        ("icanhazip", "A") => {
            let res = client
                .get("https://ipv4.icanhazip.com")
                .send()
                .await?
                .error_for_status()?;
            Ok(res.text().await?.trim().to_string())
        }
        ("icanhazip", "AAAA") => {
            let res = client
                .get("https://ipv6.icanhazip.com")
                .send()
                .await?
                .error_for_status()?;
            Ok(res.text().await?.trim().to_string())
        }
        ("ifconfig", "A") => {
            let res = client
                .get("https://ifconfig.me/ip")
                .send()
                .await?
                .error_for_status()?;
            Ok(res.text().await?.trim().to_string())
        }
        ("ifconfig", "AAAA") => {
            let res = client
                .get("https://ifconfig.me/ip")
                .header("Accept", "text/plain")
                .send()
                .await?
                .error_for_status()?;
            Ok(res.text().await?.trim().to_string())
        }
        _ => bail!("unsupported query_api or record_type: {query_api}/{record_type}"),
    }
}
