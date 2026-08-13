# ERP 审批运行与待办责任合同

> 状态：已确认，必须执行  
> 适用范围：一期、二期以及后续接入 BPM 的全部人工审批、确认、复核与异常处理任务  
> 实施原则：审批实例负责流程推进，`work_item` 只负责当前人工责任，业务领域命令负责正式业务事实

## 1. 合同效力

1. 本合同是审批运行、任务分派、多级审批和待办交互的唯一横向业务合同。
2. `erp-phase-1.md`、`erp-phase-2.md` 负责定义业务准入、审批条件和生效结果；
   `erp-data-model.md` 负责定义物理字段、索引和事务不变量；W 文件负责页面布局和业务操作。
3. 上述文档不得另行定义任务领取、租约、通用完成、多级步骤推进或 BPM 直写业务表的规则。
   发生冲突时，以本合同为准并同步修正文档和实现。
4. 本合同落地后，原 `UNCLAIMED / IN_PROGRESS` 任务状态、客户端领取态、通用
   `work_item.complete` 和“转交即创建后继任务”的设计全部失效。

## 2. 领域边界

| 对象 | 唯一职责 | 禁止职责 |
| --- | --- | --- |
| `approval_definition` | 标识一套可启动的审批定义和当前发布版本 | 保存运行中审批状态 |
| `approval_step_definition` | 定义版本内的步骤顺序、任务类型、分派模式和处理人解析规则 | 保存具体处理人或业务决定 |
| `approval_instance` | 冻结定义版本、业务对象、提交版本并记录整条审批运行状态 | 代替业务单据保存正式业务状态 |
| `approval_step_instance` | 记录单个步骤的激活、处理决定和处理审计 | 承担队列责任或直接修改业务事实 |
| `work_item` | 表达当前需要哪名用户或哪个责任池处理一件事 | 决定下一审批步骤或独立完成业务动作 |
| 领域决定记录 | 保存审批意见以及由此形成的正式领域事实 | 承担任务分派 |
| `workflow_action` | 保存面向单据的通用动作审计 | 作为审批流程状态源 |

`work_item` 可以引用 `approval_step_instance_id`。不属于审批流程的独立复核、异常处理或人工确认任务，
该字段允许为空，但仍必须遵守本合同的责任、并发和完成规则。

## 3. 当前交付范围

当前版本只交付下列能力：

1. 代码注册、显式版本化的审批定义；
2. 固定顺序的串行多级审批；
3. `DIRECT` 和 `POOL` 两种人工步骤分派；
4. `APPROVE`、`REJECT_TO_APPLICANT`、`TERMINATE_APPROVAL` 三类步骤结果；
5. 当前步骤完成后激活唯一下一步骤，或结束审批并执行唯一强类型业务动作；
6. 管理员受控转交、退回责任池和阻塞恢复；
7. 面向未来 BPM 的稳定运行时端口、运行时标识和外部关联身份。

当前版本不得实现：

- 可视化流程设计器；
- 任意表达式、脚本、回调 URL 或运行时动态代码；
- 并行网关、会签、或签、加签、减签、自由跳转和任意退回；
- 允许页面直接选择下一节点或直接创建待办；
- 批量“开始处理”、批量转交或批量审批。

需要新增上述能力时，必须先修订本合同和数据模型，再实施代码。

## 4. 审批定义合同

### 4.1 定义版本

每次可影响运行语义的修改必须发布新版本。已启动实例永久绑定原版本，不得被新版本反向修改。

`approval_definition` 至少包含：

| 字段 | 约束 |
| --- | --- |
| `definition_key` | 稳定业务编码；同义流程不得另建编码 |
| `version` | 同一 `definition_key` 内单调递增 |
| `name` | 管理与审计名称 |
| `runtime_kind` | `INTERNAL` 或 `BPM` |
| `status` | `DRAFT`、`PUBLISHED`、`RETIRED` |
| `published_at` / `published_by` | 发布审计；发布后定义内容不可修改 |

`approval_step_definition` 至少包含：

| 字段 | 约束 |
| --- | --- |
| `approval_definition_id` | 引用唯一定义版本记录；步骤写入时父定义必须为 `DRAFT` |
| `step_key` | 版本内稳定且唯一 |
| `sequence_no` | 从 1 递增；当前版本只允许严格串行 |
| `work_item_type` / `handler_key` | 必须引用已注册的固定任务类型和处理器 |
| `assignment_mode` | `DIRECT` 或 `POOL` |
| `assignee_resolver_key` | 服务端注册的固定解析器；不得保存脚本或任意表达式 |
| `allowed_decisions` | 当前步骤允许的决定集合 |

