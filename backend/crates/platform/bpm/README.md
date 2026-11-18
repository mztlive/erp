# bpm：纯流程模型与状态引擎

`bpm` 根据流程定义、当前状态和命令计算审批的下一步：推进到下一节点、终结实例，或记录受阻。它返回状态快照、领域事件和任务意图，不保存数据、不创建 ERP 工作项，也不执行业务单据动作。

## 职责

- 定义带版本的审批流程、人工审批节点和连线，以及运行实例、节点执行、冻结审批人绑定与命令身份和回执模型。
- 规划草稿节点的替换和复制，按节点顺序生成连线，校验定义图和运行时的冻结绑定。
- 计算启动、通过、驳回、阻塞、原人恢复和取消；同样的输入产生同样的迁移结果。

### 使用场景

节点与连线规则、图校验、单人审批和多人或签的状态迁移，以及相应的事件和任务意图，都在本 crate 修改。若变动的是单据是否需要审批、人员的 ERP 资格或待办如何落库，应到下文对应的上层 crate 修改。

## 目录结构

| 位置 | 内容 |
| --- | --- |
| [src/model/](src/model/mod.rs) | 定义、实例、执行、参与人、冻结指派和领域值对象；[命令身份与摘要](src/model/command_identity.rs)及[命令回执](src/model/command_receipt.rs)只定义数据与校验规则，不读写回执。 |
| [src/graph/](src/graph/mod.rs) | `DefinitionGraph`、草稿节点编排、[线性连线生成](src/graph/linear.rs)、[整图校验](src/graph/validator.rs)和[冻结指派还原](src/graph/assignment.rs)。 |
| [src/engine/](src/engine/mod.rs) | 运行命令、资格与状态迁移；启动、通过交接和恢复复用[节点进入规则](src/engine/enter_node.rs)，输出[迁移计划](src/engine/transition_plan.rs)和[领域事件](src/engine/event.rs)。 |
| [src/ids.rs](src/ids.rs)、[src/error.rs](src/error.rs) | 流程、节点、执行、绑定和回执等类型化 ID；流程种类、对象引用、参与人引用和时间戳等边界值对象的错误。 |

### 代码入口

- [crate 根](src/lib.rs)公开 `model`、`graph`、`engine`、`ids`、`error`，并重导出 `BpmEvent`、`TransitionPlan`、`Error`、`Result`、流程 ID 以及 `ParticipantId`、`ProcessKind`、`SubjectRef`、`Timestamp`。
- `bpm::graph` 提供 `DefinitionGraph`、`generate_linear_transitions`、`build_linear_transitions` 和 `validate_linear_graph`；草稿重建、已发布图校验、决定目标查找及冻结指派还原通过 `DefinitionGraph` 的方法调用。
- `bpm::engine` 提供 `Eligibility`、各操作的命令和输入类型，以及 `plan_start`、`start`、`decide`、`reject_terminal`、`block_current`、`resume`、`plan_cancel` 和 `cancel`。准备计划返回 `StartPlan` 或 `CancelPlan`，状态迁移入口返回 `EngineResult<TransitionPlan>`。

## 架构设计

### 负责的数据与能力

**流程模型与校验。** `DefinitionGraph` 由流程定义、节点与连线组成。当前只支持单令牌的线性人工审批：完整图有 1 至 20 个节点，稳定节点键不重复，展示顺序从 1 连续编号，入口是第一节点，不依赖存储顺序。每节点恰有一条通过边和一条驳回边；通过进入下一节点，末节点通过或任一节点驳回均终结实例。历史冻结定义中指回入口的驳回边仍可读取，但按驳回终态解释，不改写旧定义或重开一轮。

**定义与运行快照。** 已发布、已退役的定义不可再编辑；新绑定应通过 `validate_published_linear` 同时检查发布状态和完整图，不能以定义状态代替整图校验。运行实例冻结定义 ID 和业务版本、`SubjectRef` 和提交版本，不保存单据内容。指定审批节点支持 1 至 50 名不重复的参与人；动态指派节点不能混用或签名单，规则和用途键由上层解释。启动前由上层解析动态人选；回读时 `with_frozen_assignees` 校验定义版本、实例及节点归属和冻结名单，用已保存的绑定还原运行图，不按当前组织关系重新选人。

### 运行输入与输出

