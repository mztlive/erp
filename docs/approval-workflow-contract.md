# ERP 单据审批流程与待办责任合同

> 状态：执行合同，待实施
>
> 签署日期：2026-08-17
>
> 生效输入：第 4.3 节政策矩阵、第 4.4 节生命周期矩阵、第 4.5 节唯一试点、第 4.6 节权限双门禁、第 13.3 节 WorkItem 映射、第 16.5 节通知合同
>
> 适用范围：ERP 内需要人工审批的全部固定单据类型
>
> 部署范围：单公司
>
> 实施原则：单据类型预先定义流程，单据创建自动绑定版本，审批实例负责路由，节点执行记录每次进入，`work_item` 只表达当前个人责任，强类型领域命令负责正式业务事实

## 1. 合同效力

1. 本合同是 ERP 单据审批定义、版本绑定、运行推进、审批待办和审批历史的唯一横向合同。
2. `DocumentType` 是审批流程适用范围的唯一业务键，不得引入其它流程范围维度或单据实例级临时流程。
3. `erp-phase-1.md`、`erp-phase-2.md` 负责定义单据准入、审批资格、岗位分离和最终业务结果；
   `erp-data-model.md` 负责定义物理字段、索引和事务不变量；W 文件负责页面布局和交互。
4. 其它文档不得另行定义流程选择、审批节点增减、审批人选择、节点推进、驳回目标或审批任务责任。
5. 发生冲突时，以本合同为准，并在同一实施批次同步修正文档、数据模型、API、权限、前端和测试。
6. 本合同替代下列旧规则：
   - 审批流程由编译期注册表和部署清单固定；
   - 审批节点使用 `POOL` 或角色解析器动态选择处理人；
   - 人工任务存在团队责任池、未分派任务、领取、开始处理和退回团队语义；
   - 采购二次确认与低毛利上级确认作为审批之外的团队业务任务与提交准入环节；
   - 启动审批时预创建全部 `WAITING` 步骤实例；
   - 驳回申请人并把审批实例置为 `REJECTED` 终态；
   - `ApprovalRuntimeKind::Bpm` 仅表示外部 BPM 运行时；
   - 以 `RETRY_CURRENT_STEP` 作为通用阻塞恢复命令；
   - 同一 `SalesOrder` 类型内按 `BusinessType` 暗中选择审批链；
   - 个人工作台与统一待办队列是两个独立页面；
   - 新旧审批运行时共存、双写或数据迁移共存期。

7. 责任模型固定为**唯一一种**：任何时刻的任何一条人工任务都恰好属于一个具体用户。全系统不存在责任池、团队任务、未分派任务、候选人集合、领取、抢单、开始处理和退回团队。任务责任只由服务端按已发布审批定义或强类型系统规则分配，用户不得自行取得或放弃责任。

## 2. 系统边界

### 2.1 固定单据类型

1. 系统支持的正式单据由强类型 `DocumentType` 枚举确定，固定为第 4.3 节矩阵列出的 20 个值。
2. 新增单据类型必须先修改领域枚举、单据注册表和审批政策，再允许创建审批流程。
3. `SalesOrder` 按 `BusinessType` 拆分为两个独立 `DocumentType`：`BusinessType::Voucher` 对应新增枚举值 `VoucherSalesOrder`，`BusinessType::GoodsService` 对应保留的 `SalesOrder`。销售单创建用例必须以对 `BusinessType` 的穷尽 `match` 分派到唯一 `DocumentType`；两者各自拥有独立的已发布定义、审批链、政策、权限和验收批次。任何按 `BusinessType` 在同一 `DocumentType` 内暗中选择流程的实现均为阻断。
4. 审批管理页面必须从服务端读取固定单据类型目录，不得允许管理员输入任意字符串创建单据类型。
5. 单据类型必须显式声明以下审批政策之一：
   - `NO_APPROVAL`：该类型不启动审批流程；
   - `PROCESS_REQUIRED`：该类型创建单据前必须存在可绑定的已发布审批流程。
6. 缺少显式审批政策时必须失败关闭，不得默认为无需审批。

### 2.2 单公司约束

1. 审批定义的发布唯一性只按 `document_type` 计算。
2. 同一 `document_type` 同时最多存在一个可供新单据绑定的 `PUBLISHED` 定义版本。
3. 组织、部门、岗位和 DataScope 只用于权限、审批资格和可见范围校验，不参与流程定义唯一键。
4. 本阶段不得引入公司级、组织级、部门级或项目级流程覆盖优先级。

### 2.3 领域职责

| 对象 | 唯一职责 | 禁止职责 |
| --- | --- | --- |
| `approval_process_definition` | 保存某个 `DocumentType` 的不可变审批流程版本 | 保存具体单据或运行状态 |
| `approval_node_definition` | 保存定义版本内的人工审批节点和指定审批人 | 保存审批结果或改变运行时审批人 |
| `approval_transition_definition` | 保存节点在事件发生后的唯一流向 | 执行业务领域动作 |
| 业务单据 | 冻结创建时绑定的流程定义版本 | 临时增减节点、换人或选择下一节点 |
| `approval_process_instance` | 保存单据审批运行状态、当前轮次和当前节点执行 | 代替业务单据保存正式业务状态 |
| `approval_node_execution` | 保存节点每次进入、处理和结束的不可变运行历史 | 承担队列查询责任 |
| `approval_instance_assignee` | 冻结实例内各节点的当前有效审批人 | 改变定义结构或节点顺序 |
| `work_item` | 表达当前节点由哪个具体用户处理 | 决定流程路由或独立完成业务动作 |
| 强类型领域命令 | 校验并形成单据正式业务结果 | 自行创建流程节点或审批任务 |
| `workflow_action` | 保存面向单据的追加式动作审计 | 作为流程状态源 |

### 2.4 分层与依赖方向（已签署）

流程领域与 ERP 业务必须分层，依赖方向单向固定：

```text
apps/web-api -> services
services     -> bpm + entities + database
database     -> bpm + entities
entities     -> bpm
bpm          -> entity-core + entity-macros + 外部基础库
```

固定职责：

1. `crates/bpm` 是无 ERP 语义、无 I/O 的下层流程领域与纯状态引擎。它拥有流程定义、节点、连线、运行实例、节点执行、实例审批人和命令收据模型，以及图规则和状态计算。它必须禁止依赖或引用 `entities`、`database`、`services`、`apps/web-api`、`mongodb`、`axum`，也不得包含 `DocumentType`、权限、DataScope、WorkItem、业务动作、HTTP DTO、通知投递或数据库 `Executor`；
2. `services::approval` 是 ERP 适配与事务编排层。它拥有 `DocumentType` 政策、`DocumentType -> ProcessKind` 穷尽映射、业务对象到 `SubjectRef` 的构造、授权重验、强类型业务动作、WorkItem 适配、审计与通知意图，并在唯一 MongoDB 事务内应用 BPM 计划和全部业务副作用；
3. `entities` 拥有 ERP 业务实体、单据审批绑定、业务对象快照、WorkItem 和通知 outbox 集成模型；
4. `database` 是 BPM 与集成模型的 MongoDB 适配层，只做映射与原子读写，不决定流程状态；
5. BPM 引擎只根据完整输入返回 `TransitionPlan` 和中性领域事件。它不得自行开启事务、读取时钟、生成 ID、访问 Repository 或调用业务回调；其输出不得包含 ERP URL、权限名、业务命令或通知模板。

任何文档不得把 BPM 描述为 MongoDB Repository、HTTP 服务、权限执行器、WorkItem 服务或业务动作回调容器。

## 3. 第一阶段交付范围

### 3.1 必须交付

1. 按固定 `DocumentType` 提前创建审批流程。
2. 审批流程草稿、校验、发布、退役和历史版本查询。
3. 严格串行的单令牌人工审批流程。
4. 每个审批节点固定指定一个具体用户。
5. 单据创建时自动绑定当前已发布定义版本。
6. 单据提交时启动已绑定定义版本的审批实例。
7. `APPROVE` 和 `REJECT` 两种审批决定。
8. 任一节点驳回后，以同一单据提交版本进入下一轮第一节点。
9. 节点每次进入都创建新的执行记录，不覆盖既有历史。
10. 每个活动节点创建一个指定到人的开放审批任务。
11. 幂等、乐观锁、事务、权限重验、岗位分离和不可变审计。
12. 审批人失效时阻塞实例，并在原审批人重新合格后受控恢复。
13. 为后续工作台提供唯一当前责任、当前轮次、当前节点和流程历史投影。

### 3.2 明确不交付

- 条件分支；
- 并行网关；
- 会签、或签；
- 加签、减签；
- 审批运行时转交、改派、转签或委托；
- 抄送；
- 定时器和超时自动流转；
- 自动服务节点；
- 子流程；
- 任意跳转、任意退回和审批人选择退回目标；
- 脚本、表达式、动态代码和任意回调 URL；
- BPMN 图形设计器；
- 外部 BPM 引擎接入；
- 单据创建或提交时临时增减节点、调整顺序、选择审批人或切换流程版本。

需要新增上述能力时，必须先扩展定义模型、发布校验和执行器合同，不得在页面或业务 Service 中增加旁路逻辑。

## 4. 单据类型审批政策

### 4.1 政策注册

每个 `DocumentType` 必须注册唯一审批政策。审批政策至少包含：

| 字段 | 约束 |
| --- | --- |
| `document_type` | 固定 `DocumentType` 枚举值 |
| `approval_requirement` | `NO_APPROVAL` 或 `PROCESS_REQUIRED` |
| `definition_admin_permission` | 创建、编辑和发布该类型流程所需的类型级权限 |
| `runtime_admin_permission` | 管理视图、应急撤回、恢复原审批人和受阻取消所需的类型级权限 |
| `approver_eligibility_policy` | 校验指定用户能否审批该类单据的强类型策略 |
| `separation_of_duties_policy` | 提交人、经办人和审批人之间的岗位分离规则 |
| `required_node_purposes` | 发布时必须恰好满足的 ERP 节点用途集合；无必备用途时为空 |
| `subject_version_source` | 冻结提交版本的权威字段，见第 4.4 节 |
| `subject_snapshot_fields` | 启动时冻结的有界业务快照字段，见第 4.4 节 |
| `work_item_owner_role` | 审批任务 `owner_role` 的唯一稳定取值 |
| `owner_organization_source` | 审批任务 `owner_organization_id` 的责任组织来源字段 |
| `start_action` | 单据提交并启动流程前执行的强类型领域动作 |
| `final_approve_action` | 最终通过时执行的唯一强类型领域动作 |
| `cancel_action` | 审批最终通过前撤回到可修正草稿的强类型领域动作 |

12 个 `PROCESS_REQUIRED` 类型都必须注册实际 `cancel_action`，不支持禁止撤回或另设受阻取消动作。业务撤回与管理员受阻取消共用同一个强类型动作；两者只允许取消 `RUNNING` / `BLOCKED` 实例，已经最终通过并形成业务事实的实例不可取消，原事实只能按对应冲正或变更合同处理。`NO_APPROVAL` 政策只包含 `document_type`、`approval_requirement` 和 `process_kind`，不得包含动作、资格、岗位分离、WorkItem 或取消配置，也不得注册空 Adapter。

