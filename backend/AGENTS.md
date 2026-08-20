# 仓库指南

## 架构概览
- HTTP Handler -> Service -> Repository -> MongoDB，遵循类 DDD 分层：Handler 只做协议适配，Service 负责编排，Repository 屏蔽持久化细节。
- `crates/bpm` 是纯流程领域与状态引擎：拥有流程定义、节点、连线、运行实例、节点执行、实例审批人和命令收据，以及图规则与状态计算。禁止依赖或引用 `entities`、`database`、`services`、`apps/web-api`、`config`、`mongodb`、`axum`、`id-generator`、权限宏或通知客户端。
- `entities` 是 ERP 业务实体及 BPM 集成引用（单据绑定、业务对象快照、WorkItem、通知 outbox）。目标审批流程模型不得放在 `entities/src/approval`。
- `services::approval` 是政策、授权、事务和业务副作用适配层；BPM 状态计算必须留在 `bpm`，不得下沉到 Service。
- `database` 是 BPM 模型与 ERP 业务/集成模型的 MongoDB 适配层；禁止把 MongoDB 类型或 `Executor` 反向引入 `bpm`。
- 依赖只允许单向：`apps/web-api` / `apps/cli` → `services` → `{database,entities,bpm}`，以及 `database` → `{entities,bpm}`、`entities` → `bpm`。禁止 `bpm` 反向依赖任何 ERP crate。`apps/cli` 禁止依赖 `web-api`。
- 权限体系：Handler 使用 `#[permission_macros::permission(...)]` 标注，`apps/web-api/build.rs` 会解析路由并生成前端权限定义。
- 配置统一走 `config::SafeConfig`（CLI 参数 + 可选 Nacos 热更新），Web API 日志/Tracing 位于 `apps/web-api/src/core/tracing`。

## 项目结构与归属
- `apps/web-api`：Axum HTTP API。Handlers 位于 `src/core/handler/{auth,admin}` 及 `handler/upload.rs`；路由注册在 `src/core/routes/{public,admin,account}.rs`；统一返回 `ApiResponse`。管理员路由固定走 JWT + RBAC 中间件。
- `apps/cli`：运维命令行。提供 `init-admin`（创建或修复超级管理员）和 `reset-password`（只改已有管理员密码）。只依赖 `services` / `database` / `config`，禁止依赖 `web-api`。
- `services`：领域服务编排层，按域分目录（如 `iam`、`consumer`、`audit`）。`services::approval` 只做政策/授权/事务/副作用适配，不得实现流程状态机。新增领域需提供 `dto.rs`；如果只有一个 service 文件，代码直接写在 `mod.rs` 中；如果有多个 service 文件，再创建独立 `service.rs` 文件。业务规则优先放在实体/值对象中。
- `entities`：ERP 业务实体、值对象及 BPM 集成引用（如 `account_core`、`consumer`、`role`、`rbac`、`auth`、`approval_integration`）。目标审批领域模型不在本 crate。
- `database`：MongoDB 仓储层与 `DatabaseExt` 访问器（如 `account_core`、`role`、`consumer`），同时承担 BPM 模型与 ERP 集成模型的持久化适配。
- `crates`：共享工具与基础设施（`bpm` 纯流程领域与状态引擎、`id-generator` ID、`storage` 上传、`entity-core`/`entity-macros`、`permission-macros`）。
- `config`：配置加载与 Nacos 热更新（`SafeConfig`）。
- `docs`：专项说明（Casbin RBAC、权限生成等）。
- `scripts`：脚本与自动化工具。
- `logs`：仅在显式设置 `LOG_TO_FILE=true` 时使用的本地文件日志目录。

