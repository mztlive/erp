# RS Project Template

Rust + Next template that ships an Axum-based Web API, Mongo-backed repositories, and an admin frontend. Authentication, RBAC, account management, and file upload are wired end to end.

## What’s Included
- **Axum Web API (`apps/web-api`)**: JWT authentication for ERP operators, Casbin RBAC backed by MongoDB, account management, and authenticated image upload with read-only local file serving.
- **Mongo repositories (`database/`)**: Generic `Repository<T>` with soft-delete, paging, and transaction helpers, plus typed accessors via `DatabaseExt`.
- **Domain/services (`entities/`, `services/`)**: Domain entities and application services for ERP operator accounts, audit logs, and RBAC.
- **Shared crates (`crates/`)**: coordination-free UUID generation (`id-generator`), local file storage (`storage`), and proc macros for entities and permissions.
- **Admin frontend (`fronts/admin`)**: Next.js 15 management console for accounts, roles, audit logs, and the other active admin domains.

## Project Layout
- `apps/web-api/` – Axum entrypoint, routes, authentication/rate-limit middleware, Casbin authorization, and handlers (`core/handler/{admin,auth,upload.rs}`).
- `services/` – Business orchestration grouped by active domains such as accounts, auditing, and IAM.
- `entities/` – Active domain entities, value objects, and validation helpers.
- `database/` – Generic MongoDB repositories, typed `DatabaseExt` accessors, entity-specific queries, indexes, and transaction support.
- `config/` – Config loader with CLI args and optional Nacos hot-reload.
- `crates/` – Shared libraries (`id-generator`, `storage`, `entity-*`, `permission-macros`).
- `fronts/admin/` – Next.js admin application.
- `config.toml.example` – Minimal local configuration template.

## API Surface (current)
- **Public**
  - `GET /health`
  - `POST /login` – Back-office login, returns JWT.
  - `GET|HEAD /uploads/{filename}` – Read a generated upload filename; directory indexes and write methods are disabled.
- **Admin (JWT + RBAC)**
  - `/admin/admins` – List, create; `/admin/admins/{id}` – update/delete; `/admin/admins/{id}/role` – update role.
  - `/admin/roles` – List/create; `/admin/roles/{id}` – update/delete.
  - `/admin/audit-logs` – Paginated management audit records.
- **Authenticated account**
  - `GET /account/profile` – Current ERP operator profile.
- **Authenticated back-office upload**
  - `POST /upload` – Keeps the existing Multipart and `{ url }` response contract, but requires a valid back-office JWT. JPEG/PNG/WebP/GIF files are limited to 5 MiB and checked by extension, declared MIME, and file header. Admission is limited to 10 uploads/subject/minute, 100 uploads/minute globally, and 4 concurrent requests per process. A serialized check rejects writes that would leave less than `upload_min_free_bytes` free (512 MiB by default).
- **Response shape**: All handlers return `ApiResponse { status, errorMessage, data, success }`.

## Configuration
1. Copy the sample and fill required fields:
   ```bash
   cp config.toml.example config.toml
   ```
   The sample JWT secret is deliberately invalid. Replace it with at least 32 random bytes before
   starting the API; the process fails fast on short or published placeholder values.
2. Key fields:
   - `[app]` `port`, `secret` (JWT key), `upload_path` (a dedicated non-root local or absolute storage directory), `upload_min_free_bytes` (post-write filesystem reserve), `file_base_url` (public prefix for file URLs; defaults to the API’s `/uploads` mount)
   - `[database]` `uri`, `db_name`
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
1. Prerequisites: Rust toolchain (edition 2021, rustfmt/clippy), MongoDB instance, Node 18+ if you touch the frontend.
2. Initialize or repair the super admin. Re-running this command restores the account and rotates
   its name/password to the supplied values:
   ```bash
   cargo run -p web-api --bin init_super_admin -- \
     --config-path ./config.toml \
     --account admin \
     --password 'ChangeMe123!' \
     --name 'System Admin'
   ```
3. Start the API:
   ```bash
   RUST_LOG=info cargo run -p web-api -- --config-path ./config.toml
   # LOG_FORMAT=json to emit JSON tracing
   ```
4. Admin frontend:
   ```bash
   cd fronts/admin
   npm install
   NEXT_PUBLIC_API_URL=http://localhost:10001 npm run dev
   ```

## Development Checklist
- Format and lint: `cargo fmt --all` and `cargo clippy --workspace --all-targets --all-features`.
- Tests: `cargo test --workspace` (service and entity tests live inline); add a happy-path test for every functional change.
- Frontend (if modified): `npm run lint` and keep Biome formatting (4-space indent) intact.
- Docker: `docker-compose.yml` builds the local Web API and admin stack; production uses
  digest-pinned images through `docker-compose.production.yml` and `Jenkinsfile`.
  See `DEPLOY.md` for local build, Jenkins parameters, deployment, and rollback.

## Extending the Template
- Add new domains under `services/src/<domain>/` with a `dto.rs` and only the modules required by real use cases; expose protocol adapters under `apps/web-api/src/core/handler/{admin,auth}` or the dedicated shared handler file such as `upload.rs`.
- Reuse service-layer DTOs in handlers instead of duplicating request structs; prefer thin wrappers only when HTTP validation differs.
- Use `database::Repository` via `DatabaseExt` for all persistence, and keep business rules inside entities/value objects.
- Add external-provider abstractions only when a real integration has at least one caller.

## Notes
- Tracing writes to stdout/stderr by default so the container runtime can rotate it. Set
  `LOG_TO_FILE=true` only when file retention and rotation are managed externally.
- Back-office JWTs include the account persistence version. Deploying this contract invalidates
  older back-office tokens, so operators must sign in again.
- MongoDB transactions require a replica set or transaction-capable sharded cluster. Startup probes
  the deployment and rejects standalone MongoDB before reporting the API ready.
- Login and upload rate limits are process-local safeguards, not cluster-wide quotas. Login keys use Axum's TCP peer address; deployments behind a reverse proxy must preserve the real peer through a trusted network topology instead of blindly trusting forwarded headers. The API fails closed at the configured free-space watermark, but the check-and-save lock is also process-local; multiple instances sharing one volume need external quota coordination. The service does not delete old files, so production deployments still need disk monitoring and an upload retention policy. `file_base_url` may point to an external CDN instead of the built-in read-only file service.
