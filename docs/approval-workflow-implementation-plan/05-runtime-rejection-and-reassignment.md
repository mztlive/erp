# 阶段 05：BPM 引擎、ERP 运行编排、轮次驳回、人员恢复与管理员改派

> 阶段性质：P3 BPM Engine 与 ERP Service 编排工作包
>
> 阶段目标：将旧步骤状态机替换为 `bpm` 内基于节点、事件、连线的纯单令牌状态引擎，并由 ERP Service 在唯一 MongoDB 事务内应用引擎计划和业务副作用
>
> 允许状态：依赖阶段 00—04 的冻结端口；HTTP 由阶段 06 实现，共享入口不得由本阶段修改

## 1. 文件责任

本阶段负责实现纯 BPM 引擎：

```text
backend/crates/bpm/src/engine/
├── mod.rs
├── start.rs
├── decision.rs
├── enter_node.rs
├── cancel.rs
├── resume.rs
├── reassign.rs
├── transition_plan.rs
└── event.rs
```

同时重写 ERP 事务编排：

```text
backend/services/src/approval/execution/
├── mod.rs
├── start.rs
├── decision.rs
├── cancel.rs
├── resume.rs
├── reassign.rs
├── authorization.rs
├── idempotency.rs
├── apply_plan.rs
├── notification_outbox.rs
├── notification_worker.rs
├── observability.rs
└── view.rs
```

同时负责：

- `backend/services/src/approval/dto.rs` 的运行命令与响应；
- `backend/services/src/work_item/{mod,dto,presentation}.rs` 的审批任务写保护、family、路由和中文投影；
- BPM 纯状态引擎测试和 ERP 审批编排单元测试。

本阶段不得修改 `backend/database/tests/**` 或 `backend/apps/web-api/tests/**`。Repository、事务故障注入和 HTTP 集成测试由阶段 11 在 P6-PILOT/P6-FINAL 编写。

`bpm::engine` 只允许接收 `bpm` 模型、命令值对象、已收敛资格结果和调用方提供的时间/ID，不得接收或调用 `Executor`、Repository、MongoDB、`DocumentType`、业务实体、权限/DataScope、WorkItem、强类型业务动作、审计或通知。它必须返回确定性的 `TransitionPlan` 和 `BpmEvent`；同一输入必须产生同一语义结果。

`services::approval::execution` 是唯一目标应用编排入口。它负责加载持久化事实、写时授权重验、ID/时间提供、调用 BPM、执行强类型业务动作、把中性任务意图映射为 WorkItem、把 `BpmEvent` 映射为审计和通知意图，并在一个 MongoDB 事务中应用全部写入。既有 `services::approval::runtime` 只保留失败关闭的旧命令，P0-D 必须删除。

## 2. 统一节点进入

`bpm::engine` 只允许一个纯函数规划当前节点执行：

```text
plan_enter_node(instance, definition_graph, node_key, round_no,
                participant, eligibility, execution_id, now)
  -> TransitionPlan
```

Service 必须在调用前：

1. 从 `approval_instance_assignee` 读取当前责任人；
2. 重验账号、任职、审批资格、DataScope、对象读取权和岗位分离；
3. 将责任人转换为 `ParticipantId`，将校验结果收敛为 `Eligible` 或稳定 `BlockedReason`；
4. 提供新执行 ID 和当前时间；不得让 BPM 调用 ID 生成器或系统时钟。

BPM 函数必须：

1. 校验定义节点和对应事件连线完整；
2. 计算实例内单调递增 `execution_no`；
3. 有效时在计划中创建 `ACTIVE` 执行和中性 `HumanTaskRequested` 事件；
4. 人员资格无效时在计划中创建 `BLOCKED` 执行，写稳定 `blocker_code`，实例置 `BLOCKED`，不产生任务请求；
5. 图或关联结构损坏时不得构造缺字段执行；能够形成合法快照时生成结构阻塞计划，无法形成合法快照时返回不可提交的 BPM 不变量错误；
6. 在计划中更新实例 `current_node_execution_id` 并输出中性领域事件。

Service 必须校验并应用计划：`HumanTaskRequested` 映射为唯一 `OPEN + DIRECT` 审批 WorkItem；BPM 事件映射为 workflow action、审计和通知 outbox。计划应用失败必须回滚整个事务。

启动、通过进入下一节点、驳回进入新轮次入口、原审批人恢复和管理员改派必须复用该 BPM 函数和同一 Service 适配规则。不得复制多套状态机或人员解析逻辑。

