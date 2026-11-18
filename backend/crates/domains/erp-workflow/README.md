# erp-workflow：ERP 审批与工作项

本 crate 把通用审批流程接入 ERP：定义单据怎样进入审批，把流程计算结果保存为可查询的审批记录和待办，并管理工作项的责任与状态。审批通过后如何让采购单生效、如何释放付款占用等，仍由拥有业务规则的领域及其 Process 处理。

## 职责

- **审批政策与定义。** 按单据类型登记是否需要审批、对应的流程种类、审批资格、快照字段和业务动作。`ApprovalDefinitionService` 管理定义及发布版本；需要审批的单据绑定已发布定义，已绑定的单据不因后续发布自动切换版本。
- **审批运行与持久化。** `ApprovalRuntimeService` 提供实例、进度和材料读取，以及提交决定、恢复受阻实例、受阻取消和绑定升级等入口。本域维护 BPM 模型的 MongoDB 仓储、命令回执、冻结主体快照、审批抄送和通知 Outbox；发送通知不在审批写事务内执行。
- **单据登记与工作项。** `DocumentRegistryService` 管理单据、参与者及单据关系；工作项领域维护责任、来源链路和状态，`WorkItemService` 处理转派、受控关闭和放弃返工，`WorkItemReadService` 提供本域列表、详情和统计。工作项记录责任，不代替审批决定或所属业务的专用办理命令。

## 目录结构

公开入口及重导出见 [src/lib.rs](src/lib.rs)。

| 位置 | 内容 |
| --- | --- |
| [service/approval](src/service/approval/mod.rs) | [政策登记](src/service/approval/policy.rs)、[定义管理](src/service/approval/definition.rs)、冻结绑定、审批人解析、抄送和[运行服务](src/service/approval/execution/runtime_service.rs)。 |
| [service/approval/execution](src/service/approval/execution/mod.rs) | 将资格与持久化事实装配为 BPM 命令，再把引擎返回的计划转成 ERP 写入。`prepare_*` 不是事务提交入口。 |
| [service/document_registry](src/service/document_registry/mod.rs)、[service/work_item](src/service/work_item/mod.rs) | 单据登记与关系读取；工作项命令、查询和授权接入。 |
| [entity](src/entity/mod.rs)、[dto](src/dto/mod.rs) | 本域实体、值对象和状态规则；审批、单据登记与工作项的输入和视图。 |
| [ports](src/ports/mod.rs)、[authorization](src/authorization/mod.rs) | 外域业务事实、来源授权和审计等端口；审批实例的授权事实与查询映射。审批业务动作端口见 [action.rs](src/service/approval/action.rs)。 |
| [repository](src/repository/mod.rs)、[indexes](src/indexes/mod.rs) | MongoDB 集合访问、查询及索引注册；[BPM 仓储](src/repository/bpm.rs)只读写流程模型，不调用引擎或执行 ERP 业务动作。 |

## 架构设计

### 从采购单提交到最终审批

