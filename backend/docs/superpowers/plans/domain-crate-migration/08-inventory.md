# 阶段 08：库存

## 1. 阶段元数据

| 字段 | 值 |
| --- | --- |
| 阶段 | 08 |
| 状态 | 本地门禁通过 |
| 编制日期 | 2026-09-06 |
| 执行目录 | 仓库内 backend；源路径均相对此目录 |
| 目标 crate | `erp-inventory` |
| 执行负责人 | chore/domain-crate-08-inventory integrator |
| 输入/输出提交 | 前序 `bbd8a5ab4cbd45d09b1aeb4d75a83adb5de74dca` / 实现提交见 `.domain-migration-evidence/08/metadata.json` |
| 依据 | [设计契约](../../specs/2026-09-03-domain-crate-migration-design.md)、[公共执行合同](execution-contract.md) |

## 2. 阶段目标

迁移库存余额、流水、预留和盘点，并保持审批过账原子合同。

## 3. 前置条件

- [阶段 07](07-import.md) 已验收，证据与提交完整。
- 先阅读 [README](README.md)、公共执行合同及本阶段完整内容。
- 对照 [source-map.tsv](source-map.tsv) 的 phase=08 行、[source-symbols.tsv](source-symbols.tsv) 和本节之后的符号拆分表核验当前输入；当前清单默认归属文件数为 33，其源文件哈希只是编制快照，不是迁移已完成证明。
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

stock_balance/movement/reservation/adjustment；本阶段不改变库存数量、引用来源或过账时点。

范围内文件由本阶段符号表、phase=08 的全量文件清单、必要调用方和已登记的注册文件构成。范围外包括新业务、数据回填、依赖版本升级、生产配置/凭据、发布推送，以及既有集成测试源码。

## 6. 源路径、符号、目标与调用方

| 当前源文件（相对 backend） | 主要符号/选择范围 | 目标文件 | 执行动作 |
| --- | --- | --- | --- |
| `services/src/inventory/adjustment_post.rs` | `post_stock_adjustment` | `crates/erp-processes/src/inventory_adjustment/post.rs` | 根命令编排；库存写入进 erp-inventory |
| `services/src/inventory/start_approval/prepare.rs` | `prepare` | `crates/erp-processes/src/inventory_adjustment/approval_prepare.rs` | 库存事实与审批绑定适配 |
| `services/src/inventory/cancel_approval.rs` | `cancel` | `crates/erp-processes/src/inventory_adjustment/cancel_approval.rs` | 复用调用方 Executor |
| `database/src/repository/inventory/balance.rs` | `StockBalance` | `crates/erp-inventory/src/repository/inventory/balance.rs` | 库存事实唯一拥有仓储 |
| `entities/src/inventory/approval_snapshot.rs` | `Snapshot` | `crates/erp-inventory/src/entity/inventory/approval_snapshot.rs` | 只持本域快照；工作流格式由 adapter 映射 |


全量文件以 source-map.tsv 为默认目标；混合文件必须按上表及第 8 节拆符号。source-symbols.tsv 列出各源文件的类型、Trait、函数和声明行；未另行规定的符号名称与行为保持原样。禁止将该清单作为直接批量 mv 脚本。

本阶段还必须迁移 repository-types.tsv 中 owner_phase=08 的全部拥有仓储类型：其 prepare_at 文件由阶段 01 创建，迁到 final_type_definition，并同步 impl_sources 中的所有专用方法。本阶段开始时这些准备文件属于前序输出，不因编制时尚不存在而遗漏。

本阶段必须更新的入口调用方：

- `apps/web-api/src/app_state.rs`。
- `apps/web-api/src/core/handler/inventory/mod.rs`。
- `apps/web-api/src/core/routes/inventory.rs`。
- `apps/web-api/src/lib.rs`。
- `apps/web-api/src/main.rs`。

此外必须按 source-symbols.tsv 的选定符号用 rg 检索全部生产消费者，并复核 grouped use、类型别名、pub use、impl、宏与 cfg(test) 调用。具体跨域消费者按第 7、8 节处理；每个旧引用必须改为直接目标合同或组合入口，不能新增旧路径转发。

## 7. 目标依赖合同

inventory 仅依赖基础；审批/审计/责任人能力由 Port/组合层提供；processes 可以依赖 inventory 与 workflow。