## 3. 启动

ERP `start_approval` 必须在调用方事务内完成：

1. 查询命令收据；已存在时必须按第 11、13 节完成异载荷冲突或同载荷授权回读并结束本次命令，只有收据不存在时才允许继续第 2—11 步；
2. 锁定业务单据及 `BusinessDocument`；
3. 校验提交状态和 `subject_version`；
4. 从单据绑定加载精确定义，允许其当前为 `RETIRED`；
5. 校验单据类型和定义版本一致；
6. 重验完整定义；对实例全部审批人做账户和静态资格校验，并以当前单据资源校验 DataScope、读取权和岗位分离；
7. 构造 BPM 启动命令，调用 `bpm::engine::start` 创建 `RUNNING` 第 1 轮实例、全部节点审批人快照和入口节点 `TransitionPlan`；
8. 校验计划只包含 BPM 状态和中性任务/事件意图；
9. 执行强类型提交动作；
10. 将计划映射为 BPM 持久化、强类型业务对象快照、WorkItem、收据、workflow action、审计和通知 outbox；
11. 原子提交。

启动请求不得包含 definition key/ID、审批人、节点或下一动作。必须删除旧 `build_step_instances` 和预建 `WAITING` 逻辑。

## 4. 决定请求与锁定

Service 实现统一应用端口 `submit_decision`；BPM 实现纯 `bpm::engine::decide`。公开决定命令只包含：

```text
work_item_id
decision: APPROVE | REJECT
reason
expected_task_version
idempotency_key
```

服务端从任务推导实例、当前执行、定义、单据和 `subject_version`。事务必须按固定顺序锁定或 CAS：命令收据 → 任务 → 实例 → 当前执行 → 实例审批人 → 单据。

下列验证只对收据不存在的新命令执行；同载荷幂等回读使用第 11、13 节的专用分支，不得因原任务已完成或实例已推进而改为冲突。新命令必须同时验证：

- 任务 `OPEN` 且为审批任务；
- 执行为 `ACTIVE` 且等于实例当前执行；
- actor 同时等于任务 owner、执行 assignee 和实例节点 current assignee；
- `task_version` 匹配；
- 实例、执行和单据读取后的版本仍满足 CAS；
- 工作项 `subject_version` 与单据冻结版本一致；
- 写时权限、DataScope、对象读取权和岗位分离仍有效。

任何版本或责任不一致返回稳定冲突或权限错误，不得自动刷新后替用户重试决定。上述责任、版本和资格校验由 Service 完成；BPM 只接收已经加载的当前快照、决定和收敛资格结果，并再次验证纯流程不变量。

若 actor 仍是三方一致责任人，但写时重验发现其账户、权限、DataScope、对象读取权或岗位分离已经失效，Service 必须调用 BPM 生成阻塞计划，并在事务中：

1. 将当前执行置为 `BLOCKED` 并记录人员阻塞原因；
2. 关闭当前 OPEN WorkItem，关闭原因固定为 `APPROVAL_RUNTIME_BLOCKED`（合同 §13.3 第 4 条，全部阻塞路径共用同一关闭原因；人员失效的具体分类由执行上的 `blocker_code` 表达，不再另设任务关闭原因）；
3. 将实例置为 `BLOCKED`；
4. 写阻塞审计、通知意图和命令收据；
5. 提交后返回稳定的 blocked 结果，由 HTTP 映射为 `409 APPROVAL_INSTANCE_BLOCKED`。

不得以普通错误回滚上述阻塞事实。BPM 计划必须标记 `CommitRequired::Blocked`，应用层必须使用可提交的 `DecisionOutcome::Blocked` 表达该结果，禁止在事务闭包内直接返回导致回滚的权限错误。

`OPEN_TASK_CONFLICT` 只能在 Service 已通过 `approval_node_execution_id` 有界读取全部关联任务后提交。同一阻塞事务必须用各自版本 CAS 关闭全部 OPEN 任务，关闭原因固定为 `APPROVAL_RUNTIME_BLOCKED`；任一 CAS 失败必须回滚并返回稳定冲突，不得提交仍含 OPEN 任务的 BLOCKED 实例。

### 4.1 失败处理矩阵

