# erp-finance-posting

财务过账编排负责供应商付款、采购付款申请、客户回款、开票申请、发票及附加费用的跨域写入。

- 财务实体、金额与分配规则归 `erp-finance`；审批和任务规则归 `erp-workflow`。
- `PayableProcess`、`ReceivableProcess`、`PurchasePaymentRequestProcess` 必须接收应用装配的 `FinancePostingPorts`，不得在编排内部构造具体身份、范围或工作流适配器。
- 供应商付款和采购付款申请由应用 `finance_posting_ports(db, rbac, authorization)` 装配，`authorization` 必须来自进程共享的完整授权服务。
- 客户往来由 `receivable_posting_ports(db, authorization)` 装配；授权服务不得保存当前账号、事务或命令状态。内部用例使用同一共享授权实例和本次执行器。
- 一次命令的提交结果未知状态不得污染其他命令或应用全局权限状态。
- 审批材料、快照、定义绑定和撤回抄送经 `ApprovalRuntimePort` 执行；任务资格和财务责任经 `FinanceTaskPort` 执行。
- 付款附件使用 `PayableProcess::commit_purchase_payment_request_with_assets`，仅从已批准付款申请办理；旧应付直付 HTTP 入口通过 `reject_client_payment` 在上传前拒绝。原付款事务登记本批文件，提交确认后的视图读取失败不得丢弃文件提交状态。
- 同一业务事务必须沿用同一 `Executor`，保留原写入顺序、冻结版本、审计身份、错误载荷和命令回执恢复分类。
- 文件存储、供应商协议与其他外部调用必须位于数据库事务之外。

验证执行内联库单元测试与仓库提交门禁；禁止集成测试、ignored 测试、验收 examples 和真实外部服务。

## 附件存储前准备

组合入口先调用 `FinanceAttachmentService::new(db, authorization).prepare_attachment_upload(&actor, upload)`，通过后才允许写入对象存储。`FinanceAttachmentUpload` 只接受固定财务工作流，identity 统一维护对应完整功能资格和同角色要求。

该检查只恢复原业务入口的功能资格预检，不证明具体对象、来源或任务可操作。不得额外要求独立 `file_asset:upload` 菜单权限。文件存储后的业务写事务仍须使用当前账号、真实目标和同一执行器重新完成完整授权，随后执行文件登记及业务写入。