依赖检查覆盖 normal、build、dev 三类边；禁止只检查默认 cargo tree 的正常依赖。没有业务直接依赖也不代表运行时解耦已完成，跨域集合访问与事务调用必须独立检查。

## 8. 按顺序执行的任务清单

1. [x] 冻结库存精度、负库存/超量规则、流水与余额一致、预留释放、盘点版本与重复过账测试。

2. [x] 迁入 inventory 实体、DTO、拥有仓储与索引。将跨域来源验证转换为消费方事实 Port；库存原始查询只在本域仓储。

3. [x] 拆 adjustment_submit/start_approval/cancel_approval/adjustment_post：库存状态计算与事务内命令归 inventory，审批绑定、动作调用、WorkItem/审计推进归 processes::inventory_adjustment。

4. [x] 在 ApprovalActionRegistry 中接新库存事务内命令，保持提交/取消/受阻流程与现有错误码；不自行开始第二个事务。

5. [x] 更新 inventory Handler、履约调用方、read-models 的库存/盘点摘要，清理旧引用并执行门禁。

每个实体/仓储/服务迁移单元均按“特征测试 → 公开合同 → 实体 → 仓储 → 服务 → 流程/读模型 → 调用方 → 删除旧实现 → 边界 → 全量门禁”执行。其文件/符号范围取第 6 节与清单，测试取第 10 节，删除范围为已迁实现与旧注册，不包括历史 tests/ 档案。

## 9. 阶段内编译中断规则

- 中间状态只存在于专用 worktree，不合入共享工作分支；阶段结束时整个 workspace 必须恢复可编译。
- 原则上每完成一个可闭合迁移单元即执行对应 crate 与入口窄检查；编译错误必须按事实归属修正，不扩大 public 或把业务类型移入基础层消错。
- 需要改共享注册文件时由本阶段唯一集成负责人完成，不让并行 worktree 交叉修改。
- 日常窄检查命令在目标 crate 创建后执行：

```bash
cargo check -p erp-inventory -p web-api -p cli --locked
```

## 10. 测试与质量门禁

- 数量守恒、重复流水、乐观锁冲突、超量预留/释放、零值和精度边界。
- 审批快照内容、取消资格、重复过账及失败不推进任务；同一执行器传递。

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

新增异步一致性代替现有同事务写入；调整盘点审批时点、数量计算或流水幂等键。

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
| 输入基线 | 前序本地门禁通过 `bbd8a5ab4cbd45d09b1aeb4d75a83adb5de74dca`；分支 `chore/domain-crate-08-inventory`；source-map phase=08 行 33；owned types 5 | 已采集 `.domain-migration-evidence/08/input.json` |
| 文件与符号 | 见 `.domain-migration-evidence/08/files.tsv`；5 owned EntityRepository 迁入 erp-inventory；post/prepare/cancel/submit 根用例迁入 erp-processes/inventory_adjustment；历史 tests/ 字节不变 | 已采集 `.domain-migration-evidence/08/files.tsv` |
| 依赖 | erp-inventory 仅依赖基础；无旧三层回边；组合层允许依赖旧三层 | 已采集 `.domain-migration-evidence/08/boundary.log` |
| 旧实现清零 | 源目录、根导出、调用方搜索：`services::inventory`/`entities::inventory` 生产清零 | 已采集 `.domain-migration-evidence/08/boundary.log` |
| 测试 | `env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked`；3315 passed / 0 failed / 68 ignored；exit 0 | 已执行 `.domain-migration-evidence/08/unit-tests.log` |
| 协议与数据 | 权限生成物与阶段 07 哈希相等；索引迁入 erp-inventory 声明不变 | 已采集 `.domain-migration-evidence/08/contract-comparison.json` |
| 事务合同 | 同一 Executor、inventory 事务内写入、processes 根编排；真实数据库运行未验证 | 已采集 `.domain-migration-evidence/08/transaction-contract.json` |
| 公共门禁 | fmt/check/clippy/lib tests/bpm/service/domain/permissions/git-diff-check 全部 exit 0 | 已执行 `.domain-migration-evidence/08/quality-gates.log` |
| 编译收益 | 阶段 08 不执行编译收益复测；阈值在阶段 17 判定 | 不适用 |
| 阶段提交 | 见 metadata.json；不得标记已验收 | 实现提交待 git |
