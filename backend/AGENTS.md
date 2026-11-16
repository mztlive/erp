# 后端仓库规则

通用 Rust 编码规范见 `rust-coding-standards` skill；本文件只写本仓库特有规则，冲突时以本文件为准。

## 效率要求

非我主动要求时，不要使用use computer或者use broswer技能

## 目录结构

```text
backend/
├── apps/
│   ├── erp-server/                   # 服务端组合根；web-api 与 erp-worker 二进制
│   │   ├── build.rs                  # 显式权限目录和 HTTP 路由生成权限
│   │   ├── examples/                 # 历史验收材料（禁新增/修改/执行；不自动构建）
│   │   └── src/
│   │       ├── bootstrap.rs          # 配置、资源与启动初始化
│   │       ├── http/                 # HttpState、handler、routes、JWT、请求任务
│   │       ├── worker/               # 常驻消费、XXL、监督与停止
│   │       ├── wiring/               # Application、跨域 Port 接线与业务能力工厂
│   │       ├── runtime/              # 信号、服务排空与进程观测
│   │       └── indexes.rs            # 全域索引登记
│   └── cli/                          # init-admin、reset-password
├── crates/
│   ├── domains/                     # 19 个业务领域，包名保持 erp-<domain>
│   │   └── erp-<domain>/
│   │       └── src/
│   │           ├── entity/           # 实体、值对象、不变式
│   │           ├── dto/
│   │           ├── service/          # 本域用例
│   │           ├── repository/       # owned/ 集合仓储；extensions/ *Ext 访问器
│   │           ├── ports/            # 本域消费的外域窄 Port
│   │           ├── indexes           # 公开索引入口
│   │           └── error.rs
│   ├── orchestration/                # 按业务职责拆分的跨域编排库
│   ├── read-models/erp-read-models/   # 跨域查询、工作台投影、只读事实
│   ├── adapters/                     # 具体外部协议与技术适配
│   │   └── erp-contract-recognition/
│   ├── foundation/                   # 共享值对象、应用合同与持久化机制
│   │   ├── erp-core/
│   │   ├── application-core/
│   │   └── persistence-core/
│   ├── platform/                     # 无 ERP 业务依赖的技术组件
│   │   ├── bpm/
│   │   ├── storage/
│   │   ├── aliyun-ocr/
│   │   ├── entity-core/
│   │   ├── entity-macros/
│   │   └── id-generator/
│   └── testing/test-support/         # 仅 dev-dependency
├── config/                           # SafeConfig
├── scripts/                          # 边界与权限漂移门禁
└── docs/archive/legacy-crates/       # 旧三层原始字节档案（只读）
```

## 架构与依赖

- 调用链：Handler → 本域 Service 或 Process/ReadModel → 拥有领域 Repository → MongoDB；Handler 禁直连数据库。
- 业务领域的 normal/build/dev 依赖禁指向其他业务领域、编排库、`erp-read-models`；跨域事实经本域 `ports/` 窄 Port 获取，实现由组合层装配。
- 跨域写入放 `crates/orchestration/` 的所属业务库，命名为 Process；跨域读取放 `erp-read-models`。编排可依赖 ReadModel，禁反向；事实权限政策由拥有领域执行。
- 单域查询留在本域 Service/Repository，禁放 ReadModel。
- `erp-core`、`application-core`、`persistence-core` 禁放业务实体（AccountCore、Role、AuditLog、WorkItem、SalesOrder 等），禁依赖业务领域。
- `bpm` 禁依赖 ERP crate、MongoDB、HTTP、配置、ID 生成器、通知客户端；ERP 审批政策、工作项、BPM 持久化归 `erp-workflow`。
- `apps/cli` 复用 `erp-identity` 用例与组合层 adapter，禁依赖 `erp-server`。
- 禁恢复 `entities/`、`database/`、`services/` 旧三层 crate 或转发 façade；`docs/archive/legacy-crates/` 保持原始字节，禁接入 Cargo target。
- 父目录表示架构层次，包名表示职责；只使用 `backend/Cargo.toml` 这一个 workspace，不为分组目录建立 crate 或嵌套 workspace。
- 新增或迁移 crate 同步更新 workspace 成员、`scripts/crate-layers.json` 层次与依赖登记、边界检查配置、`crates/README.md` 与该 crate README；依赖方使用 `workspace = true`。所有活动成员必须有登记，缺失 manifest、源码或层次归属必须使门禁失败。
- 不得恢复旧 `erp-processes`、旧路径别名或兼容 façade；编排间只允许显式登记的单向依赖。具体协议适配不得反向依赖编排或应用。

