# erp-reverse-flow

## 职责

编排销售退货、采购退货、客户退款、供应商退款、回款冲正和付款冲正的跨域写入。`ReturnsProcess` 负责业务命令；审批最终通过时，应用分别调用 `finalize_approved_return`、`ReceiptReversalProcess` 或 `PaymentReversalProcess`。退款和冲正的客户端直过账入口不产生资金事实。依赖关系以 [Cargo.toml](Cargo.toml) 为准；本 crate 复用 `erp-procure-to-pay` 的采购财务交接，不直接依赖 `erp-finance-posting`。

| 记录 | 所有者与编排边界 |
| --- | --- |
| 退货单及明细、退款单、冲正单、逆向业务命令回执 | [erp-returns](../../domains/erp-returns/README.md) 定义实体、不变式和持久化。业务命令回执关联原请求、正式结果 ID 和成功审计 ID，不用审批收据或审计正文替代。 |
| 原回款和付款、应收应付、反向分配及供应商留抵 | [erp-finance](../../domains/erp-finance/README.md) 维护财务事实与金额规则。审批快照和退货记录不是过账凭证，最终动作仍须核验原资金事实。 |
| 单据注册、审批定义与实例、冻结快照、审批收据和正式任务 | [erp-workflow](../../domains/erp-workflow/README.md) 管理审批政策及任务；[erp-approval-runtime](../erp-approval-runtime/README.md) 提供绑定、启动和材料冻结等共有步骤。编排只提供逆向单据的业务动作及其执行顺序。 |
| 销售和采购原单、采购依据更新与剩余款项方案 | 分别由 `erp-sales`、`erp-procurement` 持有。编排追加实际退回凭据、保护来源版本并协调后续任务，不以退货记录覆盖原采购或发货事实。 |

## 目录结构

| 入口 | 用途 |
| --- | --- |
| [`src/lib.rs`](src/lib.rs)、[`src/ports.rs`](src/ports.rs)、[`src/errors.rs`](src/errors.rs) | 导出逆向命令模块、`ReverseFlowPorts` 与 `Error` / `Result`；保留审批稳定错误码及回执竞争、提交未知等错误分类。 |
| [`reverse_flow/mod.rs`](src/reverse_flow/mod.rs) | `ReturnsProcess`、退款终审及撤回/驳回回调；导出收付款冲正终审入口。 |
| [`sales_return.rs`](src/reverse_flow/sales_return.rs)、[`purchase_return.rs`](src/reverse_flow/purchase_return.rs)、[`direct_return.rs`](src/reverse_flow/direct_return.rs)、[`return_settlement.rs`](src/reverse_flow/return_settlement.rs) | 无审批退货登记、客户直退确认和剩余款项处置。 |
| [`customer_refund.rs`](src/reverse_flow/customer_refund.rs)、[`supplier_refund.rs`](src/reverse_flow/supplier_refund.rs)、[`receipt_reversal.rs`](src/reverse_flow/receipt_reversal.rs)、[`payment_reversal.rs`](src/reverse_flow/payment_reversal.rs)、[`payment_posting.rs`](src/reverse_flow/payment_posting.rs) | 按资金单据拆分命令；同名子目录承载创建并提交、审批或过账步骤，付款冲正由 `payment_posting.rs` 执行最终过账。 |
| [`adapter/`](src/reverse_flow/adapter/)、[`start_approval/`](src/reverse_flow/start_approval/)、[`draft_update.rs`](src/reverse_flow/draft_update.rs)、[`cancel_write.rs`](src/reverse_flow/cancel_write.rs)、[`approval_outcome.rs`](src/reverse_flow/approval_outcome.rs) | 审批主题和领域动作适配、启动、草稿更新、撤回和驳回写回。 |
| [`commit_access.rs`](src/reverse_flow/commit_access.rs)、[`command_recovery.rs`](src/reverse_flow/command_recovery.rs)、[`result.rs`](src/reverse_flow/result.rs) | 一次建单并提交的来源授权、命令回执查证及按原命令资格读取结果。 |

