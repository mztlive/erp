# 仓库指南

## 架构与归属

- 调用链：HTTP Handler → 本域 Service 或命名 Process/ReadModel → 拥有领域 Repository → MongoDB。Handler 只做协议适配；纯业务规则归实体/值对象，Service 只做编排，Repository 屏蔽查询细节，Handler 禁止直连数据库。
- 19 个业务领域 crate 拥有实际实现：identity、audit、workflow、support、party、customer、supplier、catalog、warehouse、contract、import、inventory、finance、sales、procurement、fulfillment、returns、integration、supply。
- `erp-core`、`application-core`、`persistence-core` 只留共享基础合同（通用值对象/应用/持久化合同），禁放 AccountCore、Role、AuditLog、WorkItem、SalesOrder 等业务实体。
- `erp-processes` 拥有跨域事务、正式用例、实际 adapter 与外部 I/O 边界；`erp-read-models` 拥有混合查询、工作台/中心页展示投影及唯一正式任务事实读取器。允许 Process → ReadModel，禁止反向；事实权限政策仍由拥有领域执行。
- 业务领域 normal/build/dev 均禁依赖其他业务领域、组合层或旧三层；跨域事实经窄 Port 获取，实际 adapter 在组合层装配；共享规则保留唯一领域提供方。
- `crates/bpm` 只留流程定义、运行状态和引擎规则；`erp-workflow` 负责 ERP 政策、工作项、审批集成及 BPM 持久化适配；BPM 禁依赖 ERP、MongoDB、HTTP、配置、ID 生成器或通知客户端。
- `apps/web-api` 负责 Axum 协议、AppState 装配、后台 worker 生命周期，管理员路由固定走 JWT + RBAC；`apps/cli` 只做管理员初始化与密码重置运维装配，复用身份领域用例与组合层 adapter，并登记全域索引；CLI 禁依赖 web-api。
- `crates/erp-<domain>` 含本域实体、DTO、规则、Service、Repository、collection accessor 和 indexes，不留旧路径 façade。`config`、`docs`、`scripts` 为配置、执行合同与门禁工具。
- 禁止恢复顶层 `entities/`、`database/`、`services/`；其源码、manifest 和三类依赖入边必须为零。旧三层测试与旧文档只以原始字节存 `docs/archive/legacy-crates/` 供追溯，禁接入活动 Cargo target，历史 tests 档案亦然。
- HTTP、CLI 和库测试组合根只按阶段00原逐集合顺序登记各领域公开索引入口，不复制索引定义；事务执行器、幂等恢复、ID/时钟位置和外部 I/O 边界保持业务合同。
- 管理端 Handler 必须加 `#[permission_macros::permission(...)]`（公开登录/上传/公开选品入口除外）；`apps/web-api/build.rs` 生成前台权限，以权限漂移脚本校验，不手改生成物。
- 配置用 `config::SafeConfig`；Web API 日志与 Tracing 在 `apps/web-api/src/core/tracing`；上传经 `AppState` 的 `storage::S3Storage` 写配置 bucket，公开 URL 由 `public_base_url`、`key_prefix` 与对象键生成。

## 新功能开发流程（后端）

1. **建模**：在拥有领域 `entity` 建/扩实体与值对象，封装不变式与验证（见§领域模型定义原则）。
2. **仓储**：在拥有领域 `repository` 新增仓库做实体读写与聚合，经各领域 `*Ext` 扩展 trait（如 `AccessControlExt::accounts()/roles()`）暴露；不管理事务，只按传入执行器决定是否加入事务。
3. **服务**：本域 DTO + 单域用例放拥有领域；跨域写入放命名 Process，混合读取放 ReadModel；可复用不变式必须下沉到类型，禁长期滞留 Service 私有 helper（见§类型内聚与下沉）。
4. **HTTP**：在 `apps/web-api/src/core/handler` 新增 handler，统一用 `ApiResponse` 返回；必须复用领域/组合层 DTO，禁重复定义同语义 Request/Response，仅 HTTP 形态差异（路径拆分、上下文注入、协议重命名）允许最小薄包装并实现 `From/Into` + 注释原因。
5. **路由/权限**：挂到 `apps/web-api/src/core/routes`，管理员接口放 `admin` 并走 JWT + RBAC，加 `permission` 标注；组织机构 handler 无独立路由文件，经 `access_control` 路由装配。
6. **测试与门禁**：按§测试期望补单元测试（至少 happy-path + 失败/边界，不新增集成测试）；然后一次过§CI 与质量门禁。

