use std::sync::Arc;

use anyhow::Context;

use crate::{AppState, models::SyncSnapshot};

pub async fn run_sync(state: Arc<AppState>) -> anyhow::Result<String> {
    let config = {
        let guard = state.config.read().await;
        guard.clone()
    };

    if !config.ddns.enabled {
        state
            .db
            .add_sync_history("skipped", "ddns disabled", None)
            .context("failed to add sync history")?;
        return Ok("ddns disabled".to_string());
    }

    let records = state.db.list_records().context("failed to list records")?;
    if records.is_empty() {
        state
            .db
            .add_sync_history("skipped", "no records configured", None)
            .context("failed to add sync history")?;
        return Ok("no records configured".to_string());
    }

    let provider = config
        .ddns
        .providers
        .iter()
        .find(|p| p.kind.eq_ignore_ascii_case("cloudflare"))
        .cloned()
        .context("cloudflare provider not configured")?;

    let cf_client =
        crate::services::cloudflare::CloudflareClient::new(provider.email, provider.token)?;

    let mut changed = 0_u64;
    let mut skipped_local_unchanged = 0_u64;
    let mut ip_for_state = None;

    for record in records {
        let ip = crate::services::ip::fetch_public_ip(
            &config.general.query_api,
            &record.record_type,
            config.general.fetch_timeout_secs,
        )
        .await
        .with_context(|| format!("failed to resolve public ip for {}", record.fqdn))?;

        let state_key = format!("last_public_ip:{}", record.record_type);
        if let Some(previous_ip) = state.db.get_state(&state_key)? {
            if previous_ip == ip {
                skipped_local_unchanged += 1;
                continue;
            }
        }

        ip_for_state = Some(ip.clone());
        if cf_client.upsert_dns_record(&record, &ip).await? {
            changed += 1;
        }

        state.db.set_state(&state_key, &ip)?;
    }

    if let Some(ip) = ip_for_state.as_deref() {
        state.db.set_state("last_public_ip", ip)?;
    }

    let message = format!(
        "sync completed, changed records: {changed}, skipped local unchanged: {skipped_local_unchanged}"
    );
    state
        .db
        .add_sync_history("ok", &message, ip_for_state.as_deref())
        .context("failed to add sync history")?;

    state
        .metrics
        .sync_total
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    state
        .metrics
        .sync_changed_records
        .fetch_add(changed, std::sync::atomic::Ordering::Relaxed);

    Ok(message)
}

pub fn snapshot(state: &Arc<AppState>) -> anyhow::Result<SyncSnapshot> {
    let last_public_ip = state.db.get_state("last_public_ip")?;
    let latest = state.db.latest_sync_history()?;

    let (last_sync_at, last_sync_status, last_sync_message) =
        if let Some((at, status, message)) = latest {
            (Some(at), Some(status), Some(message))
        } else {
            (None, None, None)
        };

    Ok(SyncSnapshot {
        last_public_ip,
        last_sync_at,
        last_sync_status,
        last_sync_message,
    })
}
