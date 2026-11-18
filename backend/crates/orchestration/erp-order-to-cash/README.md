# erp-order-to-cash

## 职责

本 crate 编排销售单、销售变更、询价与选品的跨域用例。Process 决定调用哪些领域、按什么顺序执行，以及哪些写入必须一起提交；业务实体、状态规则和集合仍由所属领域维护。例如，`erp-sales` 决定工作副本能否提交，Process 负责让冻结提交、审批运行事实、工作项和成功回执在同一事务中写入。

| 流程 | 生产用例 |
| --- | --- |
| `SalesOrderCommandProcess` | 实物及服务、卡券销售单的建单、工作副本保存、提交、撤回、作废和责任交接。建单可以只存草稿，也可以直接提交；客户、合同、可售商品及询价或选品来源在写入时重验。最终审批通过后生成正式版本与应收，实物及服务单还联动供给分配。 |
| `SalesChangeProcess` | 沿原销售单办理变更草稿、提交和撤回。最终通过时生成新正式修订，处理非零应收差额和相关财务任务，撤回受影响的审批中采购并同步采购依据及供给分配任务。 |
| `SalesInquiryProcess` | 销售提交需求后生成采购责任任务；采购按当前任务办理补充、内部报价、费用依据和反馈发布，销售接受指定轮次或重询。询价转销售走销售建单入口，在建单事务内重验来源、占用可转数量并保存来源关联。 |
| `SalesSelectionProcess` | 创建、复制、准备和发布选品册，协调客户、商品池和图片资产；受理访客个人选择的暂存和提交。选品方案作为开单依据时，由销售命令重验并冻结来源。 |

## 目录结构

| 入口 | 用途 |
| --- | --- |
| [Cargo.toml](Cargo.toml)、[src/lib.rs](src/lib.rs) | 依赖与公开模块；`authorization_bindings` 向应用导出询价授权装配入口。 |
| [src/order_to_cash/](src/order_to_cash/mod.rs) | `SalesOrderCommandProcess`；[建单写入](src/order_to_cash/command/create/persist.rs)、[提交](src/order_to_cash/command/submit.rs)、[交接](src/order_to_cash/handover.rs)、[询价转单](src/order_to_cash/inquiry.rs)、[选品方案开单](src/order_to_cash/command/selection.rs)和[首次正式化](src/order_to_cash/formalize.rs)。 |
| [src/sales_change/](src/sales_change/mod.rs) | `SalesChangeProcess`；[命令](src/sales_change/commands.rs)、[提交回执](src/sales_change/submission_receipt.rs)及[生效过账](src/sales_change/posting.rs)。 |
| [src/sales_inquiry/](src/sales_inquiry/mod.rs) | `SalesInquiryProcess`；[命令事务](src/sales_inquiry/execution.rs)、[销售与采购动作](src/sales_inquiry/execution/apply.rs)、[任务](src/sales_inquiry/tasks.rs)和[回执恢复](src/sales_inquiry/recovery.rs)。 |
| [src/sales_selection/](src/sales_selection/mod.rs) | `SalesSelectionProcess`；[管理端范围校验](src/sales_selection/scope_checks.rs)、[访客访问及办理](src/sales_selection/public_access.rs)、[客户、目录和图片适配](src/sales_selection/adapters.rs)。 |
| [src/services.rs](src/services.rs)、[src/repository/](src/repository/mod.rs)、[src/errors.rs](src/errors.rs) | `SalesServices` 注入合同、询价授权事实与静态映射、编排错误分类；业务集合的仓储不迁入本 crate。 |
| [src/background.rs](src/background.rs) | `SalesSelectionTaskAdapter` 以 `sales_selection_prepare` 接入 `background-runtime`；[应用 worker](../../../apps/erp-server/src/worker/mod.rs)注册并管理任务生命周期。 |

## 架构设计

### Process、领域与读模型

