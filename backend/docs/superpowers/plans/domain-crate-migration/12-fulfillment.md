# 阶段 12：履约

## 1. 阶段元数据

| 字段 | 值 |
| --- | --- |
| 阶段 | 12 |
| 状态 | 本地门禁通过 |
| 编制日期 | 2026-09-06 |
| 执行目录 | 仓库内 backend；源路径均相对此目录 |
| 目标 crate | `erp-fulfillment` |
| 执行负责人 | Codex；唯一共享注册集成负责人 |
| 输入/输出提交 | 输入 `cf89fb47`；实现 `de112614e03714bb6d1ca4ac9e25f580c5c273c8`；证据以本文件所属提交为准 |
| 依据 | [设计契约](../../specs/2026-09-03-domain-crate-migration-design.md)、[公共执行合同](execution-contract.md) |

## 2. 阶段目标

迁移履约事实与资格规则，并保持入库、发货、电子交付、服务履约和客户验收的原子过程。

## 3. 前置条件

- [阶段 11](11-procurement.md) 已验收，证据与提交完整。
- 先阅读 [README](README.md)、公共执行合同及本阶段完整内容。
- 对照 [source-map.tsv](source-map.tsv) 的 phase=12 行、[source-symbols.tsv](source-symbols.tsv) 和本节之后的符号拆分表核验当前输入；当前清单默认归属文件数为 45，其源文件哈希只是编制快照，不是迁移已完成证明。
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

实物/服务/电子履约和客户验收归 erp-fulfillment；涉及库存、采购、销售与任务的事务由 processes 编排。

范围内文件由本阶段符号表、phase=12 的全量文件清单、必要调用方和已登记的注册文件构成。范围外包括新业务、数据回填、依赖版本升级、生产配置/凭据、发布推送，以及既有集成测试源码。

## 6. 源路径、符号、目标与调用方

