# 仓库指南

## 架构概览

- HTTP Handler → 本域 Service 或命名 Process/ReadModel → 拥有领域 Repository → MongoDB。Handler 只做协议适配；纯业务规则属于实体/值对象。
- 实际业务实现由 19 个领域 crate 拥有：identity、audit、workflow、support、party、customer、supplier、catalog、warehouse、contract、import、inventory、finance、sales、procurement、fulfillment、returns、integration、supply。
- `erp-core`、`application-core`、`persistence-core` 只拥有共享基础合同，不得放入 AccountCore、Role、AuditLog、WorkItem、SalesOrder 等业务实体。
- `erp-processes` 拥有跨领域事务、正式用例与消费方适配器；`erp-read-models` 拥有混合查询、展示投影及唯一正式任务事实读取器。允许 Process → ReadModel，不允许反向依赖。
- 业务领域 normal/build/dev 均不得依赖其他业务领域、组合层或旧三层；通过窄 Port 取得外域事实，实际 adapter 在组合层装配。共享规则保留唯一领域提供方。
- `crates/bpm` 只拥有流程定义、运行状态和引擎规则。`erp-workflow` 拥有 ERP 政策、工作项、审批集成及 BPM 持久化适配；BPM 禁止依赖 ERP、MongoDB、HTTP、配置、ID 生成器或通知客户端。
- 入口可直接依赖领域与组合层；CLI 禁止依赖 web-api。所有活动 `entities`、`database`、`services` crate 源码、manifest 和三类依赖入边必须为零。
- HTTP、CLI 和库测试组合根只按阶段00原逐集合顺序登记各领域公开索引入口，不复制索引定义。事务执行器、幂等恢复、ID/时钟位置和外部 I/O 边界必须保持业务合同。
- Handler 使用 `#[permission_macros::permission(...)]` 标注；`apps/web-api/build.rs` 生成前台权限，必须以权限漂移脚本校验，不手改生成物。
- 配置使用 `config::SafeConfig`，Web API 日志与 Tracing 位于 `apps/web-api/src/core/tracing`。

## 项目结构与归属

- `apps/web-api`：Axum HTTP 协议、AppState 装配、后台 worker 启停与生命周期。管理员路由固定走 JWT + RBAC。
- `apps/cli`：现有管理员初始化与密码重置用例的运维装配，复用身份领域和实际消费方 adapter。
- `crates/erp-<domain>`：本域实体、DTO、规则、Service、Repository、collection accessor 和 indexes；不保留旧路径 façade。
- `crates/erp-processes`：命名跨域流程、实际 adapter、事务和外部 I/O 调用边界。
- `crates/erp-read-models`：混合查询、工作台、中心页和展示数据；事实权限政策仍由拥有领域执行。
- `crates/{erp-core,application-core,persistence-core}`：通用值对象、应用和持久化合同；其余技术 crate 保持既有职责。
- `config`、`docs`、`scripts`：配置、执行合同与门禁工具。历史 tests 档案不属于活动 Cargo target。
- 旧三层历史测试与旧文档统一保存在 `docs/archive/legacy-crates/`，只保留原始字节供追溯；禁止恢复顶层 `entities/`、`database/`、`services/`，禁止将归档接入活动 Cargo target。

## 新功能开发流程（后端）

