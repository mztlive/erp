# erp-procure-to-pay

## 职责

本 crate 编排从销售需求到采购生效的跨域命令：采购责任分派、供给分配与建单、采购送审与生效、采购变更及来源销售变更后的联动。它也处理费用采购与询价费用采用、采购付款安排待办以及采购单下载和发件。采购生效会形成原始应付、确认成本及适用的履约草稿或执行意图，但不等于已付款、已收货或已向供应商下单。

[采购领域 `erp-procurement`](../../domains/erp-procurement/README.md) 拥有采购单、草稿、冻结提交、正式修订、采购变更、销售分配和责任规则，维护实体不变式、单域服务与仓储。本 crate 通过各拥有领域的 Service、Repository 和 Port 读取事实、协调写入，不接管这些领域规则。采购列表、详情及创建依据由 [`erp-read-models`](../../read-models/erp-read-models/README.md) 组装；读侧展示的依据不替代写事务里的复验。

## 目录结构

| 源码入口 | 用途 |
| --- | --- |
| [`Cargo.toml`](Cargo.toml)、[`src/lib.rs`](src/lib.rs)、[`src/errors.rs`](src/errors.rs) | 依赖声明；公开 `procure_to_pay`、`purchase_correspondence` 和 `Error` / `Result`。 |
| [`procure_to_pay/`](src/procure_to_pay/mod.rs)、[`ports.rs`](src/procure_to_pay/ports.rs) | `PurchaseOrderProcess`、`PurchaseOrderFormalizationProcess` 及其共享的 `PurchaseOrderPorts`。 |
| [`creation_basis/create.rs`](src/procure_to_pay/creation_basis/create.rs)、[`sourcing_create.rs`](src/procure_to_pay/sourcing_create.rs) | 按单一采购依据建单，或把一批选源结果落为库存预占、仓发草稿和多张采购单。 |
| [`draft_edit.rs`](src/procure_to_pay/draft_edit.rs)、[`create_submit.rs`](src/procure_to_pay/create_submit.rs)、[`submission.rs`](src/procure_to_pay/submission.rs)、[`start_approval/`](src/procure_to_pay/start_approval/mod.rs) | 保存草稿、建单即送审或独立送审，共用冻结提交和启动审批的写入步骤。 |
| [`review.rs`](src/procure_to_pay/review.rs)、[`formalization_root.rs`](src/procure_to_pay/formalization_root.rs)、[`formalization_posting.rs`](src/procure_to_pay/formalization_posting.rs) | 从冻结提交准备正式版本并按顺序写入采购、应付、成本及履约事实。 |
| [`change/`](src/procure_to_pay/change/mod.rs)、[`source_update_sync.rs`](src/procure_to_pay/source_update_sync.rs) | 采购变更的草稿、送审、作废和生效；依据更新记录及相关待办的对账。 |
| [`responsibility/`](src/procure_to_pay/responsibility/mod.rs)、[`procurement_task_sync.rs`](src/procure_to_pay/procurement_task_sync.rs) | 责任规则维护与预览、负责人资格校验及责任计划认领；随剩余需求同步供给分配待办。 |
| [`expense_create.rs`](src/procure_to_pay/expense_create.rs)、[`expense_confirm.rs`](src/procure_to_pay/expense_confirm.rs)、[`inquiry_expense/`](src/procure_to_pay/inquiry_expense/mod.rs) | 费用建单与发生确认；采购和财务采用同一询价费用来源时协调跨轮次额度。 |
| [`payment_arrangement.rs`](src/procure_to_pay/payment_arrangement.rs)、[`payment_follow_up.rs`](src/procure_to_pay/payment_follow_up.rs)、[`return_finance.rs`](src/procure_to_pay/return_finance.rs) | 同步付款安排、设置跟进日，以及联动直退后的款票任务和往来账户。 |
| [`purchase_correspondence/`](src/purchase_correspondence/mod.rs) | 下载可打印采购单、受控发件及查询发件历史；渲染和邮件传输分别位于 [`paper.rs`](src/purchase_correspondence/paper.rs) 和 [`transport.rs`](src/purchase_correspondence/transport.rs)。 |

## 架构设计

### 采购责任与计划落地

