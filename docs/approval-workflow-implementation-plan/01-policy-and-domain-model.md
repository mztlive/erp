# 阶段 01：BPM 领域模型与 ERP 集成实体

> 阶段性质：P1 BPM Core 与实体集成工作包
>
> 阶段目标：在 `crates/bpm` 建立与 ERP 业务隔离的流程模型和纯图规则，在 `entities` 仅保留单据、业务对象快照、WorkItem 和通知集成模型
>
> 允许状态：完成后可以尚未形成业务闭环，但 workspace 必须可编译且全部现行测试通过；目标模块在接线前必须失败关闭

## 1. 文件责任

本阶段负责：

- `backend/crates/bpm/src/model/**`；
- `backend/crates/bpm/src/graph/**`；
- `backend/entities/src/document_registry/business_document.rs` 中审批绑定值对象（`ownsWithin`：只允许改 `ApprovalDefinitionBinding`，`DocumentType` 枚举归阶段 00）；
- `backend/entities/src/document_registry/workflow_action.rs` 中审批动作枚举；
- `backend/entities/src/work_item/work_item.rs` 中审批任务类型、执行关联和责任约束；
- `backend/entities/src/approval_integration/{mod,subject_snapshot,notification_outbox}.rs`；
- 新增 `bpm` 目标模型和 ERP 集成实体；既有审批模型在本阶段保持只读且不得被新路径引用，统一由 `P0-D` 在全部调用方切换后删除；
- 对应 BPM 模型、图规则、实体和值对象单元测试。

本阶段不负责 BPM 运行命令编排、Repository、Service、HTTP、前端、开发重置脚本、集成测试和共享模块导出。共享 ID、模块导出和依赖入口必须已由阶段 00 提供；出现新增共享修改时停止本阶段并发起独立 P0 amendment。

## 2. BPM 与业务政策边界

### 2.1 BPM 边界类型

`bpm` 必须定义并验证：

- `ProcessKind`：稳定、非空、有长度上限的流程种类；不得导入或别名到 `DocumentType`；
- `SubjectRef`：稳定、非空的 `subject_kind + subject_id`；不得包含业务实体或 Repository；
- `ParticipantId`：处理人不透明 ID；不得依赖 `AccountId`、组织、角色或权限类型；
- `Timestamp`：UTC 时间值对象，由调用方显式提供；不得调用系统时钟或依赖 `entities::common::time::Instant`；
- P0 冻结的全部 BPM ID、状态枚举和错误；
- 只表达流程语义的 `DefinitionGraph`、`TransitionPlan` 和 `BpmEvent` 数据合同。

阶段 00 必须在 `backend/services/src/approval/process_kind.rs` 实现 ERP/BPM 穷尽映射；阶段 03 必须在 `policy.rs` 实现 ERP 政策；阶段 04 必须实现强类型业务动作。下列业务合同属于 `services`，不得定义在 `bpm`：

```rust
pub enum DocumentApprovalPolicy {
    NoApproval {
        document_type: DocumentType,
        process_kind: bpm::ProcessKind,
    },
    ProcessRequired(ProcessRequiredApprovalPolicy),
}

pub struct ProcessRequiredApprovalPolicy {
    pub document_type: DocumentType,
    pub process_kind: bpm::ProcessKind,
    pub definition_admin_permission: Permission,
    pub runtime_admin_permission: Permission,
    pub approver_eligibility_policy: ApproverEligibilityPolicy,
    pub separation_of_duties_policy: SeparationOfDutiesPolicy,
    pub required_node_purposes: &'static [ApprovalNodePurpose],
    pub subject_version_source: ApprovalSubjectVersionSource,
    pub subject_snapshot_fields: &'static [ApprovalSubjectSnapshotField],
    pub work_item_owner_role: WorkItemOwnerRolePolicy,
    pub owner_organization_source: OwnerOrganizationSource,
    pub start_action: ApprovalBusinessAction,
    pub final_approve_action: ApprovalBusinessAction,
    pub cancel_action: ApprovalBusinessAction,
}

pub enum ApprovalNodePurpose {
    SalesOrderProcurementConfirmation,
}

pub enum ApprovalSubjectVersionSource {
    SalesOrderSubmissionNo,
    SalesChangeSubmissionNo,
    EntityApprovalSubjectVersion,
}

pub enum ApprovalSubjectSnapshotField {
    DocumentNo,
    ResponsibleOrgId,
    SubmittedBy,
    SubmittedAt,
    CounterpartyOptional,
    TotalAmount,
    TotalQuantity,
    LineCount,
}
```

