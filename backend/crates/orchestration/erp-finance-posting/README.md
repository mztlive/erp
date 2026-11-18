# erp-finance-posting

## 职责

本 crate 负责编排财务跨域写入：采购付款申请与供应商付款、客户回款与核销、开票申请、进项和销项发票过账，以及手工成本和销售附加费用登记。它把来源单据、审批或执行任务、财务事实、附件关系和审计衔接到相应的业务事务中。

- [erp-finance](../../domains/erp-finance/README.md) 拥有应收、应付、收付款、发票、成本及分配事实，维护金额、额度和状态规则、仓储与命令回执。过账编排负责从采购、销售、供应商等领域取得所需事实并协调写入，不另实现核销和分摊规则。
- [erp-workflow](../../domains/erp-workflow/README.md) 定义审批政策和工作项规则；本 crate 实现审批达到相应结果时的财务动作，例如核销客户回款或为已批准的付款申请派发出纳任务。
- [erp-read-models](../../read-models/erp-read-models/README.md) 负责财务中心、跨域列表和报表读取。[实际经营盈亏报表](../../read-models/erp-read-models/src/finance/actual_profit_loss/mod.rs) 按读取权限和筛选口径聚合正式收入与成本；它不执行过账。Process 可以复用读模型取得来源事实或命令结果视图，但不维护报表口径。

## 目录结构

| 源码入口 | 用途 |
| --- | --- |
| [`src/lib.rs`](src/lib.rs) | 公开财务过账模块、附件预检入口、端口及错误类型。 |
| [`src/ports.rs`](src/ports.rs)、[`src/errors.rs`](src/errors.rs) | 应用注入的财务能力和错误映射。 |
| [`finance_posting/payable/`](src/finance_posting/payable/mod.rs) | 应付、供应商付款与回单、进项发票过账；[`payment_request/`](src/finance_posting/payable/payment_request/mod.rs) 处理采购付款申请的提交、撤回及批准后的执行。 |
| [`finance_posting/receivable/`](src/finance_posting/receivable/mod.rs) | 应收、客户回款、销项发票登记与红冲；[`invoice_request/`](src/finance_posting/receivable/invoice_request/mod.rs) 处理开票申请及批准额度的使用。 |
| [`finance_posting/cost/`](src/finance_posting/cost/mod.rs)、[`finance_posting/extra_cost/`](src/finance_posting/extra_cost/mod.rs) | 手工成本登记、销售附加费用凭证登记与冲减。 |
| [`task_access.rs`](src/finance_posting/task_access.rs)、[`command_recovery.rs`](src/finance_posting/command_recovery.rs) | 核对正式任务与办理动作；统一部分财务命令的提交查证和回读错误分类。 |
| [`src/attachment_upload.rs`](src/attachment_upload.rs) | 对象存储写入前的固定财务工作流资格预检。 |

## 架构设计

### 权限与应用装配

`PayableProcess`、`PurchasePaymentRequestProcess` 和 `ReceivableProcess` 接收应用注入的 `FinancePostingPorts`，不在编排内部构造身份、数据范围或工作流适配器。[服务端装配](../../../apps/erp-server/src/wiring/finance_posting.rs) 分别通过 `finance_posting_ports(db, rbac, authorization)` 和 `receivable_posting_ports(db, authorization)` 接入同一个进程共享的 `AuthorizationService`。共享服务不保存当前账号、事务或命令状态；用例在当前 `Executor` 上使用本次操作人的身份核对真实对象、完整来源或精确任务，菜单资格不能代替完整授权。

[`ApprovalRuntimePort`](../erp-approval-runtime/src/runtime_port.rs) 提供审批材料授权、定义绑定与复验、启动快照和撤回抄送；`FinanceTaskPort` 提供工作项资格校验和财务责任人解析。实际付款和开票以正式任务为依据，核对任务关联、责任人和版本，不以客户端传来的任务或来源声明代替取证。

### 事务与审批后的过账

Process 建立业务写事务；若由审批决策触发，则沿用审批原事务，不另行提交。授权、版本和金额检查以及相关仓储、端口、附件元数据和审计写入复用同一 `Executor`，任何一步失败都不留下部分业务结果。对象存储、供应商协议等外部调用不放入数据库事务。

- **采购付款。** [提交付款申请](src/finance_posting/payable/payment_request/submit.rs) 时，核对采购来源，冻结生效版本、账户与逐采购金额，占用应付余额，并在同一事务保存本轮提交快照、启动审批和记录审计。最终批准只授予冻结额度并[派发指定出纳的执行任务](src/finance_posting/payable/payment_request/execution.rs)，不产生付款。出纳办理时，事务重新核对申请、任务、冻结收款账户与来源，先扣减申请占用，再[核销应付、写入付款分配和过账审计](src/finance_posting/payable/posting.rs)，并同步任务进度。
- **客户回款。** [提交回款](src/finance_posting/receivable/customer_receipt_submit.rs) 时按已绑定的审批定义冻结本次提交材料；只有[审批最终通过](src/finance_posting/receivable/customer_receipt_posting.rs)才在审批事务内核销应收、写成功审计并刷新相关销售单的回款进度。相比之下，[开票申请批准](src/finance_posting/receivable/invoice_request/execution.rs)只授予开票额度并派发任务，额度在实际开票时消耗。

### 幂等、查证与审计

供应商付款、进项发票登记等命令通过 `erp-finance` 的[独立命令回执](../../domains/erp-finance/src/service/command_receipt.rs)记录原命令身份、载荷指纹与结果引用，并与正式财务事实及关联的成功审计同事务提交。重放时核对原幂等键和原载荷，按当前权限回读原对象；同键异载荷不能视为成功，重放不再次过账或补写成功事件，也不通过反查审计推断是否提交。销售附加费用采用派生的成本事实主键识别原命令；手工成本的业务唯一键用于拒绝重复登记，并非所有命令都走同一回执路径。

提交结果未知时只查证原命令；不能确认结果时保留原未知错误，不把它解释为已回滚，也不改变共享授权状态。调用方重试须保留原幂等键与原载荷。以付款为例，结果详情按原任务和冻结申请版本重新授权；详情读不到，不等于付款或附件未提交。

## 附件存储前准备

应用组合入口先调用 `FinanceAttachmentService::new(db, authorization).prepare_attachment_upload(&actor, upload)`，通过后才写入对象存储。`FinanceAttachmentUpload` 只接受固定工作流：客户回款、销项发票草稿、销项发票过账、进项发票、销售附加费用和采购付款。[identity](../../domains/erp-identity/README.md#业务授权-api) 统一维护对应的完整功能资格及同角色要求。此预检仅检查功能资格，不证明具体对象、来源或任务可操作，不产生可复用的写入凭证，也不得额外要求独立的 `file_asset:upload` 菜单权限。文件存储后的业务写事务仍须用当前账号、真实目标及该事务的同一执行器重新完成完整授权，并在原业务事务中登记本批文件及写入业务关系。

供应商付款由 [HTTP 付款入口](../../../apps/erp-server/src/http/handler/purchase_payment_request/payments.rs)调用 [`PayableProcess::commit_purchase_payment_request_with_assets`](src/finance_posting/payable/payment.rs)，只能从一份已批准的申请办理。旧应付直付入口通过 `reject_client_payment` 在上传前拒绝。首次办理必须使用本批上传的银行回单，命中原成功回执时不消费新文件。付款结果将 `assets_committed` 与详情回读结果分开；[对象存储补偿](../../../apps/erp-server/src/http/handler/file_asset/compensation.rs)只清理本次确认未消费的对象，提交未知或已确认提交后回读失败时不得误删。
