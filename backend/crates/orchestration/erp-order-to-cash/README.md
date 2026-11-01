# erp-order-to-cash

本 crate 负责销售、询价与选品的跨领域命令与事务协调。

- 必须通过拥有领域的 Service、Repository 与 Port 执行业务步骤。
- 事务以下步骤必须复用调用方的 `Executor`；外部 I/O 必须位于事务之外。
- 权限、原始错误分类、冻结身份、命令回执与未知提交恢复合同必须保持。
- 应用负责装配共享具体 adapter，并向各流程注入 `services::SalesServices`；本 crate 不得依赖应用入口或供应商协议实现。
- 审批绑定、材料授权、冻结快照及撤回抄送必须通过 `erp-approval-runtime::ApprovalRuntimePort` 复用原有步骤。
- 采购联动所需财务任务责任必须通过 `FinanceResponsibilityPort` 在原事务中推进；不得改成异步消息。
- 选品后台任务使用本库 `background::SalesSelectionTaskAdapter` 接入 `background-runtime`；应用负责生命周期。
- 开发验证执行 `cargo test -p erp-order-to-cash --lib` 与 `cargo clippy -p erp-order-to-cash --all-targets`；禁止真实服务验收例程。