## 新功能开发流程（后端）
1. **建模**：在 `entities` 中创建/扩展实体与值对象，封装不变式与验证。
2. **仓储层**：在 `database/src/repository` 新增仓库，实现实体读写与聚合；通过 `DatabaseExt` 暴露。
3. **服务层**：在 `services/src/<domain>` 添加 `dto.rs`；编排流程，不得绕过仓库层。
4. **HTTP 层**：在 `apps/web-api/src/core/handler` 新增 handler，默认必须复用 service DTO，禁止重复定义等价请求/响应类型；仅在 HTTP 形态差异时允许最小薄包装并实现 `From/Into`。
5. **路由/权限**：将新接口挂到 `apps/web-api/src/core/routes`；管理员路由必须位于 `admin` 并走 JWT + RBAC；为 handler 添加 `#[permission_macros::permission(...)]`。
6. **测试**：新增至少一个 happy-path 测试，并补充至少一个失败/边界路径测试。
7. **检查**：执行 `cargo fmt --all`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features`、`./scripts/check-bpm-boundaries.sh`。
8. **回归**：执行 `cargo test --workspace`，确保变更无回归。

## 编码约定
- Rust 格式遵循 rustfmt（最大宽度 110）；模块 snake_case，类型 CamelCase，常量大写蛇形。
- **分支表达约定**：
  - 固定模式匹配且分支语义清晰时可以使用 `match`。
  - 对于 `Option` 的简单透传/转换场景，避免使用 `match x { Some(v) => ..., None => None }`。
  - 对于提前返回场景，优先 guard clauses；允许使用 `let-else`。
  - 避免 `let x = if ... { ... } else { ... };` 这种赋值式复杂分支；优先拆分为 guard + 私有函数。
  - 可优先使用 `Option` 组合子（`map`/`and_then`/`filter`）和私有解析函数。
- Service 负责流程编排，Repository 屏蔽查询细节；Handler 不得直接访问数据库。
- `ApiResponse` 为统一返回结构，所有 handler 均需复用。
- **注释约定**：
  - 所有公共方法必须包含多行文档注释（参数、返回值、错误）。
  - 私有方法在包含校验分支、业务规则或复杂流程时必须补充文档注释；简单 getter/format 转换可省略。
- **响应约定**：Handler 默认复用 `services` DTO/View 作为响应模型，禁止为同一语义重复定义等价 Response 类型；仅在内部调用链且无敏感字段泄漏风险时可直接传递实体。
- **请求约定**：Handler 必须复用 `services` DTO 作为请求体；仅在 HTTP 形态差异（如路径参数拆分、上下文字段注入、协议字段重命名）时允许最小补充包装，并实现 `From/Into`。
- **DTO 复用禁止项**：
  - 禁止在 `apps/web-api/src/core/handler/**` 中定义与 `services/src/**/dto.rs` 同语义且字段同构的重复 Request/Response 类型。
  - 若确需包装，必须在类型或转换处注释说明 HTTP 形态差异原因，并提供显式转换实现（`From/Into` 或等价实现）。
- **Service 模块组织约定**：如果 service 层只有一个 service 文件，就把代码写到 `mod.rs` 中；只有当 service 层有多个 service 文件需要拆分时，才创建独立 `service.rs`。
- **Service 方法命名约定**：查询类方法使用名词（如 `consumer_list`、`role_list`），操作类方法保持动词（`create`、`update`、`delete`）。
- **流程控制约定**：优先守卫子句（guard clauses），避免深层嵌套 if-else。
- **方法长度约定**：
  - 业务代码方法（`apps/web-api/src/core/handler`、`services/src`、`database/src/repository`、`entities/src`）应尽量控制在 30 行以内（有效代码行，不含空行与纯注释行）。
  - 超出 30 行时必须拆分私有 helper，保持单一职责和可测试性。
  - 测试代码、`build.rs`、宏实现不受该条约束。
- 使用 `tracing` 输出结构化日志并带上下文字段（id、account、request_id 等）。
- 上传文件必须通过 `AppState` 注入的 `storage::S3Storage` 写入配置的 S3 bucket；公开 URL 必须由 `public_base_url`、`key_prefix` 与对象键生成。

## 类型内聚与下沉编码要求
- **核心原则**：凡是“不依赖数据库/外部 I/O 的业务规则”，优先封装到 `entities`（实体/值对象）或 DTO 自身，不得长期滞留在 `services` 私有 helper 中。
- **Service 的职责边界**：Service 仅负责流程编排、事务边界、仓储调用、跨聚合协作；不得承载可复用的不变式实现。
- **必须下沉到类型的方法类别**：
  - 账号类型与状态判定（如 kind 校验、账号可用性判定）。
  - 权限覆盖与子集判定（如 `permission_covers`、`ensure_permissions_subset`）。
  - 角色ID/账号ID等输入规范化（trim、去重、空值校验、类型化转换）。
  - DTO/上下文对象的确定性构造（如 `RoleActor::from_account`）。
  - 值对象内部字段优先级规则（如 profile 电话优先于账号电话）。
- **必须留在 Service 的方法类别**：
  - 依赖仓储查询结果才能判断的方法（如唯一性冲突检查、跨集合存在性检查）。
  - 事务内多步骤写入与一致性维护逻辑。
  - 查询过滤拼装与分页编排（Repository 查询参数组织）。
- **重复 helper 处理要求**：
  - 当同类规范化/校验逻辑在 2 个及以上 Service 出现，必须抽取到 `entities` 公共方法或值对象。
  - 禁止在不同 Service 复制粘贴同一套 `normalize_*`、`ensure_*`、`permission_matches` 逻辑。
- **新类型设计要求**：
  - 优先使用显式值对象表达“已规范化输入”（如 `entities::RoleIdSet`）。
  - 提供 `as_slice` / `into_vec` / `to_strings` 等最小必要接口，避免外部重复转换。
- **迁移与验收要求**：
  - 下沉后必须删除原 Service 重复私有 helper，避免双份规则源。
  - 至少补充一条实体/值对象单元测试覆盖该规则（happy-path + 失败路径）。
  - 变更后必须通过：`cargo fmt --all`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features`、`cargo test --workspace`、`./scripts/check-bpm-boundaries.sh`。

## 性能优化约定（社区最佳实践）
- **先度量后优化**：在改性能前先用 `tracing`/指标/基准确认热点；优先解决高频路径与大对象分配。
- **减少不必要分配**：热点路径优先借用 `&str`/`&T`，仅在需要所有权时 `clone`；能用 `Cow`/`Arc` 共享就不要拷贝。
- **避免中间容器**：能用迭代器直接 `collect` 就不要多次 `map`+`collect`；已知大小用 `Vec::with_capacity`。
- **避免隐式 `to_string()`**：能传 `&str` 就不要创建新 `String`；使用 `as_ref`/`as_deref` 降低拷贝。
- **关注 I/O 与查询**：批量查询避免 N+1；MongoDB 使用投影/索引减少传输与反序列化成本。
- **并发与阻塞隔离**：CPU 密集或阻塞 I/O 使用 `spawn_blocking`，避免阻塞 async runtime。

## 事务使用约定
- 事务边界由 Service 控制；Repository 不管理事务，只按调用方传入的执行器决定本次操作是否加入事务。
- Repository 的每个方法都接收 `executor: &mut dyn Executor`（`database::Executor`），不再提供 `_with_session` 重复方法。
- **单集合操作原则**：仅涉及单个集合的 CRUD（无需跨集合保证原子性）时，不需要事务，传入 `&mut NoTransaction`。
- **多集合/多步骤原子性原则**：涉及多个集合的写入/更新/删除，或需要保证原子性的关联操作，必须使用 MongoDB 事务，把事务闭包拿到的 `&mut ClientSession` 作为执行器传入。
- 事务入口来自 `mongodb::Client`，统一使用 `database::Transactional::with_transaction`（自动 commit/abort）。
- 多步骤写入的 Repository 与 policy 方法（如角色绑定替换、角色规则删除）必须收到事务执行器，注释中已注明该约束。

## 构建、运行与工具
- 初始化配置：`cp config.toml.example config.toml`，填写 `app`、`database` 与 `s3`。
- API：`cargo run -p web-api -- --config-path ./config.toml`（支持 `RUST_LOG=info|debug`、`LOG_FORMAT=json`）。
- CLI：`cargo run -p cli -- init-admin --account admin --name "System Admin"`；`cargo run -p cli -- reset-password --account admin`。密码优先 `--password`，其次环境变量 `ERP_ADMIN_PASSWORD`，否则交互输入。
- Workspace：`cargo build --workspace`、`cargo test --workspace`。
- 质量门禁：`cargo fmt --all`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features`、`cargo test --workspace`、`./scripts/check-bpm-boundaries.sh`。
- Docker：`./manage.sh start|status|logs` 封装 `docker compose`；仅按 `docker-compose.yml` 只读挂载 `config.toml`，文件对象写入 S3。

## 测试期望
- 单元测试内联（`mod tests`），覆盖新的业务规则与边界。
- 集成测试放在各 crate 的 `tests/` 目录。
- 每个功能改动至少包含一个 happy-path 测试；路由/行为变更需更新 HTTP 层测试。
- 对权限与参数校验等场景，至少覆盖一个失败路径测试。
- 上传/临时产物不纳入版本控制，提交前清理大体积日志。

## CI 与质量门禁
- CI 必须执行并通过：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features -D warnings`、`cargo test --workspace`、`./scripts/check-bpm-boundaries.sh`。
- 任何生成文件（如权限定义）必须在 CI 中校验未漂移。