### 4.2 发布校验

定义发布必须同时满足：

1. 至少一个步骤，且 `sequence_no` 连续、无重复；
2. 所有任务类型、处理器、决定和处理人解析器均已注册；
3. 每个决定都有唯一的下一步或终结语义；
4. 最终通过已绑定唯一强类型领域动作；
5. 角色、组织、数据范围和岗位分离策略可由服务端校验；
6. `BPM` 定义已配置外部定义身份和消息可靠性设施。

步骤只能在父定义为 `DRAFT` 时新增、修改或删除。发布必须在同一受控操作中完成全部校验并把定义及其
步骤冻结；`PUBLISHED` 和 `RETIRED` 定义及步骤均不可修改。任一校验失败时禁止发布。
`start_approval` 只能选择 `PUBLISHED` 定义。当前阶段发布由代码和部署清单完成，不提供管理端配置页面。

## 5. 运行实例合同

### 5.1 审批实例

`approval_instance` 至少包含：

| 字段 | 约束 |
| --- | --- |
| `definition_key` / `definition_version` | 启动时冻结，后续不得修改 |
| `runtime_kind` | 从定义复制；运行中不得切换 |
| `business_object_type` / `business_object_id` | 稳定业务对象身份 |
| `subject_version` | 被审批的不可变提交或业务版本 |
| `instance_version` | API 对实例持久化乐观锁版本的命名；每次实例写入后递增 |
| `status` | `RUNNING`、`APPROVED`、`REJECTED`、`TERMINATED`、`CANCELLED`、`BLOCKED` |
| `current_step_instance_id` | `RUNNING` 或 `BLOCKED` 时指向当前步骤；终态必须为空 |
| `external_instance_id` | 仅 `BPM` 运行时使用；同一运行时内唯一 |
| `blocker_code` / `blocked_at` | 仅 `BLOCKED` 时必填；保存当前结构化阻塞原因和进入时间 |
| `started_by` / `started_at` / `ended_at` | 运行审计 |

同一业务对象和 `subject_version` 对同一审批定义同时最多存在一个非终态实例。

### 5.2 步骤实例

`approval_step_instance` 至少包含：

| 字段 | 约束 |
| --- | --- |
| `approval_instance_id` / `step_key` | 引用冻结定义中的步骤 |
| `sequence_no` | 从定义复制 |
| `step_version` | API 对步骤持久化乐观锁版本的命名；每次步骤写入后递增 |
| `status` | `WAITING`、`ACTIVE`、`APPROVED`、`REJECTED`、`TERMINATED`、`CANCELLED`、`BLOCKED` |
| `decision` / `decision_reason` | 仅正式决定后写入；驳回原因按业务规则必填 |
| `decided_by` / `decided_at` | 决定审计 |
| `external_activity_id` | 仅 `BPM` 运行时使用 |
| `blocker_code` / `blocked_at` | 仅 `BLOCKED` 时必填；与实例当前阻塞原因一致 |

一个 `approval_instance` 同时最多一个 `ACTIVE` 或 `BLOCKED` 当前步骤。`ACTIVE` 步骤必须存在一个开放待办；
`BLOCKED` 步骤允许在阻塞发生前已有一个开放待办，也允许因尚未解析出处理人而没有待办；
`WAITING` 步骤不得提前创建 `work_item`。

## 6. 待办责任合同

### 6.1 状态与责任分离

`work_item` 至少包含：

| 字段 | 约束 |
| --- | --- |
| `status` | `OPEN`、`COMPLETED`、`CLOSED` |
| `assignment_mode` | `DIRECT` 或 `POOL` |
| `owner_role` | 责任角色；两种分派模式均必填 |
| `owner_organization_id` | 责任组织；用于团队资格和主管授权范围过滤 |
| `owner_user_id` | 当前个人责任人；`POOL` 未开始处理时为空 |
| `assignment_source` | 定义步骤、系统规则、管理员转交或其它已注册来源 |
| `assigned_at` | 首次形成个人责任的时间；未形成时为空 |
| `started_at` | 任一责任人首次进入正式处理的时间；未开始时为空 |
| `current_assignment_at` | 当前个人责任开始生效的时间；责任池无人负责时为空 |
| `last_activity_at` | 最近一次非终结动作时间 |
| `task_version` | API 对持久化乐观锁版本的命名；每次任务写入后递增 |
| `approval_step_instance_id` | 审批任务必填；独立任务允许为空 |
| `business_object_type` / `business_object_id` / `subject_version` | 当前任务所针对的业务事实 |
| `completed_at` / `completed_by` | 正式完成审计 |
| `closed_at` / `closed_by` / `close_reason` | 受控关闭审计 |