| 失败时点 | 当前决定是否接受 | 必须提交的事实 | 对外结果 |
| --- | --- | --- | --- |
| 当前审批人资格失效 | 否 | 当前执行 BLOCKED、当前任务 CLOSED、实例 BLOCKED、审计/outbox/收据 | 409 blocked |
| 决定前发现图或实例关联损坏 | 否 | 当前执行或实例结构性 BLOCKED、当前任务 CLOSED、审计/outbox/收据 | 409 blocked |
| 当前通过后下一审批人失效 | 是 | 当前执行/任务完成；新 BLOCKED 执行；实例 BLOCKED；无新任务 | 2xx，响应状态 BLOCKED |
| 最终领域动作失败 | 否 | 全事务回滚，不保留决定或完成任务 | 领域错误或 500 |
| CAS/版本竞争 | 否 | 全事务回滚 | 409 version conflict |
| 收据 duplicate key | 取决于已提交收据 | 当前事务回滚，事务外重读收据 | 原 2xx 或 409 payload conflict |

结构性 blocker 必须在写入前使用现有合法快照构造。无法构造合法阻塞事实时全事务回滚、返回 500 并触发最高级别告警，不得写半结构实体。

## 5. 通过

`APPROVE` 必须由 Service 完成写时校验后调用 `bpm::engine::decide`。BPM 计划与 Service 应用必须满足：

1. 在任何状态写入前加载并验证当前节点两条唯一连线及下一节点快照；图损坏时不接受本次决定，并按结构阻塞规则提交 BLOCKED；
2. 将当前执行 CAS 为 `APPROVED`，保存决定人、时间和可选原因；
3. 将任务 CAS 为 `COMPLETED`；
4. 读取已验证的 `APPROVE` 连线；
5. 指向下一节点时复用 BPM 节点进入规则；若下一人失效，计划必须保留本次通过事实并创建 `BLOCKED` 下一执行；
6. 指向 `APPROVED` 时 BPM 先生成待终结计划；Service 必须先执行强类型最终动作，再应用实例 `APPROVED`、清空当前执行引用和结束时间；
7. Service 将 BPM 事件映射为收据、决定事实、workflow action、审计和通知 outbox；
8. 原子提交。

最终领域动作必须由命令收据和实例终态双重防重。不得保留 `sequence_no + 1` 推进。

## 6. 驳回与下一轮

`REJECT` 必须要求非空原因，并由 `bpm::engine::decide` 固定生成下列计划，再由 Service 应用：

1. 当前执行 CAS 为 `REJECTED`；
2. 当前任务 CAS 为 `COMPLETED`；
3. 验证唯一 `REJECT` 连线指向入口；
4. 实例 `current_round_no` checked add 1；
5. 在同一 `subject_version` 下复用 BPM 节点进入规则创建新执行；
6. 入口人员有效时实例保持/恢复 `RUNNING`；失效时实例为 `BLOCKED`；
7. BPM 输出驳回和轮次重启事件；Service 映射审计和通知 outbox；
8. 原子提交。

第一节点本人驳回也必须产生下一轮新执行和新任务。不得修改业务单据内容、提交版本或实例定义；不得把实例置 `REJECTED`。

## 7. 取消

`cancel_approval` 只能由业务撤回用例调用。Handler 必须校验 `approval_instance:cancel` 和非空撤回原因；Service 必须校验 actor 等于 `subject_snapshot.submitted_by`，或具备该类型 `runtime_admin_permission`。两类 actor 都必须在事务内重验对象读取权、DataScope、单据允许撤回、实例为 `RUNNING` 或人员失效类别的 `BLOCKED`、实例/当前执行/单据版本和可空任务版本；运行管理员路径必须另记应急代办身份。`RUNNING` 必须锁定并关闭当前唯一 `OPEN` 任务；`BLOCKED` 必须证明不存在 `OPEN` 任务且请求未携带任务版本。非人员一致性 blocker 必须拒绝该端口并改用 `cancel_blocked_approval`。随后调用 `bpm::engine::cancel` 取得取消计划，执行政策注册的强类型 `cancel_action`，应用实例/执行 `CANCELLED`、清空实例当前执行引用并映射审计/通知。

若实例为 `BLOCKED` 且没有任务，只取消当前阻塞执行。已批准实例和独立工作项 API 均不得调用该端口。

