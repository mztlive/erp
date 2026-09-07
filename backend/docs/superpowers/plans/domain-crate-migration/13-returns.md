# 阶段 13：退货与逆向流程

## 1. 阶段元数据

| 字段 | 值 |
| --- | --- |
| 阶段 | 13 |
| 状态 | 本地门禁通过 |
| 编制日期 | 2026-09-06 |
| 执行目录 | 仓库内 backend；源路径均相对此目录 |
| 目标 crate | `erp-returns` |
| 执行负责人 | Codex；唯一共享注册集成负责人 |
| 输入/输出提交 | 输入 `5b83e18e`；实现 `453cd48082793b8f40e5afa37d5d225a747fd1b0`；证据以本文件所属提交为准 |
| 依据 | [设计契约](../../specs/2026-09-03-domain-crate-migration-design.md)、[公共执行合同](execution-contract.md) |

## 2. 阶段目标

迁移退货、退款与冲销领域，保留跨销售/采购/财务/库存/履约的逆向原子合同。

## 3. 前置条件

- [阶段 12](12-fulfillment.md) 已验收，证据与提交完整。
- 先阅读 [README](README.md)、公共执行合同及本阶段完整内容。
- 对照 [source-map.tsv](source-map.tsv) 的 phase=13 行、[source-symbols.tsv](source-symbols.tsv) 和本节之后的符号拆分表核验当前输入；当前清单默认归属文件数为 39，其源文件哈希只是编制快照，不是迁移已完成证明。
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

销售退货、采购退货、客户/供应商退款、收款/付款冲销及关联分摊。

范围内文件由本阶段符号表、phase=13 的全量文件清单、必要调用方和已登记的注册文件构成。范围外包括新业务、数据回填、依赖版本升级、生产配置/凭据、发布推送，以及既有集成测试源码。

## 6. 源路径、符号、目标与调用方

| 当前源文件（相对 backend） | 主要符号/选择范围 | 目标文件 | 执行动作 |
| --- | --- | --- | --- |
| `services/src/returns/customer_refund.rs` | `refund` | `crates/erp-processes/src/reverse_flow/customer_refund.rs` | 退款跨领域事务 |
| `services/src/returns/receipt_reversal.rs` | `reversal` | `crates/erp-processes/src/reverse_flow/receipt_reversal.rs` | 收款冲销根流程 |
| `services/src/returns/payment_reversal.rs` | `reversal` | `crates/erp-processes/src/reverse_flow/payment_reversal.rs` | 付款冲销根流程 |
| `services/src/returns/offset_batch.rs` | `offset` | `crates/erp-processes/src/reverse_flow/offset_batch.rs` | 分摊冲销协调 |
| `entities/src/returns/cumulative_limit.rs` | `CumulativeAmountLimit；ensure_within_limit` | `crates/erp-returns/src/entity/returns/cumulative_limit.rs` | 累计上限与禁止重复反转规则 |
| `database/src/repository/returns_posted_totals.rs` | `posted` | `crates/erp-returns/src/repository/returns_posted_totals.rs` | 逆向领域累计事实 |


全量文件以 source-map.tsv 为默认目标；混合文件必须按上表及第 8 节拆符号。source-symbols.tsv 列出各源文件的类型、Trait、函数和声明行；未另行规定的符号名称与行为保持原样。禁止将该清单作为直接批量 mv 脚本。

本阶段还必须迁移 repository-types.tsv 中 owner_phase=13 的全部拥有仓储类型：其 prepare_at 文件由阶段 01 创建，迁到 final_type_definition，并同步 impl_sources 中的所有专用方法。本阶段开始时这些准备文件属于前序输出，不因编制时尚不存在而遗漏。

本阶段必须更新的入口调用方：

- `apps/web-api/src/app_state.rs`。
- `apps/web-api/src/core/handler/returns/mod.rs`。
- `apps/web-api/src/core/routes/returns.rs`。
- `apps/web-api/src/lib.rs`。
- `apps/web-api/src/main.rs`。

此外必须按 source-symbols.tsv 的选定符号用 rg 检索全部生产消费者，并复核 grouped use、类型别名、pub use、impl、宏与 cfg(test) 调用。具体跨域消费者按第 7、8 节处理；每个旧引用必须改为直接目标合同或组合入口，不能新增旧路径转发。

## 7. 目标依赖合同

returns 仅依赖基础；reverse_flow 组合各领域；任何领域均不得反向依赖 returns 的业务服务。

依赖检查覆盖 normal、build、dev 三类边；禁止只检查默认 cargo tree 的正常依赖。没有业务直接依赖也不代表运行时解耦已完成，跨域集合访问与事务调用必须独立检查。

## 8. 按顺序执行的任务清单

1. [x] 固化原正向操作定位、全额/部分反转、累计上限、重复退款/冲销、超额拒绝与版本冲突测试。

2. [x] 迁入 returns 自身实体/DTO/拥有仓储/索引；与财务、履约、采购的关联仅保存原稳定 ID/快照事实。

3. [x] 把 sales_return/purchase_return/customer_refund/supplier_refund/receipt_reversal/payment_reversal 的跨域事务移入 processes::reverse_flow；按原调用顺序委托各域事务内接口。

4. [x] 根回执、审批取消和动作分发统一由逆向流程与 approval_dispatch 接线；每个失败点终止后续步骤，原提交结果未知分类保持不变。