`NoApproval` 不得包含动作、资格、岗位分离、WorkItem 或取消配置，也不得注册空 Adapter。`ProcessRequired` 必须包含完整运行政策和实际强类型 `cancel_action`，不得是 `Noop`。业务撤回和受阻取消必须复用该动作；最终通过后的实例不得取消。

`required_node_purposes` 只有 `SalesOrder` 包含且恰好包含一次 `SalesOrderProcurementConfirmation`，其他 11 个类型固定为空。`subject_version_source` 必须逐行等于合同 §4.4.1；`subject_snapshot_fields` 必须逐行等于合同 §4.4.5，公共字段不得遗漏，金额与数量字段按合同必填范围注册。三者都不得使用字符串、运行时 Map 或数据库配置补位。

`DocumentType -> ProcessKind` 必须使用穷尽 `match`，并保证一对一稳定映射；不得让客户端提交 `ProcessKind`，不得从数据库任意字符串反向推导未注册 `DocumentType`。`ApprovalBusinessAction` 必须是 `services` 内的强类型枚举或强类型命令分派，不得保存到 BPM 流程定义，不得从客户端字符串反序列化。本阶段不得为了让类型编译而实现 `Noop` 动作。

`work_item_owner_role` 必须给出唯一稳定值，不得从审批用户的多个角色中任选。`owner_organization_source` 必须指向单据生命周期矩阵中的责任组织字段，不得取当前登录组织或空值补位。

### 2.2 完整性

阶段 03 的政策注册及阶段 00 的 `DocumentType -> ProcessKind` 映射必须使用对 `DocumentType` 的穷尽 `match`，不得使用缺项可返回 `None` 的 HashMap 初始化。新增 `DocumentType` 后，编译器或完整性测试必须强制补政策和映射。政策完整性测试必须逐一断言 `required_node_purposes`、`subject_version_source`、`subject_snapshot_fields`、三类强类型动作、责任角色与责任组织来源均与合同矩阵一致。

已签署矩阵（合同 §4.3）与实体位置对应如下。`approval_subject_version` 列标记必须新增该字段的类型（合同 §4.4.1 第 2 条）：