`cancel_blocked_approval` 是非人员一致性 blocker 的唯一业务退出路径，只允许具备运行管理权限和实例 DataScope 的管理员调用。它必须校验实例为 `BLOCKED`、blocker 不属于人员失效、全部预期版本匹配，并执行同一政策注册的 `cancel_action`。成功后当前执行和实例均为 `CANCELLED`，当前执行引用清空，业务单据回到可修正草稿。该端口不得修复定义、跳过节点或把原决定标记为成功。若当前损坏已使合法取消计划无法形成，必须保持冻结并按第 8 节前向修复，不得构造半结构终态。

## 8. 阻塞码

至少定义并稳定映射：

```text
APPROVER_ACCOUNT_INACTIVE
APPROVER_EMPLOYMENT_INVALID
APPROVER_NOT_ELIGIBLE
APPROVER_OUT_OF_DATA_SCOPE
APPROVER_CANNOT_READ_SUBJECT
SEPARATION_OF_DUTIES_VIOLATION
DEFINITION_GRAPH_CORRUPTED
INSTANCE_LINK_CORRUPTED
OPEN_TASK_CONFLICT
SUBJECT_VERSION_CONFLICT
INTERNAL_INVARIANT_BROKEN
```

阻塞原因必须结构化持久化，并归入以下恢复类别：

| 类别 | blocker | 恢复方式 |
| --- | --- | --- |
| 人员失效 | account、employment、eligibility、scope、read、SOD | 原审批人已恢复时执行 `resume_current_approver`；仍失效时由管理员改派 |
| 图或关联损坏 | definition、instance link | 告警并执行 `cancel_blocked_approval`；禁止改派或切换定义 |
| 任务冲突 | open task conflict | 告警并执行 `cancel_blocked_approval`；禁止重开或删除任务后继续 |
| 版本损坏 | subject version conflict | 告警并执行 `cancel_blocked_approval`；禁止改写冻结版本 |
| 内部不变量 | internal invariant | 无法形成合法取消计划时保持冻结、readiness 失败并前向修复代码；不得直接改库 |

日志可记录内部诊断，但 API 不得泄露敏感授权细节。

## 9. 人员恢复与管理员改派

### 9.1 恢复当前审批人

`resume_current_approver` 只处理人员失效 blocker 在原当前审批人重新满足资格后的恢复。请求必须包含预期 instance/execution/assignment 版本、可空的 closed-task 版本和幂等键，不接受目标用户、节点或恢复动作枚举。Service 必须：

1. 校验 actor 具有 `approval_instance:resume`、该 `DocumentType` 的运行管理权和实例 DataScope；
2. 校验实例和当前执行均为 `BLOCKED`，blocker 属于人员失效；
3. 校验实例审批人绑定未被改派，当前审批人与旧 BLOCKED 执行审批人一致；
4. 重验该审批人的账号、任职、权限、对象读取权、DataScope 和岗位分离已经全部恢复；
5. 将旧 BLOCKED 执行 CAS 为 `SUPERSEDED`，结束原因为 `ASSIGNEE_RECOVERED`；旧任务保持 `CLOSED`；
6. 以 `ASSIGNEE_RECOVERY` 为分派来源，在相同 round 和 node 下创建新的 `ACTIVE` 执行和新的唯一 `OPEN` WorkItem；不得重开旧任务；
7. 实例指向新执行并恢复为 `RUNNING`，实例审批人绑定和定义审批人均不变化；
8. 写收据、恢复审计和通知 outbox，并原子提交。

该命令不是通用重试：它不能处理结构、任务、版本或内部 blocker，不能选择用户，不能沿连线推进，也不能重复执行原审批决定。

### 9.2 管理员改派

`reassign_current_approver` 请求必须包含目标用户、非空原因、预期 instance/execution/assignment 版本和幂等键。若阻塞执行曾产生任务，还必须携带该已关闭任务版本。Service 必须完成业务资格校验，再调用 `bpm::engine::reassign` 生成替换执行计划：