1. 审批政策由代码注册，用户不能配置任意业务动作。
2. 流程结构和审批人由管理端配置，不能继续由代码注册固定步骤。
3. 流程引擎不得根据任意字符串动态调用领域 Service。
4. 流程最终通过后必须通过 `document_type` 绑定的强类型领域动作形成正式业务事实。

### 4.2 缺失政策

1. 未注册审批政策的单据类型不得创建需要审批的单据。
2. `PROCESS_REQUIRED` 但不存在可绑定定义时，单据创建必须返回稳定错误 `APPROVAL_PROCESS_NOT_CONFIGURED`。
3. 不得先创建无审批绑定的单据，再由制单人补选流程或审批人。

### 4.3 政策矩阵（已签署）

下表是全部 20 个固定 `DocumentType` 的唯一确定审批政策。实施人员不得增删行、不得留空、不得在编码阶段重新选择。

类型级权限命名规则固定为 `<document_type_snake_case>:approval_definition_admin` 与 `<document_type_snake_case>:approval_runtime_admin`。

| `DocumentType` | 中文名 | `approval_requirement` | 类型级权限前缀 |
| --- | --- | --- | --- |
| `SalesOrder` | 销售单（实物及服务） | `PROCESS_REQUIRED` | `sales_order` |
| `VoucherSalesOrder` | 卡券销售单 | `PROCESS_REQUIRED` | `voucher_sales_order` |
| `SalesChangeOrder` | 销售变更单 | `PROCESS_REQUIRED` | `sales_change_order` |
| `PurchaseOrder` | 采购单 | `PROCESS_REQUIRED` | `purchase_order` |
| `PurchaseChangeOrder` | 采购变更单 | `PROCESS_REQUIRED` | `purchase_change_order` |
| `StockAdjustment` | 库存调整单 | `PROCESS_REQUIRED` | `stock_adjustment` |
| `CustomerReceipt` | 客户回款单 | `PROCESS_REQUIRED` | `customer_receipt` |
| `SupplierPayment` | 供应商付款单 | `PROCESS_REQUIRED` | `supplier_payment` |
| `CustomerRefund` | 客户退款单 | `PROCESS_REQUIRED` | `customer_refund` |
| `SupplierRefund` | 供应商退款单 | `PROCESS_REQUIRED` | `supplier_refund` |
| `ReceiptReversal` | 回款冲正单 | `PROCESS_REQUIRED` | `receipt_reversal` |
| `PaymentReversal` | 付款冲正单 | `PROCESS_REQUIRED` | `payment_reversal` |
| `PurchaseReceipt` | 采购收货单 | `NO_APPROVAL` | 不适用 |
| `Delivery` | 仓发单 | `NO_APPROVAL` | 不适用 |
| `ElectronicDelivery` | 电子交付单 | `NO_APPROVAL` | 不适用 |
| `ServiceFulfillment` | 服务履约单 | `NO_APPROVAL` | 不适用 |
| `CustomerAcceptance` | 客户验收单 | `NO_APPROVAL` | 不适用 |
| `Invoice` | 发票 | `NO_APPROVAL` | 不适用 |
| `SalesReturnCase` | 销售退货单 | `NO_APPROVAL` | 不适用 |
| `PurchaseReturnOrder` | 采购退货单 | `NO_APPROVAL` | 不适用 |

签署依据：`PROCESS_REQUIRED` 的 12 个类型是当前业务状态机中已存在人工复核态（`Pending*Review` 及等价值）的全部类型。8 个 `NO_APPROVAL` 类型本期不新增审批环节：其中 6 个当前从草稿直接过账或确认；`SalesReturnCase`（`PENDING_WAREHOUSE_ACCEPTANCE`、`PENDING_PROCUREMENT`、`PENDING_FINANCE`）与 `PurchaseReturnOrder`（`PENDING_EXECUTION`）虽有人工处理态，但它们是履约与执行分工态、不是审批复核态，因此同样签署为 `NO_APPROVAL`，其状态机不受第 4.4.2 节收敛约束。

`NO_APPROVAL` 类型不得注册审批适配器、不得绑定定义、不得创建审批实例或审批任务。将上表任一 `NO_APPROVAL` 行改为必须审批，必须先修订本合同并新增该类型的第 4.4 节生命周期行。

### 4.4 生命周期矩阵（已签署）

下表是 12 个 `PROCESS_REQUIRED` 类型的唯一确定生命周期。每行取值均为唯一确定值，实施人员不得代为选择。

#### 4.4.1 状态与版本

| `DocumentType` | 创建状态 | 允许提交状态 | 启动后状态 | 最终通过后状态 | `subject_version` 权威来源 |
| --- | --- | --- | --- | --- | --- |
| `SalesOrder` | `CommercialStatus=DRAFT` / `ReviewStatus=NOT_SUBMITTED` | `DRAFT` / `NOT_SUBMITTED` | `PENDING_REVIEW` / `IN_APPROVAL` | `EFFECTIVE` / `APPROVED` | `sales_order_submission.submission_no` |
| `VoucherSalesOrder` | `CommercialStatus=DRAFT` / `ReviewStatus=NOT_SUBMITTED` | `DRAFT` / `NOT_SUBMITTED` | `PENDING_REVIEW` / `IN_APPROVAL` | `EFFECTIVE` / `APPROVED` | `sales_order_submission.submission_no` |
| `SalesChangeOrder` | `DRAFT` | `DRAFT` | `IN_APPROVAL` | `EFFECTIVE` | `sales_change_submission.submission_no` |
| `PurchaseOrder` | `DRAFT` | `DRAFT` | `IN_APPROVAL` | `EFFECTIVE` | 新增 `approval_subject_version` |
| `PurchaseChangeOrder` | `DRAFT` | `DRAFT` | `IN_APPROVAL` | `EFFECTIVE` | 新增 `approval_subject_version` |
| `StockAdjustment` | `DRAFT` | `DRAFT` | `IN_APPROVAL` | `POSTED` | 新增 `approval_subject_version` |
| `CustomerReceipt` | `DRAFT` | `DRAFT` | `IN_APPROVAL` | `POSTED` | 新增 `approval_subject_version` |
| `SupplierPayment` | `DRAFT` | `DRAFT` | `IN_APPROVAL` | `POSTED` | 新增 `approval_subject_version` |
| `CustomerRefund` | `DRAFT` | `DRAFT` | `IN_APPROVAL` | `POSTED` | 新增 `approval_subject_version` |
| `SupplierRefund` | `DRAFT` | `DRAFT` | `IN_APPROVAL` | `POSTED` | 新增 `approval_subject_version` |
| `ReceiptReversal` | `DRAFT` | `DRAFT` | `IN_APPROVAL` | `POSTED` | 新增 `approval_subject_version` |
| `PaymentReversal` | `DRAFT` | `DRAFT` | `IN_APPROVAL` | `POSTED` | 新增 `approval_subject_version` |

关于 `subject_version` 的固定规则：

1. `SalesOrder`、`VoucherSalesOrder`、`SalesChangeOrder` 使用提交时形成的不可变提交记录 `submission_no`；
2. 其余 9 个类型必须在业务实体上新增 `approval_subject_version: u32`，初值 0，每次提交在同一事务内 checked add 1。`PurchaseOrder` 的生效 `purchase_revision.revision_no` 只在最终通过时生成，`purchase_order_submission.submission_no` 与 `purchase_change_submission.submission_no` 又是字符串业务编号，两者均不得充当审批版本；
3. 任何类型都不得使用 `BaseModel.version` 作为 `subject_version`；
4. `subject_version` 在实例启动后永久不可变，驳回和轮次递增都不改变它。

关于销售单提交与审批启动的固定规则：

1. `SalesOrder` 与 `VoucherSalesOrder` 的允许提交状态均为 `DRAFT` / `NOT_SUBMITTED`。销售提交必须直接启动审批实例、冻结 `subject_version`、把 `ReviewStatus` 置为 `IN_APPROVAL` 并创建第一节点的 `DocumentApproval` 任务；
2. 不得设置提交准入阶段。`ReviewStatus` 的提交后继只能是 `IN_APPROVAL`；不得新增「准入已通过」字段、准入状态或第二条启动路径；
3. `SalesOrder` 已发布定义不再要求 `node_purpose=SALES_ORDER_PROCUREMENT_CONFIRMATION`。流程管理员按普通节点配置销售单审批链；空白草稿可预置一个名为「采购确认」的普通节点，允许删除。BPM 推进、决定 DTO 和业务副作用不得按用途分支；
4. `subject_snapshot.submitted_by` 固定记该销售单的提交销售；
5. 12 个 `PROCESS_REQUIRED` 类型的 `on_approval_start` 一律由该类型自身的提交命令调用，不存在由其它环节代为启动的类型。

#### 4.4.2 状态机收敛

本次改造必须删除业务状态机中的逐节点审批态，节点粒度事实唯一存在于 `approval_node_execution`：

1. `ReviewStatus` 固定为 `NOT_SUBMITTED`、`IN_APPROVAL`、`APPROVED` 三值。必须删除 `PENDING_SALES_LEADER`、`PENDING_OPERATIONS`、`REJECTED`、`PENDING_PROCUREMENT_CONFIRMATION` 和 `PENDING_LOW_MARGIN_SUPERIOR`。销售单当前节点只存在于 `approval_node_execution`；
2. `StockAdjustmentState` 的 `PENDING_WAREHOUSE_REVIEW` 与 `PENDING_FINANCE_REVIEW` 合并为唯一 `IN_APPROVAL`；`PurchaseChangeOrderStatus` 的 `PENDING_WAREHOUSE_IMPACT` 与 `PENDING_FINANCE_REVIEW`、`SalesChangeOrderStatus` 的 `PENDING_IMPACT_CONFIRMATION` 与 `PENDING_FINANCE_REVIEW`、`PurchaseOrderStatus` 的 `PENDING_FINANCE_REVIEW`、资金类 6 个类型的 `PENDING_REVIEW` 同样收敛为唯一 `IN_APPROVAL`；
3. 业务状态机不得再出现「第几级审批」语义。页面展示的当前节点、当前审批人和轮次一律取审批实例投影；
4. 驳回不改变业务状态：单据保持 `IN_APPROVAL`，实例进入下一轮。必须删除 `ReviewStatus::REJECTED → NOT_SUBMITTED` 的旧「驳回回到草稿再改单」路径，并禁止驳回触发 `CommercialStatus::PENDING_REVIEW → DRAFT`；该邻接只允许由第 4.4.4 节签署的 `cancel_action` 触发；
5. 12 个类型的撤回和受阻取消一律回到该类型的 `DRAFT` / `NOT_SUBMITTED`，审批驳回又不改变业务状态，因此审批导致的业务 `REJECTED` 已无可达路径，必须删除：资金类 6 个类型的 `REJECTED`、`ReviewStatus::REJECTED`、`SalesChangeOrderStatus::Rejected`、`PurchaseChangeOrderStatus::Rejected`、`StockAdjustmentState::Rejected`，以及采购单上与审批实例事实重复的整个 `PurchaseReviewStatus`（`Pending` / `Approved` / `Rejected`）字段。作废仍由各类型现有 `VOIDED` 表达，审批结果一律取审批实例投影。