| `DocumentType` | 当前实体 | 政策 | 需新增 `approval_subject_version` |
| --- | --- | --- | --- |
| `SalesOrder` | `entities/src/sales_order/sales_order.rs` | `PROCESS_REQUIRED` | 否，用 `sales_order_submission.submission_no` |
| `VoucherSalesOrder` | `entities/src/sales_order/sales_order.rs`（同实体，按 `BusinessType::Voucher` 分派） | `PROCESS_REQUIRED` | 否，用 `sales_order_submission.submission_no` |
| `SalesChangeOrder` | `entities/src/sales_review/sales_change_order.rs` | `PROCESS_REQUIRED` | 否，用 `sales_change_submission.submission_no` |
| `PurchaseOrder` | `entities/src/purchase_order/order.rs` | `PROCESS_REQUIRED` | **是**（生效 `purchase_revision.revision_no` 在最终通过时才生成） |
| `PurchaseChangeOrder` | `entities/src/purchase_order/change_order.rs` | `PROCESS_REQUIRED` | **是**（现有 `submission_no` 是 `String` 业务号） |
| `StockAdjustment` | `entities/src/inventory/stock_adjustment.rs` | `PROCESS_REQUIRED`（试点） | **是** |
| `CustomerReceipt` | `entities/src/receivable/customer_receipt.rs` | `PROCESS_REQUIRED` | **是** |
| `SupplierPayment` | `entities/src/payable/supplier_payment.rs` | `PROCESS_REQUIRED` | **是** |
| `CustomerRefund` | `entities/src/returns/customer_refund.rs` | `PROCESS_REQUIRED` | **是** |
| `SupplierRefund` | `entities/src/returns/supplier_refund.rs` | `PROCESS_REQUIRED` | **是** |
| `ReceiptReversal` | `entities/src/returns/receipt_reversal.rs` | `PROCESS_REQUIRED` | **是** |
| `PaymentReversal` | `entities/src/returns/payment_reversal.rs` | `PROCESS_REQUIRED` | **是** |
| `PurchaseReceipt` | `entities/src/fulfillment/purchase_receipt.rs` | `NO_APPROVAL` | 不适用 |
| `Delivery` | `entities/src/fulfillment/delivery.rs` | `NO_APPROVAL` | 不适用 |
| `ElectronicDelivery` | `entities/src/fulfillment/electronic_delivery.rs` | `NO_APPROVAL` | 不适用 |
| `ServiceFulfillment` | `entities/src/fulfillment/service_fulfillment.rs` | `NO_APPROVAL` | 不适用 |
| `CustomerAcceptance` | `entities/src/fulfillment/customer_acceptance.rs` | `NO_APPROVAL` | 不适用 |
| `Invoice` | `entities/src/receivable/invoice.rs` | `NO_APPROVAL` | 不适用 |
| `SalesReturnCase` | `entities/src/returns/sales_return_case.rs` | `NO_APPROVAL` | 不适用 |
| `PurchaseReturnOrder` | `entities/src/returns/purchase_return_order.rs` | `NO_APPROVAL` | 不适用 |

`approval_subject_version: u32` 初值 0，每次提交在同一事务内 checked add 1，写入后对该 `subject_version` 永久不可变。任何类型都不得使用 `BaseModel.version` 作为 `subject_version`。

本阶段必须为目标状态机提供可编译的新增字段和目标枚举值。删除旧状态、旧字段和旧实体会同时影响 Repository、Service 与 HTTP 调用方，因此必须由对应 `DocumentType` 子阶段完成调用方切换，并由 `P0-D` 清除最后的跨域残留。本阶段不得合并无法编译的横向删除：

1. 新增 `IN_APPROVAL` 目标值以及 9 个类型的 `approval_subject_version`，不得在本阶段删除现有调用方仍引用的旧值；
2. 新增 `WorkItemType::DocumentApproval`、`approval_node_execution_id` 和 `AssignmentSource::ApprovalRuntime`；新审批任务构造器必须从第一天起要求非空 `owner_user_id`，不得读取旧责任模式；
3. `P3-ADAPTER-PILOT` 和每个后续 `DocumentType` 子阶段必须在同一 PR 内切换该类型全部调用方、收敛该类型状态机并通过 workspace 门禁；
4. `P3-RUNTIME` 必须让领取类命令失败关闭，`P3-HTTP` 必须移除对应端点；新审批路径不得调用旧运行时；
5. `P0-D` 在全部类型切换后删除旧审批实体、旧业务确认实体、旧 ID、旧状态、旧任务类型、`AssignmentMode` 和旧 `AssignmentSource` 值，并以全仓零命中作为合并门禁。

所有 `PROCESS_REQUIRED` 政策必须给出合同 §4.4.4 指定的实际强类型动作实现。不得用 `Noop`、动态字符串、HTTP 回调或“默认通过”占位。

## 3. BPM 文件布局

`backend/crates/bpm/src` 必须收敛为：