1. 校验专门的运行管理权限和实例 DataScope；
2. 锁定实例、当前执行、实例审批人绑定及可选开放任务；
3. 只接受 `BLOCKED` 实例，且当前 blocker 必须属于人员失效类别；运行正常的 ACTIVE 节点不得进行管理员酌情换人；
4. 重新证明原审批人当前仍不满足资格；原审批人已恢复有效时返回 `APPROVAL_CURRENT_APPROVER_RECOVERED`，客户端只能调用 `resume_current_approver`，不得直接决定；
5. 校验目标用户账号、任职、资格、对象读取权、DataScope 和岗位分离；
6. BPM 计划更新实例节点 `current_assignee_participant_id` 和分派来源 `ADMIN_REASSIGN`，保留 `definition_assignee_participant_id`；Service 负责用户 ID 与 `ParticipantId` 的显式转换；
7. 将旧 BLOCKED 执行 CAS 为 `SUPERSEDED`，结束原因为 `ADMIN_REASSIGNED`；已有 WorkItem 必须保持 CLOSED，不得重新打开或覆盖历史 owner；
8. 以 `ADMIN_REASSIGN` 为分派来源，在相同 round 和 node 下创建新的 ACTIVE 执行，`execution_no` 单调递增，并创建新的唯一 OPEN WorkItem；
9. 实例指向新执行并恢复为 `RUNNING`；新执行不继承旧 blocker；
10. BPM 输出改派和恢复事件；Service 映射审计、收据和通知 outbox；
11. 原子提交。

后续轮次进入该节点必须使用新责任人。改派不得修改旧执行审批人快照、旧任务责任快照或流程定义。

## 10. 工作项硬隔离

在 `backend/services/src/work_item/mod.rs` 中，`claim`、`start_processing`、`release_to_team` 三个旧命令必须立即改为稳定失败关闭，保证 P3-RUNTIME 独立合并时现有 HTTP 调用方仍可编译但不能继续产生旧责任语义。P3-HTTP 删除对应 DTO、权限项、HTTP 端点和路由，P4-WORKFLOW 删除前端调用，P0-D 最终删除 Service 符号。仍然保留的通用 `reassign`、`close` 和任何通用完成路径必须先检查 `approval_node_execution_id`：非空时统一拒绝，固定错误码 `APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN`。新审批运行编排位于 `backend/services/src/approval/execution/**`；现有 `approval/runtime.rs` 只作为待删除旧基线文件存在，不得创建同名目录。

审批任务只有决定端口能完成，只有审批运行改派端口能换人。非审批任务同样不再有责任池语义：其创建方必须在创建事务内解析出唯一 `owner_user_id`，解析失败必须失败关闭并告警，不得写入空责任人。

`backend/services/src/work_item/dto.rs` 中 `WorkItemFamily::Approval` **已存在**，本阶段只把 `DocumentApproval` 穷尽映射到该 family，并从旧卡券专用类型清单移除两种卡券审批；不得新增第二个审批 family。`handler_route` 必须读取 WorkItem 中的 DocumentType 路由上下文，按已签署页面映射返回目标 workspace；缺少映射必须失败关闭，不得退回默认 W05。

`presentation.rs` 必须为 `DocumentApproval` 返回通用中文责任、影响和下一步描述；具体单据名称和节点名称来自服务端安全快照。`mod.rs` 中按 WorkItemType 穷举的对象策略、职责分离策略和通用写命令保护必须全部增加 `DocumentApproval` 分支。新增类型后任何漏配必须由穷尽 match 或完整性测试发现。

## 11. 幂等摘要合同

每种命令必须显式定义 `command_kind`、`scope_id` 和 canonical payload：

| 命令 | scope | canonical payload |
| --- | --- | --- |
| start | `process_kind + subject.kind + subject.id + subject_version` | 绑定 ID、定义版本、subject version、actor participant ID |
| decision | `approval_node_execution_id` | work item ID、decision、trim 后 reason、期望 task version、actor ID |
| cancel | `approval_process_instance_id` | subject version、期望实例/执行/任务版本、trim 后 reason、actor ID |
| resume | `approval_node_execution_id` | 期望 instance/execution/assignment/closed-task version、actor ID |
| reassign | `approval_node_execution_id` | target user、期望 instance/execution/assignment/task version、trim 后 reason、actor ID |
| cancel_blocked | `approval_process_instance_id` | blocker、期望 instance/execution/task version、trim 后 reason、actor ID |

canonical 编码必须使用固定字段顺序、明确 null 表示、UTF-8 和稳定枚举值；禁止直接对任意 Map/JSON 序列化结果取 hash。幂等键必须 trim、非空并限制长度，日志只能记录摘要，不得记录原键。

全部命令必须共用一个幂等分支：先按 `command_kind + scope_id + idempotency_key` 查收据；收据存在且 payload hash 不同时立即冲突；收据存在且 hash 相同时，只加载收据结果引用所需的对象并执行当前授权重验，不再要求原任务 OPEN、原执行 ACTIVE 或实例保持原状态，也不得重做任何写入。只有收据不存在时才执行正常命令前置条件和状态转移。

