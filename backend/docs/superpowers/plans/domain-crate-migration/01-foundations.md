# 阶段 01：公共基础与仓储类型解耦

## 1. 阶段元数据

| 字段 | 值 |
| --- | --- |
| 阶段 | 01 |
| 状态 | 本地门禁通过 |
| 编制日期 | 2026-09-06 |
| 执行目录 | 仓库内 backend；源路径均相对此目录 |
| 目标 crate | `erp-core`, `application-core`, `persistence-core` |
| 执行负责人 | 本阶段唯一集成负责人（分支 `chore/domain-crate-01-foundations`） |
| 输入/输出提交 | 前序 `7640f7b916891225ff19f5f1e1148b8a093fe0ff` / 证据见 `.domain-migration-evidence/01/` |
| 依据 | [设计契约](../../specs/2026-09-03-domain-crate-migration-design.md)、[公共执行合同](execution-contract.md) |

## 2. 阶段目标

抽离稳定共享值、应用合同与 MongoDB 机械能力，并消除 Repository 固有实现跨 crate 限制。

## 3. 前置条件

- [阶段 00](00-baseline.md) 已验收，证据与提交完整。
- 先阅读 [README](README.md)、公共执行合同及本阶段完整内容。
- 对照 [source-map.tsv](source-map.tsv) 的 phase=01 行、[source-symbols.tsv](source-symbols.tsv) 和本节之后的符号拆分表核验当前输入；当前清单默认归属文件数为 23，其源文件哈希只是编制快照，不是迁移已完成证明。
- 前序阶段已改变公共类型时，以前序验收提交为实际输入，登记相应路径/签名变化；禁止自动重生成清单来隐藏未经审核的漂移。
- 进入专用 worktree；重叠未提交工作已明确归属。清单与当前源不符时先校正计划，不覆盖他人修改。

## 4. 不可变业务合同

- HTTP 路径/方法、DTO 字段及序列化、错误码/状态码、RBAC 和数据范围保持不变。
- collection、BSON 类型、索引名称/键/唯一性、金额/数量/时间、幂等键与回执保持不变。
- 跨领域原子流程复用同一个 Executor；领域事务内方法不得另开事务，外部 I/O 不得持有 session。
- 不创建事件总线、微服务、双写、兼容 façade 或第二份业务实现。
- 当前仓库只执行纯内联单元测试；历史 tests/ 原样保留，不作为迁移中的 Cargo target，不启动真实 MongoDB。
- 共用的资料、附件、审计、审批编排一旦迁入 processes，所有仍调用它的旧 services 外层用例必须在同阶段上移到对应命名流程；旧 services 只保留可被调用的单域/事务内接口，不得反向依赖 processes。
- 本阶段外的代码仅允许进行为已迁符号编译所必需的导入、类型和装配更新；新增业务变化必须独立处理。

## 5. 范围内与范围外事项

覆盖三个基础 crate、id-generator、所有旧仓储类型和公共调用点。按 repository-types.tsv 逐类型执行；本阶段是较大规模基础改造，不能只移动 base.rs。

范围内文件由本阶段符号表、phase=01 的全量文件清单、必要调用方和已登记的注册文件构成。范围外包括新业务、数据回填、依赖版本升级、生产配置/凭据、发布推送，以及既有集成测试源码。

## 6. 源路径、符号、目标与调用方