#### 4.4.3 采购确认节点与禁用业务环节

本系统不得创建团队业务任务。下列业务概念的目标处置固定为：

| 业务概念 | 禁用 `WorkItemType` | 禁用状态 | 唯一目标处置 |
| --- | --- | --- | --- |
| 采购二次确认 | `ProcurementConfirmation` | `ReviewStatus::PENDING_PROCUREMENT_CONFIRMATION` | 收敛为 `SalesOrder` 已发布定义中的一个普通单人审批节点 |
| 低毛利上级确认 | `LowMarginManagerConfirmation` | `ReviewStatus::PENDING_LOW_MARGIN_SUPERIOR` | 整体删除，不保留任何替代环节 |

**采购确认节点**

1. `SalesOrder` 定义不再强制包含 `node_purpose=SALES_ORDER_PROCUREMENT_CONFIRMATION`。流程管理员按普通节点增删、排序、改名称和指定审批人；是否设置采购相关节点由管理员决定；
2. 该节点的决定只有通过和驳回，请求体只允许第 14.3 节的五个字段。驳回按第 11 章处理：不改变销售单和 `subject_version`，实例轮次加一并回到入口节点；
3. 原双向环 `PENDING_PROCUREMENT_CONFIRMATION ↔ PENDING_LOW_MARGIN_SUPERIOR` 随低毛利环节删除而消失，因此第 3.2 节「不交付条件分支和除驳回回入口以外的环路」的约束不再被违反，该环节成为合法审批节点；
4. **选源事实后移到采购单**：原采购二次确认承载的「选择供应商供给、最新成本、供货数量、预计交期、履约方式」不再属于销售单审批，一律在销售单生效后创建采购单时录入（`erp-phase-1.md` §7.4）。因此审批决定不需要携带任何业务字段，第 14.3 节的决定请求合同保持不变；
5. 必须删除：`ProcurementConfirmation`、`ProcurementConfirmationLine` 实体及其集合、`ProcurementConfirmationStatus`、`ProcurementRejectReasonCode`、`WorkItemType::ProcurementConfirmation` 及其全部 Repository、Service、Handler、路由、索引、前端类型与文案。驳回原因统一使用审批驳回原因文本，不保留独立的采购驳回原因代码枚举；
6. 固定业务口径：销售单生效前不存在已选定的供应商成本，因此生效前的毛利只能是估算值，口径见第 4.4.6 节。
7. `services::approval` 发布时不再要求销售单采购确认用途；其它类型仍不得带用途。运行时不得因用途改变 BPM 路由、决定字段、驳回规则或领域动作。

**低毛利上级确认**

1. 必须删除：`LowMarginManagerConfirmation` 实体及其集合、`LowMarginManagerConfirmationStatus`、`LowMarginManagerConfirmationId`、`WorkItemType::LowMarginManagerConfirmation`、`ReviewStatus::PENDING_LOW_MARGIN_SUPERIOR`，以及 `services/src/sales_review/low_margin_confirmation.rs`、`services/src/sales_order/procurement_rejection.rs` 中的相关编排、对应 Repository 扩展、索引、HTTP 端点、路由、权限项和前端类型与文案；
2. 销售照原条件承接不再需要任何上级确认环节：销售不撤回、不改单，在下一轮审批中由各节点重新决定；
3. 毛利风险本身**不删除**，收敛为只读提示，见第 4.4.6 节。

**卡券运营环节**是 `VoucherSalesOrder` 审批链中的普通单人节点。必须删除 `card_sales_operations_pool`、`card_sales_unique_sales_manager`、「开始处理 / 退回团队」动作和 `WorkItemType::CardSalesOperationApproval`、`CardSalesManagerApproval`。

本节废止后，全系统人工任务只有一种责任模型（第 1 章第 7 条）：每条任务恰好属于一个具体用户。任何文档、代码、页面或测试再出现「团队待处理」「未分派」「责任池」「领取」「开始处理」「退回团队」均为阻断。

#### 4.4.4 强类型动作与撤回合同

| `DocumentType` | `on_approval_start` | `on_final_approve` | `cancel_action` | 最终副作用 |
| --- | --- | --- | --- | --- |
| `SalesOrder` | `SalesOrderService::start_approval_submission` | `SalesOrderService::formalize_approved_submission` | `SalesOrderService::cancel_approval_submission` | 冻结提交修订为正式版本 |
| `VoucherSalesOrder` | `SalesOrderService::start_approval_submission` | `SalesOrderService::formalize_approved_submission` | `SalesOrderService::cancel_approval_submission` | 冻结提交修订为正式版本，并向商城发送执行投影 |
| `SalesChangeOrder` | `SalesChangeOrderService::submit_sales_change` | `SalesChangeOrderService::apply_effective_change` | `SalesChangeOrderService::cancel_approval` | 生成生效修订并改写销售单 |
| `PurchaseOrder` | `PurchaseOrderService::submit` | `PurchaseOrderService::formalize_approved_order` | `PurchaseOrderService::cancel_approval` | 采购单生效，允许执行 |
| `PurchaseChangeOrder` | `PurchaseChangeService::submit_change` | `PurchaseChangeService::apply_effective_change` | `PurchaseChangeService::cancel_approval` | 改写采购单并同步履约影响 |
| `StockAdjustment` | `InventoryService::submit_stock_adjustment` | `InventoryService::post_stock_adjustment` | `InventoryService::cancel_stock_adjustment_approval` | 库存移动过账 |
| `CustomerReceipt` | `ReceivableService::submit_customer_receipt` | `ReceivableService::post_customer_receipt` | `ReceivableService::cancel_customer_receipt_approval` | 应收核销与资金入账 |
| `SupplierPayment` | `PayableService::submit_supplier_payment` | `PayableService::post_supplier_payment` | `PayableService::cancel_supplier_payment_approval` | 应付核销与资金出账 |
| `CustomerRefund` | `ReturnsService::submit_customer_refund` | `ReturnsService::post_customer_refund` | `ReturnsService::cancel_customer_refund_approval` | 客户退款出账 |
| `SupplierRefund` | `ReturnsService::submit_supplier_refund` | `ReturnsService::post_supplier_refund` | `ReturnsService::cancel_supplier_refund_approval` | 供应商退款入账 |
| `ReceiptReversal` | `ReturnsService::submit_receipt_reversal` | `ReturnsService::post_receipt_reversal` | `ReturnsService::cancel_receipt_reversal_approval` | 回款冲正 |
| `PaymentReversal` | `ReturnsService::submit_payment_reversal` | `ReturnsService::post_payment_reversal` | `ReturnsService::cancel_payment_reversal_approval` | 付款冲正 |

1. 12 个类型的 `cancel_action` 成功后，单据一律回到该类型的 `DRAFT` / `NOT_SUBMITTED`，`subject_version` 不回退；修改后重新提交必须递增 `subject_version` 并创建新实例；
2. 原提交人只能在业务撤回规则和服务端 `allowed_actions` 同时允许时撤回；撤回原因一律必填。具备该类型 `approval_runtime_admin` 权限和实例 DataScope 的运行管理员可在原提交人无法处理时应急撤回，但必须走同一业务撤回端口并记录应急代办身份。管理员受阻取消只处理第 12.5 节的非人员一致性 blocker，仍执行同一 `cancel_action`，不得另设终态或动作；
3. 最终通过后实例已是 `APPROVED`，不得调用 `cancel_action`。资金过账、库存移动、正式修订等最终事实只能通过业务变更或冲正单处理；
4. 上表方法名是本合同签署的强类型端口名。现有 `post_*` 领域方法必须被对应 Service 端口包装调用，不得由审批运行时直接调用 Repository 或用 `$set` 绕过领域不变式；
5. 12 个类型的 `on_approval_start` 一律由该类型自身的提交命令在同一事务内调用（第 4.4.1 节提交规则第 5 条）。`SalesOrder` 不再存在由准入环节代为启动的特例，全系统只有一条启动路径；
6. 表中 `SalesChangeOrderService`、`PurchaseChangeService` 是本合同签署的**目标端口名**。当前基线没有这两个结构：`submit_sales_change` 在 `services/src/sales_review/sales_change_order.rs`，`submit_change` 在 `services/src/purchase_order/change.rs` 的 `PurchaseOrderService` 上。实施时按 `docs/dev-plan/domains.md` 的域归属落地端口，结构命名不构成合同偏离。

#### 4.4.5 业务对象快照

`subject_snapshot` 对全部 12 个类型固定为下列有界强类型结构，不得使用任意 Map、未约束 JSON 或 BSON `Document`：

```text
document_no            单据业务编号
responsible_org_id     责任组织（同时是 WorkItem 的 owner_organization_id 来源）
submitted_by           提交人账号 ID
submitted_at           提交时间
counterparty           对手方引用（客户/供应商/仓库，按类型穷尽枚举，可为空）
total_amount           金额合计（资金、销售、采购、变更、退款、冲正类必填）
total_quantity         数量合计（库存调整、销售、采购类必填）
line_count             行数
```

`work_item_owner_role` 与 `owner_organization_source` 对全部 12 个类型固定为：`owner_role` 取该类型第 4.3 节权限前缀对应的稳定角色语义值 `<prefix>_approver`；`owner_organization_id` 取 `subject_snapshot.responsible_org_id`。不得取当前登录人组织或空字符串补位。`owner_role` 只是审计与展示用的稳定语义标签，不是分派依据：责任始终由 `owner_user_id` 唯一表达，不得据 `owner_role` 反推可处理人集合。

#### 4.4.6 毛利风险提示

低毛利上级确认删除后，毛利风险保留为**只读提示**，不构成任何审批环节、任务或阻断：

1. 销售单详情与审批区展示销售单毛利率估算值和「低毛利」风险标记；
2. 估算成本口径固定为：服务端按业务时点有效且当前可供的供给中最低的供给价计算，不展示供应商身份、不展示具体供给条款，也不写回单据字段；
3. 毛利阈值是系统配置项，只影响是否显示风险标记；
4. 风险标记不改变单据状态、不阻断提交、不阻断任何节点的通过，也不创建任务；
5. 销售单生效并按 `erp-phase-1.md` §7.4 创建采购单后，实际成本和实际毛利以采购单事实为准，估算值不再展示为结论。

### 4.5 唯一试点（已签署）

唯一试点 `DocumentType` 固定为 **`StockAdjustment`（库存调整单）**。

签署依据：单一创建入口（`inventory/mod.rs::create_stock_adjustment`）；已存在 create / submit / post 三段命令；两个复核节点足以验证多节点推进、驳回与轮次递增；域内隔离，无 `BusinessType` 分流、无商城同步、无资金外部事实；其现有人工 approve 中间旁路正好是本次必须清除的目标。

`StockAdjustment` 未通过 `P6-PILOT` 前，其余 11 个 `PROCESS_REQUIRED` 类型不得开始接入，也不得启用共享开发环境。

### 4.6 权限双门禁（已签署）

