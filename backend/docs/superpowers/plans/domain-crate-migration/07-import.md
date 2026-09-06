# 阶段 07：导入任务

## 1. 阶段元数据

| 字段 | 值 |
| --- | --- |
| 阶段 | 07 |
| 状态 | 本地门禁通过 |
| 编制日期 | 2026-09-06 |
| 执行目录 | 仓库内 backend；源路径均相对此目录 |
| 目标 crate | `erp-import` |
| 执行负责人 | chore/domain-crate-07-import integrator |
| 输入/输出提交 | 前序 `2f77816b06145648ecb11dbe4f151b914457c267` / 实现提交 `8911cb1f466130b565868a8ec9daf087997a6eb3`（证据见 `.domain-migration-evidence/07/metadata.json`） |
| 依据 | [设计契约](../../specs/2026-09-03-domain-crate-migration-design.md)、[公共执行合同](execution-contract.md) |

## 2. 阶段目标

迁移导入批次与确认事实，并将跨领域导入应用过程放入组合层。

## 3. 前置条件

- [阶段 06](06-catalog-warehouse-contract.md) 已验收，证据与提交完整。
- 先阅读 [README](README.md)、公共执行合同及本阶段完整内容。
- 对照 [source-map.tsv](source-map.tsv) 的 phase=07 行、[source-symbols.tsv](source-symbols.tsv) 和本节之后的符号拆分表核验当前输入；当前清单默认归属文件数为 25，其源文件哈希只是编制快照，不是迁移已完成证明。
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

保留已有 batch/row/confirmation/receipt 和 WorkItem 关联行为，不创建新的导入协议或重放策略。

范围内文件由本阶段符号表、phase=07 的全量文件清单、必要调用方和已登记的注册文件构成。范围外包括新业务、数据回填、依赖版本升级、生产配置/凭据、发布推送，以及既有集成测试源码。

## 6. 源路径、符号、目标与调用方

| 当前源文件（相对 backend） | 主要符号/选择范围 | 目标文件 | 执行动作 |
| --- | --- | --- | --- |
| `services/src/legacy_import/mod.rs` | `LegacyImportService` | `crates/erp-import/src/service/legacy_import/mod.rs` | 批次和行状态管理 |
| `services/src/legacy_import/apply_batch.rs` | `apply` | `crates/erp-processes/src/import_apply/batch.rs` | 跨领域事实写入与工作项推进 |
| `services/src/legacy_import/confirmation/complete.rs` | `complete` | `crates/erp-processes/src/import_apply/confirmation.rs` | 确认完成的原子编排 |
| `entities/src/legacy_import/import_row_factory.rs` | `Import` | `crates/erp-import/src/entity/legacy_import/import_row_factory.rs` | 导入行自身规则保留；业务实体构造转换按符号拆到 processes |
| `database/src/repository/legacy_import/supersede_batch.rs` | `WorkItem` | `crates/erp-processes/src/import_apply/supersede.rs` | WorkItem 更新委托 workflow；导入状态方法留 import 仓储 |


全量文件以 source-map.tsv 为默认目标；混合文件必须按上表及第 8 节拆符号。source-symbols.tsv 列出各源文件的类型、Trait、函数和声明行；未另行规定的符号名称与行为保持原样。禁止将该清单作为直接批量 mv 脚本。

本阶段还必须迁移 repository-types.tsv 中 owner_phase=07 的全部拥有仓储类型：其 prepare_at 文件由阶段 01 创建，迁到 final_type_definition，并同步 impl_sources 中的所有专用方法。本阶段开始时这些准备文件属于前序输出，不因编制时尚不存在而遗漏。

本阶段必须更新的入口调用方：

- `apps/web-api/src/app_state.rs`。
- `apps/web-api/src/core/handler/legacy_import/mod.rs`。
- `apps/web-api/src/core/routes/legacy_import.rs`。
- `apps/web-api/src/lib.rs`。
- `apps/web-api/src/main.rs`。

此外必须按 source-symbols.tsv 的选定符号用 rg 检索全部生产消费者，并复核 grouped use、类型别名、pub use、impl、宏与 cfg(test) 调用。具体跨域消费者按第 7、8 节处理；每个旧引用必须改为直接目标合同或组合入口，不能新增旧路径转发。

## 7. 目标依赖合同

import 不依赖 workflow/support/业务目标领域；跨域应用和替代事务由 processes 组合；read-models 组装导入任务摘要。

依赖检查覆盖 normal、build、dev 三类边；禁止只检查默认 cargo tree 的正常依赖。没有业务直接依赖也不代表运行时解耦已完成，跨域集合访问与事务调用必须独立检查。