| 当前源文件（相对 backend） | 主要符号/选择范围 | 目标文件 | 执行动作 |
| --- | --- | --- | --- |
| `entities/src/money.rs` | `Amount；Quantity；UnitPrice；Rate；MoneyVisitor；D128BytesSeed；line_amounts` | `crates/erp-core/src/money.rs` | 保留现有 serde、精度、舍入和 Decimal128 实现 |
| `entities/src/ids.rs` | `id_type!` | `crates/erp-core/src/ids.rs` | 迁入既有稳定 ERP ID；BPM 七类 ID 继续只定义在 bpm |
| `entities/src/common/time.rs` | `BusinessDate；Instant` | `crates/erp-core/src/common/time.rs` | 与 common、validation、field_update、通用值错误一起迁移 |
| `entities/src/command.rs` | `CommandFingerprint；CommandIdentity；CommandReceipt；CommandReceiptFact；CommandReceiptMatch` | `crates/application-core/src/command.rs` | 只迁纯合同；收据持久化留审计/流程适配 |
| `entities/src/account_core.rs` | `AccountKind；InvalidAccountKind` | `crates/erp-core/src/identity.rs` | 仅拆稳定操作人类别；AccountCore/Secret 不迁入基础层 |
| `services/src/audit/mod.rs` | `AuditActor` | `crates/application-core/src/context.rs` | 仅迁身份数据、new/id/kind；resource_log 系列留领域审计适配 |
| `services/src/page.rs` | `Page` | `crates/application-core/src/page.rs` | 连同 query.rs 与 owned_task.rs 迁移 |
| `services/src/errors.rs` | `ErrorClass；Error；ErrorCode；duplicate_index_conflict_message` | `crates/application-core/src/error.rs` | 仅建立稳定应用分类；数据库来源与领域索引消息不整体上移 |
| `database/src/repository/base.rs` | `Repository；QueryFilter；Pagination；PageResult` | `crates/persistence-core/src/repository/base.rs` | 泛型机械能力迁移；按类型清单先创建本地领域仓储 |
| `database/src/executor.rs` | `Executor；NoTransaction` | `crates/persistence-core/src/executor.rs` | 与 connection、transaction、mongo_ops、errors、regex_filter 同阶段迁移 |
| `crates/id-generator/src/document_number.rs` | `DocumentNumberGenerator` | `crates/id-generator/src/document_number.rs` | 使用 persistence-core 执行器与原取号算法 |
| `scripts/check-bpm-boundaries.sh` | `ApprovalSubjectSnapshotId；ApprovalNotificationOutboxId` | `scripts/check-bpm-boundaries.sh` | ID 权威路径更新到 erp-core；同阶段更新所有编译调用方 |


全量文件以 source-map.tsv 为默认目标；混合文件必须按上表及第 8 节拆符号。source-symbols.tsv 列出各源文件的类型、Trait、函数和声明行；未另行规定的符号名称与行为保持原样。禁止将该清单作为直接批量 mv 脚本。

本阶段还必须迁移 repository-types.tsv 中 owner_phase=01 的全部拥有仓储类型：其 prepare_at 文件由阶段 01 创建，迁到 final_type_definition，并同步 impl_sources 中的所有专用方法。本阶段开始时这些准备文件属于前序输出，不因编制时尚不存在而遗漏。

本阶段必须更新的入口调用方：

- `apps/cli/src/main.rs`。
- `apps/web-api/src/app_state.rs`。
- `apps/web-api/src/core/errors.rs`。
- `apps/web-api/src/lib.rs`。
- `apps/web-api/src/main.rs`。

此外必须按 source-symbols.tsv 的选定符号用 rg 检索全部生产消费者，并复核 grouped use、类型别名、pub use、impl、宏与 cfg(test) 调用。具体跨域消费者按第 7、8 节处理；每个旧引用必须改为直接目标合同或组合入口，不能新增旧路径转发。

## 7. 目标依赖合同

erp-core 不依赖业务、应用、MongoDB 驱动；application-core 不依赖持久化或业务；persistence-core 不依赖旧三层或领域；id-generator → persistence-core。旧三层可依赖新基础。新基础不依赖 test-support，避免 test-support → entities 的回边。

依赖检查覆盖 normal、build、dev 三类边；禁止只检查默认 cargo tree 的正常依赖。没有业务直接依赖也不代表运行时解耦已完成，跨域集合访问与事务调用必须独立检查。

## 8. 按顺序执行的任务清单

1. [x] 先冻结 money.rs 现有字符串与 Decimal128 编解码、状态迁移错误、业务时间和命令指纹测试结果。记录 database/executor.rs、transaction.rs 与 mongo_ops.rs 的执行器/错误传播合同；不得改变重试与 OutcomeUnknown 语义。

2. [x] 创建三个有实际实现的 crate，统一用 workspace.dependencies。erp-core 依赖 entity-core/entity-macros 与现有 serde/定点数库；application-core 只依赖纯基础；persistence-core 依赖纯基础与 MongoDB。金额编解码按设计第 4.2 节保持，不跨 crate 实现外部 serde Trait。

