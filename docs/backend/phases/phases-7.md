# Phase 7：应收应付、票款核销、卡券复核与成本事实

## 1. 分支与隔离

| 项目 | 约定 |
| --- | --- |
| 分支名 | `codex/backend-p1-07-finance-ledger` |
| 基线 | 与全部 phase 相同的冻结 `BACKEND_PHASE_BASE_SHA` |
| 实现语言 | 读取冻结基线中的统一后端语言/版本决定；缺失时停止，不自行选栈 |
| 独占目录 | `backend/modules/finance-ledger/**` |
| 编译要求 | 不要求根工程编译；账务不变量、端口和测试向量必须完整 |
| 禁止修改 | 根构建、共享 API/OpenAPI、真实迁移、其他 phase、前端 |

## 2. 目标与唯一写者

本 phase 独占以下第一期财务事实：

- 应收账户、应收分录及冲减；客户回款及分配；统一发票、销项票分配；
- 应付账户、应付分录及冲减；供应商付款及分配；进项票分配；
- 卡券销售单期初/增量票款逐单复核链；
- `cost_entry`、`cost_allocation`、直接履约费用、实际成本和成本冲减；
- 客户退款、供应商退款、回款/付款冲正、红票等财务纠错事实；
- W11、W12、W13 的查询、命令和安全字段投影。

W15/W16 分析投影由 Phase 9 负责；本 phase 只提供不可变财务事实快照。

依据：`erp-phase-1.md` §6.4、§7.5、§8.7、§9、§10；
`erp-data-model.md` §6.8～§6.11、§8.3；W11、W12、W13。

## 3. 模块结构

```text
backend/modules/finance-ledger/
  domain/{receivable,payable,receipt,payment,invoice,allocation,card_review,cost,correction}/
  application/{commands,queries}/
  ports/
  contracts/
  persistence-spec/
  fixtures/
  tests/
  DECISIONS.md
```

## 4. 核心命令

### 4.1 客户侧

- `OpenReceivableFromSalesFact`、`AppendReceivableDelta`；
- `PostCustomerReceipt`、`AppendReceiptAllocation`；
- `RegisterSalesInvoice`、`AppendSalesInvoiceAllocation`；
- `ReverseReceipt`、`RegisterRedSalesInvoice`、`CreateCustomerRefund`。

### 4.2 供应商侧

- `OpenPayableFromPurchaseFact`、`AppendPayableDelta`；
- `PostSupplierPayment`、`AppendPaymentAllocation`；
- `RegisterPurchaseInvoice`、`AppendPurchaseInvoiceAllocation`；
- `ReversePayment`、`RegisterRedPurchaseInvoice`、`CreateSupplierRefund`。

### 4.3 卡券复核与成本

- `OpenCardFundsReview`、`RefreshCardFundsReviewSubject`、
  `CompleteCardFundsReview`、`DeferCardFundsReview`；
- `RecordConfirmedCost`、`RecordActualCost`、`RecordDirectFulfillmentCost`、
  `AppendCostReduction`、`AllocateCost`。

## 5. 财务不变量

- 金额为十进制定点：金额两位、单价最多四位、数量最多六位；逐行舍入再汇总，
  发票尾差单独记录。应收应付与收付款为含税，利润输入同时提供不含税口径。
- 应收、应付、回款、付款、销项票、进项票是六类独立事实。
- 资金核销和发票核销是两条独立多对多轨道，不能以一条累计金额覆盖。
- 分配金额均为正，动作只用 `APPLY` / `REVERSE`；已过账分配不可更新或删除。
- 回款只核销相同结算主体的应收；付款只核销相同供应商的应付，禁止跨主体。
- 提交时按固定顺序锁资金/发票、账户、分录并重算双侧余额；禁止超资金、超开放余额。
- 应收/应付减少必须显式 offset；如已有核销，同事务先追加所需反向分配。
- 蓝票、红票及反向引用清楚；红票不删除原票，部分红冲保留剩余有效额。
- 退款、冲正和红票互不替代；累计纠错不得超过原事实。
- 正式事实按业务事实键幂等，命令按请求幂等键重放同一结果。

### 5.1 卡券第一期边界

- 卡券期初应收由正式销售快照成交金额派生，已收和已开票固定从 0 开始。
- 复核必须逐单登记真实回款/发票，或以非空受控证据确认“从 0 起”；不建零金额假单。
- 复核链单根递增；链尾通过且 `subjectHash` 与当前销售版本、应收、净回款和净发票一致
  才有效。相关事实变化产生新的 `SYNC_DELTA` 复核，旧结论不复制。
- 卡券直接印刷/仓储/配送费用可记录，但实际供货成本第一期不录入，卡券成本未覆盖
  不等于零成本，绝不能形成卡券实际利润。

## 6. 独立端口

- `SalesReceivableFactPort`、`PurchasePayableFactPort`；
- `FulfillmentCostFactPort`、`MallVoucherSalesFactPort`；
- `WorkItemTransactionPort`、`AuthorizationPort`、`AuditPort`、`AttachmentPort`；
- `OutboxPort`、`OperationResultPort`。

本 phase 保存自己需要的不可变来源快照或 recording fixture，不 import 销售、采购、
履约、同步或任务实现。Phase 10 将端口绑定为同库事务或可靠 outbox 消费。

## 7. 测试要求

1. 定点金额、税额守恒、舍入和发票尾差。
2. 应收/应付来源事实幂等创建、版本差额和冲减。
3. 回款/付款多对多、跨主体拒绝、超额拒绝和并发重算。
4. 资金与发票分配独立；销/进项、蓝/红票类型不混用。
5. offset 前反向分配、退款/冲正/红票累计上限和原事实保留。
6. 卡券期初零基线、复核链不分叉、指纹变化失效、重复任务完成。
7. 成本阶段、分配守恒、卡券成本排除和“未覆盖不是零成本”。
8. 权限不足时销售只能看到授权合计，不能通过错误/导出泄露供应商成本。

## 8. 未决项与 fail-closed

- W13 Q1：默认排序策略未确认时只能使用 Phase 1 返回的统一待办优先级/到期顺序，
  财务域不得自行按客户或账龄重排并改变处理顺序。
- W11 Q1、W12 Q1：是否必须一次分配完未确认；由服务端策略明确，缺失时禁止
  自动全分配，允许的剩余余额行为必须显式返回。
- W11 Q3、W12 Q2：普通回款/付款是否需要复核未确认；不得擅自增加或跳过审批。
- W11 Q4：发票号码规范化规则未固化时不做危险的自动去重合并。
- W12 Q3：仓储默认只能见付款门禁状态和原因，不返回具体金额。
- W12 Q4：混合来源应付优先策略缺失时禁用自动混合分配，要求显式选择或分组。
- W13 Q2～Q4：岗位分离、证据白名单和移动端正式动作均由服务端策略控制。
- W13 Q5：驳回复核的后继任务未登记，驳回只完成当前任务并返回
  `REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED`，不得创建临时任务。

## 9. 完成标准

- 六类财务事实、两条核销轨道、纠错和卡券复核都有守恒/并发/幂等测试。
- 卡券实际供货成本和实际利润没有被第一期代码启用。
- 仅写独占目录，不直接修改其他 phase 的事实或共享持久化。
- 向 Phase 10 交付逻辑约束、事务锁顺序、端口、错误码、测试向量和未决策略。
