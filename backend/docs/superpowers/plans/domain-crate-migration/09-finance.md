# 阶段 09：财务

## 1. 阶段元数据

| 字段 | 值 |
| --- | --- |
| 阶段 | 09 |
| 状态 | 未开始 |
| 编制日期 | 2026-09-06 |
| 执行目录 | 仓库内 backend；源路径均相对此目录 |
| 目标 crate | `erp-finance` |
| 执行负责人 | 进入执行中前登记；该阶段只有一个共享注册文件集成负责人 |
| 输入/输出提交 | 前序验收提交 / 本阶段验收后填写 |
| 依据 | [设计契约](../../specs/2026-09-03-domain-crate-migration-design.md)、[公共执行合同](execution-contract.md) |

## 2. 阶段目标

将应收、应付、成本迁入统一财务 crate，并解除财务对销售与工作流的直接调用。

## 3. 前置条件

- [阶段 08](08-inventory.md) 已验收，证据与提交完整。
- 先阅读 [README](README.md)、公共执行合同及本阶段完整内容。
- 对照 [source-map.tsv](source-map.tsv) 的 phase=09 行、[source-symbols.tsv](source-symbols.tsv) 和本节之后的符号拆分表核验当前输入；当前清单默认归属文件数为 90，其源文件哈希只是编制快照，不是迁移已完成证明。
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

财务事实归 erp-finance；收款/付款/开票后销售进度、审批、任务、审计由组合层保持原事务编排。

范围内文件由本阶段符号表、phase=09 的全量文件清单、必要调用方和已登记的注册文件构成。范围外包括新业务、数据回填、依赖版本升级、生产配置/凭据、发布推送，以及既有集成测试源码。

## 6. 源路径、符号、目标与调用方

| 当前源文件（相对 backend） | 主要符号/选择范围 | 目标文件 | 执行动作 |
| --- | --- | --- | --- |
| `services/src/receivable/card_funds_register.rs` | `update_sales_order_money_progress` | `crates/erp-processes/src/finance_posting/card_funds.rs` | 外层跨域流程上移；收款事实写入留 finance |
| `services/src/receivable/invoice.rs` | `update_sales_order_money_progress` | `crates/erp-processes/src/finance_posting/invoice.rs` | 开票与销售进度同事务 |
| `services/src/receivable/customer_receipt_commit.rs` | `receipt` | `crates/erp-processes/src/finance_posting/customer_receipt.rs` | 收款根幂等/提交结果编排 |
| `services/src/payable/payment.rs` | `SupplierPayment` | `crates/erp-processes/src/finance_posting/supplier_payment.rs` | 审批与附件/应付/任务组合 |
| `database/src/repository/receivable_sales_order_summary.rs` | `sales_order_amount_summary` | `crates/erp-finance/src/repository/receivable/sales_order_summary.rs` | 仅应收集合聚合归财务公开事实接口 |
| `database/src/repository/customer_center_receivable.rs` | `customer_center_receivable` | `crates/erp-finance/src/repository/receivable/customer_center.rs` | 保留 Decimal128 数据库聚合和原分页/索引语义 |
| `services/src/receivable/dto/query.rs` | `WorkItemView` | `crates/erp-read-models/src/finance/dto.rs` | 嵌入工作项的跨域视图从 finance DTO 分离 |
| `services/src/receivable/card_funds_task.rs` | `WorkItemService` | `crates/erp-processes/src/finance_posting/card_funds_task.rs` | 任务推进归流程 |
| `services/src/payable/payment_task.rs` | `WorkItemService` | `crates/erp-processes/src/finance_posting/payment_task.rs` | 应付任务与财务写入共用 Executor |


全量文件以 source-map.tsv 为默认目标；混合文件必须按上表及第 8 节拆符号。source-symbols.tsv 列出各源文件的类型、Trait、函数和声明行；未另行规定的符号名称与行为保持原样。禁止将该清单作为直接批量 mv 脚本。

本阶段还必须迁移 repository-types.tsv 中 owner_phase=09 的全部拥有仓储类型：其 prepare_at 文件由阶段 01 创建，迁到 final_type_definition，并同步 impl_sources 中的所有专用方法。本阶段开始时这些准备文件属于前序输出，不因编制时尚不存在而遗漏。

本阶段必须更新的入口调用方：

- `apps/web-api/src/app_state.rs`。
- `apps/web-api/src/core/handler/cost/mod.rs`。
- `apps/web-api/src/core/handler/payable/mod.rs`。
- `apps/web-api/src/core/handler/receivable/mod.rs`。
- `apps/web-api/src/core/routes/cost.rs`。
- `apps/web-api/src/core/routes/payable.rs`。
- `apps/web-api/src/core/routes/receivable.rs`。
- `apps/web-api/src/lib.rs`。
- `apps/web-api/src/main.rs`。

