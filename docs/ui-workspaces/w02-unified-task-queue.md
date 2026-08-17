# W02 · 统一待办队列

> 状态：已确认，待按新责任合同改造
> 页面模式：M3 连续处理队列
> 主要路由：`/workspace/tasks`
> 主要角色：全部已登录用户；业务主管与管理员按授权管理团队任务
> 最后更新：2026-08-17
> 权威合同：`../approval-workflow-contract.md`

## 1. 定位与边界

W02 是全部人工任务的统一队列容器。`work_item` 是当前人工责任的唯一事实源；
审批步骤由审批运行时推进，正式业务事实由任务类型绑定的强类型领域命令形成。

用户必须能够：

1. 分别查看已经由本人负责的“我的待办”和尚无个人责任人的“团队待处理”；
2. 读懂任务对象、原因、影响、下一步，以及时限、责任角色和当前责任人；
3. 对 `POOL` 任务执行原子“开始处理”，对 `DIRECT` 任务直接进入处理；
4. 在同类任务中连续处理，并在责任变化、版本冲突或结果未知时安全恢复。

W02 不得：

- 定义销售、采购、财务、审批或集成业务状态机；
- 根据页面选择推进下一审批步骤；
- 提供通用任务完成表单或公共 `complete_work_item`；
- 提供批量开始处理、批量转交或批量审批；
- 用关闭替代未完成的审批、确认、结果未知或补偿；
- 保存租约、令牌、到期时间或客户端责任状态。

## 2. 责任与并发

### 2.1 任务事实

| 事实 | 含义 |
| --- | --- |
| `status=OPEN`、`owner_user_id=本人` | 我的待办；当前用户承担个人责任 |
| `status=OPEN`、`assignment_mode=POOL`、`owner_user_id=NULL` | 团队待处理；合格成员可开始处理 |
| `status=OPEN`、`processing_state=APPROVAL_BLOCKED` | 审批受阻；保留责任但不可执行普通任务动作 |
| `status=COMPLETED` | 强类型业务动作已经成功并完成任务 |
| `status=CLOSED` | 重复、误派或已有有效替代任务；不代表业务完成 |

`status` 不表达任务是否已被分派。页面不得把 `OPEN` 翻译成“未领取”或“处理中”；
当前责任由 `owner_user_id` 单独展示。`processing_state` 只表达当前是否可处理，不替代任务状态或审批状态。

### 2.2 分派模式

| 模式 | 创建时责任 | 页面动作 |
| --- | --- | --- |
| `DIRECT` | 服务端已经解析并写入唯一 `owner_user_id` | 直接处理，不显示“开始处理” |
| `POOL` | 只有责任角色和数据范围，`owner_user_id` 为空 | 显示“开始处理” |

`POOL` 的“开始处理”必须由服务端使用条件更新原子写入当前用户。两个用户并发操作时只能一人成功；
同一用户重复请求按幂等成功返回。不存在租约到期、自动释放或续租。

页面刷新、跨页返回和浏览器重开后必须重新查询服务端任务；不得从 `sessionStorage`、组件状态或
本地 mock 恢复个人责任。

### 2.3 退回与转交

- “退回团队”只清空原 `POOL` 开放任务的个人责任并记录原因；原任务保持 `OPEN`。
- “转交”只更新原开放任务的责任人并记录原因；不得完成原任务再创建同义后继任务。
- 服务端必须重新校验目标用户的有效任职、角色、数据范围、对象参与权和岗位分离。
- 主管只可管理授权组织范围；管理员不获得代替业务人员作出审批决定的权力。

## 3. 用户、权限与数据范围

| 用户场景 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 直接指派或已开始处理 | W01“处理”、顶栏待办 | `owner_user_id` 为当前用户的开放任务 | 查看、处理；有权时退回或转交 |
| 团队责任池成员 | W02“团队待处理” | 当前角色、组织和数据范围均匹配的未分派 `POOL` 任务 | 开始处理 |
| 业务主管 | `managed` 团队任务视图 | 授权组织范围内全部开放任务，包括已由下属负责的任务 | 查看责任分布、受控转交、退回团队 |
| 系统管理员 | 审批阻塞视图 | 技术职责内的 `BLOCKED` 审批实例 | 重试原步骤；不指定处理人、不跳步骤、不替代业务决定 |
| 只读参与者 | 对象时间线 | 有历史查看权的已完成任务 | 查看历史，不重新处理 |

权限规则：