`status` 不表达是否已分派。`OPEN + owner_user_id IS NULL` 表示责任池待处理；
`OPEN + owner_user_id IS NOT NULL` 表示已由具体用户负责。

### 6.2 直接指派

`DIRECT` 步骤激活时，服务端必须在同一运行事务中：

1. 根据冻结业务上下文执行 `assignee_resolver_key`；
2. 校验候选用户处于有效任职、具备角色与数据范围，并满足岗位分离；
3. 创建 `OPEN` 待办并直接写入责任组织、`owner_user_id`、`assigned_at`、`current_assignment_at`；
4. 将待办放入该用户“我的待办”。

直接指派任务不需要、也不得提供“开始处理”。解析不到唯一有效用户时，不得猜测、回退到任意管理员
或转入公共池；步骤和实例必须进入 `BLOCKED`，写入结构化阻塞原因并通知有权管理员。
直接指派任务第一次提交非终结动作或正式决定时，以原子 `if_null` 写入 `started_at`；只读打开不得写入。

### 6.3 责任池分派

`POOL` 步骤激活时，服务端写入 `owner_role`、责任组织和适用的数据范围，`owner_user_id` 保持为空，
待办进入“团队待处理”。用户点击“开始处理”时，MongoDB Repository 必须执行等价于下列条件与更新的
单文档原子 `find_one_and_update`：

```text
filter = {
  id: work_item_id,
  status: OPEN,
  assignment_mode: POOL,
  owner_user_id: null
}
update = {
  $set: {
    owner_user_id: actor_id,
    assignment_source: SELF_START,
    assigned_at: if_null(assigned_at, now),
    started_at: if_null(started_at, now),
    current_assignment_at: now,
    last_activity_at: now,
    version: add(version, 1)
  }
}
```

条件还必须包含 `version = expected_task_version`。这里的 `version` 通过 API 返回为 `task_version`；
Repository 应使用更新管道或等价原子表达式实现 `if_null`，不得先读后写。

更新前和提交时均必须校验用户角色、数据范围、对象参与权和岗位分离。更新影响行数为零时：

- 当前责任人就是请求人：按幂等成功返回当前任务；
- 当前责任人为其他用户：返回处理权冲突和当前可展示责任人；
- 任务不是开放状态或用户已失去资格：返回对应业务错误。

不得使用租约、过期时间、续租、领取令牌或客户端会话来判断责任归属。刷新页面后必须以服务端任务事实恢复。

### 6.4 退回团队与转交

1. “退回团队”只适用于 `POOL` 开放任务，清空当前 `owner_user_id` 和 `current_assignment_at`，
   保留 `assigned_at`、`started_at`，并追加原因与审计；
   原 `work_item` 保持 `OPEN`，不得创建后继任务。
2. “转交”更新原开放任务的当前责任人和 `current_assignment_at`，保留首次时间；
   `assigned_at` 原为空时以 `if_null` 写入本次时间；`started_at` 仍只表示首次实际处理。
   不得把原任务标记完成后再创建同义后继任务。
3. 转交目标必须重新校验有效任职、角色、数据范围、对象参与权和岗位分离。
4. 普通处理人是否允许退回或转交由任务类型的服务端策略决定；管理员操作必须具备专门权限并填写原因。
5. 审批决定已形成、任务已完成或任务已关闭后，不允许通过转交改写历史责任。

## 7. 串行多级审批执行合同

### 7.1 启动

`start_approval` 必须在一个本地事务中：

1. 锁定并校验业务对象及 `subject_version`；
2. 解析唯一已发布审批定义版本；
3. 创建 `RUNNING` 审批实例；
4. 创建所有步骤实例，第一步为 `ACTIVE`，其余为 `WAITING`；
5. 按第一步分派规则创建唯一开放待办；
6. 写入业务提交事实、`workflow_action` 和审计。

第一步处理人解析失败时，本次启动事务必须改为落下 `BLOCKED` 实例和 `BLOCKED` 当前步骤，
不创建开放待办，并返回结构化阻塞结果；不得以整个启动回滚掩盖需要管理员处理的配置问题。