## API 契约与兼容性
- 新增字段默认向后兼容，禁止无迁移地删除或重命名线上字段。
- 错误响应使用统一结构与稳定错误语义；新增错误场景应在接口文档中说明。
- 外部接口变更需在 PR 描述列出影响范围、回滚策略和兼容窗口。

## 安全基线
- 日志中不得输出明文密码、token、验证码、身份证号等敏感信息。
- 上传接口必须校验文件大小、扩展名和 MIME；对象键必须是安全相对路径，并限制在配置的 `key_prefix` 下。
- 权限失败和关键数据修改必须记录审计日志。
- 对高频敏感接口（登录、验证码、上传）应具备限流能力或预留限流扩展点。

## 数据治理（MongoDB）
- 新增查询必须评估索引需求，避免线上 N+1 与全表扫描。
- 唯一约束通过唯一索引保证，不仅依赖应用层校验。
- 需要过期清理的数据应使用 TTL 索引或明确归档任务。
- 变更集合字段时，需提供迁移脚本与回滚预案（尤其索引变更）。

## 外部依赖容错
- 外部 HTTP 调用应统一设置超时、重试上限和错误分类。
- 涉及资金/状态机变更的操作必须具备幂等键或去重机制。
- 依赖失败需降级到可观测错误，并记录上下文（account/request_id）。

## 领域模型定义原则
领域模型（Entity）是业务逻辑的核心，定义时应遵循以下原则：

### 1. 结构设计原则
- **基础字段**：所有实体必须包含 `BaseModel`（通过 `#[serde(flatten)]` 扁平化），包含 `id`、`version`、`created_at`、`updated_at`、`deleted_at`。
- **ID 类型**：优先使用 newtype 包装（如 `ProjectId`、`RoleId`）。
- **数据传递**：创建/更新操作使用独立的 `Data` 结构，不包含系统字段，便于参数传递和验证。

### 2. 验证与规范化原则
- **构造函数验证**：`new()` 方法必须进行完整的数据验证和规范化，包括：
  - 必填字段非空验证（使用 `non_empty_trimmed`）
  - 字符串长度限制（定义常量如 `NAME_MAX_LEN`）
  - 业务规则验证（如列表项数量、唯一性、关联一致性）
  - 自动规范化（去除首尾空白、截断超长字符串）
- **更新方法**：`update()` 方法应复用相同验证逻辑，但通常不允许修改关键字段（如 `parent_id`、`internal_code` 等需要专门操作修改的字段）。
- **验证函数**：复杂验证逻辑提取为私有函数（如 `ensure_list_size`、`ensure_unique_platforms`），提高可读性和可测试性。

### 3. 树形结构设计原则
- **层级表示**：使用 `parent_id: Option<IdType>` 表示父子关系，`None` 表示根节点。
- **内部编号**：为支持高效层级查询，使用 `internal_code: String` 字段存储路径编码：
  - 根节点：直接使用自身ID（如 `"001"`）
  - 子节点：父节点内部编号 + `"_"` + 自身ID（如 `"001_002"`）
  - 查询子节点：使用前缀匹配（regex 或范围查询）快速查询整个子树
  - 限制：设置 `INTERNAL_CODE_MAX_LEN` 防止层级过深
- **层级操作**：移动节点等操作需要更新整个子树 `internal_code`，应在 Service 层通过事务保证一致性。

### 4. 序列化与持久化原则
- **派生宏**：实体必须派生 `#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]`。
- **扁平化**：`BaseModel` 使用 `#[serde(flatten)]` 扁平化到实体中；持久化映射与当前 Mongo 模型保持一致。

### 5. 方法设计原则
- **不变式封装**：业务规则和不变式应封装在实体方法中（如 `is_root()`、`can_be_operated_by()`），而非暴露给外部判断。
- **辅助方法**：提供便捷查询方法（如 `role_ids()`、`is_root()`），避免外部代码重复实现。
- **文档注释**：所有公共方法必须包含多行文档注释，说明参数、返回值和可能错误。

### 6. 测试原则
- **单元测试**：在实体文件内通过 `#[cfg(test)] mod tests` 编写单元测试，覆盖：
  - 创建和更新验证逻辑
  - 边界条件（空值、超长字符串、无效数据）
  - 业务规则（层级关系、唯一性约束）
- **测试数据**：使用 `BaseModel::fake()` 或构造最小化测试数据，避免依赖外部资源。

### 7. 示例参考
参考 `entities/src/account_core.rs` 与 `entities/src/role.rs` 作为标准实现：
- `AccountCore`：字段规范化、状态与凭证更新规则
- `Role`：字段规范化、系统角色约束和启停规则


# 前端管理台页面开发最佳实践指南

本文档总结了在本项目（Next.js + Shadcn UI + TanStack Query/Table）中开发管理后台列表页面的布局规范、代码结构设计及性能优化最佳实践。

---

## 0. 代码风格约束（新增）

### 0.1 方法注释要求

- **所有方法必须写注释**：包含用途与关键行为（如错误分支、鉴权、跳转等）。
- **公共导出方法必须使用 JSDoc**：建议描述参数、返回值与异常/错误处理。

示例：
```ts
/**
 * 发起 API 请求并返回 ResultAsync。
 */
export const apiRequestResult = <T>(endpoint: string, options: RequestInit = {}) => { /* ... */ };
```

### 0.2 函数声明风格

- **统一使用箭头函数风格**：`export const xxx = (...) => { ... }`
- **避免 function 声明**：除非有明确的 this 绑定需求（本项目一般不需要）

示例：
```ts
export const buildRequestConfig = (options: RequestInit): RequestInit => { /* ... */ };
```

## 1. Hook 与 UI 分离原则

### 1.1 核心理念

**「Hook 是领域逻辑的封装，组件是 UI 逻辑的归属」**

- **可复用的领域逻辑**（API 调用、错误处理）→ 封装为 Hook
- **页面特有的 UI 逻辑**（弹窗状态、协调逻辑）→ 留在组件中