1. **建模**：在拥有领域的 `entity` 模块创建/扩展实体与值对象，封装不变式与验证。
2. **仓储层**：在拥有领域的 `repository` 模块新增仓库，实现实体读写与聚合；通过 `DatabaseExt` 暴露。
3. **服务层**：在拥有领域维护 DTO 和单域用例；跨域写入放入命名 Process，混合读取放入 ReadModel，不得绕过仓库层。
4. **HTTP 层**：在 `apps/web-api/src/core/handler` 新增 handler，默认必须复用 service DTO，禁止重复定义等价请求/响应类型；仅在 HTTP 形态差异时允许最小薄包装并实现 `From/Into`。
5. **路由/权限**：将新接口挂到 `apps/web-api/src/core/routes`；管理员路由必须位于 `admin` 并走 JWT + RBAC；为 handler 添加 `#[permission_macros::permission(...)]`。
6. **测试**：按“测试期望”覆盖维度新增单元测试（至少一个 happy-path，加失败/边界路径，尽量覆盖全面）；不新增集成测试。
7. **检查**：执行 `cargo fmt --all`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features`、`./scripts/check-bpm-boundaries.sh`、`./scripts/check-domain-boundaries.sh`。
8. **回归**：执行 `env -u ERP_TEST_MONGO_URI cargo test --workspace --lib`，确保变更无回归；不执行任何集成测试。

## 编码约定

- Rust 格式遵循 rustfmt（最大宽度 110）；模块 snake_case，类型 CamelCase，常量大写蛇形。
- **导入约定**：不允许在代码中写完整路径引用（例如 `erp_workflow::service::approval::binding::binding_decision`）。所有引用必须在文件顶部通过 `use` 导入后再使用短名；发生命名冲突时允许定义别名（`use ... as ...`）。`use` 语句本身、`mod` 声明、以及必须用路径消歧的 `<Type as Trait>::item` 不受此条约束。
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
- **响应约定**：Handler 默认复用 领域或组合层的 DTO/View 作为响应模型，禁止为同一语义重复定义等价 Response 类型；仅在内部调用链且无敏感字段泄漏风险时可直接传递实体。
- **请求约定**：Handler 必须复用 领域或组合层的 DTO 作为请求体；仅在 HTTP 形态差异（如路径参数拆分、上下文字段注入、协议字段重命名）时允许最小补充包装，并实现 `From/Into`。
- **DTO 复用禁止项**：
  - 禁止在 `apps/web-api/src/core/handler/**` 中定义与 拥有领域或组合层的 DTO 同语义且字段同构的重复 Request/Response 类型。
  - 若确需包装，必须在类型或转换处注释说明 HTTP 形态差异原因，并提供显式转换实现（`From/Into` 或等价实现）。
- **Service 模块组织约定**：如果 service 层只有一个 service 文件，就把代码写到 `mod.rs` 中；只有当 service 层有多个 service 文件需要拆分时，才创建独立 `service.rs`。
- **Service 方法命名约定**：查询类方法使用名词（如 `consumer_list`、`role_list`），操作类方法保持动词（`create`、`update`、`delete`）。
- **流程控制约定**：优先守卫子句（guard clauses），避免深层嵌套 if-else。
- **错误传播约定**：尽量不要使用 `map_err`。能用 `?` 直接传递的错误就用 `?`；需要转换错误类型时，优先通过 `thiserror` 的 `#[from]` 或显式 `From` 实现，而不是在调用点 `map_err`。
- **方法长度约定**：
  - 业务代码方法（`apps/web-api/src/core/handler`、领域 crate 的 `service`、`repository`、`entity` 及命名 Process/ReadModel）应尽量控制在 30 行以内（有效代码行，不含空行与纯注释行）。
  - 超出 30 行时必须拆分私有 helper，保持单一职责和可测试性。
  - 测试代码、`build.rs`、宏实现不受该条约束。
- 使用 `tracing` 输出结构化日志并带上下文字段（id、account、request_id 等）。
- 上传文件必须通过 `AppState` 注入的 `storage::S3Storage` 写入配置的 S3 bucket；公开 URL 必须由 `public_base_url`、`key_prefix` 与对象键生成。

## 类型内聚与下沉编码要求

- **核心原则**：凡是“不依赖数据库/外部 I/O 的业务规则”，优先封装到 拥有领域的实体/值对象或 DTO 自身，不得长期滞留在 组合层或 Service 私有 helper 中。
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
  - 当同类规范化/校验逻辑在 2 个及以上 Service 出现，必须抽取到拥有领域的公共方法或值对象。
  - 禁止在不同 Service 复制粘贴同一套 `normalize_*`、`ensure_*`、`permission_matches` 逻辑。
- **新类型设计要求**：
  - 优先使用显式值对象表达“已规范化输入”（如 `erp_identity::RoleIdSet`）。
  - 提供 `as_slice` / `into_vec` / `to_strings` 等最小必要接口，避免外部重复转换。
- **迁移与验收要求**：
  - 下沉后必须删除原 Service 重复私有 helper，避免双份规则源。
  - 至少补充一条实体/值对象单元测试覆盖该规则（happy-path + 失败路径）。
  - 变更后必须通过：`cargo fmt --all`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features`、`env -u ERP_TEST_MONGO_URI cargo test --workspace --lib`、`./scripts/check-bpm-boundaries.sh`、`./scripts/check-domain-boundaries.sh`；不执行任何集成测试。

## 性能优化约定（社区最佳实践）

- **先度量后优化**：在改性能前先用 `tracing`/指标/基准确认热点；优先解决高频路径与大对象分配。
- **减少不必要分配**：热点路径优先借用 `&str`/`&T`，仅在需要所有权时 `clone`；能用 `Cow`/`Arc` 共享就不要拷贝。
- **避免中间容器**：能用迭代器直接 `collect` 就不要多次 `map`+`collect`；已知大小用 `Vec::with_capacity`。
- **避免隐式 `to_string()`**：能传 `&str` 就不要创建新 `String`；使用 `as_ref`/`as_deref` 降低拷贝。
- **关注 I/O 与查询**：批量查询避免 N+1；MongoDB 使用投影/索引减少传输与反序列化成本。
- **并发与阻塞隔离**：CPU 密集或阻塞 I/O 使用 `spawn_blocking`，避免阻塞 async runtime。

## 事务使用约定

- 事务边界由本域用例或跨域 Process 控制；Repository 不管理事务，只按调用方传入的执行器决定本次操作是否加入事务。
- Repository 的每个方法都接收 `executor: &mut dyn Executor`（`persistence_core::Executor`），不再提供 `_with_session` 重复方法。
- **单集合操作原则**：仅涉及单个集合的 CRUD（无需跨集合保证原子性）时，不需要事务，传入 `&mut NoTransaction`。
- **多集合/多步骤原子性原则**：涉及多个集合的写入/更新/删除，或需要保证原子性的关联操作，必须使用 MongoDB 事务，把事务闭包拿到的 `&mut ClientSession` 作为执行器传入。
- 事务入口来自 `mongodb::Client`，统一使用 `persistence_core::Transactional::with_transaction`（自动 commit/abort）。
- 多步骤写入的 Repository 与 policy 方法（如角色绑定替换、角色规则删除）必须收到事务执行器，注释中已注明该约束。

## 构建、运行与工具

- 初始化配置：`cp config.toml.example config.toml`，填写 `app`、`database` 与 `s3`。
- API：`cargo run -p web-api -- --config-path ./config.toml`（支持 `RUST_LOG=info|debug`、`LOG_FORMAT=json`）。
- CLI：`cargo run -p cli -- init-admin --account admin --name "System Admin"`；`cargo run -p cli -- reset-password --account admin`。密码优先 `--password`，其次环境变量 `ERP_ADMIN_PASSWORD`，否则交互输入。
- Workspace：`cargo build --workspace`、`env -u ERP_TEST_MONGO_URI cargo test --workspace --lib`（只跑单元测试，不执行任何集成测试）。
- 质量门禁：`cargo fmt --all`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features`、`env -u ERP_TEST_MONGO_URI cargo test --workspace --lib`、`./scripts/check-bpm-boundaries.sh`、`./scripts/check-domain-boundaries.sh`；不执行任何集成测试。
- Docker：`./manage.sh start|status|logs` 封装 `docker compose`；仅按 `docker-compose.yml` 只读挂载 `config.toml`，文件对象写入 S3。

## 测试期望

- 单元测试内联（`mod tests`），要求尽量覆盖全面：
  - 每个功能改动至少一个 happy-path 单元测试；路由/行为变更需更新 HTTP 层单元测试。
  - 失败与边界路径：参数校验、权限拒绝、空输入/空集合、重复输入、超长/超限、缺失关联与软删除数据。
  - 查询与分页：过滤组合、稳定排序、总数一致、边界页（空页/尾页/越界页）、批量归组与去重。
  - 金额与数量：零值、正负值、精度、舍入、溢出与守恒；聚合逻辑须与基准算法对拍。
  - 状态与幂等：全部允许/禁止迁移、幂等重放、异载荷冲突、版本冲突与失败原子性。
  - 覆盖不到的维度必须在 PR 描述中说明理由，不得以“后续补集成测试”为由省略。
- 不新增、不修改、不执行任何集成测试：禁止运行各 crate `tests/` 目录、任何 `--test` 目标、`--include-ignored` 或依赖真实 MongoDB / 外部服务的测试命令。
- 上传/临时产物不纳入版本控制，提交前清理大体积日志。

## CI 与质量门禁

- CI 必须执行并通过：`cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features -D warnings`、`env -u ERP_TEST_MONGO_URI cargo test --workspace --lib`、`./scripts/check-bpm-boundaries.sh`、`./scripts/check-domain-boundaries.sh`；CI 不执行任何集成测试。
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

参考 `crates/erp-identity/src/entity/account_core.rs` 与 `crates/erp-identity/src/entity/role.rs` 作为标准实现：

- `AccountCore`：字段规范化、状态与凭证更新规则
- `Role`：字段规范化、系统角色约束和启停规则