1. **先定责任，再派待办。** 实物及服务销售单[送审前](../erp-order-to-cash/src/order_to_cash/procurement.rs)逐行校验采购责任；[销售正式化](../erp-order-to-cash/src/order_to_cash/formalize.rs)按冻结的销售提交行形成责任计划，在[销售生效事务](../erp-order-to-cash/src/order_to_cash/formalization_posting.rs)中调用 [`claim_plan`](src/procure_to_pay/responsibility/resolver.rs) 重验稳定行、规则、具体负责人及授权策略版本并认领轮次，再由销售流程写入冻结行范围的供给分配待办。展示姓名不参与计划身份比较。[规则维护](src/procure_to_pay/responsibility/service.rs)在同一事务校验选择器、负责人资格并记录审计；[预览](src/procure_to_pay/responsibility/preview.rs)只读，可逐行报告诊断，不占用轮次。
2. **读取依据与执行计划分开。** [`creation_basis_list`](../../read-models/erp-read-models/src/purchase_center/creation_basis/query.rs)只返回当前账号开放待办的冻结范围内、仍有剩余量的采购和库存依据。操作人确认选源后，[批量写入](src/procure_to_pay/sourcing_create/apply.rs)在原事务核验开放任务、负责人和冻结范围，使用采购域的 [`SourcingPlan`](../../domains/erp-procurement/src/entity/purchase_order/sourcing_plan/plan.rs)校验同一销售行的拆分总量，推进一次销售供给并发守卫。再核验计划中各单的建单资格，并按实际写入阶段重读库存及采购依据。库存份额形成预占和仓发草稿；采购缺口按供应商、采购类型、付款条件、履约责任、价格依据、专项报价版本及适用的目标收货仓拆单。逐单建单并送审、任务同步、审计和整批回执同事务提交。
3. **覆盖变化后同步责任。** [供给分配待办同步](src/procure_to_pay/procurement_task_sync.rs)按稳定销售行重新计算责任范围内的剩余量：开放任务更新摘要，归零后完成；终态范围再次出现缺口时创建继任任务，不重开历史任务。[销售变更](../erp-order-to-cash/src/sales_change/procurement.rs)在自己的生效事务内调用本 crate 撤回受影响的审批中采购，并同步采购依据更新记录和待办。采购负责人可[登记维持原采购](src/procure_to_pay/source_maintain.rs)，记录原因并关闭依据更新待办，不改写采购内容或应付。

### 采购单与变更的事务顺序

- **建单与送审。** 按依据建单和批量选源都先保存草稿及业务单据登记，再在同一事务分配首次正式号、冻结提交并启动审批；失败不留下未送审草稿。已有草稿另走独立提交入口。两条路径共用[审批启动写入](src/procure_to_pay/start_approval/start_persist.rs)：审批回执先于注册行启动守卫和冻结提交写入；旧草稿失效、审批实例、材料快照、入口任务、命令回执及成功审计一起提交。[撤回](src/procure_to_pay/cancel_approval.rs)恢复可编辑草稿，[驳回](src/procure_to_pay/rejection.rs)派生编辑副本并保留原审批快照；[作废草稿](src/procure_to_pay/void_order.rs)则释放采购覆盖，并在原事务同步供给分配任务。
- **首次生效。** [审批动作分发](../../../apps/erp-server/src/wiring/approval_dispatch/action_registry.rs)在最终通过时调用 `formalize_approved_order_apply`。正式化从冻结提交准备写入计划，在原事务复验销售来源影响、结算主体和交付安排，再写采购修订及明细、销售分配、审核结论和当前版本指针。随后写入原始应付、同步采购经办人的付款安排待办、写入 `CONFIRMED` 成本，按履约责任准备收货、直发、电子交付或服务草稿及适用任务，最后写成功审计。
- **变更生效。** [发起变更](src/procure_to_pay/change/submit.rs)时以当前采购修订为基准，独立绑定已发布审批定义；送审时冻结本轮提交，客户端不能指定定义或审批人。最终通过时，[生效流程](src/procure_to_pay/change/effect.rs)核对基准修订，[过账步骤](src/procure_to_pay/change/posting.rs)沿原事务复核数量、交付与结算主体，推进来源销售守卫，新增修订及分配后切换当前采购版本。即使金额不变，也同步应付来源版本；有金额差才追加应付差额，不在此追加成本分录或改写已发生事实。[旧 API 执行](../erp-supplier-orders/src/supplier_api_orders/change_guard.rs)须未发单或已核实取消且可安全停止，才能由新版本接替。采购待办、提交结论、变更状态和成功审计随该事务完成。
- **费用采购。** [费用建单](src/procure_to_pay/expense_create.rs)与首次送审同事务，关联询价费用时同时占用来源额度。[确认费用发生](src/procure_to_pay/expense_confirm.rs)以当前生效费用行写入发生事实、凭证关联和实际成本，已有询价占用只转为已办理、不再扣减；符合条件的作废才释放未办理占用，撤回送审或驳回不释放。财务直接登记询价费用时也使用[同一来源额度](src/procure_to_pay/inquiry_expense/mod.rs)，但不另建应付。

### 事务、授权与结果恢复