```text
bpm/src/
├── lib.rs                  # P0 冻结入口
├── ids.rs                  # P0 冻结 ID
├── error.rs                # P0 冻结纯领域错误
├── model/
│   ├── mod.rs
│   ├── types.rs
│   ├── process_definition.rs
│   ├── node_definition.rs
│   ├── transition_definition.rs
│   ├── process_instance.rs
│   ├── node_execution.rs
│   ├── instance_assignee.rs
│   └── command_receipt.rs
└── graph/
    ├── mod.rs
    ├── linear.rs
    └── validator.rs
```

`backend/entities/src/approval_integration` 只允许包含业务对象快照和通知 outbox；不得重新定义流程定义、实例、执行、审批人或命令收据。既有 `backend/entities/src/approval/**` 只为保持未切换调用方可编译，禁止由任何新路径引用；`P0-D` 必须在全部调用方切换后删除该目录及公开入口。最终生产代码不得同时存在旧步骤实例与目标节点执行两套公开类型，也不得存在 `entities::approval::*` 与 `bpm::*` 两套新模型。

`ApprovalSubjectSnapshotId` 和 `ApprovalNotificationOutboxId` 必须由 `entities` 唯一定义；其余新 BPM ID 必须由 `bpm` 唯一定义。

### 3.1 固定枚举

`bpm::model::types` 必须包含并只允许以下核心语义：

| 类型 | 取值 |
| --- | --- |
| `ApprovalDefinitionStatus` | `DRAFT`、`PUBLISHED`、`RETIRED` |
| `ApprovalNodeType` | 第一阶段仅 `USER_APPROVAL` |
| `ApprovalTransitionEvent` | `APPROVE`、`REJECT` |
| `ApprovalTerminalResult` | `APPROVED` |
| `ApprovalDecision` | `APPROVE`、`REJECT` |
| `ApprovalProcessInstanceStatus` | `RUNNING`、`APPROVED`、`CANCELLED`、`BLOCKED` |
| `ApprovalNodeExecutionStatus` | `ACTIVE`、`APPROVED`、`REJECTED`、`CANCELLED`、`BLOCKED`、`SUPERSEDED` |
| `ApprovalAssigneeBindingSource` | `DEFINITION`、`ADMIN_REASSIGN` |
| `ApprovalExecutionAssignmentSource` | `DEFINITION`、`ADMIN_REASSIGN`、`ASSIGNEE_RECOVERY` |
| `ApprovalExecutionEndReason` | `ADMIN_REASSIGNED`、`ASSIGNEE_RECOVERED` |

按合同 §8.2，实例审批人绑定与节点执行的来源必须是**两个独立枚举**：绑定只能持有 `DEFINITION` 或 `ADMIN_REASSIGN`，执行才允许 `ASSIGNEE_RECOVERY`。不得共用一个类型，否则绑定在类型上能够持有 `ASSIGNEE_RECOVERY`。

目标 `bpm` 与 ERP 集成公开 API 不得导出下列旧符号。准备期为未切换调用方保留的旧定义不得被目标代码引用，并由 P0-D 删除：

- `ApprovalRuntimeKind`；
- `ApprovalAssignmentMode`；
- `ApprovalDecision::RejectToApplicant`；
- `ApprovalDecision::TerminateApproval`；
- `ApprovalInstanceStatus::Rejected/Terminated`；
- `ApprovalStepStatus::Waiting/Terminated`。

### 3.2 流程定义

`ApprovalProcessDefinition` 必须保存：

- `ProcessKind`，不得保存 `DocumentType` 或任意未验证字符串；
- `definition_version`；
- `name`；
- `status`；
- `entry_node_key`；
- 创建、发布、退役人和时间；
- 独立 `definition_lock_version` 视图，物理上可继续使用 `BaseModel.version`；但必须由调用方提供的 `Timestamp` 逐字段构造 `BaseModel`，不得调用 `BaseModel::new()`，因为它内部读取 `chrono::Local::now()`。