| 当前源文件（相对 backend） | 主要符号/选择范围 | 目标文件 | 执行动作 |
| --- | --- | --- | --- |
| `services/src/fulfillment/delivery_posting.rs` | `delivery` | `crates/erp-processes/src/fulfillment_execution/delivery.rs` | 发货跨库存/销售/任务流程 |
| `services/src/fulfillment/customer_acceptance_posting.rs` | `acceptance` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance.rs` | 验收与销售/任务推进 |
| `services/src/fulfillment/service_fulfillment_confirm.rs` | `confirm` | `crates/erp-processes/src/fulfillment_execution/service_confirm.rs` | 服务履约确认根事务 |
| `services/src/fulfillment/electronic_delivery.rs` | `electronic` | `crates/erp-processes/src/fulfillment_execution/electronic_delivery.rs` | 跨域根编排；密文与本域事实规则留 fulfillment |
| `entities/src/fulfillment/acceptance_eligibility.rs` | `Eligibility` | `crates/erp-fulfillment/src/entity/fulfillment/acceptance_eligibility.rs` | 纯资格与累计数量规则 |
| `database/src/repository/fulfillment/purchase_receipt_totals.rs` | `totals` | `crates/erp-fulfillment/src/repository/fulfillment/purchase_receipt_totals.rs` | 纯本域入库累计事实 |


全量文件以 source-map.tsv 为默认目标；混合文件必须按上表及第 8 节拆符号。source-symbols.tsv 列出各源文件的类型、Trait、函数和声明行；未另行规定的符号名称与行为保持原样。禁止将该清单作为直接批量 mv 脚本。

本阶段还必须迁移 repository-types.tsv 中 owner_phase=12 的全部拥有仓储类型：其 prepare_at 文件由阶段 01 创建，迁到 final_type_definition，并同步 impl_sources 中的所有专用方法。本阶段开始时这些准备文件属于前序输出，不因编制时尚不存在而遗漏。

本阶段必须更新的入口调用方：

- `apps/web-api/src/app_state.rs`。
- `apps/web-api/src/core/handler/fulfillment/mod.rs`。
- `apps/web-api/src/core/routes/fulfillment.rs`。
- `apps/web-api/src/lib.rs`。
- `apps/web-api/src/main.rs`。

此外必须按 source-symbols.tsv 的选定符号用 rg 检索全部生产消费者，并复核 grouped use、类型别名、pub use、impl、宏与 cfg(test) 调用。具体跨域消费者按第 7、8 节处理；每个旧引用必须改为直接目标合同或组合入口，不能新增旧路径转发。

## 7. 目标依赖合同

fulfillment 不依赖 inventory/procurement/sales/support/workflow；流程在 processes，跨域队列在 read-models。

依赖检查覆盖 normal、build、dev 三类边；禁止只检查默认 cargo tree 的正常依赖。没有业务直接依赖也不代表运行时解耦已完成，跨域集合访问与事务调用必须独立检查。

## 8. 按顺序执行的任务清单

1. [x] 固定履约资格、批次行、数量分摊、累计验收、部分履约/反向事实、电子密文与服务证据行为。

2. [x] 迁入履约实体、DTO、拥有仓储和索引；采购来源、库存与销售事实通过窄 Port 输入，禁止持有外域完整聚合。

3. [x] 将 *_posting、purchase_receipt、service_fulfillment_confirm 及 customer_acceptance_task 的跨域外层过程移到 processes::fulfillment_execution；履约、库存、采购、销售、WorkItem 依次调用各自事务内接口。

4. [x] 电子交付加解密规则保留原算法/密钥注入，外部发送放事务外；服务证据和文件引用按既有附件编排接线。

5. [x] 替换采购阶段暂留的旧履约调用，更新 fulfillment Handler、履约队列/工作台/销售进度与退货消费方。

6. [x] 清除旧模块，执行纯守恒与失败轨迹测试和公共门禁；检查记录的反向事实引用没有改变。

每个实体/仓储/服务迁移单元均按“特征测试 → 公开合同 → 实体 → 仓储 → 服务 → 流程/读模型 → 调用方 → 删除旧实现 → 边界 → 全量门禁”执行。其文件/符号范围取第 6 节与清单，测试取第 10 节，删除范围为已迁实现与旧注册，不包括历史 tests/ 档案。

## 9. 阶段内编译中断规则

- 中间状态只存在于专用 worktree，不合入共享工作分支；阶段结束时整个 workspace 必须恢复可编译。
- 原则上每完成一个可闭合迁移单元即执行对应 crate 与入口窄检查；编译错误必须按事实归属修正，不扩大 public 或把业务类型移入基础层消错。
- 需要改共享注册文件时由本阶段唯一集成负责人完成，不让并行 worktree 交叉修改。
- 日常窄检查命令在目标 crate 创建后执行：

```bash
cargo check -p erp-fulfillment -p web-api -p cli --locked
```

## 10. 测试与质量门禁

- 采购收货/发货/验收的数量守恒、超量拒绝、批次关联、重复确认和部分反转。
- 服务证据、电子密文、资格判断以及授权拒绝；测试日志不包含密文原文或密钥。
- 同 Executor 贯穿库存、履约、销售进度和任务；失败之后没有后续成功推进。

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

履约规则混入组合层；原子操作变成多事务；电子交付外部 I/O 持有 session；累计验收口径变化。

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
| 输入基线 | 前序 cf89fb47；专用阶段 worktree | .domain-migration-evidence/12/input.json |
| 文件与符号 | 45 个清单旧路径、5 个 owned 准备文件已清零；跨域符号按固定提供方拆分 | source-clearance.json、files.tsv及逐符号报告 |
| 依赖 | normal/build/dev 全依赖闭包无其他业务域或旧三层 | domain-dependencies.json、metadata.json、boundary.log |
| 测试 | 3440 passed、0 failed、68 ignored；31 个历史 tests 档案逐字不变 | test-summary.json、unit-tests.log、historical-tests.json |
| 协议与数据 | changed/missing/added 均为0；原始48条needs_review逐项复核；权限生成物无漂移 | contract-review.json、parser-limitations-review.json、permissions.log |
| 事务合同 | 实际生产调用链、同Executor、原首错/写入顺序和I/O边界逐项复核；真实数据库运行未验证 | idempotency-transaction-review.json及各分片语义报告 |
| 公共门禁 | fmt/check/严格clippy/lib tests/BPM/service/domain/permissions/git diff 全部exit0；516个第三方包版本和校验和不变 | quality-gates.log、dependency-lock.json |
| 编译证据 | 本阶段边界与全workspace编译通过；最终三个性能场景统一在阶段17计时判定 | compile-applicability.json、domain-dependencies.json |
| 阶段提交 | 实现de112614；证据以本文件提交记录为准；状态最高本地门禁通过 | input.json |

表内未写目录前缀的证据均位于 `.domain-migration-evidence/12/`。原始扫描保留exit2与needs_review；必须结合逐项复核证据使用，不得标为扫描器直接通过。

## 16. 固定调用与证据边界

- `erp-fulfillment` 持有履约实体、资格和数量规则、DTO、仓储、索引及本域写入；采购/销售/文件元数据通过最小消费事实输入，领域不得持有外域聚合或仓储依赖。
- `fulfillment_execution::FulfillmentProcess` 保留原密钥、敏感数据codec、默认RBAC与对象读取注入；客户验收沿用唯一 `CustomerAcceptanceProcess`，跨域工作台归 `fulfillment_center::FulfillmentReadService`。
- 收货按原逐行顺序写余额、库存流水、最后流水、销售分配、预占、冻结与分录；全部行成功后才写收货状态、完成任务、更新采购进度、处理仓发草稿及审计。每行失败必须停止后续行。
- 仓发保持消耗预占、分录、释放预占、扣减可用量、流水、最后流水的原顺序；供应商直发只执行原采购与先款门槛，不新增自有库存动作。
- 服务确认保留原地点规范化、codec加密、同明文指纹、pending引用解析和证据校验顺序；同一Executor内先凭证登记、再确认写入、任务与条件验收任务、最后审计。提交结果未知时沿用原附件补偿判断。
- 验收进度为空时不读取财务余额或刷新销售资金进度；反向事实、分配引用与任务重开保持原合同。
- 编号仍按上海业务日生成，计数器使用NoTransaction；不得纳入业务事务回滚回收。
- 真实数据库运行未验证；纯测试与源码复核不构成实际MongoDB回滚、并发或未知提交恢复证明。