```
┌─────────────────────────────────────────────────────────┐
│  page.tsx / component.tsx (UI 层)                       │
│  ├── 调用领域 Hooks                                      │
│  ├── 管理 UI 状态 (弹窗、编辑项等)                        │
│  ├── 处理 UI 协调逻辑 (成功后关闭弹窗等)                  │
│  └── JSX 渲染                                           │
└─────────────────────────────────────────────────────────┘
                          ▲
                          │ 返回: { data, mutate, isPending }
                          │
┌─────────────────────────────────────────────────────────┐
│  hooks/use-xxx.ts (领域逻辑层)                           │
│  ├── Query Hooks: 数据获取 (可复用)                      │
│  ├── Mutation Hooks: 数据变更 (可复用)                   │
│  └── 基础设施 Hooks: 错误处理、Toast (通用)              │
└─────────────────────────────────────────────────────────┘
```

### 1.2 分离标准

| 应放入 Hook 的逻辑 | 应留在组件的逻辑 |
|-------------------|-----------------|
| `useQuery` / `useMutation` 封装 | JSX 结构 |
| 可复用的状态逻辑 | 页面特有的 UI 状态 (如 `isDialogOpen`) |
| 通用错误处理 | UI 协调逻辑 (如成功后关闭弹窗) |
| 通用工具函数 | 简单的派生计算 |

### 1.3 判断是否需要抽取 Hook

**核心问题：这段逻辑是否可复用？**

应该抽取为 Hook：
- ✅ API 调用逻辑（Query/Mutation）— 几乎总是需要复用
- ✅ 错误处理、Toast 提示逻辑 — 全局统一
- ✅ 通用的表单验证逻辑

不应该抽取为 Hook：
- ❌ 页面特有的 UI 状态（弹窗开关、当前编辑项）
- ❌ 页面特有的筛选/分页状态（除非多个页面共享相同筛选逻辑）
- ❌ UI 协调逻辑（成功后关闭弹窗、失败后显示错误）

---

## 2. Hook 分类与粒度指南

### 2.1 Hook 分类体系

```
src/hooks/
├── use-error-handler.ts      # 基础设施 Hook（通用工具）
├── use-toast.ts              # 基础设施 Hook（通用工具）
│
├── use-roles.ts              # 领域 Hook (角色)
│   ├── useRoles()            #   └── Query: 获取列表
│   ├── useRolePermissions()  #   └── Query: 获取权限
│   └── useRoleOperations()   #   └── Mutations: CRUD 操作
│
└── use-products.ts          # 领域 Hook (商品)
    ├── useProducts()        #   └── Query: 分页列表
    └── useProductOperations()#  └── Mutations: CRUD 操作
```

**核心原则：Hook 是领域驱动的，不是页面驱动的。**

不推荐创建 `useProductPage` 这类页面级 Hook，原因：
1. **无法复用**：与特定页面绑定的 Hook 失去了抽象的价值
2. **混淆职责**：容易把 UI 协调逻辑（如"成功后关闭弹窗"）混入 Hook
3. **过度抽象**：如果一个 Hook 只服务于一个页面，不如直接写在组件中

### 2.2 三类 Hook 详解

#### A. Query Hook（数据获取）

**职责**：封装 `useQuery`，提供数据获取能力。

**粒度原则**：
- 一个 Query Hook 对应一个 API 端点或一类紧密关联的查询
- 支持参数化查询（筛选、分页）
- 返回 TanStack Query 的完整状态

```tsx
// ✅ 好的示例：职责单一，支持参数
export function useProducts(params: ProductListParams) {
    return useQuery({
        queryKey: ['products', params],
        queryFn: () => productApi.getProductList(params),
        placeholderData: keepPreviousData,
    });
}

// ✅ 好的示例：关联查询可以组合
export function useRoleOptions() {
    return useQuery({
        queryKey: ['roles', 'options'],
        queryFn: roleApi.getAssignableRoles,
        staleTime: 5 * 60 * 1000, // 选项类数据可以缓存更久
    });
}

// ❌ 避免：在 Query Hook 中混入 Mutation 逻辑
export function useProductsWithMutations() { /* 违反单一职责 */ }
```

#### B. Mutation Hook（数据变更）

**职责**：封装相关的 `useMutation` 操作，统一处理成功/失败回调。

**粒度原则**：
- 将同一领域的 CRUD 操作聚合到一个 Hook
- 内部处理 `invalidateQueries` 和 Toast 提示
- 返回稳定的 `mutate` 函数引用和 `isPending` 状态

```tsx
// ✅ 好的示例：聚合 CRUD，返回稳定引用
export function useProductOperations() {
    const queryClient = useQueryClient();
    const { handleError, handleSuccess } = useErrorHandler();

    const createMutation = useMutation({
        mutationFn: productApi.createProduct,
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ['products'] });
            handleSuccess('商品创建成功');
        },
        onError: handleError,
    });

    const updateMutation = useMutation({
        mutationFn: ({ id, data }: { id: string; data: UpdateProductRequest }) =>
            productApi.updateProduct(id, data),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ['products'] });
            handleSuccess('商品更新成功');
        },
        onError: handleError,
    });

    const deleteMutation = useMutation({
        mutationFn: productApi.deleteProduct,
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ['products'] });
            handleSuccess('商品删除成功');
        },
        onError: handleError,
    });

    // ✅ 返回稳定的函数引用，而非整个 mutation 对象
    return {
        createProduct: createMutation.mutate,
        updateProduct: updateMutation.mutate,
        deleteProduct: deleteMutation.mutate,
        isCreating: createMutation.isPending,
        isUpdating: updateMutation.isPending,
        isDeleting: deleteMutation.isPending,
    };
}
```

### 2.3 Hook 粒度决策树

```
需要封装逻辑？
    │
    ├── 是 API 调用？
    │   ├── 是查询 → Query Hook (useXxx / useXxxList)
    │   └── 是变更 → Mutation Hook (useXxxOperations)
    │
    ├── 是可复用的状态逻辑？
    │   └── 是 → State Hook (useXxxForm / usePagination)
    │   └── 否 → 直接写在组件中
    │
    └── 是通用工具逻辑？
        └── 基础设施 Hook (use-error-handler.ts)
```

### 2.4 什么逻辑应该留在组件中？

以下逻辑**不应该**抽取为 Hook，应直接写在页面组件中：

1. **UI 状态**：弹窗开关 (`isDialogOpen`)、当前编辑项 (`editingItem`)
2. **UI 协调逻辑**：成功后关闭弹窗、提交后重置表单
3. **页面特有的筛选状态**：如果筛选逻辑不复用，直接用 `useState` 即可
4. **派生数据的简单计算**：如 `const products = data?.items ?? []`