实体方法必须至少提供：

- `new_draft`；
- `rename_draft`；
- `set_entry_node_draft`；
- `publish`；
- `retire`；
- `ensure_mutable`。

发布和退役方法只维护实体自身状态；跨集合验证和事务由阶段 03 Service 完成。

### 3.3 节点和连线

`ApprovalNodeDefinition` 必须保存 `node_key`、`node_name`、`node_type`、`display_order`、`assignee_participant_id`、`assignee_label_snapshot`。`ParticipantId` 和 label 由调用方提供；BPM 不得查询账号、角色或组织。

不得保存：

- `work_item_type`；
- `handler_key`；
- `assignment_mode`；
- `assignee_resolver_key`；
- `allowed_decisions`。

`ApprovalTransitionDefinition` 必须保证：

- `from_node_key + event` 唯一语义；
- `to_node_key` 与 `terminal_result` 二者恰有一个；
- `REJECT` 连线的目标必须为节点 key，不得携带终态；
- `APPROVE` 连线只能是节点目标或 `APPROVED` 终态之一。

“所有驳回回到入口”“只有末节点通过进入终态”等依赖完整图的规则必须放在阶段 03 图聚合校验器；单条连线实体不得假装能够识别入口或末节点。

### 3.4 运行实例

`ApprovalProcessInstance` 必须保存 BPM 定义 ID/版本、`ProcessKind`、`SubjectRef`、`subject_version`、当前执行引用和合同第 8.1 节其他流程字段，并增加 `current_round_no`。实例在 `BLOCKED` 时必须保存 `blocker_code` 和 `blocked_at`，恢复为 `RUNNING` 或进入终态时必须清空当前 blocker 投影；执行历史中的 blocker 事实不得改写。实例不得保存 `DocumentType`、业务实体或业务快照结构。实例方法必须实现：

- 创建 `RUNNING` 第 1 轮实例；
- 设置当前节点执行；
- 开始下一轮；
- 进入/退出 `BLOCKED`；
- 最终 `APPROVED`；
- `CANCELLED`。

实例不得提供 `reject()` 或 `terminate()` 终结方法。`next_round()` 只能从 `RUNNING` 调用，且使用 checked add 防止溢出。

### 3.5 节点执行

`ApprovalNodeExecution` 必须保存合同第 8.3 节全部字段：

- 节点身份和名称快照；
- `round_no`；
- `execution_no`；
- 责任人 ID 和名称快照；
- 激活、决定、阻塞字段；
- 可空的 `replaces_execution_id` 和 `ended_reason`，用于改派时保留旧执行链；
- 独立执行版本。

构造器只创建 `ACTIVE` 或 `BLOCKED` 的当前执行，不得创建 `WAITING`。结束后不得重新激活或修改决定字段。`supersede(reason)` 只能把 `BLOCKED` 执行转为 `SUPERSEDED`，并写入 `ended_at` 和固定结束原因；`SUPERSEDED` 不属于当前执行状态，也不得重新激活。人员改派使用 `ADMIN_REASSIGNED`，原审批人恢复后重新建立执行使用 `ASSIGNEE_RECOVERED`。两种路径均不得覆盖旧执行审批人快照，必须创建同轮次、同节点、递增 `execution_no` 的新执行。

新执行必须保存 `ApprovalExecutionAssignmentSource`。实例审批人绑定初始来源为 `ApprovalAssigneeBindingSource::DEFINITION`，普通进入节点时执行复制当前绑定来源；管理员改派必须把绑定来源更新为 `ADMIN_REASSIGN`，使当前及后续轮次可追溯。原审批人恢复时，仅当次重建执行使用 `ASSIGNEE_RECOVERY`，实例审批人绑定及其来源保持不变——这正是两个枚举必须分开的原因。执行必须保存结构化 `blocked_reason`。阻塞原因至少区分：