| 操作 | 调用方提供什么，BPM 返回什么 |
| --- | --- |
| [启动准备](src/engine/start_plan.rs)与[启动](src/engine/start.rs) | `plan_start` 核对冻结定义版本与逐节点绑定，返回入口和装配好的绑定。`start` 接收解析后的图、节点绑定和资格，以及实例/入口执行 ID、流程种类、单据引用与提交版本、启动人和时间；全部节点的所有审批人都须合格，才会创建首轮实例、冻结整链指派并进入入口。 |
| [决定与阻塞](src/engine/decision.rs) | `decide` 使用实例、当前活动执行、冻结运行图、决定人及其资格、决定、事由和时间；需要进入后继时再使用其资格和新执行 ID、序号。`reject_terminal` 只接收驳回决定，驳回须有事由且不重启。`block_current` 用调用方给出的阻塞码和时间生成完整的当前执行阻塞计划。 |
| [原人恢复](src/engine/resume.rs) | `resume` 接收受阻实例和当前执行、冻结绑定与运行图、复验后的原人资格、新执行 ID、递增序号和时间。只允许人员资格类阻塞恢复；在同轮、同节点以新执行替换旧执行，不改派、不重开旧任务。 |
| [取消准备](src/engine/cancel_plan.rs)与[取消](src/engine/cancel.rs) | `plan_cancel` 使用实例、当前执行和调用方统计的开放任务数，返回任务关闭策略及是否需要受阻取消端口。`cancel` 根据实例、当前执行、取消人、非空事由、关闭任务标记及时间结束实例和当前执行，并按标记返回关闭意图。 |

`TransitionPlan` 包含更新后的实例、新建和更新的执行、启动时创建的冻结指派、`BpmEvent`、`TaskIntent` 和 `CommitRequired`。事件记录启动、节点进入、通过、驳回、受阻等流程事实；任务意图仅表达创建、完成或关闭，不带 ERP 路由、权限或通知模板。计划和意图不是持久化模型，由调用方将模型快照和映射后的写入一起提交。

- **或签共用一个节点执行。** 进入时只为当时合格的成员分别生成任务意图，无人合格则受阻；失去资格的或签成员不能提交决定。首个有效决定结束该执行，工作项适配层完成决定人的任务并在同一事务中关闭同执行的其他开放任务。
- **提交类别不同于实例状态。** `Blocked` 表示不接受当前决定，但合法的阻塞事实必须提交，例如当前节点的决定连线缺失或重复。若通过已接受、只是后继节点受阻，则保留 `Proceed`，同时提交通过和后继受阻。终态驳回也用 `Proceed`，最终通过和取消分别用 `TerminalApproved` 和 `Cancelled`。若连线指向不存在的后继节点，无法形成合法快照，引擎返回 `EngineError::Uncommittable`；这类错误或模型不变式错误不得改写为可提交的阻塞或终态。

### 使用与边界要求

- **`bpm` 只计算。** ID、时间和已核实的 `Eligibility` 一律由调用方提供；不得访问系统时钟、自行生成 ID，或依赖 ERP 业务领域、MongoDB、HTTP、配置、权限、工作项或通知客户端。
- **[erp-workflow](../../domains/erp-workflow/README.md) 是 ERP 领域适配层。** 它管理审批政策、定义绑定、审批运行用例、工作项、通知 Outbox 和 MongoDB 仓储；[计划适配](../../domains/erp-workflow/src/service/approval/execution/apply_plan.rs)把 BPM 输出转换为待写数据，写入方在事务内按版本和状态条件提交。这里的 `ApprovalRuntimeService` 处理实例读取、审批决定等用例，不是 `erp-approval-runtime` 的 `ApprovalRuntime`。
- **[erp-approval-runtime](../../orchestration/erp-approval-runtime/README.md) 是业务 Process 共用的编排库。** 它提供动态审批人解析及复验、[启动写入、材料冻结与入口任务创建](../../orchestration/erp-approval-runtime/src/approval_start.rs)、撤回抄送和按原回执精确回放。业务写入步骤复用 Process 提供的 `Executor`，不接管业务事务，也不分派审批后的单据动作。`erp-workflow` 和 `erp-approval-runtime` 可以使用 `bpm`；`bpm` 不反向依赖二者，`erp-workflow` 也不依赖编排库。

### 协作示例

审批人通过采购审批的当前节点时，`erp-workflow` 核验工作项和人员资格，再向 BPM 提交决定。BPM 返回当前节点的完成意图，以及下一节点的派发或阻塞计划；若是末节点，则返回最终通过计划。工作流按政策经动作端口调用[应用动作注册表](../../../apps/erp-server/src/wiring/approval_dispatch/action_registry.rs)，由所属业务 Process 在原事务中执行采购生效，再由工作流落地审批状态和工作项变更。BPM 不直接调用其中任何一步。

## 验证要求

仅修改本文档时，在 `backend/` 执行 `git diff --check -- crates/platform/bpm/README.md`。代码变更按[后端开发规则](../../../AGENTS.md#验证)和[统一质量门禁](../../README.md#质量门禁)执行；本 crate 的库单测命令为 `env -u ERP_TEST_MONGO_URI cargo test -p bpm --lib --locked`（在 `backend/` 执行，不运行集成测试或真实外部服务测试）。