```tsx
// ✅ 正确：UI 相关逻辑留在组件中
export default function ProductsPage() {
    // 领域 Hook
    const { data: products } = useProducts(params);
    const { createProduct, isCreating } = useProductOperations();
    
    // UI 状态：直接写在组件中
    const [isDialogOpen, setIsDialogOpen] = useState(false);
    const [editingProduct, setEditingProduct] = useState<Product | null>(null);
    
    // UI 协调逻辑：直接写在组件中
    const handleCreate = (data: CreateProductRequest) => {
        createProduct(data, {
            onSuccess: () => setIsDialogOpen(false), // UI 行为
        });
    };
    
    return (/* JSX */);
}

// ❌ 错误：把 UI 状态放进 Hook
export function useProductPage() {
    const [isDialogOpen, setIsDialogOpen] = useState(false); // ❌ 这是 UI 状态
    // ...
}
```

---

## 3. Hook 命名规范

### 3.1 命名约定

| Hook 类型 | 命名模式 | 示例 |
|----------|---------|------|
| Query（单条） | `use{Entity}` | `useProduct(id)` |
| Query（列表） | `use{Entities}` | `useProducts(params)` |
| Query（选项） | `use{Entity}Options` | `useRoleOptions()` |
| Mutation | `use{Entity}Operations` | `useProductOperations()` |
| 通用工具 | `use{Action}` | `useErrorHandler()` |

### 3.2 返回值命名约定

```tsx
// Query Hook 返回值：保留 TanStack Query 原始命名
const { data, isPending, isFetching, error } = useProducts(params);

// Mutation Hook 返回值：动词 + 状态
const {
    createProduct,    // mutate 函数用动词
    isCreating,        // isPending 用 is + 动名词
    updateProduct,
    isUpdating,
} = useProductOperations();
```

---

## 4. 页面布局规范

管理台列表页应遵循统一的 **双卡片 (Two-Card) 布局** 模式，以提供清晰的视觉层级和操作流。

### 4.1 结构图示

```
+-------------------------------------------------------+
| Page Header                                           |
| [ Title & Description ]               [ Create Action]|
+-------------------------------------------------------+
|                                                       |
| +---------------------------------------------------+ |
| | Filter Card (筛选区域)                             | |
| | [ Filter Inputs (Grid Layout) ]                  ｜ ｜
| | [ Status Filters ] 
| | ------------------------------------------------- | |
| |                         [ Search & Reset Actions ]| |
| +---------------------------------------------------+ |
|                                                       |
| +---------------------------------------------------+ |
| | Table Card (数据区域)                              | |
| | [ Card Header: Title     Batch Actions (Right) ]  | |
| | [ Data Table ]                                    | |
| | [ Pagination ]                                    | |
| +---------------------------------------------------+ |
|                                                       |
+-------------------------------------------------------+
```

### 4.2 详细规范

1.  **页头 (Page Header)**:
    *   左侧：页面标题 (H1) 和简短描述 (text-muted-foreground)。
    *   右侧：主要的新增/创建操作按钮（如“新建商品”）。

2.  **筛选卡片 (Filter Card)**:
    *   **容器**: 独立的 `Card` 组件。
    *   **表单区域**: 使用 CSS Grid 布局 (`grid-cols-4`) 排列筛选控件（输入框、选择器、滑块等），确保对齐整齐。
    *   状态/类型筛选（如 `HorizontalRadioGroup`）位于分隔线下方左侧。
    *   **分隔线**: 使用 `border-t` 分隔主要筛选条件和辅助操作。
    *   **操作区域**:
        *   **搜索/重置按钮**位于分隔线下方**右侧** (`flex justify-end`)。
    *   **交互原则**: 所有的筛选输入应绑定到本地 state (`filterDraft`)，只有点击“搜索”或“重置”按钮时，才将状态同步到查询 state (`appliedFilters`) 并触发 API 请求。

3.  **表格卡片 (Table Card)**:
    *   **容器**: 独立的 `Card` 组件。
    *   **卡片头 (Card Header)**:
        *   左侧：卡片标题（如“商品列表”）。
        *   右侧：**批量操作按钮组**。
        *   **批量按钮状态**: 默认 `disabled`，仅当表格中有选中行时高亮可用。
    *   **表格内容**: 包含 `DataTable` 和分页控件。

## 5. 代码结构与组件职责

### 5.1 组件职责分离

为了保持页面组件 (`page.tsx`) 的整洁，应将复杂逻辑和子组件剥离：

*   **`page.tsx`**: 负责页面整体布局、状态管理 (State)、数据获取 (Query) 和 变更操作 (Mutation)。
*   **`components/data-table/`**: 通用的表格组件逻辑（已封装）。
*   **`components/xxx-dialog.tsx`**: 所有的弹窗组件（新建、编辑、删除、批量删除等）应独立封装，通过 props 接收 `open`, `onOpenChange` 和 `onSubmit`。
*   **`types/xxx.ts`**: 定义完整的 TypeScript 类型接口。

### 5.2 TanStack Query 与 Mutation 最佳实践

在处理数据变更（增删改）时，遵循以下模式以避免不必要的重渲染和性能问题：

1.  **解构 Mutation**:
    不要直接将 `mutation` 对象传递给依赖项，而是解构出 **稳定** 的 `mutate` 函数和 **状态** 值。

    ```tsx
    // ✅ GOOD
    const { mutate: updateProduct, isPending: isUpdatePending } = useMutation({ ... });

    // ❌ BAD
    const updateMutation = useMutation({ ... });
    ```

2.  **列定义 (Columns) 的 Memoization**:
    表格列定义 `columns` 必须使用 `useMemo` 缓存。在依赖项中，**只包含具体的原始值**（如 `isUpdatePending` 布尔值），**严禁** 包含整个 mutation 对象，否则会导致表格在每次组件重绘时都强制刷新（导致图片闪烁、滚动条跳动等问题）。

    ```tsx
    const columns = useMemo(() => [ ... ], [
        handleEdit,
        handleDelete,
        isUpdatePending, // ✅ 仅依赖布尔值
        // updateMutation // ❌ 严禁依赖整个对象
    ]);
    ```

3.  **回调函数 (Callbacks)**:
    所有传递给子组件或 `useMemo` 的事件处理函数，必须使用 `useCallback` 包裹。

### 5.3 性能优化