- `APPROVER_ACCOUNT_INACTIVE`；
- `APPROVER_EMPLOYMENT_INVALID`；
- `APPROVER_NOT_ELIGIBLE`；
- `APPROVER_OUT_OF_DATA_SCOPE`；
- `APPROVER_CANNOT_READ_SUBJECT`；
- `SEPARATION_OF_DUTIES_VIOLATION`；
- `DEFINITION_GRAPH_CORRUPTED`；
- `INSTANCE_LINK_CORRUPTED`；
- `OPEN_TASK_CONFLICT`；
- `SUBJECT_VERSION_CONFLICT`；
- `INTERNAL_INVARIANT_BROKEN`。

只有前六类人员资格阻塞允许进入管理员改派判断；其余结构或一致性阻塞不得由 `reassign` 清除。

### 3.6 实例审批人

`ApprovalInstanceAssignee` 必须以 `process_instance_id + node_key` 为唯一业务身份，使用 `ParticipantId` 保存定义审批人、当前审批人、来源、改派审计和 `assignment_version`。

实体必须提供 `reassign(target, actor, reason, at)`，并保证：

- 定义审批人永久不变；
- 当前审批人和改派审计原子更新；
- 空原因、同人无意义改派和终态实例改派由应用层/实体共同拒绝；
- 后续轮次读取 `current_assignee_participant_id`。

### 3.7 幂等收据

`ApprovalCommandReceipt` 必须保存命令类型、作用域 ID、幂等键、请求摘要、结果引用和创建时间。相同键同载荷回读，相同键不同载荷冲突。不得继续把 JSON 结果隐藏在 `AuditLog.message` 中充当唯一收据。

### 3.8 业务对象快照与通知 outbox

命令收据属于 BPM 模型并适用 3.7；它不得包含 HTTP envelope、业务 DTO 或 MongoDB 类型。

`ApprovalSubjectSnapshot` 属于 ERP 集成实体，必须放在 `entities::approval_integration`，并以 `approval_process_instance_id` 作为不可变关联。它必须保存 `DocumentType`、业务对象 ID、`subject_version`、责任组织、提交人和政策签署的有界业务快照。快照必须使用显式版本化类型或穷尽枚举，不得保存任意 Map、未约束 JSON、Repository 类型或 BSON `Document`。BPM 实例只保存 `SubjectRef + subject_version`，不得保存该业务快照。

`ApprovalNotificationOutbox` 属于 ERP 集成实体，必须放在 `entities::approval_integration`。它必须保存业务事件去重键、事件种类、收件人、模板参数、投递状态、尝试次数、下次尝试时间、租约持有者、租约截止时间、最后错误分类和死信时间。模板参数不得包含 token 或完整敏感单据数据。`bpm` 只能输出中性 `BpmEvent`，不得生成收件人、模板或投递策略。

## 4. 单据审批绑定

在 `BusinessDocument` 增加可选值对象 `ApprovalDefinitionBinding`：

```rust
pub struct ApprovalDefinitionBinding {
    pub approval_process_definition_id: bpm::ApprovalProcessDefinitionId,
    pub approval_definition_version: u32,
    pub approval_binding_version: u64,
    pub approval_definition_bound_at: Instant,
}
```

规则：

1. `NO_APPROVAL` 单据绑定为空；
2. `PROCESS_REQUIRED` 单据创建完成前必须形成完整绑定；
3. ID、版本和时间必须整体设置或整体为空；
4. 普通更新不得修改绑定；
5. 只有“升级未提交单据流程版本”用例可以整体替换绑定。

`approval_binding_version` 是独立 CAS 版本，初次绑定为 1，每次受控升级 checked add。不得复用 `BusinessDocument.base.version` 或具体业务实体版本；升级请求必须同时携带业务单据期望版本和审批绑定期望版本。