每个请求必须同时通过 Handler 动作级权限与 Service 资源级政策门禁。第二道门禁按动作穷尽映射，不得把管理员类型级权限错误施加给普通审批人或原提交人：

| 动作 | Handler 门禁 | Service 门禁 |
| --- | --- | --- |
| 固定类型目录读取 | `approval_process:read` | 返回第 4.3 节 20 行非敏感目录；不得包含定义节点或审批人详情 |
| 定义版本与详情读取 | `approval_process:read` | `PROCESS_REQUIRED` 类型的 `definition_admin_permission` 或 `runtime_admin_permission`；`NO_APPROVAL` 无定义详情 |
| 定义创建、编辑、发布、退役 | 对应 `approval_process:*` | 该类型 `definition_admin_permission` |
| 未提交绑定升级 | `approval_instance:upgrade_binding` | 该类型 `definition_admin_permission` + 单据对象读取权 + DataScope |
| 管理视图、恢复、受阻取消 | `approval_instance:read|resume|cancel_blocked` | 该类型 `runtime_admin_permission` + 实例对象读取权 + DataScope |
| 审批决定 | `approval_instance:decide` | 当前开放任务责任 + 当前审批人资格 + 对象读取权 + DataScope + 岗位分离 |
| 正常撤回 | `approval_instance:cancel` | 原提交人，或具备该类型 `runtime_admin_permission` 的应急运行管理员；两者都必须具备对象读取权、DataScope 且通过业务撤回规则 |
| 实例与历史普通读取 | `approval_instance:read` | 本人发起、本人当前责任或业务对象读取权与 DataScope；不得因此获得管理动作 |

具备动作级权限不得自动获得任一业务对象或 `DocumentType` 的管理权；普通审批责任与原提交人身份也不得反向授予定义管理或运行管理权。两类管理员权限必须进入后端权限种子、生成物和权限目录。政策缺失或应注册的类型权限缺失属于服务端部署不变量错误，固定错误码 `APPROVAL_POLICY_NOT_REGISTERED`，只允许映射为 500 并触发 readiness 失败，不得映射为 4xx。

## 5. 审批流程定义合同

### 5.1 流程定义

`approval_process_definition` 至少包含：

| 字段 | 约束 |
| --- | --- |
| `id` | 不可复用的定义版本主键 |
| `document_type` | 唯一适用的固定单据类型 |
| `definition_version` | 同一 `document_type` 内从 1 单调递增 |
| `name` | 管理和审计名称 |
| `status` | `DRAFT`、`PUBLISHED`、`RETIRED` |
| `entry_node_key` | 唯一入口审批节点 |
| `created_by` / `created_at` | 草稿创建审计 |
| `published_by` / `published_at` | 发布审计 |
| `retired_by` / `retired_at` | 退役审计 |
| `definition_lock_version` | 草稿并发修改使用的持久化乐观锁版本 |

必须建立：

- `(document_type, definition_version)` 唯一索引；
- `document_type + status=PUBLISHED` 部分唯一索引；
- 定义主键唯一索引；
- 按 `document_type + definition_version` 查询历史版本的索引。

### 5.2 节点定义

`approval_node_definition` 至少包含：

| 字段 | 约束 |
| --- | --- |
| `approval_process_definition_id` | 所属定义版本 |
| `node_key` | 定义版本内稳定且唯一 |
| `node_name` | 面向用户的审批层级名称 |
| `node_type` | 第一阶段固定为 `USER_APPROVAL` |
| `node_purpose` | 可空的稳定用途键；BPM 仅按不透明值保存，ERP 发布政策负责解释 |
| `display_order` | 从 1 开始连续递增，仅用于线性编辑和展示 |
| `assignee_user_id` | 提前配置的具体审批用户，必填 |
| `assignee_name_snapshot` | 发布时冻结的显示快照 |

1. 不得保存责任模式字段。`assignment_mode` 与 `AssignmentMode` 枚举已随第 1 章第 7 条整体删除，节点责任只有「指定到具体用户」一种。
2. 不得保存角色池、候选人集合、任意表达式或处理人解析脚本。
3. 同一人员是否允许出现在多个节点，由对应 `DocumentType` 的岗位分离政策决定。
4. 定义中的指定人员不自动获得单据读取权限；发布、绑定、启动和决定时仍必须执行权限及 DataScope 校验。
5. `SalesOrder` 发布不再要求 `SALES_ORDER_PROCUREMENT_CONFIRMATION`；其他 `DocumentType` 仍不得使用该用途。已有草稿或已发布版本上的遗留用途不阻断发布，保存时清除。BPM 图规则不得包含 ERP 用途常量。

### 5.3 连线定义

`approval_transition_definition` 至少包含：

| 字段 | 约束 |
| --- | --- |
| `approval_process_definition_id` | 所属定义版本 |
| `from_node_key` | 事件来源节点 |
| `event` | 第一阶段只允许 `APPROVE`、`REJECT` |
| `to_node_key` | 指向下一节点时必填 |
| `terminal_result` | 指向流程终点时固定为 `APPROVED` |

必须建立 `(approval_process_definition_id, from_node_key, event)` 唯一索引，保证同一节点对同一事件只有一个出口。

### 5.4 线性流程生成规则

管理页面只编辑有序审批节点。服务端根据顺序确定性生成连线：

```text
N1 --APPROVE--> N2
N2 --APPROVE--> N3
N3 --APPROVE--> APPROVED

N1 --REJECT--> N1
N2 --REJECT--> N1
N3 --REJECT--> N1
```

1. 页面不得直接提交任意连线图。
2. `REJECT` 的唯一目标固定为 `entry_node_key`。
3. 最后节点的 `APPROVE` 唯一进入 `APPROVED` 终点。
4. 第一阶段不得生成其它循环、分支或并行路径。
5. 底层必须保存节点和连线，不得把运行推进实现为散落的 `sequence_no + 1` 业务判断。

## 6. 定义生命周期

### 6.1 草稿

1. 只有具备该 `DocumentType` 流程管理权限的管理员可以创建和编辑草稿。
2. 同一 `document_type` 同时最多保留一个活动草稿；需要并行草稿时必须先修订本合同。
3. 草稿允许增删节点、调整顺序、修改节点名称和指定审批人。`SalesOrder` 空白草稿在第一次保存节点前可为零节点；页面可预置一个名为「采购确认」的普通节点，允许删除。服务端不再给任何节点盖章用途，也不再阻止删除原采购确认节点；
4. 普通制单人、审批人和单据查看人不得修改流程草稿。
5. 草稿修改必须携带 `expected_definition_lock_version`。

### 6.2 发布校验

定义发布必须同时满足：

1. `document_type` 是已注册的固定类型，且审批政策为 `PROCESS_REQUIRED`；
2. 至少一个、最多二十个 `USER_APPROVAL` 节点；
3. 节点 `display_order` 从 1 连续递增，无重复；
4. 节点编码、名称和指定用户均合法；
5. 所有指定用户账号有效，并满足该单据类型的审批资格和岗位分离政策；
6. 唯一入口节点等于顺序第一节点；
7. 所有节点均从入口可达；
8. `APPROVE` 连线形成一条无环单线，唯一终点为 `APPROVED`；
9. 每个节点的 `REJECT` 连线唯一回到入口节点；
10. 不存在孤立节点、重复连线、其它环路或不支持的节点类型；
11. 最终通过已绑定该 `DocumentType` 的唯一强类型领域动作。

任一校验失败时禁止发布，不得保存部分已发布结构。

### 6.3 发布和退役

发布新版本必须在一个事务中：

1. 锁定目标草稿和该 `document_type` 当前已发布定义；
2. 完成全部发布校验；
3. 冻结定义、节点、连线和审批人快照；
4. 将当前 `PUBLISHED` 版本置为 `RETIRED`；
5. 将目标草稿置为 `PUBLISHED`；
6. 写入发布和退役审计；
7. 提交事务。

`PUBLISHED` 和 `RETIRED` 定义永久不可修改。任何结构或人员变化必须创建更高版本。

## 7. 单据创建与流程绑定

### 7.1 创建时绑定

业务单据至少保存：

| 字段 | 约束 |
| --- | --- |
| `approval_process_definition_id` | `PROCESS_REQUIRED` 单据创建时必填 |
| `approval_definition_version` | 创建时冻结，后续不得由普通业务操作修改 |
| `approval_definition_bound_at` | 绑定时间 |

`PROCESS_REQUIRED` 单据创建必须在同一事务中：

1. 校验创建权限和单据输入；
2. 查询该 `document_type` 当前唯一 `PUBLISHED` 定义；
3. 重验定义结构和全部指定用户仍有效；
4. 创建业务单据；
5. 冻结定义 ID、定义版本和绑定时间；
6. 写入 `approval.definition.bound` 审计；
7. 提交事务。

定义不存在、结构损坏或任一指定用户失效时，单据创建整体失败。

### 7.2 创建与启动分离

创建单据只绑定流程定义，不创建审批实例、节点执行或审批任务。

```text
创建单据
→ 自动绑定已发布定义版本
→ 编辑单据草稿
→ 提交单据并冻结 subject_version
→ 启动审批实例
→ 激活第一节点并创建审批任务
```

草稿阶段不得提前进入审批人的待办。

### 7.3 绑定版本稳定性

1. 单据创建后永久记录创建时绑定的确切定义版本。
2. 后续发布新版本不得静默替换已有单据的绑定。
3. `RETIRED` 定义不得绑定给新单据。
4. 已在定义为 `PUBLISHED` 时完成绑定的未提交单据，仍允许按该 `RETIRED` 版本启动审批。
5. `start_approval` 不接受客户端提交任意定义 ID；服务端必须从业务单据读取已绑定定义。
6. 无有效绑定的 `PROCESS_REQUIRED` 单据不得提交。

### 7.4 未提交单据受控升级

当旧定义人员失效或业务要求统一升级时，仅管理员可以执行“升级未提交单据流程版本”：

1. 目标单据必须仍为未提交、未启动审批状态；
2. 目标版本必须是该 `document_type` 当前 `PUBLISHED` 版本；
3. 必须重新校验全部指定用户、权限和岗位分离；
4. 必须整套替换定义 ID 和版本，不得只修改单个节点或审批人；
5. 必须携带单据和流程绑定的期望版本；
6. 必须记录原定义、新定义、操作人、原因和时间；
7. 普通制单人不得调用该操作。

已启动审批的单据禁止升级绑定定义。

## 8. 审批运行模型

### 8.1 审批实例

`approval_process_instance` 至少包含：

| 字段 | 约束 |
| --- | --- |
| `approval_process_definition_id` | 从单据绑定复制，运行中不可修改 |
| `definition_version` | 从单据绑定复制，运行中不可修改 |
| `document_type` / `business_object_id` | 被审批单据身份 |
| `subject_version` | 被审批的不可变提交版本 |
| `status` | `RUNNING`、`APPROVED`、`CANCELLED`、`BLOCKED` |
| `current_round_no` | 从 1 开始，驳回重启后递增 |
| `current_node_execution_id` | `RUNNING` 或 `BLOCKED` 时指向当前执行 |
| `blocker_code` / `blocked_at` | 仅 `BLOCKED` 时必填 |
| `started_by` / `started_at` / `ended_at` | 运行审计 |
| `instance_version` | 实例乐观锁版本 |

