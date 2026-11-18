# erp-approval-runtime

## 职责

本 crate 为多个业务 Process 提供共用的审批执行步骤：启动写入、材料冻结、入口任务创建、动态审批人解析与复验、撤回抄送，以及按原命令回执回读结果。它也提供审批附件读取前和交付前的复验，不负责读取对象存储或返回 HTTP 响应。

本 crate 不决定单据是否需要审批，不开启或提交业务写事务，不分派审批后的业务动作。回放只读取原回执指向的实例；提交结果未知时不重跑原写事务，也不选用其他实例代替原结果。

## 目录结构

公开导出见 [`src/lib.rs`](src/lib.rs)。`ApprovalRuntime`、`ApprovalRuntimePort`、`Error` 和 `Result` 可从 crate 根导入。

| 源码 | 公开入口与用途 |
| --- | --- |
| [`approval_start.rs`](src/approval_start.rs) | `persist_start_runtime`、`persist_start_snapshot`、`persist_start_tasks` 与 `StartTaskContext`：分步持久化启动结果。 |
| [`approval_start/replay.rs`](src/approval_start/replay.rs) | 从 `approval_start` 导出的 `replay_start`、`StartReplayErrors`：按完整命令身份回读启动实例。 |
| [`approval_recovery.rs`](src/approval_recovery.rs) | `recover_approval_command`、`ApprovalCommandFailure`：原事务退出后有界查证命令结果。 |
| [`approver_directory.rs`](src/approver_directory.rs) | `resolve_approvers`、`resolve_start_approvers`、`revalidate_approvers`：解析申请人的团队或部门负责人、加载已有实例的运行图、复验动态指派。 |
| [`approval_materials.rs`](src/approval_materials.rs) | `ApprovalMaterialTarget`、`readable`、`revalidate`、`authorize_attachment`：限定材料读取目标、交付前复验、关联附件前核验转授资格。 |
| [`runtime_port.rs`](src/runtime_port.rs) | `ApprovalRuntimePort` 与实现 `ApprovalRuntime<A>`：向业务 Process 提供应用装配的审批能力。 |
| [`resource_grants.rs`](src/resource_grants.rs) | `append_assignments`：为销售单、采购单的正式人工任务追加订单读取授权事件。 |
| [`errors.rs`](src/errors.rs) | 内部错误模块；对外导出 `Error`、`Result`，保留字段校验、工作流稳定错误码及命令恢复分类。 |

## 架构设计

### 与 BPM、工作流和业务 Process 的分工

- [`bpm`](../../platform/bpm/src/engine/mod.rs) 根据定义图、命令和已确认的人员资格，计算流程迁移、事件及任务意图；不读取数据库、不判断 ERP 权限，也不创建工作项。
- [`erp-workflow`](../../domains/erp-workflow/src/lib.rs) 管理 ERP 审批政策、定义绑定、运行事实、工作项和抄送等合同。其 [`prepare_start`](../../domains/erp-workflow/src/service/approval/execution/start.rs) 根据命令身份区分回放与新启动；新启动调用 BPM 并生成待写计划 `PlannedWrites`。工作流自己的 `ApprovalRuntimeService` 承接实例读取和审批决定等用例，不是本 crate 的 `ApprovalRuntime`。
- 业务 Process 负责单据授权、业务校验和写入，持有事务并决定何时调用共有步骤。审批决定若触发领域动作，由工作流经 `ApprovalDomainActionPort` 调用[应用的动作注册表](../../../apps/erp-server/src/wiring/approval_dispatch/action_registry.rs)，再路由给所属业务 Process；本 crate 不参与动作分派。

### 启动写入与事务

业务 Process 先确认当前提交资格和冻结定义，准备启动计划，再在同一事务登记启动回执、推进单据启动守卫并写入业务提交。[采购启动写段](../erp-procure-to-pay/src/procure_to_pay/start_approval/start_persist.rs) 以启动回执作为审批写段的第一写；新建并提交时可沿用外层已开始的事务。后续共有步骤复用该 `Executor`，不另开事务：