重复请求必须按业务幂等键返回原审批实例，不得创建第二条运行链。

### 7.2 通过当前步骤

`submit_decision(APPROVE)` 必须锁定审批实例、活动步骤、当前待办和业务对象，并按下列唯一顺序执行：

1. 重验实例与步骤仍活动；
2. 重验当前用户就是任务责任人，并仍满足权限、数据范围和岗位分离；
3. 重验 `subject_version` 和业务前置条件；
4. 写步骤决定和领域审批记录；
5. 将当前待办置为 `COMPLETED`；
6. 存在下一步骤时，将其从 `WAITING` 激活并创建唯一开放待办；
7. 不存在下一步骤时，执行定义绑定的强类型领域动作，再将实例置为 `APPROVED`；
8. 写 `workflow_action`、审计和必要通知。

`INTERNAL` 运行时的上述写入必须处于同一数据库事务。任何一步失败都必须整体回滚。

### 7.3 驳回与终止

- `REJECT_TO_APPLICANT`：完成当前待办，将当前步骤置为 `REJECTED`，审批实例置为 `REJECTED`，
  业务对象进入该流程定义的退回状态；不得激活其它审批步骤。
- `TERMINATE_APPROVAL`：完成当前待办，将步骤置为 `TERMINATED`，实例置为 `TERMINATED`，
  执行流程定义绑定的终止领域动作。
- 修改后重新提交必须创建新的 `approval_instance`，并重新绑定新的 `subject_version`；
  原实例和决定永久保留，不得改回运行中。

### 7.4 取消与阻塞

撤回只允许调用 `cancel_approval`。服务端必须重验业务允许撤回、尚未形成不可逆决定以及当前步骤策略；
成功后关闭当前开放待办、取消未执行步骤并将实例置为 `CANCELLED`。

处理人解析失败、定义注册缺失、外部运行时身份丢失或其它不能安全推进的条件必须使步骤与实例进入
`BLOCKED`。阻塞发生前尚未创建待办时不得补建猜测任务；已有开放待办必须保留原身份和责任，
但全部普通责任动作、非终结动作和正式决定均被阻断。阻塞不得被页面当作驳回、通过或普通待处理。

### 7.5 阻塞恢复

阻塞恢复只能调用 `recover_approval`，并且只能执行固定动作 `RETRY_CURRENT_STEP`。命令必须携带
审批实例、当前步骤及可选当前待办的期望版本、结构化恢复原因和幂等键；不得携带审批决定、目标下一步骤、
指定处理人或业务字段。

服务端必须在同一事务或 BPM 可靠消息边界内：

1. 锁定并重验实例、当前步骤、可选开放待办和业务对象仍对应原 `subject_version`；
2. 确认实例与步骤均为 `BLOCKED`，且阻塞原因已经由服务端配置、注册表或外部相关性查询证明消除；
3. 无开放待办时重新执行冻结步骤的分派解析并创建唯一开放待办；
4. 已有开放待办时保留其身份和首次时间；重新校验当前责任，`DIRECT` 按冻结解析器校正责任，
   `POOL` 当前责任已失效时清空个人责任并退回团队；
5. `INTERNAL` 将步骤恢复为 `ACTIVE`、实例恢复为 `RUNNING`；`BPM` 还必须先确认唯一外部实例和活动相关性；
6. 清除当前阻塞字段，递增实例、步骤和已变化待办的版本，写入不可变恢复审计。

任一校验失败时实例和步骤继续保持 `BLOCKED`，不得产生部分待办、跳过步骤或代替审批人作出决定。
恢复成功后仍须由当前责任人执行原步骤的强类型决定。

## 8. 命令与接口合同

### 8.1 稳定运行时端口

应用层只允许通过下列稳定端口推进审批：

```text
start_approval(command)
submit_decision(command)
cancel_approval(command)
recover_approval(command)
```

内部实现为 `InternalApprovalRuntime`；未来外部实现为 `BpmApprovalRuntime`。业务 Handler、页面和定时任务
不得根据流程定义自行创建步骤或待办，也不得依赖具体运行时实现。

### 8.2 待办动作

待办层只允许：

```text
start_processing(work_item_id, expected_task_version, actor_id)
release_to_pool(work_item_id, expected_task_version, actor_id, reason)
reassign(work_item_id, expected_task_version, target_user_id, actor_id, reason)
close_invalid_work_item(work_item_id, expected_task_version, actor_id, reason)
```