普通跨域命令由 Process 开启写事务；审批回调、销售变更联动及其他同事务步骤复用调用方的 `Executor`，不另开写事务。决定写入的授权与业务校验、仓储和 Port 调用及成功审计沿用原执行器。部分正式化准备单独读取冻结事实及历史版本以构造计划，不代替原事务中的复验和乐观锁检查。外部 HTTP、对象存储及供应商调用不放在数据库事务中。

[服务端装配](../../../apps/erp-server/src/wiring/purchase.rs)注入进程共享的 `AuthorizationService`、采购数据范围、[`ApprovalRuntimePort`](../erp-approval-runtime/src/runtime_port.rs)和财务责任能力。HTTP 负责传递认证身份，不代替用例授权；[写命令授权](src/procure_to_pay/authorization.rs)按当前操作和真实对象在原事务核验范围，列表可见或历史审批参与不授予写权限。工作项规则和审批事实由 [`erp-workflow`](../../domains/erp-workflow/README.md) 管理，通用流程计算归 [`bpm`](../../platform/bpm/README.md)，绑定、材料冻结及精确实例回放等共有步骤由 [`erp-approval-runtime`](../erp-approval-runtime/README.md) 提供。

带回执的命令以原操作人、命令和规范化载荷确定请求身份，按用例绑定来源对象或冻结版本；同键异载荷不能视为成功。[采购变更提交回放](src/procure_to_pay/change/submit/recovery.rs)核对原回执和原提交，而不是返回当前审批状态。遇提交结果未知、回执冲突或瞬态事务冲突时，按入口合同只读查证原命令和当前授权，不重执行写入、不改选最新审批实例；不能确认原结果时，不把未知提交当作已回滚，调用方重试须保留原幂等键和载荷。错误边界保留字段校验明细、稳定审批错误码，以及回执冲突、瞬态冲突和提交未知的区别。

[采购发件](src/purchase_correspondence/mod.rs)是独立的外部 I/O 边界：先在事务内核验发送资格和期望的生效采购版本，保存唯一发件占用，只有首次确定提交才调用邮件服务，结果另行写回。重复请求或占用提交未知时只查原记录，不重新发送。下载沿用采购详情授权；发件历史不返回邮件正文或服务凭据。

### 相邻业务的分工

| 相邻领域或编排 | 与本 crate 的交接 |
| --- | --- |
| [`erp-sales`](../../domains/erp-sales/README.md)、[`erp-order-to-cash`](../erp-order-to-cash/README.md) | 持有销售需求、询价与销售修订，负责销售生效和变更；销售流程调用本 crate 的责任计划及采购同步入口，不由采购流程修改销售业务内容。 |
| [`erp-supplier`](../../domains/erp-supplier/README.md)、[`erp-supply`](../../domains/erp-supply/README.md)、[`erp-supplier-catalog`](../erp-supplier-catalog/README.md) | 提供供应商身份与商务结算版本、SKU 供给及报价事实；[专项报价采用](src/procure_to_pay/inquiry_supply.rs)复用目录编排的供给资格能力。 |
| [`erp-inventory`](../../domains/erp-inventory/README.md)、[`erp-fulfillment`](../../domains/erp-fulfillment/README.md)、[`erp-fulfillment-execution`](../erp-fulfillment-execution/README.md) | 各领域拥有库存余额、预占和履约单据。本 crate 从选源或采购生效触发预占、草稿和任务；实收、发货、交付与验收的执行和过账由履约编排负责。 |
| [`erp-supplier-orders`](../erp-supplier-orders/README.md) | 采购生效时只在原事务[登记 API 供给执行意图](../erp-supplier-orders/src/supplier_api_orders/enqueue.rs)，手工供给不入队。供应商订单编排在事务外发单、跟踪并查证未知结果。应用组合根装配具体供应商协议适配器，本 crate 不依赖应用入口或供应商协议实现。 |
| [`erp-finance`](../../domains/erp-finance/README.md)、[`erp-finance-posting`](../erp-finance-posting/README.md)、[`erp-reverse-flow`](../erp-reverse-flow/README.md) | 财务领域拥有应付、付款及成本事实；采购生效只形成应付与成本，采购经办人的跟进日也不是付款到期日。付款申请、实际付款与核销由财务过账编排办理，退货和退款由逆向编排办理；它们按用例在原事务回调本 crate 的付款安排、询价费用来源或退货款票同步步骤。 |

## 验证

业务代码改动按[后端开发规则](../../../AGENTS.md#验证)执行定向 Clippy 和 `erp-procure-to-pay` 库单元测试，不运行集成测试、忽略测试或依赖真实服务的验收例程。仅修改本文档时，只对本文件运行 `git diff --check`。