3. [x] 移动 pure common、money、ID、validation、field_update、通用值错误；从 account_core.rs 只提取稳定 AccountKind。同步所有 entities::money/common/ids/FieldUpdate/通用 Error 引用，删除旧声明与 re-export；实体业务模块继续留在旧 entities。

4. [x] 移动 Page、SortDir、PageView、查询归一化及 owned_task；CommandReceipt 只含纯收据算法。把 AuditActor 的数据和身份访问器放入 application-core，其生成 AuditLog 的方法改为审计侧消费 AuditActor 的函数或本地 Trait。稳定 AppError 不包含旧 entities/database 错误或审批业务枚举。

5. [x] 按 repository-types.tsv 给每个实体建立 <Entity>Repository<'a> 本地拥有类型，准备路径为 database/src/repository/owned/<entity>.rs；领域专用 impl 全部改为该类型。该类型组合 persistence_core::Repository<'a, Entity>，本地 query/command 方法是唯一业务实现；禁止 type alias 或 Deref 暴露全部底层能力。同名实体的分散 impl 必须使用同一个拥有类型。

6. [x] 同步全部 extensions/<domain>.rs 的关联类型、工厂返回值、构造器和参数类型；保留原 db.<accessor>() 方法名。只代理实际需要的通用 CRUD，Mongo 句柄只允许 Repository 层使用。base.rs 的 list_work_item_brief_entities_by_ids 等消费方专用命名改成通用事实投影原语或迁到读模型仓储，禁止基础层接入 WorkItem 类型。

7. [x] 移动连接、执行器、事务和 Mongo 操作实现，更新 id-generator 的 manifest、lib.rs 和 document_number.rs。旧 database 仅保留业务仓储、领域扩展与索引聚合，不保留基础模块兼容转发。services::Error 临时对新基础错误做显式稳定映射；未知索引提示保留原结果。

8. [x] 在同一阶段更新所有生产调用方、内联测试导入、#[path] 与 include_str! 路径；历史 tests/ 不修改且不作为 target。补齐基础 crate 的 BSON 纯内存测试依赖，执行基础/旧三层/入口窄检查与公共门禁；更新 BPM ID 检查路径。

每个实体/仓储/服务迁移单元均按“特征测试 → 公开合同 → 实体 → 仓储 → 服务 → 流程/读模型 → 调用方 → 删除旧实现 → 边界 → 全量门禁”执行。其文件/符号范围取第 6 节与清单，测试取第 10 节，删除范围为已迁实现与旧注册，不包括历史 tests/ 档案。

## 9. 阶段内编译中断规则

- 中间状态只存在于专用 worktree，不合入共享工作分支；阶段结束时整个 workspace 必须恢复可编译。
- 原则上每完成一个可闭合迁移单元即执行对应 crate 与入口窄检查；编译错误必须按事实归属修正，不扩大 public 或把业务类型移入基础层消错。
- 需要改共享注册文件时由本阶段唯一集成负责人完成，不让并行 worktree 交叉修改。
- 日常窄检查命令在目标 crate 创建后执行：

```bash
cargo check -p erp-core -p application-core -p persistence-core -p web-api -p cli --locked
```

## 10. 测试与质量门禁

- JSON 金额字符串、BSON wire Decimal128、从 Document/字节反序列化、拒绝浮点输入、负数/零/精度/舍入与原测试等价。
- 逐实体拥有仓储可通过其领域方法编译；不得出现 E0116、E0117 或跨 crate 私有字段访问。
- Executor 对 ClientSession 的同一实例传递，未知提交结果与瞬态冲突分类不变；纯测试不执行真实事务。
- 命令指纹、候选收据 ID 顺序、异载荷冲突、操作人检查及 HTTP 错误消息黄金值不变。

纯测试必须验证行为、数据形态或失败语义；不以源码字符串包含检查替代领域行为断言。既有 include_str! 结构检查可以保留并更新正确路径。

从 backend 运行；先用一次受控 cargo check --workspace 更新本地 path crate 的锁文件，审查第三方版本未变，再执行 --locked 门禁。