1. 服务端先按模块权限、当前角色、组织、数据范围和对象参与权过滤；前端不得取全量后隐藏。
2. 可见不等于可处理；页面使用 `allowedActions` 和 `actionBlockers`，每次提交仍由服务端重验。
3. 当前责任人不自动获得对象全部字段读取权；敏感字段继续按领域权限遮罩。
4. 页面打开期间权限被收回时，立即移除敏感数据和正式动作，只保留必要任务身份与返回入口。

## 4. 入口与路由

| 场景 | 入口 | URL / 焦点 | 返回规则 |
| --- | --- | --- | --- |
| 打开我的待办 | 顶栏、侧栏 | `/workspace/tasks?scope=mine` | 保留上一业务页签 |
| 打开团队待处理 | 范围切换 | `/workspace/tasks?scope=team` | 无权限时不展示入口 |
| 管理团队任务 | 主管入口 | `/workspace/tasks?scope=managed` | 无任务责任管理权限时返回 403 |
| 查看处理历史 | 范围切换 | `/workspace/tasks?scope=history` | 只读；默认查询已完成和已关闭 |
| 恢复受阻审批 | 管理员异常入口 | `/workspace/tasks?view=approval-blockers` | 使用审批阻塞查询，不混入普通任务队列 |
| 从 W01 进入 | 指标或任务条目 | 携带 `family`、`due`、`scope`；任务焦点内部恢复 | 返回 W01 恢复筛选和焦点 |
| 外部深链当前任务 | 通知或对象时间线 | 允许 `currentWorkItemId`；服务端仍重验可见性 | 不可见时返回安全错误，不泄露对象 |
| 打开对象中心 | 当前任务“打开对象” | 聚焦稳定对象页签；W02 页签保留 | 关闭对象后回当前任务 |

`queueContextId` 由服务端建立稳定队列顺序，不写入 URL。任务焦点允许由 `sessionStorage` 恢复，
但任务责任、权限、对象版本和动作能力必须重新查询。

## 5. 页面布局

```text
┌ PageHeader：统一待办   我的待办 18 · 团队待处理 7 · 团队任务 · 处理历史   [刷新] ┐
┌ sticky 处理面 ──────────────────────────────────────────────────────┐
│ [我的待办] [团队待处理] [团队任务] [处理历史]  搜索 | 类型 | 时限 │
└────────────────────────────────────────────────────────────────────┘
├──────────────────────────────┬───────────────────────────────────────┤
│ 任务队列 34%                 │ 当前任务处理区 66%                    │
│ 对象 · 原因 · 截止 · 责任    │ SequentialProcessBar 第 3/28         │
│ [当前条目]                   │ 对象摘要、业务影响、处理器            │
│ [普通条目]                   │                                       │
│ [处理权已变化]               │ [去确认采购计划] [退回团队] [转交]    │
└──────────────────────────────┴───────────────────────────────────────┘
```

| 区域 | 目的 | 固定规则 |
| --- | --- | --- |
| 页头 | 展示责任范围、任务数和更新时间 | 不随右区滚动 |
| sticky 处理面 | 范围、搜索、主筛和清除 | 主筛不超过 3 个；不裸飘画布 |
| 左侧队列 | 识别上下项、超期和当前责任 | 独立滚动；当前项有文字选中态 |
| 处理导航 | 展示当前位置和下一项偏好 | 不代替正式动作结果 |
| 任务摘要 | 展示对象、原因、影响、版本和责任 | 只读；字段由任务类型白名单提供 |
| 类型处理器 | 执行强类型表单和决定 | 只使用前端受控注册组件 |
| 结果区 | 固定本次正式结果 | 先展示确定结果，再允许移动下一项 |

“全部类型”只用于找任务。进入正式处理后，队列必须收敛到同一 `work_item_type` 或兼容处理器组。
默认排序为已超期、优先级降序、`due_at` 升序、`created_at` 升序，由服务端完成。

## 6. 字段与筛选

### 6.1 展示字段