1. `REJECTED` 不是实例终态，不得继续用于表达本合同的驳回语义。
2. 同一单据和 `subject_version` 同时最多一个 `RUNNING` 或 `BLOCKED` 实例；定义 ID 不得进入该唯一性边界，即使绑定漂移也不得为同一业务版本启动第二条活动链。
3. 第一阶段实例只允许一个当前活动令牌，对应唯一当前节点执行。
4. 运行实例不得原地切换定义版本。

### 8.2 实例审批人绑定

`approval_instance_assignee` 至少包含：

| 字段 | 约束 |
| --- | --- |
| `approval_process_instance_id` / `node_key` | 实例内节点唯一身份 |
| `definition_assignee_user_id` | 从发布定义复制，永久保留 |
| `current_assignee_user_id` | 从发布定义复制，必须始终等于 `definition_assignee_user_id` |
| `assignment_source` | 固定为 `DEFINITION` |
| `changed_by` / `changed_at` / `change_reason` | 必须为空；审批运行时不得改变责任人 |
| `assignment_version` | 恢复命令使用的乐观锁版本 |

启动实例时必须为定义中的全部节点冻结实例审批人绑定。普通用户不能修改。

实例审批人绑定的 `assignment_source` 只允许 `DEFINITION`。节点执行另有独立的来源字段，只允许 `DEFINITION` 和 `ASSIGNEE_RECOVERY`：普通进入节点时记为 `DEFINITION`；原审批人恢复时实例绑定及其来源保持不变，仅当次重建的执行记为 `ASSIGNEE_RECOVERY`。两者必须是两个独立枚举，不得共用一个类型，以免实例绑定在类型上能够持有 `ASSIGNEE_RECOVERY`。

### 8.3 节点执行

`approval_node_execution` 至少包含：

| 字段 | 约束 |
| --- | --- |
| `approval_process_instance_id` | 所属运行实例 |
| `node_key` / `node_name` | 冻结节点身份和名称 |
| `round_no` | 本次执行所属审批轮次 |
| `execution_no` | 实例内单调递增的执行序号 |
| `status` | `ACTIVE`、`APPROVED`、`REJECTED`、`CANCELLED`、`BLOCKED`、`SUPERSEDED` |
| `assignment_source` | `DEFINITION` 或 `ASSIGNEE_RECOVERY` |
| `replaces_execution_id` / `ended_reason` | 仅恢复替换链使用；`ended_reason` 固定为 `ASSIGNEE_RECOVERED` |
| `assignee_user_id` / `assignee_name_snapshot` | 本次进入节点时冻结的实际审批人 |
| `decision` / `decision_reason` | 正式决定及原因 |
| `decided_by` / `decided_at` | 决定审计 |
| `activated_at` | 节点进入时间 |
| `blocker_code` / `blocked_at` | 仅 `BLOCKED` 时必填 |
| `execution_version` | 执行乐观锁版本 |

1. 每次流程令牌进入节点必须创建新的执行记录。
2. 不得预创建未来节点的 `WAITING` 执行记录。
3. 不得重置或覆盖已结束执行记录。
4. 同一实例允许同一 `node_key` 在不同轮次出现多条执行记录；同一轮次内也允许因人员恢复出现多条执行记录。
5. 同一实例同时最多一个 `ACTIVE` 或 `BLOCKED` 执行。
6. `SUPERSEDED` 只能由 `BLOCKED` 转入，且只能由原审批人恢复触发，必须同时写入 `ended_at` 和固定 `ended_reason`。`SUPERSEDED` 不属于当前执行状态，不得重新激活，也不得被计入「同时最多一个 `ACTIVE` 或 `BLOCKED`」约束。

必须建立：

- `(approval_process_instance_id, execution_no)` 唯一索引；
- `(approval_process_instance_id, round_no, node_key, execution_no)` 历史索引；不得仅按前三项唯一，否则同轮恢复无法创建新执行；
- 实例内 `status in [ACTIVE, BLOCKED]` 的部分唯一索引；
- 按实例、轮次、执行序号查询历史的索引。

### 8.4 节点进入

运行时进入任一审批节点时必须执行统一节点进入操作：

1. 根据实例和 `node_key` 读取实例审批人绑定；
2. 重新校验当前审批人的账号、任职、审批资格、DataScope 和岗位分离；
3. 校验通过时创建 `ACTIVE` 节点执行和唯一 `OPEN` 审批任务；
4. 校验失败时创建 `BLOCKED` 节点执行，将实例置为 `BLOCKED`，写入结构化阻塞原因，且不得创建猜测任务；
5. 更新实例 `current_node_execution_id`；
6. 节点执行、实例、任务和审计必须在调用方事务内一起提交。

启动、通过进入下一节点和驳回进入下一轮第一节点必须复用同一节点进入规则，不得各自复制人员校验和任务创建逻辑。

## 9. 审批启动合同

`start_approval` 必须由单据提交用例调用，并在一个本地事务中：

1. 锁定业务单据；
2. 校验单据允许提交，形成并冻结 `subject_version`；
3. 从单据读取已绑定定义 ID 和版本；
4. 校验该定义仍与单据 `DocumentType` 一致；
5. 校验定义在单据创建时已合法绑定；
6. 重验全部指定审批人账号、权限、DataScope 和岗位分离；
7. 创建 `RUNNING` 审批实例，`current_round_no=1`；
8. 为全部节点创建实例审批人绑定；
9. 按统一节点进入规则进入第一节点并创建唯一 `OPEN` 审批任务；
10. 执行该单据类型的强类型提交动作；
11. 写入 `workflow_action`、审计和通知；
12. 提交事务。

重验失败时不得猜测审批人、回退角色池或由提交人临时换人。事务必须失败，并返回稳定结构化错误。

重复启动请求必须按幂等键回读同一实例，不得产生第二条运行链。

## 10. 通过合同

`submit_decision(APPROVE)` 必须在一个本地事务中：

1. 锁定审批任务、实例、当前节点执行、实例审批人绑定和业务单据；
2. 校验任务仍为 `OPEN`；
3. 校验当前节点执行仍为 `ACTIVE`，且仍是实例当前执行；
4. 校验当前用户同时等于任务责任人、节点执行审批人和实例节点当前审批人；
5. 校验 `task_version`、`instance_version`、`execution_version` 和 `subject_version`；
6. 重验账号、权限、DataScope、对象读取权和岗位分离；
7. 将当前节点执行置为 `APPROVED`；
8. 将当前审批任务置为 `COMPLETED`；
9. 根据定义读取当前节点唯一 `APPROVE` 连线；
10. 指向下一节点时，按统一节点进入规则创建下一节点执行和审批任务；下一审批人失效时保存当前通过结果，并让下一节点及实例进入 `BLOCKED`；
11. 指向 `APPROVED` 终点时，执行该单据类型的最终强类型领域动作，再将实例置为 `APPROVED`；
12. 写入决定记录、`workflow_action`、审计和通知；
13. 提交事务。

任一写入失败必须整体回滚。最终强类型领域动作必须只执行一次。

若当前用户仍同时等于任务责任人、节点执行审批人和实例节点当前审批人，但第 6 步发现其账号、审批权限、DataScope、对象读取权或岗位分离已经失效，本次决定不得生效。Service 必须在同一事务中把当前执行置为 `BLOCKED`、写入对应人员失效 `blocker_code`、以 `APPROVAL_RUNTIME_BLOCKED` 关闭当前 `OPEN` 任务、把实例置为 `BLOCKED`，并写入审计、通知 outbox 和命令收据；提交后固定返回 `409 APPROVAL_INSTANCE_BLOCKED`。不得以普通权限错误回滚这些阻塞事实。第 11.2 节驳回事务的第 1 步适用同一规则。

## 11. 驳回重启合同

### 11.1 固定语义

`REJECT` 的唯一语义是：

> 完成当前节点执行，以同一单据 `subject_version` 进入下一审批轮次，并从定义的第一节点重新开始。

驳回不得：

- 返回申请人编辑；
- 改变单据内容或 `subject_version`；
- 结束审批实例；
- 允许审批人选择目标节点；
- 允许审批人选择新的处理人；
- 跳过第一节点；
- 重置或覆盖上一轮历史。

### 11.2 事务顺序

`submit_decision(REJECT)` 必须在一个本地事务中：

1. 执行与通过相同的当前责任、版本、权限和岗位分离校验；
2. 要求非空驳回原因；
3. 将当前节点执行置为 `REJECTED`；
4. 将当前审批任务置为 `COMPLETED`；
5. 校验当前节点唯一 `REJECT` 连线指向 `entry_node_key`；
6. 将实例 `current_round_no` 加一；
7. 按统一节点进入规则进入新轮次第一节点；
8. 第一节点审批人有效时创建 `ACTIVE` 执行和唯一 `OPEN` 审批任务，并保持实例状态为 `RUNNING`；
9. 第一节点审批人失效时创建 `BLOCKED` 执行并将实例置为 `BLOCKED`，不得回退角色池或猜测责任人；
10. 保持单据处于审批中且 `subject_version` 不变；
11. 写入驳回决定、轮次迁移、`workflow_action`、审计和通知；
12. 提交事务。

### 11.3 示例

```text
第 1 轮：张三通过 → 李四通过 → 王五驳回
第 2 轮：张三待审批 → 李四 → 王五
```

第一节点本人驳回时，也必须结束当前执行并创建下一轮第一节点的新执行，不得复用原执行记录。

## 12. 取消、阻塞、恢复与受阻取消

### 12.1 取消

1. 取消不是审批决定，只能由业务单据的受控撤回用例调用。
2. 12 个 `PROCESS_REQUIRED` 类型均允许在最终通过前调用。撤回原因必填；调用人必须等于 `subject_snapshot.submitted_by`，或具备该类型 `approval_runtime_admin` 权限。两者都必须通过 `approval_instance:cancel`、对象读取权、DataScope、业务允许撤回、单据版本、实例版本和当前执行版本的事务内重验，任一不满足即失败关闭；运行管理员路径必须另记应急代办身份。
3. 实例为 `RUNNING` 时必须锁定并校验当前唯一 `OPEN` 任务及其版本，成功后关闭该任务；实例为 `BLOCKED` 时只接受人员失效类别，必须证明当前执行没有 `OPEN` 任务、任务版本为空，不得虚构或重开任务。非人员一致性 blocker 只能调用第 12.5 节受阻取消。
4. 成功后将当前执行置为 `CANCELLED`、将实例置为 `CANCELLED`、清空实例当前执行引用，并执行强类型撤回动作。
5. 已经最终通过、已取消或不存在当前执行的实例不得取消。

### 12.2 阻塞

阻塞原因必须结构化持久化，取值固定为下列稳定 `blocker_code`，并按恢复类别归类：