## 编码约定

- 格式 rustfmt（宽 110）；模块 snake_case，类型 CamelCase，常量大写蛇形；日志用 `tracing` 带 id、account、request_id 等上下文；敏感信息禁入日志。
- **导入**：禁写完整路径引用（如 `erp_workflow::service::...`），一律顶部 `use` 后用短名，冲突用 `as` 别名；`use`/`mod` 语句本身及 `<Type as Trait>::item` 消歧除外。
- **分支与错误**：优先 guard clauses（含 `let-else`），拆复杂分支为私有函数；`Option` 简单透传用 `map/and_then/filter`，禁 `match x { Some(v) => ..., None => None }`；固定模式语义清晰可用 `match`。错误优先 `?` + `thiserror #[from]/From` 转换，需映射语义（如转 `BadRequest`）时允许 `map_err`，纯透传用 `?`。
- **注释**：所有公共方法必须多行文档注释（参数/返回值/错误）；私有方法仅在含校验分支、业务规则或复杂流程时补充，简单 getter/format 转换可省。
- **Service 组织与命名**：单个 service 文件写 `mod.rs`，多个才拆 `service.rs`；查询用名词（`role_list`），操作用动词（`create/update/delete`）。
- **方法长度**：业务方法（handler、领域 `service/repository/entity`、Process/ReadModel）限 30 有效行内，超限拆私有 helper；测试、`build.rs`、宏实现除外。

## 类型内聚与下沉

- 原则：不依赖 DB/外部 I/O 的业务规则，优先进拥有领域实体/值对象或 DTO 自身；Service 仅留流程编排、事务边界、仓储调用、跨聚合协作。
- **必须下沉**：账号类型/可用性判定、权限覆盖判定（`Permission::covers`/`PermissionSet::covers`、workflow `permission_covers` 等）、RoleId/AccountId 类输入规范化（trim/去重/空校验/类型化）、DTO 确定性构造（`AvailableWorkItemAccount::from_account`、`AdminItem::from_account` 等）。复用输入优先显式值对象（如 `RoleIdSet`），只给 `as_slice/to_strings` 等最小接口。
- **留在 Service**：依赖仓储结果的判断（唯一冲突、跨集合存在性）、事务内多步写入一致性、查询过滤拼装与分页编排。
- 同类 `normalize_*`/`ensure_*`/权限覆盖 helper 在 ≥2 个 Service 出现，必须上提到拥有领域公共方法/值对象，禁复制粘贴；下沉后删原私有 helper，并为该规则补实体/值对象单元测试（happy-path + 失败路径）。

## 事务使用约定

- 边界由本域用例或跨域 Process 控制；Repository 方法统一收 `executor: &mut dyn Executor`（`persistence_core::Executor`），Repository 层无 `_with_session` 重复方法（Process 内部事务闭包 helper 如 `persist_*_with_session` 除外）。
- 单集合 CRUD（无需跨集合原子性）传 `&mut NoTransaction`；多集合写入或需原子性的关联操作，用 `mongodb::Client` 经 `persistence_core::Transactional::with_transaction` 开事务，把 `&mut ClientSession` 传给 Repository/policy 方法。

## 构建、运行与工具

- 初始化：`cp config.toml.example config.toml`，填 `app`、`database`、`s3`。
- API：`cargo run -p web-api -- --config-path ./config.toml`（`RUST_LOG=info|debug`、`LOG_FORMAT=json`）；CLI：`cargo run -p cli -- init-admin --account admin --name "System Admin"` / `reset-password --account admin`，密码优先 `--password`，其次 `ERP_ADMIN_PASSWORD`，否则交互输入。
- Docker：`./manage.sh start|status|logs` 封装 `docker compose`；只按 `docker-compose.yml` 只读挂载 `config.toml`，文件对象写 S3。