| 字段 | 用户文案 | 来源与规则 |
| --- | --- | --- |
| `workItemTypeLabel` | 任务类型 | 固定代码映射，不显示原枚举；对象标题不得再重复一遍 |
| `businessObjectLabel` | 对象单号/名称 | 稳定业务身份。采购二次确认必须是 `销售单 {单号}`，不得写成任务类型名 |
| `counterpartyLabel` | 往来客户/供应商 | 有则展示在对象标题旁；空值不上屏 |
| `responsibilityLabel` | 由你处理 / 团队待处理 / 由某人处理 | 本人负责显示「由你处理」；他人显示真实姓名；禁止「当前处理人」 |
| `priorityLabel` | 紧急 / 高 / 普通 | 普通级不额外堆徽章 |
| `dueAt` | 截止 / 已超期 | 相对时间加绝对时间；未设置时不上屏，不写「未设置」 |
| `ownerRoleLabel` / `ownerUserLabel` | 责任角色 / 处理人 | 姓名按账号主数据展示 |
| `reasonLabel` | 为什么需要处理 | `reason_code` 固定中文；禁止把内部事件码空格化上屏 |
| `impactSummary` | 业务影响 | 说后果和规模，例如「不确认则销售单不能生效 · 3 行 / ¥12,800」；禁止复读任务类型 |
| `nextActionHint` | 下一步 | 说明进入对应页面后要做什么 |
| `subjectVersionLabel` | 本任务针对版本 | 可读版本；正式提交由服务端校验 |
| `taskVersion` | 不上屏 | 任务自身乐观锁版本；责任命令必须原样回传，不得用 `subjectVersion` 代替 |
| `allowedActions` | 开始处理、退回团队、转交及领域动作 | blocker 必须使用可读业务原因 |

处理器元数据只允许 `handlerKey`、`destinationWorkspaceId`、`summarySections` 和
`resultQueryKey`。不得向客户端下发任意 URL、组件代码或可选择的 `completionAction`。

### 6.2 筛选

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 责任范围 | `mine` | `scope=mine|team|managed|history` | 固定文案“我的待办 / 团队待处理 / 团队任务 / 处理历史”；后两项按权限展示 |
| 任务类型 | 全部 | `type={fixedCode}` | 连续处理时收敛到单类型或兼容组 |
| 任务族 | 全部 | `family=approval|finance|fulfillment|exception` | 与 W01 展示分类一致 |
| 时限 | 全部开放 | `due=today|overdue|range` | 使用已确认工作时区 |
| 历史状态 | 按 scope 固定 | `status=completed|closed` | 仅 `scope=history` 可用；其它 scope 固定为 `OPEN` |
| 优先级 | 全部 | `priority=...` | 多选；服务端排序 |
| 搜索 | 空 | `q=` | 搜稳定单号、对象或往来方；服务端过滤 |
| 排序 | 超期与优先级 | `sort=priority_due` | 可切创建时间或截止时间 |

新任务到达只显示“有 N 条新任务”，不得使当前项突然跳位。

## 7. 操作合同

| 操作 | 前置条件 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- |
| 开始处理 | 开放 `POOL` 任务；当前用户仍有资格 | 原任务写入本人责任，状态仍为 `OPEN` | 他人已处理时显示“处理权已变化，请刷新” |
| 任务内非终结动作 | 本人负责、对象版本有效、处理器允许 | 追加证据或工作数据；任务仍为 `OPEN` | 保留输入；刷新版本或查询原结果 |
| 正式领域动作 / 审批决定 | 本人负责；权限、版本和岗位分离均通过 | 业务事实、步骤决定和任务完成同事务形成 | 停在当前项；查询原操作结果或同键重试 |
| 退回团队 | 开放 `POOL` 任务且策略允许 | 清空原任务个人责任并记录原因 | 保持服务端当前责任，刷新后重试 |
| 转交 | 目标用户合格且策略允许 | 更新原任务责任并记录原因 | 保持服务端当前责任，不创建替代任务 |
| 关闭 | 仅重复、误派或已有有效替代任务 | 原任务置为 `CLOSED`；不改业务事实 | 保持原状态；显示关闭阻断原因 |
| 上 / 下一项 | 当前队列上下文有目标 | 切换焦点 | 目标失效时定位下一有效项 |
| 打开对象 | 有对象查看权 | 聚焦对象页签，W02 上下文保留 | 权限收回时返回任务最小摘要 |

`scope=managed` 的主管可对授权范围内已分派任务执行服务端允许的 `REASSIGN`；只有 `POOL` 任务允许
`RELEASE_TO_TEAM`。`scope=history` 的所有任务均只读。审批阻塞恢复不属于待办责任动作：管理员必须在
`view=approval-blockers` 调用 `recover_approval(RETRY_CURRENT_STEP)`，且不得从任务行构造恢复命令。