5. [x] 更新 returns Handler、财务/履约摘要和工作台视图，清除旧源码及注册；历史 MongoDB 测试档案原样保留且不执行。

每个实体/仓储/服务迁移单元均按“特征测试 → 公开合同 → 实体 → 仓储 → 服务 → 流程/读模型 → 调用方 → 删除旧实现 → 边界 → 全量门禁”执行。其文件/符号范围取第 6 节与清单，测试取第 10 节，删除范围为已迁实现与旧注册，不包括历史 tests/ 档案。

## 9. 阶段内编译中断规则

- 中间状态只存在于专用 worktree，不合入共享工作分支；阶段结束时整个 workspace 必须恢复可编译。
- 原则上每完成一个可闭合迁移单元即执行对应 crate 与入口窄检查；编译错误必须按事实归属修正，不扩大 public 或把业务类型移入基础层消错。
- 需要改共享注册文件时由本阶段唯一集成负责人完成，不让并行 worktree 交叉修改。
- 日常窄检查命令在目标 crate 创建后执行：

```bash
cargo check -p erp-returns -p web-api -p cli --locked
```

## 10. 测试与质量门禁

- 同一正向操作的累计反转限额、单次/多次/交错部分反转、重复重放与异载荷冲突。
- 销售/财务/采购/库存/履约反向事实守恒；失败轨迹与权限拒绝。
- 退款单据、分摊字段、错误码、BSON 和索引与原结果一致。

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

正向操作识别或反向幂等键变化；重复反转未拒绝；部分回退造成事实不守恒；审批/任务先于业务失败点成功。

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

| 证据 | 已核验结果 | 证据路径（仓库根） |
| --- | --- | --- |
| 输入基线 | 前序 5b83e18e；专用阶段 worktree | .domain-migration-evidence/13/input.json |
| 文件与符号 | 39 个清单旧路径、8 个 owned 准备文件已清零；跨域符号按固定提供方拆分 | source-clearance.json、files.tsv及逐符号报告 |
| 依赖 | normal/build/dev 全依赖闭包无其他业务域或旧三层 | domain-dependencies.json、metadata.json、boundary.log |
| 测试 | 3481 passed、0 failed、68 ignored；31 个历史 tests 档案逐字不变 | test-summary.json、unit-tests.log、historical-tests.json |
| 协议与数据 | changed/missing/added 均为0；原始47条needs_review逐项复核；权限生成物无漂移 | contract-review.json、parser-limitations-review.json、permissions.log |
| 事务合同 | 实际生产调用链、同Executor、原首错/写入顺序和I/O边界逐项复核；真实数据库运行未验证 | idempotency-transaction-review.json及各分片语义报告 |
| 公共门禁 | fmt/check/严格clippy/lib tests/BPM/service/domain/permissions/git diff 全部exit0；516个第三方包版本和校验和不变 | quality-gates.log、dependency-lock.json |
| 编译证据 | 本阶段边界与全workspace编译通过；最终三个性能场景统一在阶段17计时判定 | compile-applicability.json、domain-dependencies.json |
| 阶段提交 | 实现453cd480；证据以本文件提交记录为准；状态最高本地门禁通过 | input.json |

表内未写目录前缀的证据均位于 `.domain-migration-evidence/13/`。原始扫描保留exit2与needs_review；必须结合逐项复核证据使用，不得标为扫描器直接通过。

## 16. 固定调用与证据边界

- `erp-returns` 唯一持有退货退款实体、DTO、状态/金额限额规则、仓储、索引及本域写入；领域不得依赖财务、工作流或旧三层。组合入口为 `reverse_flow::ReturnsProcess`，六详情与三列表唯一归 `returns_center::ReturnsReadService`。
- 销售退货和采购退货保留原首行处理及NO_APPROVAL登记，不补加来源存在性、多行或库存回补行为。四个客户端post入口始终返回原ConflictError，结果类型为Infallible。
- 客户/供应商退款均按原分配规划及批量事实读取回冲结算，逐offset先写、全部成功后写减少分录、再写反向allocation；退款单随后Posted/CAS及审计。原收付款单不因此Reversed，不新增任务或销售刷新。
- 付款冲正先完成全部结算回冲，再按原HashSet迭代同步付款任务，之后写反向分配、原付款Reversed/CAS、冲正单Posted/CAS、审计。回款冲正保持财务回冲、冲正单写入、审计后重新读取分配并逐销售刷新。
- 四commit保留命令receipt优先、原ID/clock时点、源版本先于Posted校验，以及任意事务错误后的已提交结果恢复。普通submit独立保留receipt-first序；仅客户/供应商退款具备原preflight授权replay及最多8次fresh恢复。
- 取消Apply仍按开放任务、runtime、审批任务、本域CAS、审计执行；Replay仍执行本域CAS与审计。授权、绑定、运行时与所有跨域写入复用调用方Executor，不新增内层事务。
- 8个集合与19个索引、四类资金状态混合大小写、Decimal128/时间/Option、来源查询AND及双空查询所有未删除对象均保持原合同。
- 原始扫描exit2与needs_review保留；纯内联测试、替身与源码核验不构成实际MongoDB回滚、并发或未知提交恢复证明。真实数据库运行未验证。
