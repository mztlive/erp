# erp-returns：退货、退款与冲正

## 职责

`erp-returns` 保存逆向业务单据：记录为什么退、退了多少、退款或冲正多少，以及对应的原销售、采购或资金事实。退货记录实物或履约处置，退款记录资金退回或供应商留抵，冲正记录对原收付款事实的纠错。登记退货不等于库存已经变动或资金已经过账；纠错保留原事实，通过可追溯的反向记录处理。

### 负责的数据与能力

| 业务 | 本域负责的规则 | 源码入口 |
| --- | --- | --- |
| 销售退货／拒收 | 处理单记录原销售单、可选验收依据和退回路线（退公司仓、直退供应商或无实物退回）。明细申请量须为正，实际退回量不得为负或超过申请量；记有实退量时，可重新入库量不得超过实退量，且须验收合格。处理单已完成或作废后不可编辑。 | [处理单](src/entity/returns/sales_return_case.rs)、[退货明细](src/entity/returns/sales_return_line.rs) |
| 采购退货 | 退货单关联原采购单，可关联销售退货处理单；区分公司仓退供应商和客户直退供应商。明细指向原采购版本行，退货量须为正。直退可分批登记，要求供应商收货凭据、原因及不重复的正数明细，且不得超过当前行的可直退量。 | [采购退货单](src/entity/returns/purchase_return_order.rs)、[采购退货明细](src/entity/returns/purchase_return_line.rs)、[直退凭据](src/entity/returns/direct_return.rs) |
| 客户退款／供应商退款 | 客户退款的原回款与原应收分录二选一；供应商退款的原付款与原应付分录二选一。这里的资金退款过账要求已过账的原回款或原付款；若来源是原应收／应付分录，则不走此过账入口，应由财务冲减分录。供应商采购处置区分现金退款与留抵。 | [客户退款](src/service/customer_refund.rs)、[供应商退款](src/service/supplier_refund.rs) |
| 回款冲正／付款冲正 | 分别引用原客户回款和原供应商付款。过账前复核原资金事实及累计冲正额度；通过后写回本域冲正单，不删除原收付款。四种资金纠错单共用正金额、经办人与复核人分离等规则。 | [回款冲正](src/service/receipt_reversal.rs)、[付款冲正](src/service/payment_reversal.rs)、[共用规则](src/entity/returns/common.rs) |
| 命令成功回执 | 为四种退款／冲正单据的「一次创建并提交」和「提交已有单据」命令各保存独立结构化回执（四个 `.commit`、四个 `.submit`），以本域唯一命令身份查证提交结果。 | [回执实体](src/entity/command_receipt.rs)、[回执服务](src/service/command_receipt.rs) |

### 使用场景

修改逆向单据、退货数量、退款／冲正状态或销售退货单域查询，从本 crate 入手。若要调整审批、库存、核销或正式任务何时写入，找对应 Process；若要调整余额、反向分配或库存事实本身，改拥有该事实的领域。

## 目录结构

### 代码入口

| 目录或文件 | 分工 |
| --- | --- |
| [Cargo.toml](Cargo.toml)、[src/lib.rs](src/lib.rs) | 依赖与公开入口；导出领域模块、错误和结果类型，以及命令回执实体、类型化结果、仓储扩展和服务。 |
| [entity/](src/entity/mod.rs) | [returns/](src/entity/returns/mod.rs) 定义单据、明细、值对象和无 I/O 的状态与数量／金额规则；`command_receipt.rs` 定义回执模式和匹配合同。 |
| [dto/](src/dto/mod.rs) | 写入请求、草稿更新、销售退货查询参数和响应视图；[提交指纹](src/dto/submit_reversal.rs)由原提交请求构造。 |
| [service/](src/service/mod.rs) | `ReturnsService` 及各业务模块负责构造单据、读取本域事实、执行版本与状态守卫、在调用方事务内持久化；另有[销售退货查询](src/service/sales_return_query.rs)、[审批守卫](src/service/approval.rs)和[命令回执服务](src/service/command_receipt.rs)。 |
| [repository/](src/repository/mod.rs) | MongoDB 集合访问器、筛选分页、主从写入、[已核实直退读取](src/repository/direct_return.rs)及[已过账累计聚合](src/repository/returns_posted_totals.rs)。 |
| [indexes/](src/indexes/mod.rs) | `ensure` 按既有顺序注册退货、退款、冲正集合索引，再注册[回执命令 ID 唯一索引](src/indexes/command_receipt.rs)。 |
| [ports/](src/ports/mod.rs) | 当前未声明端口；外域读取结果以窄事实传入，本域不自行查外域集合。 |

## 架构设计

### 依赖与协作边界