“暂挂”不得创造 `PAUSED` 状态。需要释放个人责任时使用“退回团队”；需要保留个人责任但稍后继续时，
只记录非终结说明并保持原任务归属。任务类型不得以“暂挂”绕过其强类型业务规则。

## 8. 数据合同

### 8.1 查询

```ts
type WorkItemStatus = "OPEN" | "COMPLETED" | "CLOSED"
type AssignmentMode = "DIRECT" | "POOL"

type UnifiedTaskQueueQuery = {
  scope: "mine" | "team" | "managed" | "history"
  family?: "approval" | "finance" | "fulfillment" | "exception"
  workItemType?: string
  status?: WorkItemStatus
  due?: "today" | "overdue" | { from: string; to: string }
  priorities?: number[]
  query?: string
  sort: "priority_due" | "due_asc" | "created_desc"
  cursor?: string
  queueContextId?: string
  currentWorkItemId?: string
  timezone: string
}

type QueueWorkItem = {
  workItemId: string
  taskVersion: string
  workItemType: string
  handlerKey: string
  status: WorkItemStatus
  assignmentMode: AssignmentMode
  ownerRole: string
  ownerOrganization: { id: string; displayName: string }
  ownerUser?: { id: string; displayName: string }
  processingState: "READY" | "APPROVAL_BLOCKED"
  processingBlocker?: { code: string; message: string }
  businessObjectType: string
  businessObjectId: string
  subjectVersion: string
  allowedActions: string[]
  actionBlockers: string[]
}
```

Query Key 至少包含当前用户、活动角色、筛选和 `queueContextId`。返回的 `handlerKey`
必须存在于前端受控注册表；未识别类型进入阻塞态，不回退到通用表单。
`mine/team/managed` 只接受或默认 `status=OPEN`；`history` 只接受
`status=COMPLETED|CLOSED`。不兼容组合返回 400，不得静默改写筛选。
审批阻塞步骤保留的开放待办必须返回 `processingState=APPROVAL_BLOCKED`、权限安全的阻塞摘要和空
`allowedActions`；普通任务列表可展示其责任，但不得把它计入“可立即处理”数量。
所有承接任务的业务工作面嵌入投影必须返回与本类型同源的 `taskVersion`。

### 8.2 命令

队列容器只发送下列责任命令：

```ts
type WorkItemResponsibilityCommand = (
  | { kind: "START_PROCESSING"; workItemId: string; expectedTaskVersion: string }
  | { kind: "RELEASE_TO_TEAM"; workItemId: string; expectedTaskVersion: string; reason: string }
  | { kind: "REASSIGN"; workItemId: string; expectedTaskVersion: string; targetUserId: string; reason: string }
  | { kind: "CLOSE"; workItemId: string; expectedTaskVersion: string; reasonCode: string; comment?: string }
) & { idempotencyKey: string }
```

`expectedTaskVersion` 必须取自当前查询返回的 `taskVersion`。409 响应必须返回最新安全任务摘要和新
`taskVersion`，客户端刷新后由用户重新确认，不得自行递增。正式业务动作由 `handlerKey` 对应的强类型命令定义；
所有会写任务的强类型命令同样必须携带 `expectedTaskVersion`；审批任务通过服务端 `submit_decision` 端口推进。
不存在 `CLAIM` 或通用 `COMPLETE` 动作。

### 8.3 阻塞审批恢复

审批阻塞视图使用独立合同，不伪装成 `QueueWorkItem`：

```ts
type BlockedApprovalView = {
  approvalInstanceId: string
  instanceVersion: string
  currentStepInstanceId: string
  stepVersion: string
  workItem?: QueueWorkItem
  businessObjectLabel: string
  blockerCode: string
  blockerMessage: string
  blockedAt: string
  allowedActions: Array<"RETRY_CURRENT_STEP">
}

type RecoverApprovalCommand = {
  approvalInstanceId: string
  currentStepInstanceId: string
  expectedInstanceVersion: string
  expectedStepVersion: string
  expectedTaskVersion?: string
  recoveryAction: "RETRY_CURRENT_STEP"
  reason: string
  idempotencyKey: string
}
```

存在开放待办时 `expectedTaskVersion` 必填且必须来自 `workItem.taskVersion`；不存在待办时必须省略。
恢复响应返回新的实例、步骤和可选任务版本。恢复失败或结果未知时保持受阻，不得本地显示恢复成功。

## 9. 页面状态