- `http/` 与 `worker/` 不得相互依赖；`wiring/` 不得依赖 HTTP 类型或启动后台循环。HTTP 专属状态归 `HttpState`，共享资源和业务工厂归 `Application`。

## 新功能落点

1. **实体**：拥有领域 `entity/` 建/扩实体与值对象，封装不变式。
2. **仓储**：拥有领域 `repository/` 新增仓储，经本域 `*Ext` trait 暴露（如 `AccessControlExt::accounts()/roles()`）。
3. **服务**：本域 DTO 与单域用例放拥有领域；跨域写入放 Process，混合读取放 ReadModel。
4. **Handler**：`apps/erp-server/src/http/handler/<domain>/`，返回 `ApiResponse`；请求/响应复用领域或组合层 DTO，禁重复定义同语义结构。仅路径拆分、上下文注入、协议重命名允许薄包装，须实现 `From` 并注释原因。
5. **路由**：新增 `routes/<domain>.rs`，在 `routes/admin.rs` 合并，经 `with_permission` 走 JWT + RBAC；组织机构路由挂在 `routes/access_control.rs`。
6. **索引**：新查询评估索引；在本域 `indexes` 登记，组合根（`apps/erp-server/src/indexes.rs`、`apps/cli/src/indexes.rs`）只调用领域公开入口，保持现有逐集合顺序，禁复制索引定义。
7. **测试与门禁**：按 §测试 补本 crate 单元测试，按 §质量门禁 收尾并按条件矩阵执行对应检查。

## HTTP 与权限

- 管理端接口必须通过 `with_permission` 引用 `apps/erp-server/permission_catalog/` 中的显式权限常量；处理函数不声明权限属性。公开选品、结算主体登录目录等既有例外保持原认证链路。
- 新权限在所属目录模块定义并加入 `ENTRIES`；新模块登记 `MODULES`。新增受保护路由模块同步登记 `build_permissions.rs` 的扫描清单。权限代码只定义一次，复用时保留接口各自的展示说明。
- 前端权限由 `apps/erp-server/build.rs` 生成，禁手改生成物；改动后跑 `./scripts/check-permissions-drift.sh`。
- 配置只经 `config::SafeConfig` 读取；上传只经共享 `Application` 的 `storage::S3Storage` 写入。

## 编码约定

- 导入：禁在代码中写完整路径（如 `erp_workflow::service::...`），顶部 `use` 后用短名，冲突用 `as`；`<Type as Trait>::item` 消歧除外。
- 注释：所有公共方法写多行文档注释，含 `# 参数`、`# 返回`、`# 错误` 段。
- Service：模块只有一个服务时实现写 `mod.rs`，多个时拆文件；查询方法用名词（`role_list`），操作用动词（`create/update/delete`）。
- 方法长度：本次新增或修改的生产方法硬上限 50 有效行（空行和纯注释不计）；handler、领域 `service/repository/entity`、Process、ReadModel 仍优先 30 有效行，超限拆私有 helper。测试、`build.rs`、宏实现除外；禁止为通过全量 `./scripts/check-rust-size.sh` 去顺带拆分未触及的历史方法。
- 文件体积：单个生产源文件不超过 1200 物理行（扣除 `#[cfg(test)]` / `#[test]` / `tests/`）。

## 逻辑归属

| 逻辑                                                | 归属                                  |
| --------------------------------------------------- | ------------------------------------- |
| 不依赖 I/O 的判定：状态、可用性、权限覆盖、范围包含 | 拥有领域的实体/值对象方法             |
| 输入规范化：trim、去重、非空、类型化                | 值对象构造函数；下游只接收值对象      |
| 由实体确定性生成 DTO                                | DTO 的 `from_*` 关联函数              |
| 依赖仓储结果的判断：唯一冲突、关联存在性            | 本域 Service；跨域放 Process          |
| 多步写入、事务边界                                  | 本域 Service；跨域放 Process          |
| 查询条件与分页                                      | Service 组装参数，Repository 实现查询 |