| 类别 | `blocker_code` | 唯一合法恢复方式 |
| --- | --- | --- |
| 人员失效 | `APPROVER_ACCOUNT_INACTIVE`、`APPROVER_EMPLOYMENT_INVALID`、`APPROVER_NOT_ELIGIBLE`、`APPROVER_OUT_OF_DATA_SCOPE`、`APPROVER_CANNOT_READ_SUBJECT`、`SEPARATION_OF_DUTIES_VIOLATION` | 原审批人已重新合格时执行第 12.4 节恢复；仍失效时保持受阻并升级处置 |
| 图或关联损坏 | `DEFINITION_GRAPH_CORRUPTED`、`INSTANCE_LINK_CORRUPTED` | 只允许第 12.5 节受阻取消；禁止切换定义 |
| 任务冲突 | `OPEN_TASK_CONFLICT` | 只允许第 12.5 节受阻取消；禁止重开或删除任务后继续 |
| 版本损坏 | `SUBJECT_VERSION_CONFLICT` | 只允许第 12.5 节受阻取消；禁止改写冻结版本 |
| 内部不变量 | `INTERNAL_INVARIANT_BROKEN` | 无法形成合法取消计划时保持冻结、readiness 失败并前向修复代码；不得直接改库 |

只有人员失效类别允许进入恢复判断。阻塞不得被页面解释为通过、驳回或普通待处理。不得自动回退到角色池或任意管理员。

### 12.3 运行时责任人变更禁止

审批运行时不交付转交、改派、转签或委托能力。系统不得提供候选人查询、动作码、权限项、HTTP 端点、应用端口或页面按钮，也不得通过通用 WorkItem 转交接口改变 `DocumentApproval` 的责任人。人员失效时必须保持 `BLOCKED`，待原审批人重新合格后执行第 12.4 节恢复；需要长期换人时必须发布新的流程定义版本，该版本只影响新创建单据。

### 12.4 恢复原审批人

当人员失效 blocker 的原当前审批人重新满足全部资格时，唯一合法恢复方式是恢复原审批人。

1. 必须具备该 `DocumentType` 的 `approval_runtime_admin` 类型级权限和实例 DataScope；
2. 只接受实例和当前执行均为 `BLOCKED`，且当前 `blocker_code` 属于人员失效类别；
3. 必须校验实例审批人绑定未变化，且当前审批人与旧 `BLOCKED` 执行的审批人一致；
4. 必须重验该审批人的账号、任职、审批权限、对象读取权、DataScope 和岗位分离已全部恢复；任一项仍失效时返回 `APPROVAL_CURRENT_APPROVER_NOT_RECOVERED`；
5. 旧 `BLOCKED` 执行 CAS 为 `SUPERSEDED`，`ended_reason=ASSIGNEE_RECOVERED`，旧任务保持 `CLOSED`，不得重开；
6. 在同一轮次、同一节点下以递增 `execution_no` 创建新的 `ACTIVE` 执行和唯一 `OPEN` 审批任务，执行的来源记为 `ASSIGNEE_RECOVERY`；
7. 实例指向新执行并恢复为 `RUNNING`；实例审批人绑定、绑定来源和定义审批人均不变化；
8. 请求只允许携带期望的实例、执行、分派版本、可空的已关闭任务版本和幂等键。不得接受目标用户、节点、决定或恢复动作枚举。

恢复不是通用重试：它不能处理结构、任务、版本或内部 blocker，不能选择用户，不能沿连线推进，也不能重复执行原审批决定。

### 12.5 受阻取消

非人员一致性 blocker 的唯一业务退出路径是受阻取消。

1. 必须具备该 `DocumentType` 的 `approval_runtime_admin` 类型级权限和实例 DataScope；
2. 只接受实例为 `BLOCKED`，且当前 `blocker_code` **不属于**人员失效类别；
3. 必须执行第 4.4.4 节该类型签署的 `cancel_action`；
4. 成功后当前执行和实例均为 `CANCELLED`，实例当前执行引用清空，业务单据进入第 4.4.4 节规定的状态；
5. 不得修复定义、跳过节点、切换定义版本、改写冻结版本或把原决定标记为成功；
6. 当前损坏已使合法取消计划无法形成时，必须保持冻结、告警并前向修复，不得构造半结构终态。

## 13. 审批任务责任合同

### 13.1 审批任务

审批任务必须满足：

```text
status = OPEN
approval_node_execution_id IS NOT NULL
owner_user_id IS NOT NULL
```

审批任务至少包含：

| 字段 | 约束 |
| --- | --- |
| `approval_node_execution_id` | 审批任务必填且唯一关联当前执行 |
| `owner_user_id` | 当前具体审批人，必填 |
| `business_object_type` / `business_object_id` | 被审批单据身份 |
| `subject_version` | 被审批的冻结提交版本 |
| `status` | `OPEN`、`COMPLETED`、`CLOSED` |
| `task_version` | 独立于单据版本的乐观锁版本 |
| `assigned_at` / `completed_at` / `completed_by` | 责任和完成审计 |

1. 每个 `ACTIVE` 节点执行恰好一个 `OPEN` 审批任务。
2. 每个 `OPEN` 审批任务必须明确属于一个具体用户。
3. 审批任务不得提供“开始处理”“退回团队”或责任池认领动作；这些动作已随第 1 章第 7 条从全系统删除，不存在任何任务类型可以调用它们。
4. 审批任务不得通过公共任务完成接口结束。
5. 只有 `submit_decision(APPROVE|REJECT)` 可以完成审批任务并推进流程。
6. `work_item` 不得选择下一节点或创建后继任务。

### 13.2 非审批任务

1. 非审批类异常处理和协同任务（同步失败、对账差异、资质到期、履约超期等）不属于本合同的 BPM 路由范围，其触发规则和完成动作仍由各自业务合同定义。
2. 但责任模型对它们同样适用且没有例外：每条非审批任务在创建时就必须由强类型系统规则解析出唯一 `owner_user_id`，`assignment_source` 只允许 `SystemRule` 或 `AdminReassign`。不得创建 `owner_user_id` 为空的任务。
3. 系统规则无法解析出唯一责任人时必须失败关闭并告警，不得退化为团队任务、不得挂到组织上、不得留空等待领取。
4. 工作台不再区分「个人任务」与「团队任务」，只有一个「待我处理」口径（第 16.4 节）。
5. 必须删除的通用任务能力：`claim`、`start_processing`、`release_to_team` 及其 HTTP 端点、权限项、查询参数、前端动作与文案；`AssignmentMode` 枚举与 `assignment_mode` 字段；`AssignmentSource` 的 `SelfStart` 与 `AdminRelease` 两个取值；按「未分派 / 团队待处理」过滤的队列视图。`transfer`（管理员改派非审批任务）保留。W02 曾规划的 `processing_state` 若落地，只允许表达「是否可执行」（例如 `APPROVAL_BLOCKED`），不得表达「已领取 / 处理中」。

### 13.3 WorkItem 适配映射（已签署）

审批任务与 `work_item` 的映射固定如下：

| 字段 | 唯一取值 |
| --- | --- |
| `work_item_type` | 新增枚举值 `DocumentApproval` |
| `approval_node_execution_id` | 审批任务唯一关联字段，必填；替换旧 `approval_step_instance_id` |
| `owner_user_id` | 当前实例审批人，必填 |
| `assignment_mode` | 字段与 `AssignmentMode` 枚举整体删除，全系统只有指定到人一种责任 |
| `assignment_source` | 新增枚举值 `ApprovalRuntime` |
| `owner_role` | 第 4.4.5 节签署的 `<prefix>_approver` |
| `owner_organization_id` | `subject_snapshot.responsible_org_id` |
| route family | 复用**已存在**的 `WorkItemFamily::Approval`（`services/src/work_item/dto.rs`），只把 `DocumentApproval` 穷尽映射进该 family，不得新增第二个审批 family；详情路由由 `DocumentType + business_object_id` 决定 |

固定规则：

1. 同一 `approval_node_execution_id` 在全生命周期（`OPEN`、`COMPLETED`、`CLOSED` 合计）最多关联一个 WorkItem；
2. 只有 `ACTIVE` 且责任有效的执行才对应 `OPEN` 任务；`BLOCKED` 执行不得有 `OPEN` 任务；
3. 审批任务只能由审批运行时的决定、恢复和取消端口完成或关闭。仍然存在的通用 `complete`、`close`、`reassign` 对 `DocumentApproval` 必须失败关闭，固定错误码 `APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN`；`claim`、`start_processing`、`release_to_team` 已按第 13.2 节第 5 条从全系统删除，不存在需要针对审批任务单独拒绝的实现；
4. 人员失效进入 `BLOCKED` 时旧 `OPEN` 任务关闭，关闭原因固定 `APPROVAL_RUNTIME_BLOCKED`；恢复成功后为新执行创建新任务，不得重开旧任务；
5. `VoucherSalesOrder` 接入通用审批后，新写路径不得再创建 `WorkItemType::CardSalesManagerApproval` 或 `CardSalesOperationApproval`，旧卡券决定入口必须失败关闭；P0-D 必须删除两个类型、`card_sales_unique_sales_manager`、`card_sales_operations_pool` 两个处理人解析器和卡券审批 handler key；
6. `SalesOrder` 接入通用审批后，新写路径不得再创建 `WorkItemType::ProcurementConfirmation` 或 `LowMarginManagerConfirmation`，对应入口必须失败关闭；P0-D 必须删除两个类型及其旧实现，不保留替代任务类型；
7. `AssignmentSource` 六个现有取值的处置固定为：`StepResolver`、`RecoveryResolver` 随旧审批运行时删除；`SelfStart`、`AdminRelease` 随责任池语义删除；只保留 `SystemRule`、`AdminReassign`，并新增 `ApprovalRuntime`，最终为三值。

## 14. 应用端口与接口合同

### 14.1 定义管理端口

应用层必须提供以下受控能力：

```text
create_definition_draft(document_type, ...)
replace_definition_nodes(definition_id, expected_definition_lock_version, nodes)
publish_definition(definition_id, expected_definition_lock_version)
retire_definition(definition_id, expected_definition_lock_version)
definition_versions(document_type)
```

1. `replace_definition_nodes` 只允许修改草稿。
2. 节点请求允许提交节点名称、顺序和具体审批人，不允许提交处理器、脚本、任意连线或业务动作。
3. 发布由服务端生成并校验连线。
4. 所有定义写操作必须审计。

### 14.2 单据绑定和运行端口

应用层必须提供：

```text
bind_published_definition_on_document_create(...)
upgrade_unsubmitted_document_definition(...)
start_approval(...)
submit_decision(...)
cancel_approval(...)
resume_current_approver(...)
cancel_blocked_approval(...)
```

`resume_current_approver` 实现第 12.4 节，`cancel_blocked_approval` 实现第 12.5 节。必须删除旧的通用恢复命令 `RETRY_CURRENT_STEP` 及其端点，不得保留别名。

业务 Handler、页面、定时任务和领域 Service 不得绕过这些端口自行创建节点执行或审批任务。

### 14.3 审批决定请求

客户端决定请求只允许包含：

```text
work_item_id
decision: APPROVE | REJECT
reason
expected_task_version
idempotency_key
```

服务端必须从 `work_item_id` 推导实例、当前节点执行、单据、定义版本和 `subject_version`。客户端不得提交：