| 状态 | 页面表现 | 恢复 |
| --- | --- | --- |
| 初载 | 页头、队列和处理区同构 Skeleton | 查询成功后原位替换 |
| 刷新 | 保留旧内容并标记刷新；正式动作提交前仍重验 | 成功更新，失败允许重试 |
| 无待办 | 区分范围内没有任务与当前筛选已处理完 | 清除筛选或返回 W01 |
| 无数据范围 | 不显示虚假 0 条；说明当前角色无范围 | 权限更新后重查 |
| 处理权已变化 | 处理器只读，显示可展示的当前责任人 | 刷新、下一项或有权转交 |
| 审批受阻 | 普通处理动作全部禁用，展示结构化业务说明 | 普通用户等待；授权管理员进入审批阻塞视图重试原步骤 |
| 对象版本冲突 | 对比任务版本与当前事实，禁用旧决定 | 刷新事实或进入服务端给出的替代任务 |
| 非终结动作成功 | 固定动作记录并明确任务仍待处理 | 继续处理；不自动下一项 |
| 结果未知 | 不显示成功，不移动下一项 | 查询原结果或沿同一幂等键重试 |
| 正式动作成功 | 固定业务结果、处理时间和下一步 | 用户确认后进入下一项 |
| 权限收回 | 删除敏感字段和动作，保留最小任务身份 | 返回可访问范围 |

## 10. 响应式与无障碍

- 1440×900 使用左 34% 队列、右 66% 处理区；两区独立滚动。
- 1024×768 左队列可收起；768×1024 改为上方任务选择器、下方处理器；375×812
  只保留阅读和被业务明确允许的简单动作。
- Tab 顺序固定为刷新、责任范围、筛选、队列、处理表单、决定区。
- 队列切换后焦点落到新对象标题，`aria-live=polite` 播报“第 N/M 项、任务类型、对象”。
- 责任冲突、版本冲突和结果未知必须同时使用文字和 tone，不得只靠颜色。
- 所有触控目标不小于 44×44px。

## 11. 与其他工作面的关系

W01、W07、W13、W18、W29 及其它任务处理页只复用 W02 的队列上下文、责任命令和页面容器。
各工作面必须保留自己的强类型业务命令，不得复制责任状态机或自行推进审批步骤。

跨工作面只传稳定对象身份、`workItemId` 和返回焦点；`taskVersion` 不通过 URL 传递，目标工作面必须
随任务投影重新查询。不得传金额、审批结论、责任人资格或 `allowedActions` 作为正式事实。

## 12. 验收清单

- [ ] `DIRECT` 任务直接进入唯一用户“我的待办”，页面不显示“开始处理”。
- [ ] 未分派 `POOL` 任务只在“团队待处理”展示；并发开始处理只能一人成功，同一用户重试幂等。
- [ ] 队列与全部嵌入任务投影均返回 `taskVersion`；责任命令使用该值，409 返回最新版本供刷新。
- [ ] `managed` 能查看授权范围内已由下属负责的开放任务并受控转交；越权组织和用户不可查询。
- [ ] `history` 只返回有权查看的 `COMPLETED/CLOSED` 任务且无动作；非法 scope/status 组合返回 400。
- [ ] 审批阻塞视图能重试原步骤；没有审批决定、指定处理人或跳步参数。
- [ ] 刷新和浏览器重开后从服务端恢复责任，不依赖租约、令牌或客户端状态。
- [ ] 正式提交同时校验当前责任人、对象版本、权限和岗位分离。
- [ ] 非终结动作后任务仍为 `OPEN`，不自动移动下一项。
- [ ] 正式业务事实、审批步骤结果和任务 `COMPLETED` 在同一事务形成。
- [ ] 退回团队和转交只更新原开放任务责任，不创建同义后继任务。
- [ ] 关闭只适用于重复、误派或有效替代任务，不改业务事实。
- [ ] 不存在批量开始处理、公共 `claim`、公共 `complete` 或客户端 `completionAction`。
- [ ] 页面可见文案不存在“领取、重新领取、团队待认领、租约、令牌、角色池”。

## 13. 业务依据

- `approval-workflow-contract.md`：审批运行、分派、多级步骤、强类型完成和 BPM 边界。
- `erp-data-model.md` §6.1、§8.0：审批实例、步骤实例、待办字段和事务不变量。
- `dev-plan/api-contract.md` §8：队列、责任动作和审批接口边界。
- `erp-ui-design.md` §3.4、§4.4、§11、§13：TaskTabs、M3、状态并发和 W02。
- `erp-ui-flows.md`：各类任务的跨工作面入口与返回上下文。