同阶段必须把 `BusinessDocument.document_no` 改为可空，并新增与其成对出现的 `document_no_assigned_at` 和一次性 `assign_document_no` 行为。创建时尚未分配正式号的草稿允许以空编号注册；编号分配必须校验非空、长度和未分配前置条件，成功后永久不可修改或清空。该改造不得改变 `DocumentType` 枚举，非空编号的数据库部分唯一索引与 Repository 原子赋值由阶段 02 实现。

## 5. 审批 WorkItem 领域映射

`WorkItem` 必须增加并只允许下列审批映射：

| 字段（仓库现有名） | 规则 |
| --- | --- |
| `work_item_type` | 固定 `WorkItemType::DocumentApproval` |
| `owner_user_id` | 当前实例审批人，必填；不得为空，不得用责任池补位 |
| `owner_role` | 合同 §4.4.5 的 `<prefix>_approver` |
| `owner_organization_id` | `subject_snapshot.responsible_org_id` |
| `assignment_source` | 固定新增值 `AssignmentSource::ApprovalRuntime`；同批删除 `StepResolver`、`RecoveryResolver` |
| `approval_node_execution_id` | 必填且全生命周期唯一，类型化替换旧 `Option<String>` 字段 `approval_step_instance_id` |

审批 WorkItem 必须提供可被应用层调用的受控 `complete_by_approval_runtime` 和 `close_by_approval_runtime` 行为。仍然保留的通用 `complete`、`close`、`transfer` 行为必须对 `DocumentApproval` 返回稳定保护错误，不能通过调用方约定规避；目标实体不得为 `claim`、`start_processing`、`release_to_team` 提供行为，旧 Service 命令按 P3-RUNTIME 失败关闭并由 P0-D 最终删除。

审批任务路由族必须由 `DocumentType + business_object_id` 决定，具体映射放在 WorkItem presentation/DTO 层；实体不得依赖前端 URL。

旧 `CardSalesManagerApproval`、`CardSalesOperationApproval` 必须在销售单切换到通用审批时删除；不得与 `DocumentApproval` 并存于生产新写路径。

## 6. 工作流动作扩展

在 `WorkflowActionType` 增加满足审计查询的固定动作：

- `ApprovalDefinitionBound`；
- `ApprovalDefinitionUpgraded`；
- `ApprovalStarted`；
- `ApprovalNodeApproved`；
- `ApprovalNodeRejected`；
- `ApprovalRoundRestarted`；
- `ApprovalBlocked`；
- `ApprovalRecovered`；
- `ApprovalReassigned`；
- `ApprovalCancelled`；
- `ApprovalBlockedCancelled`；
- `ApprovalCompleted`。

`workflow_action` 仍不是流程状态源。动作记录必须引用业务单据、审批实例、轮次和节点执行；现有结构无法承载这些引用时，增加可选的结构化 `approval_context` 值对象，不得把身份拼入 `comment`。

## 7. 强类型动作边界输入

阶段 04 必须按本阶段冻结的上下文实现 `backend/services/src/approval/action.rs`。该上下文属于 ERP Adapter，不得进入 `bpm`：

1. `ApprovalActionContext` 使用 `DocumentType`、业务单据 ID、绑定定义 ID、实例 ID、节点执行 ID、`subject_version`、actor 和幂等键；
2. `start_action`、`final_approve_action` 和 `cancel_action` 只从政策注册获得；
3. `execute` 必须接收调用方 `&mut dyn Executor`；BPM API 不得出现 `Executor`；
4. 实现不得开启第二个事务或调用 HTTP；BPM 引擎不得调用该端口；
5. 最终通过动作必须以实例/命令收据保证只执行一次；
6. 未注册动作必须失败关闭。

## 8. 硬切换删除责任

本阶段只建立可编译目标模型，不执行会破坏未切换调用方的横向删除。`P0-D` 必须在全部 `DocumentType` 子阶段完成后一次性删除：