1. **业务 Process 发起。** [采购提交](../../orchestration/erp-procure-to-pay/src/procure_to_pay/submission.rs)在自己的事务中复验采购单对象权限，识别原命令回放，然后由[提交准备](../../orchestration/erp-procure-to-pay/src/procure_to_pay/submission/preparation.rs)推进采购领域状态、准备冻结提交资料，并读取绑定的定义图和启动回执。本域的[定义与回执读取](src/service/approval/read.rs)按有序作用域取首个命中回执；命中不代表载荷和结果已核验。业务入口核对定义版本、命令身份与专属资格后，用[启动输入装配](src/service/approval/execution/start_input.rs)冻结节点审批人及其资格。
2. **BPM 只计算。** 本域 `prepare_start`、`prepare_decision` 等入口调用 [bpm](../../platform/bpm/README.md)。引擎使用传入的图、状态、ID、时间和资格，返回 `TransitionPlan`、事件及任务意图，决定推进、终结或受阻等结果；它不查询账号或数据库、不创建 ERP 工作项，也不执行业务单据动作。
3. **共有启动步骤落地。** [采购启动写入](../../orchestration/erp-procure-to-pay/src/procure_to_pay/start_approval/start_persist.rs)先用审批回执唯一键仲裁并发，再保存采购提交；[erp-approval-runtime](../../orchestration/erp-approval-runtime/README.md)在同一 `Executor` 上调用本域仓储保存实例、入口执行和审批人绑定，通过注入的 `ApprovalRuntimePort` 冻结材料、保存主体快照和启动抄送，最后按计划创建继承该快照链路的入口任务。这个编排库还复用绑定和材料授权复验、撤回抄送及精确启动回放；回放只认原回执指定的实例，不改选最新实例，也不重做提交写入。普通单据撤回的[运行事实与输入构造](src/service/approval/execution/cancel_prepare.rs)在本域，全程沿用调用方执行器。
4. **工作项触发审批决定。** [HTTP 入口](../../../apps/erp-server/src/http/handler/approval_instance/mod.rs)转交 `ApprovalRuntimeService::submit_decision`。[决定用例](src/service/approval/execution/runtime_service/decision_apply.rs)按任务 ID 和期望版本核对当前责任链，经统一授权及当前、下一节点所需的资格复验后调用引擎形成决定计划。新决定先写唯一回执；最终通过或终态驳回按政策调用 `ApprovalDomainActionPort`。[应用动作注册表](../../../apps/erp-server/src/wiring/approval_dispatch/action_registry.rs)把采购最终通过分派到 [erp-procure-to-pay](../../orchestration/erp-procure-to-pay/README.md) 的[采购生效步骤](../../orchestration/erp-procure-to-pay/src/procure_to_pay/formalization_root.rs)，业务写入复用审批事务。随后本域按版本及状态条件推进实例与节点执行，完成或关闭原任务、按计划创建新任务，并保存通知 Outbox 和审计。或签节点首个有效决定结束节点，其余开放任务在同一事务中关闭；终态回放仍须核验原决定身份和当前权限，不再次执行业务动作。