- [erp-sales](../../domains/erp-sales/README.md#依赖与协作边界)维护销售单工作副本、冻结提交、正式版本、销售变更以及询价和选品的业务规则与数据。Process 调用所属领域的 Service、Repository 和窄 Port，不复制状态规则，不接管外域集合。由[应用组合根](../../../apps/erp-server/src/wiring/sales.rs)装配共享授权、客户与合同访问、商品池、文件、审批和财务责任能力，并注入 `SalesServices`；本 crate 不反向依赖应用入口或供应商协议实现。编排间依赖遵循[后端 crate 接入要求](../../README.md#依赖与接入要求)。
- 通用询价读取走 [InquiryReadService](../../read-models/erp-read-models/src/sales_center/inquiry/mod.rs)，分别提供销售安全视图和采购责任任务视图；选品册和方案的列表、详情走 [SalesSelectionReadService](../../read-models/erp-read-models/src/sales_center/selection/mod.rs)。销售单、销售变更的跨域展示分别走 [SalesOrderReadService](../../read-models/erp-read-models/src/sales_center/order/mod.rs) 和 [SalesChangeReadService](../../read-models/erp-read-models/src/sales_center/review/mod.rs)。开单依据、费用转换候选、访客个人页、图片访问和命令结果回读是特定用例的受控入口，不替代通用列表或详情；例如[销售命令结果](src/order_to_cash/command/result.rs)在新的授权快照中按固定结果读取权限调用读模型，不额外要求详情菜单权限。

### 身份与授权

- `AuditActor` 表示已认证的实际操作人，不等同于业务负责人或组织。创建销售单、询价和选品册时从服务端取得业务组织；交接时显式校验接收人及目标业务组织，只转交开放验收任务，不改写历史归属或开放审批任务。审批绑定、提交及来源快照保留当时的身份和版本，不能在重试时换成当前版本。
- 权限常量由 `erp-permissions` 定义，判断由共享的 [AuthorizationService](../../domains/erp-identity/README.md#业务授权-api)执行。询价的 [Facts 与 Mapping](src/repository/sales_inquiry_access.rs)由[应用授权装配](../../../apps/erp-server/src/wiring/authorization/inquiries.rs)注册。写入按本次动作在原事务重验范围和版本：销售单分别校验单据、客户、合同等对象，变更沿来源销售单授权；询价销售侧按询价范围，采购侧按当前责任任务、任务版本及责任行办理，历史任务不会授予写权限。路由认证或事前检查不能替代事务内授权；查询在同一读取阶段完成 `query`、取数并在交付前复验凭证。选品访客走链接、密码及个人授权，不沿用管理端权限。

### 事务、审批与外部动作

- 命令入口为原子写入开启事务；审批回调使用调用方的事务。领域步骤、仓储及 Port 沿用原 `Executor`，不再单独提交。销售建单把销售单、稳定明细、首份工作副本和成功记录一起提交；选 `intent=SUBMIT` 时还同时写入冻结提交与审批运行事实。询价命令把需求或反馈、相关任务变更、成功审计和强类型回执一起提交。执行器的选择见[持久化读取边界](../../foundation/persistence-core/README.md#读取边界)。
- 审批政策和工作项归 `erp-workflow`，`bpm` 负责通用流程计算，Process 把审批结果接到销售业务动作。[ApprovalRuntimePort](../erp-approval-runtime/src/runtime_port.rs)复用已发布定义绑定和重验、材料授权、不可变快照及启动和撤回抄送。销售单按单据类型和冻结提交版本启动审批，销售变更使用同一运行合同；详见[销售启动](src/order_to_cash/start_approval.rs)、[变更启动](src/sales_change/start_approval.rs)及[销售撤回](src/order_to_cash/cancel_approval.rs)。
- 最终通过由[应用审批动作注册表](../../../apps/erp-server/src/wiring/approval_dispatch/action_registry.rs)分派回 Process，在原审批事务内完成[销售正式化过账](src/order_to_cash/formalization_posting.rs)或[销售变更生效](src/sales_change/posting.rs)。采购联动所需的财务任务责任经 [FinanceResponsibilityPort](../../domains/erp-workflow/src/ports/finance_responsibility.rs)在原事务推进，不改为异步消息。
- HTTP、S3、外部通知等副作用不得放入数据库事务；需与写入一致时用 outbox。选品建册时原子登记准备任务；[准备任务](../../domains/erp-sales/src/service/sales_selection/prepare_queue.rs)领取后在事务外计算陈列并复制图片快照，再在写事务内重验任务与册的版本并提交陈列。管理端图片读取先核对册权限和资产归属，再从对象存储读取字节。
- [错误转换](src/errors.rs)保留领域错误分类、审批稳定错误码以及重复回执、瞬态事务冲突、提交结果未知和已提交但结果不可读之间的区别。按原操作人、幂等键和请求指纹核对回执，同键异载荷不能重放。提交结果未知时只查证原回执并重验当前读取资格，不自动重跑写入，也不以新审批实例替换原实例；无法证实时保留未知结果，调用方使用原幂等键重试。选品公开写入的[有限重试](src/sales_selection/public_retry.rs)只处理已走回滚路径的事务体瞬态冲突，不重试未知提交或业务版本冲突。

## 验证要求

仅修改文档时，按[后端开发规则](../../../AGENTS.md#验证)只对本文件运行 `git diff --check`。修改业务代码时，在 `backend/` 运行 `cargo test -p erp-order-to-cash --lib` 和 `cargo clippy -p erp-order-to-cash --all-targets`，并遵守[质量门禁](../../README.md#质量门禁)；仅验证库单元测试，不运行集成测试或依赖真实 MongoDB、S3 等外部服务的验收例程。