1.  **组件 Memoization**:
    对于高频渲染的子组件（如表格单元格中的图片预览、复杂徽标），应使用 `React.memo` 包裹，确保 props 未变时不重绘。

    ```tsx
    // components/image-preview.tsx
    export const ImagePreview = memo(function ImagePreview(...) { ... });
    ```

2.  **DataTable 优化**:
    通用的 `DataTable` 组件也应使用 `React.memo`，防止因父组件无关状态变更（如筛选条件的输入）导致表格重绘。

3.  **图片预览**:
    列表中的图片应封装为独立的 `ImagePreview` 组件，支持点击放大预览，且不影响表格性能。

## 6. 功能交互规范

1.  **筛选触发**:
    *   输入框输入不应立即触发搜索（避免防抖带来的延迟感或无效请求）。
    *   必须通过显式的“搜索”按钮触发查询。

2.  **批量操作**:
    *   表格首列应为 Checkbox 选择列。
    *   批量操作按钮（如上架、删除）应常驻显示，但根据 `selectedRows.length` 自动切换 `disabled` 状态。
    *   批量操作需通过 `Promise.all` 并发执行单条 API，操作完成后一次性刷新列表 (`invalidateQueries`) 并清空选择。

3.  **反馈提示**:
    *   所有 Mutation 操作（提交、删除）期间，按钮应显示 Loading 状态并禁用。
    *   操作完成后应自动关闭弹窗并刷新列表数据。

---

## 7. 常见反模式与修复

### 7.1 反模式：在组件中直接定义所有 Query/Mutation

```tsx
// ❌ 反模式：Query 和 Mutation 直接写在组件中
export default function ProductsPage() {
    const queryClient = useQueryClient();
    
    // ❌ Query 直接定义，无法复用
    const { data } = useQuery({
        queryKey: ['products', params],
        queryFn: () => productApi.getProductList(params),
    });
    
    // ❌ Mutation 直接定义，错误处理逻辑重复
    const { mutate: createProduct } = useMutation({
        mutationFn: productApi.createProduct,
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ['products'] });
            toast.success('创建成功');
        },
        onError: (error) => {
            // 错误处理逻辑在每个页面重复...
            toast.error(error.message);
        },
    });
    
    // ❌ 更多 Mutation，相似代码重复...
    const { mutate: updateProduct } = useMutation({...});
    const { mutate: deleteProduct } = useMutation({...});
    
    return (/* JSX */);
}
```

**问题**：
- Query/Mutation 逻辑无法跨组件复用
- 错误处理、成功提示等逻辑重复
- 难以统一修改（如更改 Toast 样式）

### 7.2 修复：抽取领域 Hook

```tsx
// ✅ hooks/use-products.ts - 领域 Hook
export function useProducts(params: ProductListParams) {
    return useQuery({
        queryKey: ['products', params],
        queryFn: () => productApi.getProductList(params),
    });
}

export function useProductOperations() {
    const queryClient = useQueryClient();
    const { handleError, handleSuccess } = useErrorHandler(); // 统一错误处理
    
    const createMutation = useMutation({
        mutationFn: productApi.createProduct,
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ['products'] });
            handleSuccess('商品创建成功');
        },
        onError: handleError,
    });
    
    // ... 其他 mutations
    
    return {
        createProduct: createMutation.mutate,
        isCreating: createMutation.isPending,
        // ...
    };
}

// ✅ page.tsx - 组件只调用 Hook，处理 UI 逻辑
export default function ProductsPage() {
    // 领域 Hook
    const { data } = useProducts(params);
    const { createProduct, isCreating } = useProductOperations();
    
    // UI 状态留在组件中
    const [isDialogOpen, setIsDialogOpen] = useState(false);
    
    // UI 协调逻辑留在组件中
    const handleCreate = (data: CreateProductRequest) => {
        createProduct(data, {
            onSuccess: () => setIsDialogOpen(false),
        });
    };
    
    return (/* JSX */);
}
```

### 7.3 反模式：Mutation 对象作为依赖

```tsx
// ❌ 反模式：整个 mutation 对象作为依赖
const updateMutation = useMutation({...});

const columns = useMemo(() => [
    // ...列定义
], [updateMutation]); // ❌ 每次渲染 mutation 对象都是新的

// 结果：表格每次都重新渲染，图片闪烁
```

**修复**：

```tsx
// ✅ 修复：解构出稳定的值
const { mutate: updateProduct, isPending: isUpdating } = useMutation({...});

const columns = useMemo(() => [
    // ...列定义
], [isUpdating]); // ✅ 只依赖原始值
```

### 7.4 反模式：在 Hook 中硬编码 UI 行为

```tsx
// ❌ 反模式：Hook 中直接控制 UI
export function useProductOperations() {
    const [dialogOpen, setDialogOpen] = useState(false); // ❌ UI 状态不应在这里
    
    const createMutation = useMutation({
        onSuccess: () => {
            setDialogOpen(false); // ❌ 硬编码 UI 行为
            toast.success('创建成功');
        },
    });
    
    return { dialogOpen, setDialogOpen, createProduct: createMutation.mutate };
}
```

**修复**：

```tsx
// ✅ 修复：Hook 只处理数据，UI 行为由调用方决定
export function useProductOperations() {
    const createMutation = useMutation({
        mutationFn: productApi.createProduct,
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ['products'] });
            handleSuccess('商品创建成功');
        },
        onError: handleError,
    });
    
    return { 
        createProduct: createMutation.mutate,  // 调用方可以传入 onSuccess 覆盖
        isCreating: createMutation.isPending,
    };
}

// 调用方控制 UI 行为
const { createProduct } = useProductOperations();
const handleCreate = (data) => {
    createProduct(data, {
        onSuccess: () => setDialogOpen(false), // ✅ UI 行为在组件层控制
    });
};
```

### 7.5 反模式：过度拆分 Hook

```tsx
// ❌ 反模式：一个状态一个 Hook
export function useProductName() {
    const [name, setName] = useState('');
    return { name, setName };
}

export function useProductEmail() {
    const [email, setEmail] = useState('');
    return { email, setEmail };
}

// 使用时需要调用 10+ 个 Hook
const { name, setName } = useProductName();
const { email, setEmail } = useProductEmail();
// ...
```

**修复**：将关联状态聚合

```tsx
// ✅ 修复：关联状态聚合为一个 Hook
export function useProductFilters() {
    const [filters, setFilters] = useState({
        name: '',
        email: '',
        category: '',
        status: 'all',
    });
    
    const updateFilter = useCallback((field, value) => {
        setFilters(prev => ({ ...prev, [field]: value }));
    }, []);
    
    return { filters, updateFilter, resetFilters };
}
```