1. `persist_start_runtime` 要求计划包含入口节点执行。先重验动态负责人及其账号资格，再写入实例、冻结指派、入口节点执行和列表投影，返回尚未冻结附件的主题快照。新命令的动态人选在内存定义图中解析，不回写发布定义；复验时若当前组织来源、负责人指派或资格已变化，拒绝提交，不现场替换人选。
2. 调用方可给快照补充业务专属展示资料或重提信息；`persist_start_snapshot` 委托[读模型](../../read-models/erp-read-models/src/workbench/approval_materials.rs)重验材料转授资格，冻结本次材料和展示（含重提展示），先保存不可变快照，再按规则保存启动抄送。历史快照不以当前材料补齐。
3. `persist_start_tasks` 读取同事务保存的快照，按计划创建继承快照关联的人工入口 `WorkItem`，然后追加销售单、采购单的读取授权事件。事件须有同一实例的节点执行和实际参与人作为证据；入队与业务写入同事务，不代表读取授权已生效。函数返回计划顺序中首个任务的 `(ID, 版本)`，没有人工任务则返回 `None`。

任一步失败都停止后续写入，由调用方回滚。撤回时，业务 Process 负责撤回授权、业务回写及运行态和任务的变更；`ApprovalRuntimePort::persist_cancel_copies` 仅在原撤回事务内，按计划事件、抄送规则和原快照登记抄送（见[采购撤回](../erp-procure-to-pay/src/procure_to_pay/cancel_approval.rs)）。

### 精确回放与结果查证

- `replay_start` 沿用调用方传入的已授权 `Executor`，规范化原幂等键并按通用启动身份查询回执：当前 V3 范围优先，仅对协议登记的历史范围回读；带有效重提说明的命令不接受旧无说明身份。无回执返回 `None`；只校验首个命中，摘要不符即报冲突，不回退到其他回执。命中后按 `result_ref` 读取原实例，核对实例 ID、流程种类、主体类型与 ID、主体版本、启动人及定义 ID 与版本；实例当前状态不影响身份判定。原实例缺失或身份不符时拒绝。
- `resolve_start_approvers` 有启动回执时，从原实例加载冻结指派的运行图，先核对启动人与绑定定义 ID、版本，不按当前组织重新指派；命令摘要仍需由启动入口核对。无回执才按当前组织事实解析。
- 对允许结果查证的入口，调用方在原写事务退出后，根据 `ReceiptDuplicate`、`TransientTransaction` 或 `OutcomeUnknown` 决定是否调用 `recover_approval_command`，并为每次探查提供独立的只读执行上下文，核对原命令身份和所需授权。最多探查八次，命中即返回已提交结果；未命中或遇到可继续查证的错误时退避重试，其他探查错误立即返回，耗尽后保留首次写入错误。要求始终保留未知提交错误的入口不得使用此策略。

### 授权装配与附件读取

[应用组合层](../../../apps/erp-server/src/wiring/approval_runtime.rs) 注入共享 `AuthorizationService`、RBAC、具体 `WorkflowAuthorizationPort` 和 `WorkflowAuditPort`；构造不执行 I/O。除启动快照和撤回抄送外，`ApprovalRuntimePort` 还提供以下入口：

- `bind_document`：在创建事务中按工作流政策处理定义绑定。必须审批时校验当前发布定义及人员资格，写审计并返回绑定；免审时只登记政策事实，返回 `None`。
- `revalidate_binding`：在调用方传入的定义图上复验指定审批人的岗位分离、当前资格及所需订单事实。`source_readable` 通过注入的授权端口检查来源可读性，来源取证或授权设施报错时不当作普通不可读。
- `authorize_attachment`：关联已有文件前，自建事务读取文件，检查当前操作人是否为有权转授的实际上传人。前置检查不替代提交复验；`authorize_material_transfer` 和提交冻结仍须在业务事务内检查材料转授资格。

消费方按变体转换本 crate 的错误，不把未知提交、回执竞争或瞬态事务冲突压成普通冲突。

读取时，`readable` 根据精确实例或抄送回执检查当前资格和冻结附件允许清单。只有开票审批的独立当前销售来源入口可显式传销售版本，抄送不能走此路径。首次读取在获取文件元数据时记录敏感审计，比对文件与允许清单中的引用，拒绝已销毁或安全扫描标记为拒绝、隔离的文件，然后再次读取引用以复验资格。[HTTP 入口](../../../apps/erp-server/src/http/handler/approval_instance/materials.rs) 在事务外读取对象存储并校验字节；交付前调用 `revalidate` 重验读取资格、治理状态及文件版本、内容指纹和存储键，不重复写首次审计，存储键不进入响应。

### 从 `material_writes` 迁移

将启动中的 `freeze_approval_materials` 和 `persist_approval_snapshot` 合并为一次 `approval_start::persist_start_snapshot` 调用，传入原始快照和原事务 `Executor`。已注入 `ApprovalRuntimePort` 的业务 Process 可通过其同名方法调用这一步；撤回改用已装配的 `ApprovalRuntimePort::persist_cancel_copies`。
