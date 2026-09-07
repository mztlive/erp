# ERP Backend

Build ERP HTTP and CLI entrypoints from domain crates, named processes, and read models. Keep business rules and persistence with their owning domain.

## What’s Included

- **Axum Web API (`apps/web-api`)**: JWT authentication for ERP operators, Casbin RBAC backed by MongoDB, account management, and authenticated image upload to S3-compatible object storage.
- **Ops CLI (`apps/cli`)**: Initialize the super admin or reset an existing admin password without depending on `web-api`.
- **Persistence (`crates/persistence-core`)**: Generic repositories, executors, transaction support, and Mongo helpers. Domain crates own concrete repositories, collection accessors, and index definitions.
- **Business domains (`crates/erp-*`)**: 19 domains own entities, DTOs, rules, repositories and services. Cross-domain writes belong to `erp-processes`; mixed queries belong to `erp-read-models`. Ordinary domains must not depend on other business domains.
- **Shared crates (`crates/`)**: coordination-free UUID generation (`id-generator`), S3-compatible object storage (`storage`), and proc macros for entities and permissions.

## Project Layout

- `apps/web-api/` – Axum entrypoint, routes, authentication/rate-limit middleware, Casbin authorization, and handlers (`core/handler/{admin,auth,upload.rs}`).
- `apps/cli/` – Operator CLI for `init-admin` and `reset-password`.
- `crates/erp-core`, `application-core`, `persistence-core` – Shared value, application and persistence contracts.
- `crates/erp-processes` – Named cross-domain processes and concrete consumer adapters.
- `crates/erp-read-models` – Mixed queries, display projections and the shared work-item fact reader.
- Other `crates/erp-*` – Business domains with independent normal/build/dev dependency boundaries.
- Legacy `database/tests` and `services/tests` – Historical archives only; preserve bytes and exclude them from Cargo targets.
- `config/` – Config loader with CLI args and optional Nacos hot-reload.
- `crates/` – Shared libraries (`id-generator`, `storage`, `entity-*`, `permission-macros`).
- `config.toml.example` – Minimal local configuration template.

## API Surface (current)

- **Public**
  - `GET /health`
  - `POST /login` – Back-office login, returns JWT.
- **Admin (JWT + RBAC)**
  - `/admin/admins` – List, create; `/admin/admins/{id}` – update/delete; `/admin/admins/{id}/role` – update role.
  - `/admin/roles` – List/create; `/admin/roles/{id}` – update/delete.
  - `/admin/audit-logs` – Paginated management audit records.
- **Authenticated account**
  - `GET /account/profile` – Current ERP operator profile.
- **Authenticated back-office upload**
  - `POST /upload` – Keeps the existing Multipart and `{ url }` response contract, requires a valid back-office JWT, uploads the validated object to S3, and returns its public URL. JPEG/PNG/WebP/GIF files are limited to 5 MiB and checked by extension, declared MIME, and file header. Admission is limited to 10 uploads/subject/minute, 100 uploads/minute globally, and 4 concurrent requests per process.
- **Response shape**: All handlers return `ApiResponse { status, errorMessage, data, success }`.

## Configuration

1. Copy the sample and fill required fields:
   ```bash
   cp config.toml.example config.toml
   ```
   The sample JWT secret is deliberately invalid. Replace it with at least 32 random bytes before
   starting the API; the process fails fast on short or published placeholder values.
2. Key fields:
   - `[app]` `port`, `secret` (JWT key)
   - `[database]` `uri`, `db_name`
   - Required `[s3]` `bucket`, `region`, `endpoint`, `access_key_id`, `secret_access_key`, `session_token`, `key_prefix`, `force_path_style`, `public_base_url`. `public_base_url` is the public bucket or CDN root; the API appends `key_prefix` and the generated object key.
3. Optional Nacos configuration updates (database connection changes require a process restart):
   ```bash
   cargo run -p web-api -- \
     --enable-nacos true \
     --nacos-addr "http://localhost:8848" \
     --nacos-namespace "public" \
     --nacos-group "DEFAULT_GROUP" \
     --nacos-data-id "config.toml"
   ```

## Run Locally

1. Prerequisites: Rust toolchain (edition 2021, rustfmt/clippy), MongoDB instance.
2. Start the API:
   ```bash
   RUST_LOG=info cargo run -p web-api -- --config-path ./config.toml
   # LOG_FORMAT=json to emit JSON tracing
   ```
   OTLP trace export is opt-in. Set a non-empty `OTEL_EXPORTER_OTLP_ENDPOINT` and
   `OTEL_SDK_DISABLED=false` to send the existing `tracing` spans to an OpenTelemetry Collector.
   The required environment variables, sampling rules, and method instrumentation contract are in
   [`docs/opentelemetry.md`](docs/opentelemetry.md).
3. Initialize or rotate the super admin (password from `--password`, `ERP_ADMIN_PASSWORD`, or a prompt):
   ```bash
   cargo run -p cli -- init-admin --account admin --name "System Admin"
   cargo run -p cli -- reset-password --account admin
   ```

## Development Checklist

- Format and lint: `cargo fmt --all` and `cargo clippy --workspace --all-targets --all-features`.
- Tests: `env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked`. Preserve existing ignored tests; do not run historical `tests/`, `--test`, `--include-ignored`, MongoDB or external-service tests.
- Boundaries: run `./scripts/check-bpm-boundaries.sh`, `./scripts/check-domain-boundaries.sh --cutover` and `./scripts/check-permissions-drift.sh`. Legacy crate sources, manifests, and dependency edges must remain absent.
- Docker: `docker-compose.yml` builds the local Web API; production uses
  digest-pinned images through `docker-compose.production.yml` and `Jenkinsfile`.
  See `DEPLOY.md` for local build, Jenkins parameters, deployment, and rollback.

## Implementation Contract

- Extend the owning domain under `crates/erp-<domain>/src/`; use only modules required by real use cases. Place mixed writes in a named Process, mixed reads in a ReadModel, and protocol adapters in the existing HTTP handler tree.
- Reuse service-layer DTOs in handlers instead of duplicating request structs; prefer thin wrappers only when HTTP validation differs.
- Use domain-owned repositories through their collection accessors and `persistence_core::Executor` for all persistence, and keep business rules inside entities/value objects.
- Add external-provider abstractions only when a real integration has at least one caller.

## Notes

- Tracing writes to stdout/stderr by default so the container runtime can rotate it. Set
  `LOG_TO_FILE=true` only when file retention and rotation are managed externally.
- Back-office JWTs include the account persistence version. Deploying this contract invalidates
  older back-office tokens, so operators must sign in again.
- MongoDB transactions require a replica set or transaction-capable sharded cluster. Startup probes
  the deployment and rejects standalone MongoDB before reporting the API ready.
- Login and upload rate limits are process-local safeguards, not cluster-wide quotas. Login keys use Axum's TCP peer address; deployments behind a reverse proxy must preserve the real peer through a trusted network topology instead of blindly trusting forwarded headers. S3 credentials must be scoped to the configured bucket and prefix. Object retention, lifecycle cleanup, public access, and CDN cache policy are deployment responsibilities.