```bash
set -e
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked
./scripts/check-bpm-boundaries.sh
./scripts/check-service-boundaries.sh
./scripts/check-domain-boundaries.sh
./scripts/check-permissions-drift.sh
git diff --check
```

公共门禁全部通过才允许进入“本地门禁通过”；填完证据并形成完整阶段提交后进入“已验收”。不得把未执行命令标为通过。

## 11. 事务与持久化验收边界

- 使用纯内联测试、Port 替身和调用记录验证同一 Executor、原写入顺序、错误传播、幂等判断及外部 I/O 分离。
- 使用内存内 JSON/BSON 序列化和索引定义比较，检查原字段类型、Decimal128、索引键/选项和唯一性合同。
- 本阶段不运行真实数据库或其他外部服务测试。证据中固定记录“真实数据库运行未验证”；不将编译或替身测试记作真实回滚/并发测试。

## 12. 暂停条件

任一专用 Repository impl 仍属于外部泛型类型；Amount 存储不再为 Decimal128；公共 AppError 引入业务或持久化实现依赖；ID 出现第二定义源。

此外，任何业务/HTTP/权限/数据库/幂等行为变化、测试无法证明关键行为、来源不明的重叠修改或无法复用 Executor 都必须暂停。先保留失败证据并标记“阻塞”，不得关闭规则、扩大债务基线或跳过失败测试。

## 13. 回退步骤

1. 保存当前阶段文件差异、失败日志和输入提交；停止后续阶段。
2. 未合入阶段在专用 worktree 回到其输入版本或废弃该阶段分支，保留失败证据；不得对共享脏工作区执行 reset --hard/clean。
3. 已合入阶段以完整阶段提交为单位 git revert；阶段包含多个提交时一并回退，禁止只回退某一层或 Cargo 注册。
4. 恢复上一阶段所有权/阶段状态，重跑上一阶段公共门禁。外部环境和数据没有因本迁移被修改，不执行数据回填或恢复脚本。

## 14. 完成判据

- 第 8 节任务已完成；第 6 节及全量清单已核销，混合文件无遗漏符号或第二实现。
- 本阶段依赖约束及旧引用清零通过；必需入口、内联测试与公开错误映射均接入新实现。
- 第 10 节门禁与第 11 节范围内验证通过；第 15 节证据齐全，状态和提交一致。
- 适用的编译复测已完成并记录真实结果；最终性能阈值按阶段 17 判定，文档编制不构成阶段验收。

## 15. 结构化验收证据

| 证据 | 必填结果 | 初始状态 |
| --- | --- | --- |
| 输入基线 | 前序已验收 commit；阶段分支；源映射核对 | 已采集 `.domain-migration-evidence/01/input.json` |
| 文件与符号 | 新增/修改/删除列表；source-map 核销；跨域符号归属 | 已采集 `.domain-migration-evidence/01/files.tsv` |
| 依赖 | metadata/tree；normal/build/dev 边；无环与旧引用结果 | 已采集 `.domain-migration-evidence/01/boundary.log` |
| 旧实现清零 | 源目录、根导出、#[path]/include 路径、调用方搜索命令及结果 | 已采集：`database/src/repository/base.rs` 删除；无 `impl Repository<'a, Entity>` |
| 测试 | 内联测试命令、退出码、通过/失败/忽略数量、关键断言 | 3262 passed / 0 failed / 71 ignored；见 `unit-tests.log` |
| 协议与数据 | HTTP/DTO/错误/权限、JSON/BSON、索引对比 | 已采集 `contract-comparison.json`（369/21/368/154/32） |
| 事务合同 | 同一 Executor、调用顺序、失败传播、I/O 边界；真实数据库运行未验证 | 已采集 `transaction-contract.json`；真实数据库运行未验证 |
| 公共门禁 | 每条命令、工具版本、退出码和日志路径 | 全部 exit 0；见 `quality-gates.log` |
| 编译收益 | 适用场景原始样本、Fresh/Dirty、timings、中位数与改善率；不适用须写明 | 本阶段不适用；阈值在阶段 17 判定 |
| 阶段提交 | commit hash、范围、验收日期及验收人 | 本地门禁通过，未验收 |