- 纯规则禁写成 Handler、Service、Process 的私有 helper。
- 范例：`Permission::covers`、`RoleIdSet`、`AdminItem::from_account`。

## 事务

- 一个用例一个事务：只在用例入口（本域 Service 公开方法、Process 命令）用 `Transactional::with_transaction` 开启，提交与回滚由它完成。
- 入口以下的 Service、Process 步骤、Port、Repository 只收 `executor: &mut dyn Executor`；禁收 `&mut ClientSession`，禁开启、提交或嵌套事务。
- 业务步骤写在收 executor 的函数里，入口只做 `with_transaction` 包裹；其他用例复用时传入自己的 executor 加入其事务。禁为有无事务复制实现，函数名禁带 `_with_session`、`_in_transaction`。
- 单文档写入传 `&mut NoTransaction`；多文档或多集合需原子时开事务。
- 决定写入的读取（不变式、授权重验、版本、唯一性检查）用同一 executor 在事务内执行。
- 事务只包含必须原子的数据库读写；外部 HTTP、S3、供应商调用、通知禁放在事务内，需与写入保持一致的外部副作用写 outbox，提交后由后台 worker 执行。
- 提交结果未知（`CommitOutcomeUnknown`）时禁自动重放；写命令带幂等键，由调用方凭同一幂等键重试。

## 领域模型

- 实体 `#[serde(flatten)]` 内嵌 `BaseModel`，派生 `Entity`；字段序列化与现有 Mongo 文档一致。
- ID 用 `entity_macros::id_type` 生成的 newtype（`role` 沿用 `RoleId`）。
- 创建/更新入参用独立 `*Data`/`*Update` 结构，与系统字段分离。
- 校验用 `erp_core::validation::non_empty_trimmed` 与 `*_MAX_LEN` 常量；关键字段（如 `category_code`、`parent_category_id`）只许专门方法修改。
- 树形实体用邻接表 `parent_*_id: Option<Id>`（`None` 为根），实体方法拒绝自环；跨节点成环检测与子树移动在 Service 事务内完成。
- 实体测试用 `BaseModel::fake()` 构造；范例 `crates/domains/erp-identity/src/entity/account_core.rs`、`role.rs`。

## 兼容、安全与数据

- 新增字段向后兼容；禁无迁移删/改线上字段；字段/索引变更附迁移脚本与回滚预案。
- 错误用统一结构与稳定语义，新增错误场景写进接口文档；对外变更在 PR 列影响范围、回滚策略、兼容窗口。
- 上传校验大小、扩展名、MIME；对象键限 `key_prefix` 下的安全相对路径。
- 权限拒绝与关键修改记审计日志；登录、上传、公开页等高频敏感接口必须限流。
- 禁全表扫描；唯一性靠唯一索引；过期数据用 TTL 或归档任务。
- 外部 HTTP 统一超时、重试上限与错误分类；资金与状态机变更必须有幂等键或去重。
- 日志与降级错误带 `account`、`request_id` 上下文。

## 测试

- 只写内联单元测试（`mod tests`）；路由/行为变更同步更新 HTTP 层单测。
- 每个改动至少覆盖 happy-path 与失败/边界；按涉及范围覆盖：
  - 输入：非法参数、权限拒绝、空输入/空集合、重复、超长、缺失关联、软删除。
  - 分页：过滤组合、稳定排序、总数一致、空页/尾页/越界页、归组去重。
  - 金额数量：零、正负、精度舍入、溢出守恒，聚合与基准对拍。
  - 状态幂等：全迁移矩阵、幂等重放、异载荷冲突、版本冲突、失败原子性。
