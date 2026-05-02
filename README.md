# MeshOps

Rust-native dynamic DNS and network management API. Resolves your public IP and keeps DNS records in sync across providers on a configurable schedule.

> ✎ Original Code written on Python language, Checkout at [UDIP](https://github.com/notplai/MeshOps/tree/UDIP/) branches tree

> ✦ This version ships [systemd service files](/services/readme.md).

## Features

- Dynamic DNS updates via Cloudflare (more providers planned)
- Interval-based or daily unix-epoch scheduler
- REST API with token auth, live sync, and record management
- Hot config reload without restart
- SQLite-backed record state with per-provider cache
- Prometheus-compatible metrics endpoint
- Rate limiting and retry with exponential backoff
- IPv4 and IPv6 record support (A / AAAA)

## Requirements

- Rust toolchain (edition 2024)
- Internet access for IP resolution and DNS updates

## Installation

```bash
git clone https://github.com/dotplai/MeshOps.git
cd MeshOps
cargo build --release
```

## Configuration

Copy the template and edit as needed:

```bash
cp templates/config.yml config.yml
```

Environment variables are expanded in `config.yml`. Set them in a `.env` file at the project root:

```env
CLOUDFLARE_EMAIL=you@example.com
CLOUDFLARE_TOKEN=your_api_token

MESHOPS_ADMIN_USERNAME=admin
MESHOPS_ADMIN_PASSWORD=change-me
```

Minimal `config.yml`:

```yaml
auth:
  admin_username: ${MESHOPS_ADMIN_USERNAME:-admin}
  admin_password: ${MESHOPS_ADMIN_PASSWORD:-change-me}

ddns:
  enabled: true
  providers:
    - kind: cloudflare
      email: ${CLOUDFLARE_EMAIL}
      token: ${CLOUDFLARE_TOKEN}
      records:
        - fqdn: example.com
          record_type: A
          proxied: false
          ttl: 120

scheduler:
  mode: interval
  interval_secs: 300
```

<!-- See the full reference in [`templates/config.yml`](./templates/config.yml). -->

## Usage

```bash
# Run in development
./exec.sh run

# Run with a custom config
./exec.sh run --config /path/to/config.yml

# Install as a systemd service
./exec.sh install-service --replace-udip
# "--replace-udip" flags for uninstall old UDIP service and replace with MeshOps service.
```

## API

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/health` | - | Liveness probe |
| GET | `/ready` | - | Readiness probe |
| GET | `/metrics` | - | Prometheus metrics |
| GET | `/api/status` | - | Sync status snapshot |
| POST | `/api/auth/login` | - | Obtain bearer token |
| POST | `/api/sync/now` | Bearer | Trigger immediate sync |
| GET | `/api/admin/records` | Bearer | List DNS records |
| POST | `/api/admin/records` | Bearer | Create DNS record |
| PUT | `/api/admin/records/{id}` | Bearer | Update DNS record |
| DELETE | `/api/admin/records/{id}` | Bearer | Delete DNS record |
| POST | `/api/system/reload-config` | Bearer | Reload config from disk |

## Contributing

1. Fork the repository.
2. Create a branch: `git checkout -b feature-name`
3. Commit your changes: `git commit -m "Add feature-name"`
4. Push and open a pull request.

## License

Released into the public domain under the Unlicense. See [`LICENSE`](./LICENSE) for details.