此外必须按 source-symbols.tsv 的选定符号用 rg 检索全部生产消费者，并复核 grouped use、类型别名、pub use、impl、宏与 cfg(test) 调用。具体跨域消费者按第 7、8 节处理；每个旧引用必须改为直接目标合同或组合入口，不能新增旧路径转发。

## 7. 目标依赖合同

finance 内部 receivable/payable/cost 可相互调用；finance 不依赖 sales/workflow/party/support 或旧三层；跨域能力通过 Port/组合层。

依赖检查覆盖 normal、build、dev 三类边；禁止只检查默认 cargo tree 的正常依赖。没有业务直接依赖也不代表运行时解耦已完成，跨域集合访问与事务调用必须独立检查。

## 8. 按顺序执行的任务清单

1. [ ] 固定收款分配/核销、应付分配、开票额度、差额分录、负数/溢出/舍入、部分冲销和历史账户显示的特征测试。

2. [ ] 迁入 receivable/payable/cost 的实体、DTO、仓储、extensions 和索引；统一使用阶段 01 的拥有仓储。领域持久化快照含外域状态时保留原序列化值，用财务消费方事实类型和显式映射消除外域聚合依赖。

3. [ ] 建立 finance 的收款/开票/付款/成本事务内接口；从 card_funds_register、invoice、red_invoice、customer_receipt 和 payable/payment 拆出写销售进度、审批和工作项的步骤进入 processes::finance_posting。公开业务行为和步骤顺序保持不变。

4. [ ] 把 invoice_task/card_funds_task/payment_task 中任务决策与写入分别归明确财务事实判断和 workflow 命令；processes 执行最终协调。财务 DTO 中直接嵌入 WorkItemView 的视图迁 read-models。

5. [ ] 销售当前仍在旧 services：由 processes 直接调用旧销售事务内接口；erp-finance 禁止依赖旧 services。旧销售的单纯财务事实调用可单向切到 finance；需要完整跨域事务的外层入口同时上移到 processes，禁止 services→processes 回边。

6. [ ] 更新 receivable/payable/cost Handler、审批动作分发、客户中心、销售金额摘要与逆向流程调用方，删除旧财务模块。将唯一索引消息与领域错误保留在正确消费层，禁止所有错误降为 Internal。

7. [ ] 运行完整财务纯内联测试、入口检查和公共门禁；执行 Finance 增量 check/build 复测并记录仍受旧销售大 crate 影响的调用链。

每个实体/仓储/服务迁移单元均按“特征测试 → 公开合同 → 实体 → 仓储 → 服务 → 流程/读模型 → 调用方 → 删除旧实现 → 边界 → 全量门禁”执行。其文件/符号范围取第 6 节与清单，测试取第 10 节，删除范围为已迁实现与旧注册，不包括历史 tests/ 档案。

## 9. 阶段内编译中断规则

- 中间状态只存在于专用 worktree，不合入共享工作分支；阶段结束时整个 workspace 必须恢复可编译。
- 原则上每完成一个可闭合迁移单元即执行对应 crate 与入口窄检查；编译错误必须按事实归属修正，不扩大 public 或把业务类型移入基础层消错。
- 需要改共享注册文件时由本阶段唯一集成负责人完成，不让并行 worktree 交叉修改。
- 日常窄检查命令在目标 crate 创建后执行：

```bash
cargo check -p erp-finance -p web-api -p cli --locked
```

## 10. 测试与质量门禁

- 金额守恒、分配额度、全额/部分退款冲销、重复核销/开票、同幂等键异载荷、未知提交结果。
- JSON/BSON、索引名称和唯一性、Decimal128 $sum 不变；无完整子账加载替代聚合。
- 事务替身验证财务→销售进度→工作项/审计的实际原顺序和失败传播，不声明真实数据库回滚实测。

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

erp-finance 引入销售或 WorkItem 完整类型；财务字段/索引/舍入漂移；跨域步骤被拆为多笔事务；错误分类被统一吞掉。

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
| 输入基线 | 前序已验收 commit；阶段分支；源映射核对 | 未采集 |
| 文件与符号 | 新增/修改/删除列表；source-map 核销；跨域符号归属 | 未采集 |
| 依赖 | metadata/tree；normal/build/dev 边；无环与旧引用结果 | 未采集 |
| 旧实现清零 | 源目录、根导出、#[path]/include 路径、调用方搜索命令及结果 | 未采集 |
| 测试 | 内联测试命令、退出码、通过/失败/忽略数量、关键断言 | 未执行 |
| 协议与数据 | HTTP/DTO/错误/权限、JSON/BSON、索引对比 | 未采集 |
| 事务合同 | 同一 Executor、调用顺序、失败传播、I/O 边界；真实数据库运行未验证 | 未采集 |
| 公共门禁 | 每条命令、工具版本、退出码和日志路径 | 未执行 |
| 编译收益 | 适用场景原始样本、Fresh/Dirty、timings、中位数与改善率；不适用须写明 | 未执行 |
| 阶段提交 | commit hash、范围、验收日期及验收人 | 未提交 |