`start_processing` 只建立 `POOL` 任务个人责任，不推进业务状态。`release_to_pool` 和 `reassign`
只改变开放任务责任，不改变审批步骤。`close_invalid_work_item` 只用于重复、误派或已有有效替代任务；
审批、确认、结果未知和补偿未完成任务不得借此关闭。

### 8.3 正式业务动作

系统不得暴露公共 `complete_work_item` 或让客户端提交 `completion_action` 选择业务动作。
每类任务必须注册唯一强类型领域命令。命令处理器必须在同一事务中重验任务、责任、业务版本和权限，
写入领域事实后再完成任务；任何会写任务的强类型命令都必须携带查询所得 `expected_task_version`，
审批任务还必须通过 `submit_decision` 推进。

同一正式命令重复提交必须返回原结果。结果未知时客户端只能查询原操作结果或沿同一幂等键重试，
不得本地标记完成或发起另一条流程。

## 9. 权限与安全合同

1. 队列查询由服务端按用户、角色、组织、数据范围和对象参与权过滤；前端不得全量查询后隐藏。
2. `scope=mine` 只返回 `OPEN + owner_user_id=当前用户`；`scope=team` 只返回当前用户有资格处理的
   `OPEN + assignment_mode=POOL + owner_user_id IS NULL`。
3. `scope=managed` 只向具备任务责任管理权限的主管开放，返回其授权组织和数据范围内全部 `OPEN` 任务，
   包括无人负责的 `POOL` 任务和已由下属负责的 `DIRECT/POOL` 任务；不得接受任意组织或用户扩大范围。
4. `scope=history` 只返回当前用户曾负责、曾完成、曾关闭，或当前具有组织级历史查看权的
   `COMPLETED/CLOSED` 任务；历史结果只读，`allowedActions` 必须为空。
5. `mine/team/managed` 只能查询 `OPEN`，`history` 只能查询 `COMPLETED/CLOSED`；不兼容组合必须返回 400。
6. 所有队列和嵌入业务工作面的任务投影必须返回同一个 `task_version`，不得以 `subject_version` 替代。
7. 阻塞步骤保留的开放待办仍按责任范围可见，但必须返回 `processing_state=APPROVAL_BLOCKED`、
   权限安全的结构化阻塞摘要和空 `allowedActions`；不得把它计为可立即处理任务或允许普通任务动作。
8. `allowedActions` 是服务端当前判断结果，不代替动作提交时的重新校验。
9. 当前责任人不天然获得业务数据读取权；字段脱敏和对象权限仍由领域权限决定。
10. 岗位分离必须同时校验提交人、经办人、历史关键动作人和当前决定人。定义配置不得放宽领域硬约束。
11. 所有分派、开始处理、退回、转交、决定、取消、阻塞和恢复均必须写不可变审计。

## 10. 页面合同

用户界面固定使用下列词汇：

| 语义 | 用户可见文案 |
| --- | --- |
| 已由本人负责的开放任务 | 我的待办 |
| 尚无个人责任人的责任池任务 | 团队待处理 |
| 主管授权范围内的全部开放任务 | 团队任务 |
| 已完成或已关闭任务 | 处理历史 |
| 从责任池建立个人责任 | 开始处理 |
| 放弃当前个人责任并回到责任池 | 退回团队 |
| 改由另一名合格用户负责 | 转交 |
| 责任已由他人取得或发生变化 | 处理权已变化，请刷新 |

页面不得出现“领取”“重新领取”“租约”“续租”“令牌”“角色池”“工作项”“步骤实例”等实现术语。
直接指派任务打开后可立即处理；责任池任务只有“开始处理”成功后才允许提交正式动作。
页面刷新、跨页进入和浏览器重开均必须从服务端重取责任事实，不保存客户端责任真相。

## 11. BPM 兼容合同

### 11.1 对象映射

| 内部对象 | BPM 对象 |
| --- | --- |
| `approval_definition` + version | Process Definition |
| `approval_instance` | Process Instance |
| `approval_step_instance` | Activity Instance |
| `work_item` | Human Task 的 ERP 责任投影 |
| 审批决定 | Outcome / Signal |

### 11.2 边界