## 架构设计

### 应用装配与事务

- `ReturnsProcess` 必须接收应用装配的 `ReverseFlowPorts`：共享 `AuthorizationService`、`ApprovalRuntimePort` 和 `FinanceResponsibilityPort`。编排不自行构造具体身份、范围或工作流适配器。[应用装配](../../../apps/erp-server/src/wiring/reverse_flow.rs)的 `local_reverse_flow_ports(db, authorization)` 为审批等设施构造本地 RBAC 服务，但复用应用的统一授权服务；客户直退确认和剩余款项方案登记使用 `reverse_flow_ports(db, rbac, authorization)`，同时复用应用的 RBAC 服务。
- 采购退货写入在命令事务中按真实来源采购单校验 `PURCHASE_RETURN_SOURCE`。[采购退货查询](../../../apps/erp-server/src/http/handler/returns/purchase_return.rs)由应用为 `ReturnsReadService` 注入 `PurchaseDataScopePort`，读模型通过 [`PurchaseAccess`](../../read-models/erp-read-models/src/returns_center/mod.rs)按来源采购单控制列表和详情；列表资格不能代替详情重验，也不能代替写授权。
- 由命令入口开启根事务；最终通过及运行时撤回、驳回回调沿用[应用审批分派](../../../apps/erp-server/src/wiring/approval_dispatch/action_registry.rs)或[驳回分派](../../../apps/erp-server/src/wiring/approval_dispatch/action_registry/rejection.rs)传入的事务，不另开写事务。决定性授权、来源和版本读取、反向分配、原事实状态、任务、成功审计及本次命令回执使用同一 `Executor`，按各路径既定顺序执行，首个错误即停止，由根事务整体回滚。提交后的结果读取用独立授权快照，不补写已结束的事务。

### 退货登记与财务交接

- **普通退货**：销售退货与采购退货均为 `NO_APPROVAL`。同一事务先注册 `BusinessDocument`（统一绑定端口返回空绑定），再写退货单及明细并追加审计；不查发布定义、不建审批实例或审批任务。各自的单号通过[唯一索引](../../domains/erp-returns/src/indexes/returns.rs)去重，重复单号是冲突，不等同于命令回执回放。
- **客户直退供应商**：在[确认直退](src/reverse_flow/direct_return.rs)中，请求必须提供实际退回数量和供应商收货凭据，查证或写入前均按来源采购单授权。同幂等键只在原采购、操作人和完整请求一致时返回原采购退货单 ID。首次确认才校验采购版本、生效销售减量、逐行可退数量，通过销售守卫与采购版本更新防止并发超退；同事务先后登记销售和采购退货单及实际退回明细，再写审计并同步财务责任。该动作不改写原发货、不直接办理退款或红冲，也不代替采购改单生效。
- **剩余款项**：实际应退货量已确认后，采购登记与当前销售版本、目标金额相符的方案，记录供应商确认依据，并校验现金退款与留抵金额合计等于超付额。当前方案的完全相同请求不重复写；不得用当前或历史操作号替换方案，仍有处置在审批中时也不得替换。[采购财务交接](../erp-procure-to-pay/src/procure_to_pay/return_finance.rs)按净付款、净票额和处置方案同步退款与票据任务，复用已有开放任务，由 `FinanceResponsibilityPort` 解析新任务责任人。供应商退款过账和[发票红冲](../erp-finance-posting/src/finance_posting/receivable/red_invoice.rs)分别在自己的事务中复用该交接，不由本 crate 办理红票。

### 退款与冲正审批

