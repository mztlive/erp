# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

员工福利 ERP 单仓：`backend/`（Rust + Axum + MongoDB）、`erp-client/`（Next.js 16，按纯 SPA 使用）、`e2e/`（Playwright 流程测试）、`scripts/`（根级开发/清库/种子/E2E 编排）、`docs/`（产品基线与接口合同）。

## 子项目规则（权威来源）

- **后端**：改 `backend/` 前必须先读 `backend/AGENTS.md`（架构归属、编码约定、事务、测试范围、质量门禁）。它不会被自动加载。选 crate 看 `backend/crates/README.md` 与目标 crate 的 README。
- **前端**：`erp-client/CLAUDE.md` 通过 `@AGENTS.md` 加载 `erp-client/AGENTS.md`，内容包括 SPA 约束、TanStack Query/Form、界面文案、自动化 DOM id、轻预览 Sheet 和表格工具栏。Next 16 与训练数据有差异，写 Next 代码前先查 `erp-client/node_modules/next/dist/docs/`。
- `backend/apps/web-api/README.md` 仍按旧的 `entities/services/database` 三层描述。旧三层已删除，以 `backend/AGENTS.md` 为准。
- `.domain-migration-evidence/`、`.codex-audit/`、`backend/docs/archive/legacy-crates/` 只做历史证据或归档，不要修改，也不要接回 Cargo target。

## 常用命令

### 后端（在 `backend/` 执行）

- 工具链：本地 dev 用 nightly + Cranelift（`backend/.cargo/config.toml`、`rust-toolchain.toml`），crates 走 rsproxy 镜像。Docker/CI 用 `rust:1.97` + LLVM。
- 配置：`cp config.toml.example config.toml`（已 gitignore）。MongoDB 必须是副本集（需要事务），standalone 会在启动时被拒绝。
- 运行：`RUST_LOG=info cargo run -p web-api -- --config-path ./config.toml`（开发端口 10001）。CLI：`cargo run -p cli -- init-admin --account admin --name "System Admin"` 或 `reset-password --account admin`。
- 质量门禁：
  ```bash
  cargo fmt --all -- --check
  cargo check --workspace --locked
  cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
  env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked
  ./scripts/check-bpm-boundaries.sh
  ./scripts/check-domain-boundaries.sh --cutover
  ./scripts/check-permissions-drift.sh
  ```
  源码体积：`./scripts/check-rust-size.sh`（生产文件 ≤800 行、方法 ≤50 有效行，不含测试）。存量超限清零前不列入提交阶段全量清单。
- 单个 crate 或单个测试：`env -u ERP_TEST_MONGO_URI cargo test -p erp-sales --lib <测试名过滤>`。
- **只跑库单元测试**：不新增、不修改、不执行集成测试。不要跑 `--test`、`--include-ignored`、各 crate 的 `tests/`，也不要跑依赖真实 MongoDB/S3 的命令。

### 前端（在 `erp-client/` 执行）

- `npm run dev`（端口 3000）、`npm run build`。
- `npm run lint`：oxlint `--deny-warnings`，另有两个自定义检查：
  - `lint:fixed-decimal`：禁止对金额/数量/税率类字段用 `Number()`、`parseFloat` 或 epsilon 比较，改用 `lib/fixed-decimal.ts`。
  - `lint:feature-cycles`：`features/*` 之间禁止循环 import。
- `npm run format` / `npm run format:check`（oxfmt）。
- `npm test` = vitest（jsdom，匹配 `features/**/*.test.{ts,tsx}` 和 `tests/**/*.test.{ts,tsx}`）+ `node --test`（匹配 `*.test.mts`）。
- 单个测试：`npx vitest run tests/list-export.test.ts`、`npx vitest run -t "<用例名>"`，或 `node --test --experimental-strip-types <file>.test.mts`。
- 后端地址：`NEXT_PUBLIC_API_BASE_URL`，默认 `http://127.0.0.1:10001`。

### 全栈与 E2E（在仓库根执行）

- 跑单个流程：`bash scripts/run-flow.sh e2e/tests/flow-01-sales-warehouse.spec.ts`；跑全部：`bash scripts/run-flow.sh all`。
  - 每次运行都会先清空业务数据（保留账号、主数据和已发布审批定义）。
  - `E2E_RESET=0` 跳过清库；`E2E_HEADED=1` 有界面；`E2E_SLOW_MO=500` 慢动作。
