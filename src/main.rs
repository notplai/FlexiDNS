mod api;
mod config;
mod database;
mod error;
mod models;
mod services;

use std::{
	net::SocketAddr,
	sync::{
		atomic::{AtomicU64, Ordering},
		Arc,
	},
	time::Duration,
};

use anyhow::Context;
use axum::Router;
use chrono::{Duration as ChronoDuration, NaiveTime, Utc};
use clap::Parser;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use tracing::{error, info};

use crate::{
	config::{AppConfig, SchedulerMode},
	database::Db,
};

#[derive(Parser, Debug)]
#[command(author, version, about = "MeshOps v0.241.1+03.05.2026")]
struct Cli {
	#[arg(long, default_value = "config.yml")]
	config: String,
}

pub struct Metrics {
	pub sync_total: AtomicU64,
	pub sync_changed_records: AtomicU64,
	pub sync_failed_total: AtomicU64,
	pub auth_login_total: AtomicU64,
}

impl Metrics {
	fn new() -> Self {
		Self {
			sync_total: AtomicU64::new(0),
			sync_changed_records: AtomicU64::new(0),
			sync_failed_total: AtomicU64::new(0),
			auth_login_total: AtomicU64::new(0),
		}
	}
}

pub struct AppState {
	pub config_path: String,
	pub config: RwLock<AppConfig>,
	pub db: Db,
	pub metrics: Metrics,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	dotenvy::dotenv().ok();
	init_tracing();

	let cli = Cli::parse();
	let cfg = AppConfig::load(&cli.config)?;

	let db = Db::connect(&cfg.storage.sqlite_path, &cfg.ddns.providers)
		.context("failed to initialize sqlite backend")?;

	let state = Arc::new(AppState {
		config_path: cli.config.clone(),
		config: RwLock::new(cfg.clone()),
		db,
		metrics: Metrics::new(),
	});

	tokio::spawn(run_scheduler(state.clone()));

	let app: Router = api::router()
		.with_state(state)
		.layer(TraceLayer::new_for_http());

	let addr: SocketAddr = format!("{}:{}", cfg.server.host, cfg.server.port)
		.parse()
		.context("invalid listen address")?;

	info!("MeshOps API listening on {}", addr);
	let listener = tokio::net::TcpListener::bind(addr).await?;
	axum::serve(listener, app).await?;

	Ok(())
}

fn init_tracing() {
	let env = tracing_subscriber::EnvFilter::try_from_default_env()
		.unwrap_or_else(|_| "meshops=info,tower_http=info".into());

	tracing_subscriber::fmt().with_env_filter(env).init();
}

async fn run_scheduler(state: Arc<AppState>) {
	loop {
		let cfg = state.config.read().await.clone();
		if !cfg.ddns.enabled {
			tokio::time::sleep(Duration::from_secs(30)).await;
			continue;
		}

		match cfg.scheduler.mode {
			SchedulerMode::Interval => {
				if let Err(err) = services::sync::run_sync(state.clone()).await {
					state
						.metrics
						.sync_failed_total
						.fetch_add(1, Ordering::Relaxed);
					error!("scheduled sync failed: {err}");
				}
				tokio::time::sleep(Duration::from_secs(cfg.scheduler.interval_secs.max(10))).await;
			}
			SchedulerMode::UnixEpoch => {
				let duration = seconds_until(cfg.scheduler.unix_time_utc.as_str());
				tokio::time::sleep(duration).await;
				if let Err(err) = services::sync::run_sync(state.clone()).await {
					state
						.metrics
						.sync_failed_total
						.fetch_add(1, Ordering::Relaxed);
					error!("scheduled sync failed: {err}");
				}
			}
		}
	}
}

fn seconds_until(target_hms: &str) -> Duration {
	let target = NaiveTime::parse_from_str(target_hms, "%H:%M:%S")
		.unwrap_or_else(|_| NaiveTime::from_hms_opt(3, 0, 0).expect("valid fallback time"));
	let now = Utc::now();
	let today = now.date_naive();
	let target_dt = today.and_time(target).and_utc();

	let wait = if target_dt > now {
		target_dt - now
	} else {
		target_dt + ChronoDuration::days(1) - now
	};

	wait.to_std().unwrap_or_else(|_| Duration::from_secs(60))
}