- 下一节点；
- 驳回目标；
- 下一审批人；
- 流程定义 ID；
- 节点执行 ID；
- 任意业务完成动作。

响应必须返回最新实例摘要、当前轮次、当前节点、当前审批人、单据状态和存在时的下一开放任务摘要。

### 14.4 命令幂等

定义写操作、绑定升级、启动、决定、取消、恢复和受阻取消必须共用下列命令收据规则：

1. 收据唯一键固定为 `command_kind + scope_id + idempotency_key`，并保存 canonical payload hash 与不可变结果引用；
2. 收据不存在时才允许执行命令状态前置校验和业务写入；收据存在且 hash 不同必须返回稳定 `409` 幂等冲突；
3. 收据存在且 hash 相同时不得重做业务写入，也不得因原任务已完成、原执行已结束或实例已推进而改报状态冲突；
4. 同载荷回读前必须重验调用者当前动作权限及第 4.6 节对应的 Service 资源门禁，包括适用的责任、资格、类型级权限、DataScope 和对象读取权。失权时返回不泄露资源存在性的 `403` 或 `404`，不得返回收据引用；仍有权时返回收据中的不可变命令结果引用和调用者当前可读的最新视图；
5. 并发插入收据发生 duplicate key 时，当前事务必须整体回滚，并在事务外的新会话按本节规则回读；不得重做领域动作；
6. 响应中的“原结果”只保证原命令是否成功及其产生的实例、执行、任务和终态引用，不保证重放原请求时刻的可变页面快照。

## 15. 权限与安全合同

### 15.1 固定规则

1. 只有流程管理员可以创建、编辑、发布和退役流程定义。
2. 普通制单人只能查看单据创建时绑定的流程，不得修改或选择流程。
3. 普通审批人只能处理当前明确指派给自己的开放审批任务。
4. 流程发布必须校验审批人账号状态、静态审批权限、节点用途完整性和可静态判断的岗位分离。
5. 单据创建必须重验绑定定义及全部指定人员有效性。
6. 审批启动必须再次重验全部指定人员有效性。
7. 审批决定必须重验当前处理人的账号、权限、DataScope、对象读取权和岗位分离。
8. 审批运行时不得改变当前责任人；长期换人必须发布新定义且只影响新单据。
9. 服务端必须完成队列和历史的数据范围过滤；前端不得全量读取后隐藏。
10. 当前审批责任不自动授予业务字段读取权，敏感字段仍按领域权限脱敏。
11. 所有定义变更、绑定、升级、启动、通过、驳回、取消、阻塞和恢复必须写不可变审计。
12. 并发或版本冲突统一返回 `409`；权限不足返回 `403`；配置或输入校验失败使用稳定业务错误码。
13. 定义发布只校验账户有效、静态审批权限、节点间可静态判断的岗位分离和图不变式。具体单据 DataScope、对象读取权和提交人与审批人的动态隔离在单据创建绑定、启动、每次进入节点和审批决定时校验。本合同不签署「审批人必须具备该 `DocumentType` 全组织范围」的规则，因此不得把无具体资源的发布动作描述为完成了实例级 DataScope 校验。

### 15.2 动作级权限（已签署）

HTTP Handler 校验的动作级权限固定为下列 11 个值，不得增删：

```text
approval_process:read
approval_process:create
approval_process:edit
approval_process:publish
approval_process:retire
approval_instance:read
approval_instance:decide
approval_instance:cancel
approval_instance:resume
approval_instance:cancel_blocked
approval_instance:upgrade_binding
```

必须删除旧权限 `approval_instance:recover`、`approval_instance:diagnose` 和卡券专用审批决定权限。`approval_instance:diagnose` 现被 `services/src/approval/scope.rs` 用于受阻列表范围计算，其能力由 `approval_instance:read` 加类型级 `approval_runtime_admin` 完整取代，不得保留为兼容权限。每个动作必须同时通过第 4.6 节签署的对应 Service 门禁；不得把 `definition_admin_permission` 或 `runtime_admin_permission` 当作全部动作的统一第二层。

## 16. 页面合同

### 16.1 审批流程配置

管理页面必须以固定单据类型为目录展示：

| 单据类型 | 审批政策 | 当前版本 | 配置状态 |
| --- | --- | --- | --- |
| 销售单（实物及服务） | 必须审批 | v3 | 已发布 |
| 卡券销售单 | 必须审批 | v2 | 已发布 |
| 采购单 | 必须审批 | v2 | 已发布 |
| 发票 | 无需审批 | — | 不适用 |
| 库存调整单 | 必须审批 | — | 配置缺失 |

目录必须逐行列出第 4.3 节全部 20 个固定类型，`NO_APPROVAL` 行固定显示「无需审批 / 不适用」且不提供任何写入口。`PROCESS_REQUIRED` 但无已发布定义时必须显示为「配置缺失」阻断状态，不得显示为「无需审批」。

管理员可以：

- 创建更高版本草稿；
- 增删草稿节点；
- 调整草稿节点顺序；
- 修改节点名称；
- 为节点选择具体人员；
- 校验并发布；
- 查看历史版本。

管理员不得直接修改已发布版本。

### 16.2 单据页面

单据创建成功后必须只读展示：

```text
审批流程：销售单审批流程 v3

1. 销售审核    张三
2. 财务审核    李四
3. 运营审核    王五
```

页面不得出现增加、删除、排序、换人或选择其它流程版本的入口。

提交确认必须明确展示：

```text
张三 → 李四 → 王五

任一层驳回后，将从张三开始下一轮审批。
```

### 16.3 运行中单据

运行中的单据必须展示：

```text
审批状态：审批中
当前轮次：第 2 轮
当前节点：销售审核
当前审批人：张三
最近驳回：王五
驳回原因：……
```

历史必须按轮次和执行顺序展示，不得把旧轮次记录覆盖成当前状态。

### 16.4 统一工作台

#### 16.4.1 页面归并

原「W01 今日工作台」与「W02 统一待办队列」合并为**唯一一个页面**：

| 项 | 固定值 |
| --- | --- |
| 页面 | W01 · 我的工作台 |
| 路由 | `/workspace` |
| 布局 | 列表 + 详情主从，页内连续处理 |
| 废止页面 | W02；`docs/ui-workspaces/w02-unified-task-queue.md` 标记为已废止并注明能力并入 W01 |
| 旧路由 | `/workspace/tasks` 永久重定向到 `/workspace`，不保留第二个待办入口 |

TaskTabs 身份仍固定为 `workspace:today:{userId}`，登录默认着陆仍为 `/workspace`。顶栏待办角标只指向本页面。

#### 16.4.2 布局与交互

```text
┌ 我的工作台                         数据更新 12:03  [刷新] ─┐
│ [待我处理 12] [超期 3] [受阻 1] [我发起的 5]   搜索  排序  │
│  全部 | 审批 | 履约 | 财务 | 集成                           │
├─────────────────────────┬─────────────────────────────────┤
│▸销售单审批 SO-0031  ●   │ SO-2026-0031                    │
│ 采购确认   SO-0044      │ 客户A·12.8万·8行                │
│ 库存调整   IA-0011      │ 流程 v3 · 第 2 轮               │
│ 回款复核   RC-0203      │ 当前节点：销售审核              │
│ 资质到期   SUP-0007     │ 上轮驳回：王五 / …              │
│                         │ ─────────────────────────────── │
│                         │ [通过][驳回][打开单据]          │
└─────────────────────────┴─────────────────────────────────┘
无任务时不拆栏，主面板只保留一个空态。口径数量在工具条胶囊上，不用独立统计卡。
```

固定规则：

1. **口径胶囊**是筛选器，不是跳转入口，也不是经营 KPI 卡。点击写入 URL 查询参数并筛选左列，不打开第二个页面；
2. **左列**是唯一待办列表，承接原 W02 的全部查询、排序、分页和连续处理能力，条目跨领域混排并按类型页签收敛；全页只渲染一份列表；
3. **右侧详情**展示当前选中任务的单据摘要与审批上下文，只读渲染服务端事实；
4. **处理动作在详情内完成**：审批任务在详情内提交通过或驳回，提交成功后自动选中列表下一条，实现连续处理；
5. **非审批任务**在详情内只展示摘要与「打开单据」，其正式动作仍在对应业务工作面提交，本页面不得提供通用完成表单；
6. 详情区不得提供批量通过、批量驳回或批量转交；
7. 列表和详情的可执行性一律由服务端 `allowed_actions` 与 `action_blockers` 决定，每次提交仍由服务端重验；
8. 刷新、跨页返回和浏览器重开后必须重新查询服务端，不得从本地状态恢复责任事实。

#### 16.4.3 事实来源

本页面必须使用以下服务端事实：

- 待我处理：`OPEN + owner_user_id=当前用户`（审批与非审批任务同一口径，不再分区）；
- 待我审批：上一项中 `approval_node_execution_id != null` 的子集，只作为类型页签，不作为独立数量口径；
- 已超期：待我处理中 `due_at < now`；
- 受阻：待我处理中关联实例或执行为 `BLOCKED`；
- 我发起的审批：`approval_process_instance.started_by=当前用户`；
- 当前审批人：实例当前执行关联任务的 `owner_user_id`；
- 当前轮次：实例 `current_round_no`；
- 审批历史：按实例、轮次、`execution_no` 查询节点执行；
- 其他人的任务：仅具备管理权限时以「管理视图」查询，与本人口径分开计数。

固定约束：

1. 不存在「团队待处理」「未分派」「可领取」口径，服务端不得返回 `owner_user_id` 为空的开放任务；
2. 指标数量必须与列表使用同一权限快照在服务端计算，不得由前端对已加载条目求和；
3. 前端不得根据单据状态或动作日志推断当前责任人；
4. 受阻任务必须使用独立受阻样式并展示阻塞原因与可用恢复路径，不得伪装为普通待办。

### 16.5 通知合同（已签署）

事务内只允许追加通知 outbox，不得在事务内调用外部通知服务。投递由独立 worker 在事务外完成。

| 事件 | 收件人 | 去重键 |
| --- | --- | --- |
| 审批已启动 | 提交人 + 第一节点当前审批人 | `started:<instance_id>` |
| 进入节点 | 该节点当前审批人 | `entered:<execution_id>` |
| 节点通过 | 提交人 | `approved:<execution_id>` |
| 节点驳回 | 提交人 | `rejected:<execution_id>` |
| 实例受阻 | 提交人 + 该 `DocumentType` 具备 `approval_runtime_admin` 的用户 | `blocked:<execution_id>` |
| 原审批人已恢复 | 新执行审批人 + 提交人 | `resumed:<execution_id>` |
| 正常取消 | 取消时的当前审批人 + 提交人；应急撤回时再加执行管理员 | `cancelled:<instance_id>:<round_no>` |
| 受阻取消 | 提交人 + 执行取消的运行管理员 | `blocked_cancelled:<instance_id>` |
| 最终通过 | 提交人 | `completed:<instance_id>` |

固定投递策略：