- `backend/entities/src/approval/definition.rs`；
- `backend/entities/src/approval/step_definition.rs`；
- `backend/entities/src/approval/instance.rs`；
- `backend/entities/src/approval/step_instance.rs`；
- `backend/entities/src/approval` 下除临时无行为 facade 外的全部旧运行模型；
- `backend/entities/src/lib.rs` 中旧审批模型公开入口与 `entities/src/ids.rs` 中旧审批/采购确认 ID；
- `backend/services/src/approval/{registry,bootstrap,resolver,runtime}.rs`；
- `backend/entities/src/sales_review/{procurement_confirmation,low_margin_manager_confirmation}.rs` 整文件及其模块声明与 ID；
- 与外部 BPM ID、旧责任模式、预建等待步骤、终止审批、退回申请人、采购确认和低毛利上级确认相关的全部旧测试。

强类型业务动作注册不得随 `registry.rs` 一并删除，必须收敛到 `policy.rs` 或各业务 adapter。

## 9. 阶段验收

- [ ] 固定 `DocumentType` 枚举的序列化、显示名和穷尽匹配测试覆盖全部 20 个值，含 `VoucherSalesOrder`。
- [ ] `DocumentType -> ProcessKind` 为穷尽、一对一、稳定映射，且只存在于 `services::approval::process_kind` 边界层。
- [ ] 9 个类型已新增 `approval_subject_version`，且全部 12 个 `PROCESS_REQUIRED` 类型均不使用 `BaseModel.version` 作为 `subject_version`。
- [ ] 目标 `IN_APPROVAL` 值和 9 个 `approval_subject_version` 字段已建立，未切换调用方仍可编译；最终三值和旧状态零命中由各类型阶段及 `P0-D` 验收。
- [ ] 实例审批人绑定来源与执行分派来源是两个独立枚举，绑定在类型上无法持有 `ASSIGNEE_RECOVERY`。
- [ ] `bpm` 模型未调用 `BaseModel::new()` 或任何系统时钟。
- [ ] BPM 模型覆盖正常、非法状态和边界；BSON round-trip 由 database 适配测试负责。
- [ ] 已结束节点执行不能被重开或覆盖。
- [ ] `BLOCKED -> SUPERSEDED` 只能由人员恢复或管理员改派触发，且固定写入结束原因。
- [ ] 人员恢复和管理员改派都创建同轮次、同节点、更大 `execution_no` 的新执行，并保留旧快照。
- [ ] 驳回只表现为执行 `REJECTED` 和实例下一轮，不存在实例 `REJECTED`。
- [ ] 审批实体中不存在 `POOL`、`WAITING`、resolver、handler 和动态 action 字符串。
- [ ] 新 `DocumentApproval` 构造路径只允许 `ApprovalRuntime` 且要求非空责任人；旧责任模型零命中由 `P0-D` 验收。
- [ ] `DocumentApproval` WorkItem 构造时缺少责任人、角色、组织、来源或执行 ID 会失败。
- [ ] 结构性阻塞原因不能调用领域 `reassign` 恢复。
- [ ] outbox 实体覆盖 BSON round-trip、重试边界和不可变性测试。
- [ ] 业务对象快照位于 `entities::approval_integration`，使用有界强类型结构，并与 BPM 实例一一对应。
- [ ] `bpm` 不依赖或引用 ERP、MongoDB、HTTP、权限、WorkItem、通知投递和业务动作。
- [ ] 新代码只使用 `bpm` ID 与模型；既有旧模型未被新路径引用，并已登记 `P0-D` 删除责任。
- [ ] 稳定 public API 和所有仓库要求的方法均有多行 Rustdoc。
- [ ] 本阶段改动通过 `conventions.md` 第 6 节全部后端门禁，并额外通过 `cargo test -p bpm --lib`、`cargo test -p entities --lib` 和 `./scripts/check-bpm-boundaries.sh`。