1. BPM 只接管流程路由，不拥有 ERP 正式业务事实、权限规则或最终业务状态机。
2. BPM 不得直接写 ERP 领域表；所有业务结果必须回到同一强类型领域命令处理器。
3. `runtime_kind` 在实例启动时冻结。运行中的 `INTERNAL` 实例不得原地迁移为 `BPM`。
4. 接入 BPM 时，旧 `INTERNAL` 实例继续跑完；新发布定义可以选择 `BPM`，形成渐进替换。
5. 外部 BPM 无法参与 ERP 本地数据库事务。`BpmApprovalRuntime` 必须使用 outbox、inbox、
   `correlation_id`、幂等消费、可重试状态查询和人工恢复，明确处理“ERP 已提交、BPM 未确认”及其反向情况。
6. 当前 `inbox_message` 不得在未修订集成合同前被直接复用为 BPM 收件箱。

接入 BPM 前必须单独提交基础设施修订，补齐消息表、索引、重试、死信、监控和恢复验收；
不得仅替换接口地址后上线。

## 12. 迁移执行合同

### 12.1 数据迁移

旧任务按下列规则一次性迁移，不保留双重状态语义：

| 旧事实 | 新事实 |
| --- | --- |
| `UNCLAIMED` | `status=OPEN`、`assignment_mode=POOL`、`owner_user_id=NULL` |
| `IN_PROGRESS` 且存在责任人 | `status=OPEN`、保留 `owner_user_id`；分派模式按注册定义确定 |
| `COMPLETED` | `status=COMPLETED`，保留完成审计 |
| `CLOSED` | `status=CLOSED`，保留关闭审计 |

`assigned_at`、`started_at` 和 `current_assignment_at` 只能从可靠历史动作或时间字段回填；无法证明时保持空值并
记录迁移说明，不得伪造时间。旧持久化版本必须原样映射为 `task_version`；缺少版本时由迁移批次写入统一初始值，
不得从 `subject_version` 推导。已有多级审批若能唯一映射定义版本、提交版本和当前步骤，必须回填审批实例；
无法唯一映射的活动任务必须列入上线阻断清单，禁止猜测当前步骤。

### 12.2 应用切换

1. 后端先具备新字段、审批运行时和兼容读取，再执行数据迁移；
2. 前端同一发布批次删除客户端领取/租约状态，切换为“开始处理”和服务端责任事实；
3. 所有强类型任务处理器切换完成后，删除通用 claim/complete 路由及兼容代码；
4. 文档、mock、测试夹具和验收脚本同步切换，不得长期保留两套枚举；
5. 切换前后分别校验开放任务数、个人责任数、责任池数、活动审批数和孤儿步骤数。

## 13. 验收门禁

交付必须同时证明：

- [ ] 直接指派任务创建后直接进入唯一用户“我的待办”，没有“开始处理”动作。
- [ ] 责任池任务只有一个合格用户能原子开始处理，同一用户重复请求幂等成功。
- [ ] 首次责任和首次处理时间在退回团队、再次开始处理和转交后保持不变；当前责任时间准确更新。
- [ ] 页面刷新后责任不丢失，不依赖租约、令牌或浏览器内存。
- [ ] 每个可写任务投影返回独立 `task_version`，责任命令不使用 `subject_version` 代替。
- [ ] 多级审批只为当前活动步骤创建一个开放待办，未来步骤没有待办。
- [ ] 当前步骤通过与下一步骤激活处于同一事务，失败后两者都不生效。
- [ ] 最终通过同时写入审批决定、正式业务事实和任务完成；任一失败整体回滚。
- [ ] 驳回不激活下一步骤；修改重提创建新实例并保留原历史。
- [ ] 处理人解析失败进入 `BLOCKED`，不存在管理员兜底或公共池猜测。
- [ ] `recover_approval(RETRY_CURRENT_STEP)` 能在阻塞原因消除后原子恢复原步骤；失败保持阻塞，且不能跳步骤或代审批人决定。
- [ ] 转交和退回只更新原开放任务责任，不创建同义后继任务。
- [ ] 不存在公共任务完成接口，不存在客户端自选 `completion_action`。
- [ ] 队列查询和每次写动作均执行服务端数据范围、对象参与权和岗位分离校验。
- [ ] `managed` 可查询主管授权范围内已分派团队任务；`history` 可查询只读终态任务，非法 scope/status 组合被拒绝。
- [ ] `INTERNAL` 与未来 `BPM` 使用同一运行时端口；BPM 不直接写 ERP 领域表。
- [ ] 旧枚举、旧接口、旧页面文案和旧 mock 已从非历史合同中清零。