- `ensure-services.sh` 会复用已在运行的 web-api 和 next dev。**改了后端代码要先执行 `bash scripts/restart-backend.sh --build`**，否则 E2E 跑的是旧二进制。
- 新库初始化或全量种子：`E2E_RESET=1 bash scripts/reset-db.sh`。开发开单准备（同时清空目录主数据后重建）：`E2E_RESET=1 bash scripts/prepare-dev.sh`。目标是远程开发库时还要加 `E2E_ALLOW_REMOTE_RESET=1`。
- 种子岗位账号密码均为 `123456`：`admin` 超管、`xiaoshou` 销售、`lisiyong` 销售领导、`caigou` 采购、`yunying` 运营、`cangchu` 仓储、`caiwu` 财务总监（只审批）、`fukuan` 出纳、`kaipiao` 开票、`guanli` 管理层、`xitong` 系统管理员。审批链见 `scripts/prepare-dev.sh` 头注释。
- 运行日志和 PID 写在根目录 `logs/`。

## 架构要点

### 后端分层与依赖方向

- 调用链：Handler（`apps/web-api/src/core/handler/<模块>`，路由在 `core/routes/<模块>.rs`）→ 拥有领域的 Service，或命名 Process / ReadModel → 领域 Repository → MongoDB。
- 19 个业务领域 crate（`crates/erp-<domain>`）各自拥有实体、DTO、规则、Service、Repository、集合访问器和索引。**业务领域之间禁止互相依赖**（normal/build/dev 都不行），由 `check-domain-boundaries.sh` 强制。需要外域事实时由消费方声明窄 Port，adapter 在组合层装配。
- 跨域写入放 `erp-processes`，跨域读和工作台投影放 `erp-read-models`。依赖只允许 Process → ReadModel，禁止反向。
- `crates/bpm` 是纯流程引擎，不依赖 ERP、Mongo 或 HTTP。`erp-workflow` 负责把审批政策、工作项和持久化接入 ERP。
- `erp-core` / `application-core` / `persistence-core` 只放共享值类型、应用约定和持久化机制，不放业务实体。
- 持久化：Repository 方法接收 `&mut dyn Executor`。单集合操作传 `NoTransaction`；多集合原子写入由用例或 Process 调 `Transactional::with_transaction`，外部 I/O 放在事务之外。
- 组合根（web-api、cli、测试）按固定顺序逐集合登记各领域公开的索引入口，不复制索引定义。

### 前后端耦合点

- **权限生成物**：管理端 Handler 用 `#[permission_macros::permission(...)]` 标注，`apps/web-api/build.rs` 在构建时生成 `erp-client/lib/permissions.generated.ts`。这个文件必须随改动一起提交，不要手改；`check-permissions-drift.sh` 负责校验。
- **响应信封**：后端 `core/response.rs` 返回 `ApiResponse { status, errorMessage, code?, fieldErrors?, retryable?, requestId?, data, success }`，前端 `lib/api/client.ts` 统一解包并映射成 `ApiError`（Network/Auth/Http/Validation/Parse）。改一边要同步改另一边。
- 鉴权：后台 JWT + Casbin RBAC（策略存在 MongoDB），管理端路由统一走 `routes/admin.rs` 的认证 + `with_permission` 链路。

### 前端结构

- `app/(workspace)/<业务区>/.../page.tsx` 是薄壳：只解析 `searchParams`，然后渲染 `features/<domain>/pages/*`。业务数据全部在客户端取。
- `features/<domain>/` 内一般有 `api.ts`（纯请求函数）、queries/hooks（`queryKey` + `useXxxQuery` / `useXxxMutation`）、`components/`、`pages/`、`types.ts`。
- 共享件：`components/business`（业务组件，改默认文案要改成加可选 prop）、`components/form`（`useAppForm`）、`components/ui`（shadcn/Base UI）。
- `lib/` 中的关键文件：
  - `lib/workspace-registry.ts`：导航，配合 `lib/permissions.ts` 做权限门控。
  - `lib/ui-text.ts`：界面文案。
  - `lib/automation-id.ts`：自动化 id；E2E 依赖稳定的原生 `id`。

## 产品文档

- `docs/erp-phase-1.md`：第一期业务基线，包括正式单据、采购与履约流程、卡券、票款、状态纠错规则，§11 是部门职责。
- `docs/erp-phase-2.md`：第二期方案，包括 API 供应商、商城、结算。
- `docs/*-contract.md`、`docs/*-openapi.yaml`：各专题的接口与行为合同；审批接口可用 `npm run openapi:lint`（在 `erp-client/` 执行）校验。
- 改业务行为前先对照相应文档；文档与代码冲突时要指出来，不要默默选一边。
