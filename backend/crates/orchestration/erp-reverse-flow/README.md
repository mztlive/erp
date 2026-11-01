# erp-reverse-flow

逆向编排负责销售退货、采购退货、客户退款、供应商退款、回款冲正和付款冲正的跨域写入。

- 退货退款实体与状态规则归 `erp-returns`；财务逆向事实归 `erp-finance`；审批与正式任务归 `erp-workflow`。
- `ReturnsProcess` 必须接收应用装配的 `ReverseFlowPorts`，不得自行构造具体身份、范围或工作流适配器。
- 默认退货退款命令由 `local_reverse_flow_ports(db)` 创建独立身份；客户直退确认和剩余款项方案登记显式使用 `reverse_flow_ports(db, state.rbac())` 共享原应用身份。
- 审批定义绑定、冻结快照、撤回抄送和当前来源资格经 `ApprovalRuntimePort` 复用原合同。
- 采购对象访问和采购退货读取范围分别消费应用注入的本域 `PurchaseAccess` 与 `PurchaseDataScopePort`。
- 采购退款或红冲后的正式财务责任解析消费 `FinanceResponsibilityPort`；付款冲正后的原申请恢复消费 `PaymentRequestRestorationPort`。
- 所有决定性读取、反向分配、原事实状态、任务、命令回执和审计必须沿用根事务的同一 `Executor`，按原顺序执行，首次失败后停止后续步骤。
- 未知提交只能凭原命令身份核查，不得自动重新执行资金步骤。

验证执行内联库单元测试与仓库提交门禁；禁止集成测试、ignored 测试、验收 examples 和真实外部服务。
