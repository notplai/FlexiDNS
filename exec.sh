#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVICES_DIR="$PROJECT_DIR/services"
TEMPLATES_DIR="$PROJECT_DIR/templates"
DEFAULT_CONFIG="$PROJECT_DIR/config.yml"
DEFAULT_TEMPLATE_CONFIG="$TEMPLATES_DIR/config.yml"

SERVICE_NAME="MeshOps.service"
SYSTEMD_DIR="/etc/systemd/system"

print_help() {
		cat <<'EOF'
MeshOps v0.241.1+03.05.2026 helper

Usage:
	./exec.sh run [--config <path>]
	./exec.sh scaffold
	./exec.sh install-service [--user <user>] [--replace-udip]

Commands:
	run               Run MeshOps with cargo using selected config file.
	scaffold          Generate services and templates resources.
	install-service   Install MeshOps.service into systemd and optionally replace udip.service.

Examples:
	./exec.sh scaffold
	./exec.sh run --config config.yml
	./exec.sh install-service --replace-udip
EOF
}

generate_template_config() {
		mkdir -p "$TEMPLATES_DIR"
		cat >"$DEFAULT_TEMPLATE_CONFIG" <<'EOF'
server:
	host: 0.0.0.0
	port: 8080

auth:
	admin_username: ${MESHOPS_ADMIN_USERNAME:-admin}
	admin_password: ${MESHOPS_ADMIN_PASSWORD:-change-me}
	token_ttl_secs: 86400

general:
	query_api: ipify
	fetch_timeout_secs: 8
	state_path: .dumps/state.json

scheduler:
	mode: interval
	interval_secs: 300
	unix_time_utc: "03:00:00"

ddns:
	enabled: true
	providers:
		- kind: cloudflare
			email: ${CLOUDFLARE_EMAIL}
			token: ${CLOUDFLARE_TOKEN}
			cache_timeout_secs: 172800
			records:
				- fqdn: example.com
					record_type: A
					proxied: false
					ttl: 120

storage:
	sqlite_path: .dumps/meshops.db

metrics:
	enabled: true
	path: /metrics

retry:
	max_attempts: 3
	base_delay_ms: 300
	max_delay_ms: 5000

rate_limit:
	requests_per_minute: 120

webhooks:
	enabled: false
	endpoints: []
	shared_secret: null
EOF
}

generate_runtime_script() {
		mkdir -p "$SERVICES_DIR"
		cat >"$SERVICES_DIR/rxt.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_DIR"

if [[ ! -f "$PROJECT_DIR/config.yml" && -f "$PROJECT_DIR/templates/config.yml" ]]; then
		cp "$PROJECT_DIR/templates/config.yml" "$PROJECT_DIR/config.yml"
fi

if [[ -f "$PROJECT_DIR/.env" ]]; then
		set -a
		# shellcheck disable=SC1091
		source "$PROJECT_DIR/.env"
		set +a
fi

exec "$PROJECT_DIR/target/release/MeshOps" --config "$PROJECT_DIR/config.yml"
EOF
		chmod +x "$SERVICES_DIR/rxt.sh"
}

generate_service_unit() {
		local service_user
		service_user="${1:-$USER}"

		mkdir -p "$SERVICES_DIR"
		cat >"$SERVICES_DIR/$SERVICE_NAME" <<EOF
[Unit]
Description=MeshOps Dynamic DNS API Service
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=$PROJECT_DIR
ExecStart=$PROJECT_DIR/services/rxt.sh
Restart=on-failure
RestartSec=5
User=$service_user
Environment=RUST_LOG=meshops=info,tower_http=info

[Install]
WantedBy=multi-user.target
EOF
}

generate_services_readme() {
		mkdir -p "$SERVICES_DIR"
		cat >"$SERVICES_DIR/readme.md" <<'EOF'
# MeshOps Service

This directory contains generated service assets:
- MeshOps.service: systemd unit file
- rxt.sh: runtime launcher used by systemd

Install flow:
1. Build release binary: cargo build --release
2. Install service: ./exec.sh install-service --replace-udip
3. Check status: sudo systemctl status MeshOps.service
EOF
}

scaffold() {
		echo "Generating templates and service resources..."
		generate_template_config
		generate_runtime_script
		generate_service_unit "$USER"
		generate_services_readme

		if [[ ! -f "$DEFAULT_CONFIG" ]]; then
				cp "$DEFAULT_TEMPLATE_CONFIG" "$DEFAULT_CONFIG"
				echo "Copied default config to $DEFAULT_CONFIG"
		fi

		echo "Scaffold complete."
}

run_meshops() {
		local config_path="$DEFAULT_CONFIG"

		while [[ $# -gt 0 ]]; do
				case "$1" in
						--config)
								config_path="$2"
								shift 2
								;;
						*)
								echo "Unknown run flag: $1"
								exit 1
								;;
				esac
		done

		echo "MeshOps v0.241.1+03.05.2026 | Rust-native Network API"

		if [[ -f "$PROJECT_DIR/.env" ]]; then
				set -a
				# shellcheck disable=SC1091
				source "$PROJECT_DIR/.env"
				set +a
		fi

		cargo run -- --config "$config_path"
}

install_service() {
		local service_user="$USER"
		local replace_udip="false"

		while [[ $# -gt 0 ]]; do
				case "$1" in
						--user)
								service_user="$2"
								shift 2
								;;
						--replace-udip)
								replace_udip="true"
								shift
								;;
						*)
								echo "Unknown install flag: $1"
								exit 1
								;;
				esac
		done

		scaffold
		generate_service_unit "$service_user"

		echo "Building release binary..."
		cargo build --release
        
		echo "Installing $SERVICE_NAME to $SYSTEMD_DIR"
		sudo cp "$SERVICES_DIR/$SERVICE_NAME" "$SYSTEMD_DIR/$SERVICE_NAME"

		if [[ "$replace_udip" == "true" ]]; then
				if systemctl list-unit-files | grep -q '^udip.service'; then
						echo "Stopping and disabling udip.service..."
						sudo systemctl disable --now udip.service || true
				fi
		fi

		sudo systemctl daemon-reload
		sudo systemctl enable --now "$SERVICE_NAME"
		sudo systemctl status "$SERVICE_NAME" --no-pager || true
}

main() {
		local command="${1:-run}"
		if [[ $# -gt 0 ]]; then
				shift
		fi

		case "$command" in
				run)
						run_meshops "$@"
						;;
				scaffold)
						scaffold
						;;
				install-service)
						install_service "$@"
						;;
				-h|--help|help)
						print_help
						;;
				*)
						echo "Unknown command: $command"
						print_help
						exit 1
						;;
		esac
}

main "$@"