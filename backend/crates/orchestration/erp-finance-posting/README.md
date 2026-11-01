# erp-finance-posting

财务过账编排负责供应商付款、采购付款申请、客户回款、开票申请、发票及附加费用的跨域写入。

- 财务实体、金额与分配规则归 `erp-finance`；审批和任务规则归 `erp-workflow`。
- `PayableProcess`、`ReceivableProcess`、`PurchasePaymentRequestProcess` 必须接收应用装配的 `FinancePostingPorts`，不得在编排内部构造具体身份、范围或工作流适配器。
- 供应商付款和采购付款申请由应用 `finance_posting_ports(db, state.rbac())` 装配；供应商付款的局部事务身份必须与共享授权实例隔离。
- 客户往来由 `receivable_posting_ports(db)` 装配，每次创建独立身份实例，不得注入应用全局 RBAC。内部新建客户回款命令必须通过 `fresh_receivable()` 重新创建局部实例。
- 一次命令的提交结果未知状态不得污染其他命令或应用全局权限状态。
- 审批材料、快照、定义绑定和撤回抄送经 `ApprovalRuntimePort` 执行；任务资格和财务责任经 `FinanceTaskPort` 执行。
- 付款附件入口 `commit_supplier_payment_with_assets` 必须在原付款事务登记本批文件；提交确认后的视图读取失败不得丢弃文件提交状态。
- 同一业务事务必须沿用同一 `Executor`，保留原写入顺序、冻结版本、审计身份、错误载荷和命令回执恢复分类。
- 文件存储、供应商协议与其他外部调用必须位于数据库事务之外。

验证执行内联库单元测试与仓库提交门禁；禁止集成测试、ignored 测试、验收 examples 和真实外部服务。