并发请求同时插入收据时，重复键导致的事务必须整体回滚；最外层随后在新会话中重读已提交收据。同载荷返回原结果，异载荷返回 `APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT`。

## 12. 通知 worker 与可观测性

BPM 只输出中性 `BpmEvent`。Service 必须按签署通知政策把事件映射为通知 outbox；事务内仅追加 outbox。独立 worker 必须：

1. 原子取得有界批次的租约并写 worker ID、租约截止时间；
2. 在事务外调用通知提供方，设置超时和取消；
3. 以 outbox 业务去重键调用幂等发送接口；
4. 成功后按租约 owner CAS 标记 delivered；
5. 可重试失败按签署退避更新 `next_attempt_at`；
6. 超过最大次数进入 dead letter 并告警；
7. 进程退出时停止取新租约，允许租约到期后其他实例接管。

必须记录低基数指标和告警：BLOCKED 数量/最长持续时间、决定冲突、幂等冲突、决定延迟、没有合法 OPEN WorkItem 的 ACTIVE 执行、outbox backlog/oldest age/retry/dead letter。实例 ID 和用户 ID 只能进入结构化日志，不得作为指标标签。

## 13. 响应事实

启动、决定、取消、恢复、改派和受阻取消响应必须返回同一种最新视图：实例状态、当前轮次、当前节点、当前审批人、单据状态、最近驳回，以及存在时的下一开放任务摘要。响应必须在事务提交后由持久化事实映射，不得用命令输入拼装。幂等重复命令必须先重验 actor 当前的动作权限及合同 §4.6 对应的责任、资格、适用类型权限、DataScope 和对象读取权；失权时返回不泄露资源的 403/404 且不产生写入。仍有权时返回收据记录的不可变命令结果和当前有权读取的最新视图；“原结果”只指原命令是否成功、产生的实例/执行/任务引用和结束状态，不承诺重放当时的可变页面快照。

## 14. 阶段验收

- [ ] 启动、通过、驳回、取消、恢复、改派和受阻取消均为单事务。
- [ ] BPM 引擎为无 I/O 纯计算；不读取时钟、不生成 ID、不打开事务、不访问 Repository 或调用业务回调。
- [ ] 相同 BPM 输入产生相同 `TransitionPlan` 和事件序列；计划不包含 ERP URL、权限名、业务命令或通知模板。
- [ ] 任一故障注入点失败时所有集合零部分提交。
- [ ] 重复命令同载荷回读，异载荷冲突。
- [ ] 两个并发决定只有一个成功，另一个为 `409` 语义。
- [ ] 任一节点驳回都创建下一轮入口新执行，`subject_version` 不变。
- [ ] 下一审批人失效时保留上一步通过并形成 `BLOCKED` 当前执行。
- [ ] 决定时当前审批人失效会提交 BLOCKED 事实和关闭任务，而不是回滚为空。
- [ ] 原审批人恢复后通过专用恢复命令创建新执行和新任务，不会形成无 OPEN 任务的死路。
- [ ] 管理员只能改派仍处于人员失效状态的审批人；结构、任务、版本和内部阻塞不能通过改派清除。
- [ ] 非人员一致性 blocker 只能通过受阻取消退出，不能切换定义、改写冻结版本或直接改库。
- [ ] 改派结束旧执行并创建新执行、新任务，不覆盖历史快照。
- [ ] 审批任务无法通过任何通用工作项写接口修改。
- [ ] canonical hash、并发 duplicate-key 回读、outbox 租约/重试/死信有单元测试合同。
- [ ] BLOCKED、冲突、延迟和 outbox 指标具备 dashboard/runbook 入口。
- [ ] 运行路径不存在 `POOL`、`AssignmentMode`、`WAITING`、`RETRY_CURRENT_STEP`、`TERMINATE_APPROVAL` 或 `REJECT_TO_APPLICANT`。
- [ ] `claim`、`start_processing`、`release_to_team` 在 Service 中稳定失败关闭且没有新调用方；P3-HTTP 负责移除 HTTP 可达性，P0-D 负责全仓零命中。任何目标 WorkItem 创建路径都不能产出空 `owner_user_id`。
- [ ] `cargo test -p bpm engine`、`cargo test -p services approval::execution` 和 `./scripts/check-bpm-boundaries.sh` 通过；试点真实 MongoDB 事务测试由 P6-PILOT 执行，全量用例由 P6-FINAL 补齐。