- 四类资金单据可先建草稿再提交，也可从已核验的原回款或原付款[一次建单并提交](src/reverse_flow/customer_refund/commit.rs)。建单须绑定已发布定义，提交读取绑定的定义版本并由服务端解析审批人，客户端不得指定定义或审批人。按真实单据类型校验固定提交权限、当前来源资格和经办/复核分离；一次建单并提交则先校验服务端构造的经办人及来源候选，再匹配原回执。
- 提交将业务单据迁入 `IN_APPROVAL`，冻结审批主题版本；在原事务保存 BPM 启动收据、实例与执行记录、不可变快照和入口任务。[`ApprovalRuntimePort`](../erp-approval-runtime/src/runtime_port.rs)提供定义绑定、材料冻结、启动和撤回抄送，以及审批来源可读性合同；逆向命令的当前资金来源资格由统一授权服务另行校验，不能用历史快照替代。提交既有单据时，先插入 BPM 启动收据并守卫注册单据，再写业务状态和运行事实，最后保存提交审计和业务回执；一次建单并提交在启动写入后依次保存创建、提交审计及业务回执。
- 撤回须校验原单版本，按原审批主题加载实例；实际撤回先写运行事实、抄送与任务关闭，再将业务单据写回草稿并追加审计，不回退审批主题版本。驳回以运行时冻结的主题版本迁移原单并写审计。两者均不产生反向资金事实，只有最终通过才过账。

### 最终过账路径

| 单据 | 同一审批事务内的写入顺序 |
| --- | --- |
| [客户退款](src/reverse_flow/customer_refund.rs) | 校验原回款已过账及累计退款额度；写应收抵销、减少分录和反向核销，不把原回款标为冲正。再写退款过账状态和成功审计，按原回款分配刷新受影响销售单的收款进度。 |
| [供应商退款](src/reverse_flow/supplier_refund/posting.rs) | 校验原付款及累计额度；关联采购方案时复核供应商、实际退货、当前方案和分方式限额，提交时另行冻结供应商确认依据。回冲应付已付进度并写反向分配，更新原付款现金退款额或建立留抵余额（不登记现金退款或新付款）；随后同步采购财务任务，再写退款过账状态和审计。采购应付总额差额仍由采购改单处理。 |
| [回款冲正](src/reverse_flow/receipt_reversal.rs) | 校验原回款和累计冲正额度；回冲应收结算，写反向分配并将原回款标为已冲正。写冲正单状态与审计后重新读取原回款分配，逐一刷新受影响销售单，不延迟到提交后。 |
| [付款冲正](src/reverse_flow/payment_posting.rs) | 校验原付款及累计冲正额度，先回冲应付结算，再通过应用注入的 [`PaymentRequestRestorationPort` 实现](../../../apps/erp-server/src/wiring/approval_dispatch/payment_restoration.rs)按本次逐应付冲正金额恢复原付款申请。已关闭申请只更新净付款，不恢复占用或执行任务；不复活旧采购应付直付任务。之后写反向分配并累计原付款冲正额，全额冲正才将原付款标为已冲正；最后写冲正单状态和审计。 |

### 幂等与未知提交

- 提交既有单据时，精确业务回执绑定操作人、动作、单据与幂等键，并匹配请求的期望版本和规范化重提说明；一次建单并提交匹配原命令身份和请求载荷。重验当前资格后，命中回执只回读原结果，不重新启动审批；同身份异载荷拒绝。BPM 启动收据用于恢复原审批实例，不能替代精确业务回执。
- 对可能已提交的错误，[建单并提交查证](src/reverse_flow/commit_access.rs)和[既有单据提交查证](src/reverse_flow/start_approval/reversal_submit.rs)在新快照中按原命令身份重验授权并找原回执。未能确认时保留提交未知语义；不得自动重跑审批或资金写入，也不得用新操作号替代原命令查证。已确认提交后的视图读取失败不代表原写入已回滚。

## 验证

只改文档时仅对该文件运行 `git diff --check`。生产代码改动按[后端开发规则](../../../AGENTS.md#验证)执行库单元测试和提交门禁；不运行集成测试、ignored 测试、历史验收 examples 或依赖真实外部服务的测试。