---

## 8. 文件组织结构

### 8.1 推荐的目录结构

```
src/
├── app/
│   └── products/
│       ├── page.tsx                    # 页面组件（调用 Hook + UI 状态 + JSX）
│       └── components/
│           ├── product-table.tsx      # 表格组件
│           └── product-dialog.tsx     # 弹窗组件
│
├── hooks/
│   ├── use-error-handler.ts            # 基础设施 Hook（通用）
│   ├── use-toast.ts                    # 基础设施 Hook（通用）
│   │
│   ├── use-products.ts                # 商品领域 Hook
│   │   ├── useProducts()              #   Query: 分页列表
│   │   ├── useProduct(id)             #   Query: 单条详情
│   │   └── useProductOperations()     #   Mutations: CRUD
│   │
│   ├── use-roles.ts                    # 角色领域 Hook
│   │   ├── useRoles()                  #   Query: 角色列表
│   │   └── useRoleOperations()         #   Mutations: CRUD
│   │
│   └── use-consumers.ts                # 消费者领域 Hook
│       └── useConsumers()              #   Query: 消费者列表
│
├── lib/
│   └── api/
│       └── product.ts                 # API 客户端
│
└── types/
    └── product.ts                     # 类型定义
```

**注意**：没有 `use-product-page.ts` 这类页面级 Hook。UI 状态和协调逻辑直接写在 `page.tsx` 中。

### 8.2 Hook 文件内部结构

```tsx
// hooks/use-products.ts

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { productApi } from "@/lib/api";
import type { ProductListParams, UpdateProductRequest } from "@/types";
import { useErrorHandler } from "./use-error-handler";

// ========== Query Hooks ==========

/**
 * 获取商品分页列表
 */
export function useProducts(params: ProductListParams) {
    return useQuery({
        queryKey: ['products', params],
        queryFn: () => productApi.getProductList(params),
        placeholderData: keepPreviousData,
    });
}

/**
 * 获取单个商品详情
 */
export function useProduct(id: string | null) {
    return useQuery({
        queryKey: ['product', id],
        queryFn: () => productApi.getProduct(id!),
        enabled: !!id,
    });
}

// ========== Mutation Hooks ==========

/**
 * 商品 CRUD 操作
 */
export function useProductOperations() {
    const queryClient = useQueryClient();
    const { handleError, handleSuccess } = useErrorHandler();

    const createMutation = useMutation({
        mutationFn: productApi.createProduct,
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ['products'] });
            handleSuccess('商品创建成功');
        },
        onError: handleError,
    });

    const updateMutation = useMutation({
        mutationFn: ({ id, data }: { id: string; data: UpdateProductRequest }) =>
            productApi.updateProduct(id, data),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ['products'] });
            handleSuccess('商品更新成功');
        },
        onError: handleError,
    });

    const deleteMutation = useMutation({
        mutationFn: productApi.deleteProduct,
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ['products'] });
            handleSuccess('商品删除成功');
        },
        onError: handleError,
    });

    return {
        createProduct: createMutation.mutate,
        updateProduct: updateMutation.mutate,
        deleteProduct: deleteMutation.mutate,
        isCreating: createMutation.isPending,
        isUpdating: updateMutation.isPending,
        isDeleting: deleteMutation.isPending,
    };
}
```

---

## 9. 快速检查清单

开发新页面时，对照以下清单检查：

### Hook 设计
- [ ] API 调用（Query/Mutation）是否封装为领域 Hook？
- [ ] Mutation Hook 是否返回稳定的 `mutate` 函数而非整个对象？
- [ ] Hook 是否与 UI 无关、可跨页面复用？
- [ ] Hook 命名是否符合 `use{Entity}` / `use{Entity}Operations` 规范？

### 组件职责
- [ ] UI 状态（弹窗开关等）是否留在组件中？
- [ ] UI 协调逻辑（成功后关闭弹窗）是否留在组件中？
- [ ] 是否避免了创建页面级 Hook（如 `useXxxPage`）？

### 性能优化
- [ ] `useMemo` 依赖项是否只包含原始值？
- [ ] 传递给子组件的函数是否用 `useCallback` 包裹？
- [ ] 高频渲染组件是否用 `React.memo` 包裹？
- [ ] 表格列定义是否用 `useMemo` 缓存？

### 代码组织
- [ ] 弹窗组件是否独立封装？
- [ ] 类型定义是否在 `types/` 目录？

### 交互规范
- [ ] 筛选是否通过显式按钮触发？
- [ ] 批量操作按钮是否根据选中状态禁用？
- [ ] Mutation 期间是否显示 Loading 状态？
- [ ] 操作成功后是否自动刷新列表？

---

## 10. 前端错误处理方案（neverthrow）

本项目采用 **neverthrow** 实现类似 Rust 的错误处理方式。目标是让错误路径显式、可组合，并在 UI 层保持一致的提示体验。

### 10.1 目标

- 使用 `Result/ResultAsync` 显式建模成功/失败，避免隐式 `throw`。
- 统一错误结构，确保后端错误、网络错误、解析错误等可被一致处理。
- 与 TanStack Query 配合：在数据层组合错误，在 UI 边界决定是否抛出。

### 10.2 核心原则

- **API 层不直接 `throw`**，返回 `ResultAsync<T, ApiError>`。
- **UI/Query 边界才做 `throw`**（让 React Query 正确进入 error 状态）。
- **统一错误类型**：所有错误映射到 `ApiError`，避免散落的 `Error` 字符串。
- **提示集中化**：仍使用 `useErrorHandler` 做展示与消息格式化。

### 10.3 统一错误类型（建议结构）

```ts
type ApiErrorKind =
    | "Network"
    | "Http"
    | "Auth"
    | "Parse"
    | "Validation"
    | "Unknown";

export interface ApiError {
    kind: ApiErrorKind;
    message: string;
    status?: number;
    responseData?: unknown;
    cause?: unknown;
}
```

映射规则建议：
- `fetch` 失败 -> `Network`
- HTTP 非 2xx -> `Http`（包含 `status` 与 `responseData`）
- 应用层 `status=401` -> `Auth`
- JSON 解析失败 -> `Parse`
- 业务校验失败（后端返回 errorMessage）-> `Validation`
- 其他未知 -> `Unknown`

### 10.4 分层约定