- 行为测试必须执行实际生产逻辑，断言业务结果、状态变化或具体错误；禁在测试中复制业务实现后测试复制品。
- 禁用生产源码字符串、方法名、注释或文本位置/计数断言替代业务行为测试；`include_str!` / `include_bytes!` 加载测试数据不受此限制。
- 历史源码结构检查不计入业务覆盖；修改相关模块时评估删除或替换，不因迁移计划允许保留而默认继续维护。已有领域行为测试覆盖的规则，无需为每条文本断言一对一新增测试。
- 架构约束优先由类型、可见性、依赖边界和统一门禁保证，不在业务单元测试中新增方法名或旧 helper 删除检查。
- 编排测试优先复用现有纯函数和窄 Port，以替身执行真实生产编排，验证调用参数、必要顺序、重放零写入和失败停止；不得仅为测试给每个 Service/Repository 新增接口层。替身调用轨迹不能证明真实数据库回滚或并发正确性。
- 覆盖不到须在 PR 说明理由和未验证行为，禁以源码扫描、测试通过数量或“后续补集成测试”代替行为验证。
- 禁新增、修改、执行集成测试：禁跑 `tests/`、`--test`、`--include-ignored`、`examples/` 验收例程及任何依赖真实 MongoDB/外部服务的命令。
- 上传与临时产物不入库，提交前清理大日志。

## 质量门禁

均在 `backend/` 执行。**禁止在日常改动和常规提交前跑 `--workspace` 全量测试、全量 clippy 或未命中的边界脚本**；全量工作区门禁由 CI（`Jenkinsfile`）兜底，仅在跨层大重构或用户显式要求时本地执行。

### 1. 开发内环（编码迭代中）

每轮改动只跑当前受影响 crate，保持统一编译参数以复用增量缓存：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p <crate> --lib [<测试过滤>]
```

- `cargo test -p <crate> --lib` 已编译该 crate 全部生产代码，同一轮禁再叠加 `cargo check` 或 `cargo clippy`；check、test、clippy 各自产出独立编译产物，叠加会把同一改动重复编译。
- 只确认类型编译、暂不跑单测时，才用 `cargo check -p <crate> --lib`。
- 迭代过程中不跑 `cargo fmt`、不跑 `erp-server`、不跑 `scripts/check-*.sh`。

### 2. 任务收尾 / 本地提交门禁（默认只查受影响 crate）

完成功能或修复后，仅针对**本次改动的 `<crate>`** 执行：

```bash
cargo fmt --all -- --check
cargo clippy -p <crate> --lib --bins --locked -- -D warnings
env -u ERP_TEST_MONGO_URI cargo test -p <crate> --lib --locked
git diff --check
```

- **不加 `--all-features` / `--all-targets`**：避免与开发内环产生不同 feature 指纹而触发依赖重编。
- **不单独跑 `cargo check`**：`clippy` 已完整包含类型与编译检查。
- **不默认连带编译 `erp-server`**：除非本次直接修改了 `apps/erp-server`，或修改了上游 crate 暴露给 `erp-server` 的公开签名。

### 3. 条件触发门禁（仅当改动命中下表时追加）

| 本次改动触及的范围                                                               | 追加执行的检查命令                                                                                   |
| -------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| 修改了 crate 的公开类型/trait/函数签名                                           | 对直接下游调用方跑 `cargo check -p <downstream> --lib`（仅当影响面广时跑 `cargo check --workspace --lib`） |
| 修改 `Cargo.toml`、`scripts/crate-layers.json`、跨 crate 依赖或新增跨层数据访问  | `./scripts/check-domain-boundaries.sh --cutover`                                                     |
| 修改 `crates/platform/bpm/` 或 `crates/domains/erp-workflow/`                    | `./scripts/check-bpm-boundaries.sh`                                                                  |
| 修改 `apps/erp-server/permission_catalog/`、`src/http/routes/` 或 `audit_actions.rs` | `./scripts/check-permissions-drift.sh`，并同步提交 `erp-client/lib/*.generated.ts`                   |
| 仅修改文档 / 注释 / 配置示例                                                     | 不编译、不测试，仅跑 `git diff --check`                                                              |

## 运行

```bash
cp config.toml.example config.toml   # 填 app、database、s3
cargo run -p erp-server -- --config-path ./config.toml          # RUST_LOG=info|debug，LOG_FORMAT=json
# 可选：[bootstrap].initial_admin_password 仅在库中还没有 admin 时创建超级管理员，已有账号不改密码
cargo run -p cli -- init-admin --account admin --name "System Admin"
cargo run -p cli -- reset-password --account admin             # 密码：--password > ERP_ADMIN_PASSWORD > 交互输入
```
