use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use serde_json::json;

use crate::{
    config::AppConfig,
    error::AppError,
    models::{CreateRecordRequest, LoginRequest, LoginResponse, UpdateRecordRequest},
    services::sync,
    AppState,
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(metrics))
        .route("/api/status", get(status))
        .route("/api/sync/now", post(sync_now))
        .route("/api/auth/login", post(login))
        .route("/api/admin/records", get(list_records).post(create_record))
        .route(
            "/api/admin/records/{id}",
            put(update_record).delete(delete_record),
        )
        .route("/api/system/reload-config", post(reload_config))
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status": "ok"})))
}

async fn ready(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let has_records = state.db.list_records().map(|v| !v.is_empty()).unwrap_or(false);
    let status = if has_records { "ready" } else { "degraded" };
    (StatusCode::OK, Json(json!({"status": status})))
}

async fn status(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, AppError> {
    let snapshot = sync::snapshot(&state)?;
    Ok((StatusCode::OK, Json(snapshot)))
}

async fn sync_now(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    require_auth(&state, &headers)?;

    match sync::run_sync(state.clone()).await {
        Ok(message) => Ok((StatusCode::OK, Json(json!({"result": message})))),
        Err(err) => {
            state
                .metrics
                .sync_failed_total
                .fetch_add(1, Ordering::Relaxed);
            Err(AppError::Internal(err.to_string()))
        }
    }
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = state.config.read().await.clone();

    if payload.username != cfg.auth.admin_username || payload.password != cfg.auth.admin_password {
        return Err(AppError::Unauthorized);
    }

    let (token, expires_at) = state
        .db
        .create_access_token(&payload.username, cfg.auth.token_ttl_secs)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    state
        .metrics
        .auth_login_total
        .fetch_add(1, Ordering::Relaxed);

    Ok((
        StatusCode::OK,
        Json(LoginResponse {
            access_token: token,
            expires_at,
        }),
    ))
}

async fn list_records(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    require_auth(&state, &headers)?;
    let records = state
        .db
        .list_records()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((StatusCode::OK, Json(records)))
}

async fn create_record(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CreateRecordRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_auth(&state, &headers)?;
    let record_type = normalize_record_type(&payload.record_type)?;

    state
        .db
        .create_record(&payload.fqdn, record_type, payload.proxied, payload.ttl)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    Ok((StatusCode::CREATED, Json(json!({"status": "created"}))))
}

async fn update_record(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateRecordRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_auth(&state, &headers)?;

    let normalized_type = match payload.record_type.as_deref() {
        Some(rt) => Some(normalize_record_type(rt)?.to_string()),
        None => None,
    };

    let updated = state
        .db
        .update_record(
            id,
            payload.fqdn.as_deref(),
            normalized_type.as_deref(),
            payload.proxied,
            payload.ttl,
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if !updated {
        return Err(AppError::NotFound);
    }

    Ok((StatusCode::OK, Json(json!({"status": "updated"}))))
}

async fn delete_record(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    require_auth(&state, &headers)?;

    let deleted = state
        .db
        .delete_record(id)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if !deleted {
        return Err(AppError::NotFound);
    }

    Ok((StatusCode::OK, Json(json!({"status": "deleted"}))))
}

async fn reload_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    require_auth(&state, &headers)?;

    let next = AppConfig::load(&state.config_path).map_err(|e| AppError::BadRequest(e.to_string()))?;
    *state.config.write().await = next;

    Ok((
        StatusCode::OK,
        Json(json!({"status": "reloaded", "config": state.config_path})),
    ))
}

async fn metrics(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, AppError> {
    let body = format!(
        "meshops_sync_total {}\nmeshops_sync_changed_records_total {}\nmeshops_sync_failed_total {}\nmeshops_auth_login_total {}\n",
        state.metrics.sync_total.load(Ordering::Relaxed),
        state.metrics.sync_changed_records.load(Ordering::Relaxed),
        state.metrics.sync_failed_total.load(Ordering::Relaxed),
        state.metrics.auth_login_total.load(Ordering::Relaxed)
    );

    Ok((StatusCode::OK, body))
}

fn require_auth(state: &Arc<AppState>, headers: &HeaderMap) -> Result<(), AppError> {
    let auth_header = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AppError::Unauthorized)?;

    let valid = state
        .db
        .validate_access_token(token)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if !valid {
        return Err(AppError::Unauthorized);
    }

    Ok(())
}

fn normalize_record_type(input: &str) -> Result<&'static str, AppError> {
    let upper = input.to_ascii_uppercase();
    match upper.as_str() {
        "A" => Ok("A"),
        "AAAA" => Ok("AAAA"),
        _ => Err(AppError::BadRequest("record_type must be A or AAAA".to_string())),
    }
}