## 8. 按顺序执行的任务清单

1. [x] 固化分行成功/失败、部分失败重试、确认版本、重复回执、批次替代和汇总守恒测试。

2. [x] 迁入 import 自身实体/DTO/仓储/索引；将直接返回其他域聚合的 factory 拆成导入事实 DTO 与组合层转换，保持字段与错误相同。

3. [x] 把 apply_batch、execution 及 confirmation/complete 的跨域写入拆到 processes::import_apply；通过已迁移领域接口或旧 services 的事务内接口复用同一个 Executor。

4. [x] supersede_batch.rs 中对 WorkItem 的方法归属 workflow 仓储，由组合层执行，禁止 import 仓储继续直接写任务集合。

5. [x] 更新 legacy_import Handler、工作台导入摘要和批任务调用方；移除旧源码及模块声明。仅运行纯内联测试和公共门禁。

每个实体/仓储/服务迁移单元均按“特征测试 → 公开合同 → 实体 → 仓储 → 服务 → 流程/读模型 → 调用方 → 删除旧实现 → 边界 → 全量门禁”执行。其文件/符号范围取第 6 节与清单，测试取第 10 节，删除范围为已迁实现与旧注册，不包括历史 tests/ 档案。

## 9. 阶段内编译中断规则

- 中间状态只存在于专用 worktree，不合入共享工作分支；阶段结束时整个 workspace 必须恢复可编译。
- 原则上每完成一个可闭合迁移单元即执行对应 crate 与入口窄检查；编译错误必须按事实归属修正，不扩大 public 或把业务类型移入基础层消错。
- 需要改共享注册文件时由本阶段唯一集成负责人完成，不让并行 worktree 交叉修改。
- 日常窄检查命令在目标 crate 创建后执行：

```bash
cargo check -p erp-import -p web-api -p cli --locked
```

## 10. 测试与质量门禁

- 同批次部分失败、同幂等键重放、异载荷/版本冲突、有效确认替代、数量守恒。
- Port 替身证明业务写入失败后不推进确认和 WorkItem；事务上下文不替换。

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

导入仓储继续持有 WorkItem 固有 impl；重试范围扩大；替代批次使已有成功事实被重复应用。

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
| 输入基线 | 前序本地门禁通过 `2f77816b06145648ecb11dbe4f151b914457c267`；分支 `chore/domain-crate-07-import`；source-map phase=07 行 25；owned types 3 | 已采集 `.domain-migration-evidence/07/input.json` |
| 文件与符号 | 相对输入 71 路径变化（18 add / 2 delete / 25 modify / 26 rename）；3 owned EntityRepository 迁入 erp-import；apply/execution/confirmation/supersede 根用例迁入 erp-processes/import_apply；历史 tests/ 字节不变 | 已采集 `.domain-migration-evidence/07/files.tsv` |
| 依赖 | 29 个成员；无 kind=test；erp-import 不依赖 workflow/support/业务领域且无旧三层回边；组合层允许依赖旧三层 | 已采集 `.domain-migration-evidence/07/boundary.log` |
| 旧实现清零 | 源目录、根导出、#[path]/include 路径、调用方搜索命令及结果 | 已采集 `.domain-migration-evidence/07/boundary.log` |
| 测试 | `env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked`；3308 passed / 0 failed / 68 ignored；exit 0 | 已执行 `.domain-migration-evidence/07/unit-tests.log` |
| 协议与数据 | 369 条管理路由；21 个 ErrorCode；368 条索引；权限生成物与阶段 06 哈希相等 | 已采集 `.domain-migration-evidence/07/contract-comparison.json` |
| 事务合同 | Executor/NoTransaction、snapshot+majority、erp-processes run_audited 与 import_apply 同一 Executor；真实数据库运行未验证 | 已采集 `.domain-migration-evidence/07/transaction-contract.json` |
| 公共门禁 | fmt/check/clippy/test/bpm/service/domain/permissions/git-diff-check 全部 exit 0；review_approved=true；状态为本地门禁通过 | 已执行 `.domain-migration-evidence/07/quality-gates.log` |
| 编译收益 | 阶段 07 不执行编译收益复测；阈值在阶段 17 判定 | 不适用 |
| 阶段提交 | 实现提交 `8911cb1f466130b565868a8ec9daf087997a6eb3`；证据目录 `.domain-migration-evidence/07/`；review_approved=true；状态本地门禁通过；禁止标记已验收 | 已写入 `.domain-migration-evidence/07/metadata.json` |