1. 去重键在 outbox 上唯一，重复事件不得产生第二条消息；
2. 模板参数只允许包含单据类型中文名、单据业务编号、当前节点名称、当前审批人显示名、轮次号和驳回原因摘要。不得包含 Token、金额明细、对手方敏感信息或完整单据数据；
3. 最大重试次数固定为 5，即首次投递加最多 5 次重试，共最多 6 次尝试；第 1—5 次失败后的退避依次为 1 分钟、5 分钟、15 分钟、1 小时、6 小时，第 6 次失败后直接进入 dead letter；
4. 超过最大次数进入 dead letter 并按 `docs/runbooks/approval-workflow.md` 告警，不得静默丢弃；
5. worker 必须以原子条件更新取得有界批次的租约并写入 worker ID 与租约截止时间；租约到期允许其他实例接管；成功投递后不得再次取得该消息。此处的租约是消息投递机制，与已删除的人工任务领取语义无关，不得复用其命名或界面文案；
6. 投递调用必须以去重键调用幂等发送接口，并设置超时和取消。

## 17. 开发环境硬切换

### 17.1 数据边界

项目处于开发期，不存在必须保留的业务数据。本合同**不建设**任何数据迁移能力：

1. 不迁移旧定义、旧步骤定义、旧实例、旧步骤实例、旧审批任务或旧业务单据；
2. 不建设 legacy history 投影、迁移收据、checkpoint、双写或旧运行时回退；
3. 不建设新旧运行时共存期，也不允许新旧运行时同时处理同一单据；
4. 旧审批集合和旧审批字段不得存在生产读取路径。

### 17.2 硬切换顺序

开发环境按下列固定顺序一次性前向切换，不得回退到旧审批模型：

```text
停止全部写业务数据的进程
→ 运行开发业务数据重置 preview 并保存脱敏报告
→ 取得对准确目标和范围的显式确认后运行 reset execute
→ 运行 reset verify，证明旧审批集合、旧审批 WorkItem 和相关业务数据为空
→ 部署只包含新审批模型的后端与前端
→ 创建并验证全部新索引
→ 运行 readiness、权限种子和旧符号清零检查
→ 由流程管理员为第 4.3 节全部 PROCESS_REQUIRED 类型创建并发布定义
→ 先对唯一试点 StockAdjustment 冒烟，再对其余类型冒烟
```

本合同不设置全局「审批运行开关」。启用边界由数据事实天然表达：某 `PROCESS_REQUIRED` 类型不存在已发布定义时，该类型单据创建必须失败关闭并返回 `APPROVAL_PROCESS_NOT_CONFIGURED`；发布定义即该类型实质启用。因此不得新增运行模式配置项、不得引入 `DISABLED`/`ENABLED` 状态，也不得据此建立第二条运行路径。

回退只允许：停止写入、退役全部已发布定义、保留失败证据、修复代码或合同后前向部署；必要时按同一安全合同再次重置开发业务数据。不得恢复旧二进制继续写入、不得重建旧索引、不得把新数据改写成旧结构。

### 17.3 合同同步

实施时必须同步修改：

- `erp-phase-1.md` 中“不建设可配置审批流”“审批定义按代码注册”的旧约束，以及第 4.4、7.1、7.2、7.3 节的采购二次确认与低毛利上级确认流程；
- `erp-data-model.md` 中预创建步骤、驳回终结实例、旧 definition/step 集合，以及 `procurement_confirmation`、`procurement_confirmation_line`、`low_margin_manager_confirmation` 三个集合及其索引；
- `docs/dev-plan` 中代码注册、`POOL` 审批和恢复当前步骤的旧合同；
- W01 与 W02 的页面合同：按第 16.4 节合并为唯一工作台，W02 标记废止；
- `ui-glossary.md` 中「团队待处理」「未分派」「领取」「开始处理」「退回团队」「采购二次确认待办」「低毛利上级确认」等术语；
- OpenAPI、错误目录和权限种子；
- 前端 mock、查询键、任务类型、页面文案和测试夹具。

不得长期保留两套流程定义、驳回或审批任务责任语义。

## 18. 实施顺序

1. 修订并签署本合同及关联业务状态合同。
2. 建立固定 `DocumentType` 审批政策注册。
3. 建立流程定义、节点、连线、版本和发布 Repository。
4. 建立审批流程管理 Service、HTTP、权限和管理页面。
5. 改造业务单据实体及创建事务，自动绑定定义版本。
6. 将步骤实例重构为可重复进入的节点执行。
7. 建立实例审批人绑定和原审批人恢复能力。
8. 重写运行时为基于节点、事件和连线的单令牌执行器。
9. 实现通过和驳回重启事务。
10. 将全部工作项（审批与非审批）改为创建即指定到人，删除责任池、领取、开始处理和退回团队。
11. 对第 4.5 节签署的唯一试点 `StockAdjustment` 完成端到端试点和专用空数据库硬重置演练。
12. 试点通过后，按第 4.3 节矩阵对其余 11 个 `PROCESS_REQUIRED` 类型逐项独立接入新运行时，一个类型一个批次。`SalesOrder` 批次必须停止采购二次确认与低毛利旧路径的新写入并移除其 HTTP 可达性，选源后移到采购单。
13. 全部 20 个类型的 P3/P4 批次完成后执行 P0-D，删除旧注册表、bootstrap、角色解析、审批责任池、团队任务、旧销售确认实现和驳回终态路径。
14. 按第 16.4 节把 W01 与 W02 合并为唯一工作台，基于稳定的流程和责任事实重做信息架构。
15. 完成全量合同、数据、权限、并发和双用户验收后切换。

## 19. 验收门禁

交付必须同时证明：

- [ ] 系统只按固定 `DocumentType` 管理审批流程，不存在公司、组织或单据实例级流程覆盖。
- [ ] 每个 `PROCESS_REQUIRED` 单据类型同时最多一个供新单据绑定的已发布定义。
- [ ] 已发布定义不可修改，人员或结构变化必须发布更高版本。
- [ ] 单据创建自动绑定当前已发布定义版本，定义缺失或人员失效时创建失败。
- [ ] 单据页面不能增删节点、调整顺序、换审批人或切换定义版本。
- [ ] 发布新定义只影响新创建单据，不静默修改已有单据绑定。
- [ ] 已绑定旧定义的未提交单据可以按绑定关系启动；受控升级只能整套换到当前发布版本。
- [ ] 创建单据不产生审批待办，提交单据才启动实例和第一节点。
- [ ] 每个活动节点恰好对应一个指定到人的开放审批任务。
- [ ] 全系统不存在责任池认领、开始处理和退回团队动作；`AssignmentMode`、`AssignmentSource::SelfStart`、`AssignmentSource::AdminRelease` 已删除，`AssignmentSource` 为 `SystemRule`、`AdminReassign`、`ApprovalRuntime` 三值。
- [ ] 任何时刻不存在 `status=OPEN` 且 `owner_user_id` 为空的 `work_item`；该断言在集成测试中以数据库查询证明。
- [ ] 节点通过只沿唯一 `APPROVE` 连线进入下一节点或最终批准。
- [ ] 任一节点驳回都完成当前执行、轮次加一并创建第一节点的新执行和任务。
- [ ] 驳回后实例保持 `RUNNING`，单据和 `subject_version` 保持不变。
- [ ] 第一节点本人驳回也创建下一轮新执行，不复用原执行。
- [ ] 已结束节点执行不可修改，审批历史可以按轮次完整重放。
- [ ] 最终领域批准动作只执行一次，任一事务步骤失败时整体回滚。
- [ ] 重复启动、通过或驳回请求不产生重复实例、执行、任务或轮次。
- [ ] 两个并发审批决定只有一个成功，另一个返回 `409`。
- [ ] 当前审批人失效时不回退角色池，实例进入 `BLOCKED`。
- [ ] 审批运行时不存在转交、改派、转签或委托的端点、权限、动作码和页面入口；通用 WorkItem 转交对审批任务失败关闭。
- [ ] 队列查询和所有写动作均执行服务端权限、DataScope、对象读取权和岗位分离校验。
- [ ] 工作台能直接回答当前轮次、当前节点、当前责任人、最近驳回和下一步，不依赖前端推断。
- [ ] 旧代码注册流程、审批责任池、团队任务、预建 `WAITING` 步骤和驳回终态路径已从生产实现中清零；`claim`、`start_processing`、`release_to_team` 在 `backend` 与 `erp-client` 中命中为 0（开发重置脚本的固定删除条件除外）。合同、实施计划和 P0-D `deletes` 清单允许以删除或禁止条款引用旧名称，不纳入生产实现零命中。
- [ ] 第 4.3 节 20 行政策矩阵在代码中由对 `DocumentType` 的穷尽 `match` 实现，新增类型必触发编译失败或完整性测试失败。
- [ ] `VoucherSalesOrder` 与 `SalesOrder` 各自拥有独立定义、审批链、权限和验收记录；创建时按 `BusinessType` 穷尽分派。
- [ ] 8 个 `NO_APPROVAL` 类型创建成功且无绑定、无实例、无审批任务，且未注册空适配器。
- [ ] 第 4.4.1 节 12 行 `subject_version` 权威来源全部生效；新增 `approval_subject_version` 的 9 个类型均不复用 `BaseModel.version`，`PurchaseOrder` 不使用最终通过后才生成的 `purchase_revision.revision_no`。
- [ ] 第 4.4.2 节状态机收敛完成：业务状态机中不存在逐节点审批态，驳回不改变业务状态。
- [ ] 采购二次确认已收敛为 `SalesOrder` 定义中的普通审批节点：`ProcurementConfirmation` 实体、集合、状态机、驳回原因代码枚举、`WorkItemType` 和路由已删除，选源已后移到采购单。
- [ ] 低毛利上级确认已整体删除：实体、集合、状态、`WorkItemType`、Service、端点和权限项全仓命中为 0；毛利风险只以只读提示存在，不阻断任何流程。
- [ ] `ReviewStatus` 正好三值（`NOT_SUBMITTED`、`IN_APPROVAL`、`APPROVED`），`SalesOrder` 与 `VoucherSalesOrder` 的提交路径完全一致。
- [ ] W01 与 W02 已合并为唯一工作台：`/workspace/tasks` 重定向到 `/workspace`，页面无「团队待处理」分区，口径胶囊筛选不跳页，详情区可连续提交审批决定；无独立统计卡，待办列表只渲染一份。
- [ ] 第 4.4.4 节 12 行强类型动作全部实现；所有类型均可在最终通过前由原提交人受控撤回，运行管理员可填写原因应急撤回，受阻取消复用同一 `cancel_action`，最终通过后不得取消。
- [ ] 恢复原审批人与受阻取消两条路径互斥且按 blocker 类别失败关闭。
- [ ] 同一 `approval_node_execution_id` 在全部任务状态合计最多一条 WorkItem。
- [ ] 第 16.5 节全部 9 类通知事件具备去重键、重试上限和死信处置，事务内只写 outbox。
- [ ] 不存在全局审批运行开关或第二条运行路径；启用边界仅由已发布定义的存在性表达。
- [ ] `bpm` 对 ERP 业务层零反向依赖，且审批流程 ID 在生产代码中只有一个定义源。