本域直接使用 [erp-identity 的公开授权服务](../erp-identity/README.md#业务授权-api)，并提供本域对象事实与查询映射；其他业务领域的来源事实、授权就绪检查、审计及动作实现由 Process 或应用接入。本域不依赖业务 Process、`erp-approval-runtime` 或应用。跨域材料与履约聚合由 [erp-read-models](../../read-models/erp-read-models/README.md) 组装，不能借任务查询权限绕过材料授权。术语与统一分工见[总索引](../../README.md#代码中常见名称的含义)。

**付款路径的边界。** 采购付款申请以 `purchase_payment_request` 独立审批。驳回终止原审批，由[财务业务步骤](../../orchestration/erp-finance-posting/src/finance_posting/payable/payment_request/cancel.rs)释放占用，不自动启动下一轮审批。`supplier_payment` 是实际付款记录，不再发起审批。[付款执行步骤](../../orchestration/erp-finance-posting/src/finance_posting/payable/payment_request/execution.rs)只接受绑定已批准付款申请的任务，按本申请未付额度更新和完成；旧 `payable_account` 直付任务不具备执行资格。

### 工作项的办理边界

以[受控关闭](src/service/work_item/close.rs)为例：`WorkItemService` 在写事务中先对真实任务重验管理权限，再检查回执、期望版本和允许关闭的任务类型；如果提供替代任务，还要核验其权限、类别和状态。业务证据、任务终态、审计和回执同事务保存。这个入口不推进 BPM，也不能代替审批决定或付款等业务办理命令。

## 任务查询与关联链路

- 列表、检索、详情和统计由 [WorkItemReadService](src/service/work_item/read/mod.rs) 提供，复用[工作项 DTO](src/dto/work_item/mod.rs)，调用方保留固定的 `BoxFuture` 边界。
- 处理人、来源销售单和来源采购单筛选只接受稳定 ID。处理人姓名仅在授权后的展示批次按账号 ID 向 identity 批量补齐，不参与任务关键词匹配。
- 来源筛选匹配 `WorkItem.links` 同一元素中的类型与 ID；同字段的多个 ID 取并集，不同字段取交集。它仅收窄已授权的任务，不授予来源单据读取权限，也不在历史链路缺失时回退到跨域查询。
- 创建任务或更新其业务版本时，写流程在原事务保存直接对象及完整上级链路。审批入口、后续节点和驳回返工任务继承匹配实例、对象及提交版本的冻结快照，不从当前草稿重建历史链路。发布前按[任务链路迁移合同](../../../docs/work-item-links-migration.md)审计并回填历史数据。

## 任务统一授权

应用在[授权组合根](../../../apps/erp-server/src/wiring/authorization/mod.rs)登记固定的 `work_item:list`、`work_item:detail`、`work_item:manage` 映射和对象事实；`WorkItemReadService::new` 接收已装配的 `AuthorizationService`。权限规则由 identity 解释，调用方不传入自定义政策或 Mapping。

| 入口 | 授权执行 |
| --- | --- |
| 列表、检索、统计、焦点任务 | `query` → WorkItem Mapping → 按授权过滤候选 → 计数与分页 → `validate`；焦点 ID 不扩大候选范围。 |
| 单条详情、业务模块检查可读性 | 详情用 `detail` 的查询凭证按精确 ID 读取并复验；业务模块可通过本域入口调用 `require` 检查精确任务 ID。 |
| 转派、关闭、放弃返工、替代任务、幂等回放 | 在写事务中对真实任务执行 `require(work_item:manage)`；有替代任务时也核验其管理授权。展示资格和历史回执不能跳过此检查。 |
| 管理按钮 | 只查询当前已授权展示批次中的 `manage` 任务 ID，不以读取资格代替管理资格。 |
| 履约聚合队列 | [履约队列](../../read-models/erp-read-models/src/workbench/fulfillment_queue.rs)从 `QueueAccess::read_access` 取得完整约束，先过滤已授权任务，再关联业务事实、计算指标和分页，返回前复验授权。 |

**映射与办理合同：**

- `Owned` 匹配当前 `owner_user_id`；组织范围按授权时点查找该负责人的唯一有效主属组织，不以 `owner_organization_id` 代替。负责人和主属组织是根对象必需事实；已确认没有值返回 `FactValue::Known(None)`，不能以未读取代替。
- 组织关联复用 identity 的查询片段和成员索引，按任务负责人查询，不预加载全体负责人。重复有效主属关系在范围查询中被排除，对象检查返回事实错误。授权编译、查询和复验沿用同一事务、授权时点、组织版本和有效期边界；跨页版本包含读取与管理查询依赖。
- Mine、Managed、History 均先通过 `list` 授权，再应用各自的视图筛选；历史参与不为列表增加授权分支。来源材料的读取资格、授权就绪状态和业务办理条件分别检查，不用于扩大范围或改变本人待办计数。
- 任务接收、财务负责人和任务执行的完整功能资格由 identity 固定入口核验；同角色要求与权限组合只在 identity 维护。本域提供真实任务种类、来源和责任事实；对象授权之外，所属用例仍须检查岗位分离、业务状态、冻结责任行、来源是否存在及主体版本。
- `TaskAccess` 的管理、执行和决定 ID 只为当前展示批次投影，不在动作视图中保存角色权限集合，也不重新解释创建人、历史参与者和负责人的读取规则。展示提示不等于命令授权，实际写入必须重新核验。

实现位置：[WorkItem Mapping](src/repository/work_item/authorization.rs)、[授权复验与动作投影](src/service/work_item/read/identity.rs)、[对象事实端口](src/service/work_item/authorization/facts.rs)。

## 修改与验证

不涉及 I/O 的不变式放在实体或值对象；Service 或 Process 划定原子写入边界，Repository、Port 和业务步骤复用调用方的 [`Executor`](../../foundation/persistence-core/README.md#读取边界)。查询变更同步检查索引，应用复用本域公开索引入口并保持注册顺序；扩展公开合同须检查 Process、ReadModel 和应用调用方。

仅修改本文档时，对本文件运行 `git diff --check`。代码改动按[后端开发规则](../../../AGENTS.md#验证)和[统一质量门禁](../../README.md#质量门禁)验证；本 crate 的库单测命令是 `env -u ERP_TEST_MONGO_URI cargo test -p erp-workflow --lib --locked`（在 `backend/` 执行，不运行集成测试或真实外部服务测试）。