术语与统一分工见[总索引](../../README.md#代码中常见名称的含义)。本 crate 可使用 [erp-identity](../erp-identity/README.md#业务授权-api) 的公开授权接口和共享基础合同，不直接依赖其他业务领域或组合层。比如 Process 读取财务回款后，只向客户退款服务传入来源 ID、版本、金额、客户和过账状态，而不传入整个财务实体。需要外域能力时，由编排层或应用装配，不在本域新增直接领域依赖。

- **事务与审批**：[erp-reverse-flow](../../orchestration/erp-reverse-flow/README.md) 的 [reverse_flow](../../orchestration/erp-reverse-flow/src/reverse_flow/mod.rs) 持有逆向命令根事务；审批终审沿用审批运行时的事务。决定性读取、授权、本域写入、财务结果、任务、相应回执与审计复用同一个 `persistence_core::Executor`，任一步失败即停止并回滚。普通[销售退货](../../orchestration/erp-reverse-flow/src/reverse_flow/sales_return.rs)和[采购退货](../../orchestration/erp-reverse-flow/src/reverse_flow/purchase_return.rs)创建在同一事务写单头和首条明细、登记业务单据，政策为 `NO_APPROVAL`，不启动审批实例或任务。退款／冲正创建时绑定已发布定义，草稿或驳回后提交递增审批主题版本并冻结该轮审批快照；撤回回草稿但不回退版本，驳回不写资金事实。只有审批最终通过可过账，客户端不能直接过账；审批与正式任务归 [erp-workflow](../erp-workflow/README.md)。
- **本域查询**：销售退货列表、详情必须使用 `SalesReturnQueryService`；授权、表头和明细在同一读取快照内完成，列表的筛选和计数也在该快照。已完成命令授权的调用方用 `ReturnsService::load_sales_return_case_view` 装配结果，传入原 `Executor`；该装配方法不另行授权，不是对外查询入口。跨域查询和投影归 [erp-read-models](../../read-models/erp-read-models/README.md)。
- **资金限额**：本域按原收付款、按对应单据类型聚合 `posted` 金额，排除本单和已删除记录，由[累计限额值对象](src/entity/returns/cumulative_limit.rs)比较历史加本次是否超过原金额，等额允许。例如原回款 100、已有已过账客户退款 60，则本域金额检查允许再退 40，拒绝 40.01；财务还需检查实际剩余可反向核销额。现金退款累计不计供应商留抵。另有[采购处置方案限额](src/entity/returns/supplier_refund/settlement.rs)，按同采购、同方案、同结算方式计算，审批中和已过账均占用，不能与原收付款的已过账口径混用。

### 协作示例

- **客户直退供应商**：[直退 Process](../../orchestration/erp-reverse-flow/src/reverse_flow/direct_return.rs) 在同一事务复验当前采购版本及逐行可退量，保存相互关联的销售退货、采购退货及供应商收货凭据。例如原履约 47 件，核实已退 27 件，采购改单按净履约 20 件校验，不改写原交付；没有凭据的旧退货草稿不能抵扣。直退登记不产生库存入库或资金分录；涉及公司仓库存变更时，由 [erp-inventory](../erp-inventory/README.md) 维护库存事实。采购改单生效后，[本域结案步骤](src/service/direct_return_completion.rs)在同一事务完成两张退货单，不重复调整款项。
- **退款与冲正**：[erp-finance](../erp-finance/README.md) 持有原收付款、应收应付、核销分配、冲抵分录与余额。客户退款终审时，Process 先调用本域检查退款状态与累计额度，再调用[财务退款步骤](../erp-finance/src/service/receivable/customer_refund.rs)追加反向核销及相应的应收减少、抵销记录，不改原回款状态；本域写回退款单，Process 写审计并刷新销售资金进度。[回款冲正](../erp-finance/src/service/receivable/receipt_reversal.rs)则回冲核销并将原回款标为已冲正；[付款冲正](../erp-finance/src/service/payable/payment_reversal.rs)累计原付款的冲正金额，全额才将原付款标为已冲正。付款冲正涉及原付款申请时，由 Process 调用装配的恢复端口；供应商留抵由财务记余额，不伪造现金退款或新付款。

### 修改执行要求

1. 将无 I/O 的校验、状态迁移和不变式放入实体或值对象；Service 组织本域步骤。仓储复用调用方执行器，不另开事务；[单头与明细写入](src/repository/returns/cross_collection.rs)必须传入事务执行器，不能依赖仓储使多笔写入原子化。
2. 调整集合查询时同步评估索引；组合根复用公开索引入口并保持逐集合注册顺序。HTTP 优先复用本域 DTO；变更公开合同时检查 Process、ReadModel 和应用调用方。业务改动补充库单元测试，覆盖成功、失败、边界及适用的幂等或版本冲突路径。
3. `returns_command_receipts` 的命令身份、版本化指纹和强类型结果引用必须与正式业务、审批和成功审计同事务提交。单独提交的结果必须指向原退款或冲正单，指纹须包含原请求的期望版本，而非回放时的当前版本；同键异载荷或损坏回执必须明确失败。回放仍核验当前账号、静态权限、真实资金来源和经办职责，只回读原对象的当前视图，不追加业务效果或成功事件。提交结果未知时只能查证原命令，不自动重跑写入。回执只保留审计 ID，不读取审计正文，不随日志归档删除，也不设置 TTL。

## 验证要求

仅改文档时，按[后端开发规则](../../../AGENTS.md#验证)执行 `git diff --check`。业务代码变更时，从 `backend/` 执行定向库单元测试及[统一质量门禁](../../README.md#质量门禁)，不运行集成测试或依赖真实外部服务的测试。

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-returns --lib --locked
```