## 测试期望

- 单元测试内联（`mod tests`），尽量覆盖全面：每个改动至少一个 happy-path；路由/行为变更同步更新 HTTP 层单测；必想失败/边界（参数非法、权限拒绝、空输入/空集合、重复、超长超限、缺失关联、软删除）、查询分页（过滤组合、稳定排序、总数一致、空页/尾页/越界页、归组去重）、金额数量（零/正负/精度舍入/溢出守恒，聚合与基准对拍）、状态幂等（全迁移矩阵、幂等重放、异载荷冲突、版本冲突、失败原子性）。覆盖不到须在 PR 说明理由，禁以“后续补集成测试”省略。
- 禁新增/修改/执行集成测试（含 `examples/` 下需真实 Mongo 的验收例程）：禁跑各 crate `tests/`、任何 `--test` 目标、`--include-ignored` 或依赖真实 MongoDB/外部服务的命令。上传/临时产物不入库，提交前清大日志。

## CI 与质量门禁

- 本地/CI 统一跑（CI 加 `--check`/`-D warnings`/`--locked`，`domain-boundaries` 加 `--cutover`，并跑权限漂移校验；全程不跑集成测试）：
  `cargo fmt --all`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features`、`env -u ERP_TEST_MONGO_URI cargo test --workspace --lib`、`./scripts/check-bpm-boundaries.sh`、`./scripts/check-domain-boundaries.sh`、`./scripts/check-permissions-drift.sh`。

## 兼容、安全、治理与容错

- **兼容/API**：新增字段默认向后兼容，禁无迁移删/改线上字段；错误用统一结构与稳定语义，新增场景写进接口文档；外部变更在 PR 列影响范围、回滚策略和兼容窗口。
- **安全**：禁输密码/token/身份证号；上传校验大小/扩展名/MIME，对象键限配置 `key_prefix` 下的安全相对路径；权限失败与关键修改记审计日志；登录/上传等高频敏感接口具备或预留限流。
- **数据治理**：新增查询评估索引，禁 N+1 与全表扫描；唯一性靠唯一索引；过期数据用 TTL 或归档任务；字段/索引变更给迁移脚本与回滚预案。
- **容错**：外部 HTTP 统一超时、重试上限与错误分类；资金/状态机变更必须幂等键或去重；依赖失败降级为可观测错误并记 account/request_id 上下文。

## 领域模型定义原则

- **结构**：实体必含 `BaseModel`（`#[serde(flatten)]`：`id/version/created_at/updated_at/deleted_at`）；ID 优先 newtype（如 `ProductCategoryId`、`SalesOrderId`，`role` 沿用 `RoleId`）；创建/更新传参与系统字段分离，用独立 `Data` 结构。派生 `#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]`，持久化映射与现 Mongo 模型一致。
- **验证**：`new()` 做完整验证+规范化（`non_empty_trimmed`、长度常量如 `NAME_MAX_LEN`、业务规则、去空白/截断），复杂逻辑抽私有 `ensure_*`；`update()` 复用同逻辑，但关键字段（如 `category_code`、`parent_category_id`）只许专门操作改。
- **树形**：当前为纯邻接表（`parent_category_id: Option<ProductCategoryId>`，`None` 为根，`set_parent` 拒绝自环）；`internal_code` 物化路径尚未落库，跨节点成环检测在 Service 事务内完成，搬子树同样走事务。
- **方法**：不变式封进实体方法（如 `is_root/is_active/ensure_assignable`），不外泄判断。
- **测试**：实体内 `#[cfg(test)] mod tests` 覆盖创建/更新验证、边界（空/超长/非法）、层级与唯一规则，用 `BaseModel::fake()` 或最小数据。范例看 `crates/erp-identity/src/entity/account_core.rs` 与 `role.rs`。