**lib/api/base.ts**
- 提供 `apiRequestResult`：返回 `ResultAsync<ApiResponse<T>, ApiError>`。
- 旧的 `apiRequest` 可保留一段时间，仅用于历史代码。

**lib/api/*.ts**
- 对外只暴露 `ResultAsync<T, ApiError>`（例如 `login(): ResultAsync<AuthResponse, ApiError>`）。
- 内部统一 `map` 解包 `response.data`。

**hooks（React Query 边界）**
- Query/Mutation 中 **使用辅助函数将 `ResultAsync` 转成 throw**：
  - 成功时 `return data`
  - 失败时 `throw ApiError`
- `onError` 继续交给 `useErrorHandler` 统一提示。

**components**
- 非 React Query 的调用路径，直接用 `match`/`mapErr` 控制 UI 行为。

### 10.5 与 TanStack Query 的推荐姿势

- `queryFn` / `mutationFn` 必须返回真实数据或抛出 `ApiError`。
- 不要把 `Result` 当作成功值返回给 Query，否则 `isError` 永远为 `false`。
- 建议抽一个小工具：
  - `unwrapResult(resultAsync)`：成功返回值，失败抛出 `ApiError`。

### 10.6 迁移策略（渐进式）

1. 在 `lib/api/base.ts` 新增 `apiRequestResult`。
2. 新增一个领域 API（如 `authApi`）先迁移为 `ResultAsync`。
3. 在对应 Hook 处引入 `unwrapResult`，保持 UI 逻辑不变。
4. 逐步替换其他领域 API。

### 10.7 禁止与推荐

- 禁止在 API 层直接 `throw new Error("string")`。
- 禁止在 UI 层随意拼接错误结构。
- 推荐统一使用 `ApiError`，并由 `useErrorHandler` 负责最终提示。

---

## 分阶段并行开发约束

P0 之后以多 worktree 并行开发，本文节与 `docs/dev-plan/` 是本仓库并行开发的唯一仲裁依据。冲突时以 `AGENTS.md` 为准（conventions.md 声明）。

### 实施合同

- 实施合同全部在 `docs/dev-plan/` 目录：
  - `README.md`：阶段与批次总体规划；
  - `conventions.md`：跨阶段统一契约（所有权模型、冻结清单、测试与验收）；
  - `domains.md`：34 个域（D01–D34）清单与模块命名；
  - `_meta.json`：机器可读阶段矩阵（分支命名、owns 前缀、验收门禁）。

### 共享注册文件冻结清单

以下文件**只在 P0 修改**，此后对所有子阶段只读（conventions.md 第 2 节）：

```
backend/entities/src/lib.rs
backend/entities/src/ids.rs
backend/entities/src/money.rs
backend/entities/src/common/**
backend/database/src/lib.rs
backend/database/src/repository/mod.rs
backend/database/src/repository/base.rs
backend/database/src/repository/extensions/mod.rs
backend/database/src/indexes/mod.rs
backend/database/src/executor.rs
backend/database/src/transaction.rs
backend/database/src/mongo_ops.rs
backend/services/src/lib.rs
backend/services/src/errors.rs
backend/services/src/page.rs
backend/services/src/query.rs
backend/apps/web-api/src/main.rs
backend/apps/web-api/src/app_state.rs
backend/apps/web-api/src/core/routes/mod.rs
backend/apps/web-api/src/core/routes/admin.rs
backend/apps/web-api/src/core/handler/mod.rs
backend/apps/web-api/src/core/response.rs
backend/apps/web-api/src/core/errors.rs
backend/apps/web-api/build.rs
erp-client/lib/api/**
erp-client/lib/query-client.ts
```

### 各层 owns 模式

一个子阶段只能修改自己 owns 列表内的文件，新增文件也必须落在 owns 前缀内（conventions.md 第 1 节）：

| 层 | owns |
| --- | --- |
| P1 实体 | `backend/entities/src/<domain>/**` |
| P2 仓储 | `backend/database/src/repository/<domain>.rs`、`repository/extensions/<domain>.rs`、`indexes/<domain>.rs`（**不含** IT） |
| P3 服务与接口 | `backend/services/src/<domain>/**`、`core/handler/<domain>/**`、`core/routes/<domain>.rs`（**不含** IT） |
| P4 前端 | `erp-client/features/<feature>/**` 与该批次页面路由目录 |
| P6 后端集成测试 | `database/tests/<domain>_repository.rs`、`web-api/tests/<domain>_api.rs` 及 `invariants/**`、`concurrency/**` 等 |

### 两段式测试约定

- **域级仓储/HTTP 集成测试只在 P6 编写**，P2/P3 实现阶段不提交，避免与实现漂移。
  目录占位说明见 `database/tests/README.md`、`apps/web-api/tests/README.md`。
- 需要真实 MongoDB（事务需要单节点副本集）的测试统一
  `#[ignore]` + `require_mongo!()`（`ERP_TEST_MONGO_URI` 门控，由
  `backend/crates/test-support` 提供），无数据库环境 `cargo test --workspace`
  必须全绿；P6 / 发布验收执行 `cargo test --workspace -- --include-ignored`。
- 每个测试使用独立随机数据库名并在结束 drop，禁止共享固定库名。
- 本地启动 MongoDB：`backend/scripts/dev-mongo.sh` 或
  `docker compose --profile test up -d mongo`。

### worktree 与分支命名

- 分支命名以 `docs/dev-plan/_meta.json` 的 stages 为准：
  - 阶段 ID 由 `P1/A-G1/B-G1/C-G1/F1/E3/I-G1` 等组成；
  - P1–P3 分支 `feat/erp-<letter>-<batch>-<slug>`（如 `feat/erp-a-g1-platform`）；
  - P4 分支 `feat/erp-f<序号>-<slug>`；P5 分支 `feat/erp-e3-projection` 等；
  - P6 分支 `feat/erp-i-g1-platform` / `feat/erp-i-x1-invariants` 等。
- 一个子阶段一个分支一个 PR；合并前必须 rebase 到最新 `main` 并重跑全部门禁
  （conventions.md 第 9 节）。

### 冻结文件修改流程

- 禁止直接修改冻结清单内的任何文件（含在冻结目录内新增文件）。
- 确需修改时：在 PR 描述中提出，由一次独立的"地基修订 PR"
  （分支 `chore/erp-p0-amend-<主题>`）单独完成并合并，其他 worktree 随后
  rebase；一次地基修订只做一件事。
- 出现共享文件冲突即说明有人越界：**回退越界改动**，不要在 PR 里手工解冲突。
