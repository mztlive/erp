# W02 · 统一待办队列

> 状态：草稿
> 页面模式：M3 连续处理队列
> 主要路由：`/workspace/tasks`
> 主要角色：全部已登录用户；业务主管与管理员按授权处理角色池
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

用户进入 W02 后，应在一个可恢复的队列上下文中完成：

1. 找到真正归属于自己或可领取的角色池任务；
2. 读懂任务对象、原因、影响、时限和当前责任人；
3. 在同类任务中连续处理，不必每次返回列表找下一条；
4. 遇到被他人领取、版本变化或结果未知时，明确知道当前任务是否仍可处理以及如何恢复。

### 1.2 业务目标

- 以正式 `work_item` 为唯一待办事实源，不从各业务单据状态在前端反推任务。
- 用同一套查找、领取、租约、转交和完成反馈契约承载全部固定任务类型。
- 为 W07、W13、财务审核、卡券审批和异常处理提供共用队列容器；业务表单由相应任务类型处理器提供。
- 确保任务完成和正式业务状态变化属于同一强类型业务事务，前端不先行标记完成。

### 1.3 不在本工作面完成

- 不在 W02 定义销售、采购、财务或集成任务的状态机。
- 不提供“批量通过全部”；每项正式结论均须对当前对象版本重新校验。
- 不以人工关闭替代审批、确认、结果未知或未完成补偿任务。
- 不在通用摘要中复制完整销售单、采购单或对账页；深挖事实时打开对应对象中心。

## 2. 用户、权限与数据范围

| 用户场景 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 任务被指派到本人 | W01 “处理”、顶栏待办 | `owner_user_id` 为当前用户的有效任务 | 查看、处理、暂挂；有权时转交 |
| 角色池成员 | W02 “待领取”视图 | 当前有效角色可领取且数据范围匹配的任务 | 领取、处理、暂挂；租约是否释放以受控动作结果为准 |
| 业务主管 | 团队视图 | 团队及授权组织范围内任务 | 查看责任分布、按规则转交 |
| 系统管理员 | 异常视图 | 技术处理职责内的结果未知和业务异常 | 查询结果、转交业务责任人；不替代业务决策 |
| 只读参与者 | 对象时间线链接 | 已参与历史单据所对应的已完成任务 | 查看历史，不领取或重做 |

权限与并发规则：

- 服务端先按模块权限、当前角色、组织和对象数据范围过滤；前端不取全量后隐藏。
- 任务可见不等于可处理。动作一律使用 `allowedActions` 和 `actionBlockers`，禁用时显示具体原因。
- 角色池任务必须原子领取并取得租约令牌；有效租约期内其他用户不能同时处理。
- 页面打开期间权限被收回时，清除当前处理器中的敏感快照，保留任务编号和返回上下文。
- 转交不直接改责任人；服务端关闭原租约、记录转交链并创建后继任务。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 打开全部待办 | 顶栏待办、侧栏 | `/workspace/tasks?scope=mine` | 保留上一业务页签 |
| 从 W01 指标或分组进入 | “查看全部” | 携带 `family`、`due`、`scope` | 返回时恢复 W01 筛选与焦点 |
| 处理某条任务 | 任务条目 | 增加 `currentWorkItemId={workItemId}` 与 `queueContextId`；同类队列时嵌入专用处理器 | 完成后默认下一项 |
| 打开对象中心 | 当前任务“打开对象” | 新建或聚焦稳定对象页签，W02 页签保留 | 关闭对象后回当前任务 |
| 复制队列链接 | 浏览器地址 | 恢复筛选、`currentWorkItemId` 和 `queueContextId`；临时 Dialog 不恢复 | W02 |
| 任务已被处理 | 外部链接或旧页签 | 展示已完成/已转交结果和后继任务链接 | 返回当前队列首条 |

TaskTabs 身份为 `workspace:tasks:{userId}`。筛选、队列位置和当前任务只改变该页签 URL，不为每条任务创建新页签。专用 W07/W13 队列已打开时，从 W02 处理同一任务应聚焦原队列页签。

## 4. 页面布局

### 4.1 1440×900 基准布局

```text
┌ PageHeader：统一待办   个人 18 · 待领取 7 · 超期 2   [刷新] ───────┐
├ 队列工具栏：[我的待办] [待领取] [团队]  类型 · 时限 · 优先级 · 搜索 ├───────┤
├────────────────────────────────┬────────────────────────────────────────────┤
│ 任务队列 34%                 │ 当前任务处理区 66%                         │
│ 筛选摘要 · 共28项             │ SequentialProcessBar 第 3/28                │
│                                │ 对象身份、原因、影响、截止、附件           │
│ [当前条目]                    │                                            │
│ [普通条目]                    │ 任务类型处理器（W07/W13/审核/异常）       │
│ [已被他人领取]              │                                            │
│                                │ [暂挂] [打开对象]          [正式决策主动作] │
└────────────────────────────────┴────────────────────────────────────────────┘
```

### 4.2 区域说明

| 区域 | 目的 | 主组件 | 固定规则 |
| --- | --- | --- | --- |
| 页头与水位 | 说明当前责任范围和任务更新时间 | `PageHeader` `DataFreshness` | 顶部固定，不随右区滚动 |
| 队列工具栏 | 切换责任范围和同类处理集合 | 分段按钮、搜索、筛选面板 | 所有可见控件必须可用 |
| 左侧队列 | 识别上下项、超期和领取状态 | `WorkTaskItem` 紧凑变体 | 独立滚动，当前项始终有文字选中态 |
| 处理导航 | 告知位置、租约和下一项偏好 | `SequentialProcessBar` | 右区 sticky |
| 任务摘要 | 在做决定前读懂业务背景 | `DocumentSummary` `ResponsibilityPanel` | 只读，字段由任务类型白名单提供 |
| 类型处理器 | 执行强类型表单和决策 | 注册的业务组件 | 不允许服务端下发任意组件或 URL |
| 固定结果区 | 保留本次正式动作结果 | `FormalActionResult` | 成功后先展示再移动到下一项 |

### 4.3 队列组织

- “全部类型”为找任务视图；进入正式处理后，队列必须收敛到同一 `work_item_type` 或同一兼容处理器组，避免决策区在每条间完全改变。
- 默认排序：已超期 → 优先级降序 → `due_at` 升序 → `created_at` 升序。服务端排序，前端不用本地时钟重排业务优先级。
- “团队”视图不默认进入连续处理；用户必须先选中本人可处理范围并完成领取。

## 5. 展示内容与字段

### 5.1 队列条目与当前任务

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 条目 | `workItemTypeLabel` | 任务类型 | 固定 `work_item_type` 展示映射 | 不直接展示代码 | 任务可见时可见 |
| 条目 | `businessObjectLabel` | 对象类型 · 单号/名称 | 任务对象查询投影 | 稳定身份与展示标题分离 | 无对象字段权限时只显示必要身份 |
| 条目 | `statusLabel` | 待领取 / 待处理 / 处理中等 | `work_item.status` | 文字 + tone | 全部可见 |
| 条目 | `priorityLabel` | 紧急 / 高 / 普通 | `priority` | 普通不额外堆徽章 | 全部可见 |
| 条目 | `dueAt` | 截止 / 已超期 | `due_at` | 相对时间 + 可读绝对时间 | 全部可见 |
| 条目 | `owner` | 责任角色 · 责任人 | `owner_role/owner_user_id` | 未领取时写“待领取” | 姓名按组织权限显示 |
| 摘要 | `reasonLabel` | 为什么需要处理 | `reason_code` 固定文案 | 不展示内部堆栈 | 按任务可见 |
| 摘要 | `impactSummary` | 业务影响 | `impact_summary` | 必须是业务语言 | 敏感对象参数按字段权限遮罩 |
| 摘要 | `subjectVersion` | 本任务针对版本 | `subject_version/subject_hash` | 只显示可读版本，hash 不显示 | 用于并发校验 |
| 租约 | `lease` | 我已领取 / 某人处理中 / 租约已失效 | 领取结果 | 显示到期时间，不显示 token | 只有合法处理者可续租 |
| 操作 | `allowedActions` | 领取、处理、暂挂、转交、关闭 | 服务端鉴权结果 | blocker 必须可读 | 前端不根据角色名硬编码 |

### 5.2 处理器必须提供的共用元数据

| 元数据 | 用途 |
| --- | --- |
| `handlerKey` | 映射前端已注册的处理器，不作为用户文案 |
| `destinationWorkspaceId` | 需要新工作面时进行受控路由查找 |
| `summarySections` | 当前类型允许展示的只读业务摘要 |
| `completionAction` | 该任务唯一允许的完成动作身份 |
| `resultQueryKey` | 结果不确定时查询最终结果，不包含任意 URL |

## 6. 搜索、筛选、排序与默认视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 责任范围 | `mine` | `scope=mine|role_pool|team` | 无对应权限不展示切换项 |
| 任务类型 | 全部 | `type={fixedCode}` | 正式连续处理时收敛为单类型/兼容组 |
| 任务族 | 全部 | `family=approval|finance|fulfillment|exception` | 与 W01 分组一致，只是展示分类 |
| 时限 | 全部有效 | `due=today|overdue|range` | 日期边界使用已确认工作时区 |
| 状态 | 未完成 | `status=active|completed|transferred|closed` | 历史状态只读 |
| 优先级 | 全部 | `priority=...` | 多选，不在前端重新计算 |
| 搜索 | 空 | `q=` | 搜稳定单号、对象标题或任务编号；服务端过滤 |
| 排序 | 超期与优先级 | `sort=priority_due` | 可选创建时间、截止时间；队列切换后重定位当前项 |
| 当前项 | 首条可处理任务 | `currentWorkItemId={workItemId}` | 不在当前筛选中时显示原因并提供回队列 |

已选筛选必须显示摘要和结果数。队列使用游标或稳定快照顺序，处理期间新到任务不得使当前项突然跳位；用“有 N 条新任务”提示由用户刷新。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 领取 | 角色池任务 | `CLAIM` 可用，任务待领取 | 无 | 返回租约、版本和当前处理权 | 已被领取时显示最新领取人并刷新队列 |
| 续租 | 当前处理器 | 本人有效租约 | 无，受控自动 | 更新 `leaseVersion/expiresAt` | 保留本地输入，禁止正式提交，允许重新领取 |
| 执行任务内动作 | 任务类型动作区 | 租约、对象版本和内容指纹通过，且动作不代表正式完成 | 按动作风险决定 | 追加动作证据；任务保持正式非终结状态 `PENDING/IN_PROGRESS`，可返回不含 token 的 `WorkItemLeaseState` | 保留输入；版本冲突刷新比较；结果不确定时停在当前项 |
| 正式完成 | 任务类型唯一完成主动作 | 租约、对象版本、内容指纹和岗位分离全部通过 | `FormalActionConfirmDialog` | 强类型业务事务与任务完成同时生效，展示结果号和下一步，再自动下一项 | 保留输入；版本冲突刷新比较；结果不确定时停在当前项 |
| 暂挂 | 当前任务 | `DEFER` 可用 | 可选暂挂原因 | 通过任务内动作追加暂挂证据；任务返回 `PENDING/IN_PROGRESS`，释放或保留租约按服务端结果，打开下一项 | 留在当前项并说明未暂挂 |
| 转交 | 更多动作 | `TRANSFER` 可用，目标用户/角色有资格 | 必须展示责任变化与原因 | 用 `TransferWorkItemEnvelope` 原子转交原任务、失效原租约并创建 `UNCLAIMED/PENDING` 后继任务，固定展示转交链 | 失败时原任务和原租约保持服务端当前状态，刷新后可重试 |
| 关闭误派/重复任务 | 更多动作 | 仅服务端判定可关闭的重复、误派或有替代任务；任务类型允许关闭 | 必须选结构化原因并提供替代任务或受控关闭证据 | 使用 `CloseWorkItemEnvelope` 追加关闭记录并返回 `CLOSED`；不改业务事实 | 原任务和租约保持服务端当前状态，保留原因/证据后重试 |
| 上/下一项 | `SequentialProcessBar` | 当前快照内有目标 | 无 | URL 与焦点切换 | 目标失效时找当前快照的下一有效项 |
| 打开对象 | 摘要区 | 有对象查看权 | 无 | 聚焦对象中心，W02 上下文不丢 | 对象权限收回时回 W02 的可读任务摘要 |

## 8. 数据契约

### 8.1 查询

```ts
type WorkItemStatus =
  | "UNCLAIMED"
  | "PENDING"
  | "IN_PROGRESS"
  | "COMPLETED"
  | "TRANSFERRED"
  | "CLOSED"

type UnifiedTaskQueueQuery = {
  scope: "mine" | "role_pool" | "team"
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

type UnifiedTaskQueueView = {
  queueContextId: string
  queueContextCreatedAt: string
  currentWorkItemId?: string
  previousWorkItemId?: string
  nextWorkItemId?: string
  freshness: { updatedAt: string; state: "fresh" | "stale" }
  filterSummary: string
  total: number
  counts: { mine: number; rolePool: number; overdue: number }
  items: QueueWorkItem[]
  nextCursor?: string
  current?: QueueWorkItemDetail
}
```

`queueContextId` 是跨 W01/W02/W07 恢复筛选、稳定顺序和返回焦点的唯一队列上下文字段。首次进入没有该值时由服务端创建并在响应与 URL 中固定，不接受平行别名。任务身份统一使用 `workItemId`，队列定位只使用 `currentWorkItemId`、`previousWorkItemId` 和 `nextWorkItemId`。

查询要求：

- Query Key 至少包含当前用户、活动角色、权限版本、数据范围版本、筛选和 `queueContextId`。
- 任务列表、当前任务和对象摘要可分段查询；某一摘要失败不清空整个队列。
- 返回的 `handlerKey` 必须存在于前端受控注册表；未识别类型进入失败态，不猜测通用提交表单。

### 8.2 领取、任务内动作、完成与转交

```ts
type ClaimWorkItemCommand = {
  workItemId: string
  expectedLeaseVersion?: number
  idempotencyKey: string
}

type RenewWorkItemLeaseCommand = {
  workItemId: string
  claimToken: string
  leaseVersion: number
  idempotencyKey: string
}

type WorkItemLease = {
  workItemId: string
  claimToken: string
  leaseVersion: number
  leaseExpiresAt: string
  subjectVersion?: string
  subjectHash: string
}

type WorkItemLeaseState = {
  workItemId: string
  leaseVersion: number
  leaseExpiresAt: string
}

type WorkItemActionEnvelope<TAction> = {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectVersion?: string
  expectedSubjectHash: string
  idempotencyKey: string
  action: TAction
}

type WorkItemActionResult<TEvidence> = {
  workItemId: string
  workItemStatus: "PENDING" | "IN_PROGRESS"
  actionRecordId: string
  evidence?: TEvidence
  lease?: WorkItemLeaseState
  subjectVersion?: string
  subjectHash: string
}

type CompleteWorkItemEnvelope<TDecision> = {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectVersion?: string
  expectedSubjectHash: string
  idempotencyKey: string
  decision: TDecision
}

type CompleteWorkItemResult<TBusinessResult> = {
  workItemId: string
  workItemStatus: "COMPLETED"
  completionRecordId: string
  businessResult: TBusinessResult
  subjectVersion?: string
  subjectHash: string
}

type CloseWorkItemDecision =
  | {
      kind: "CLOSE_DUPLICATE"
      reasonCode: string
      replacementWorkItemId: string
      closureEvidenceReference: string
      comment?: string
    }
  | {
      kind: "CLOSE_MISROUTED"
      reasonCode: string
      replacementWorkItemId?: string
      closureEvidenceReference: string
      comment?: string
    }
  | {
      kind: "CLOSE_WITH_REPLACEMENT"
      reasonCode: string
      replacementWorkItemId: string
      closureEvidenceReference: string
      comment?: string
    }

type CloseWorkItemEnvelope<
  TClosure extends CloseWorkItemDecision = CloseWorkItemDecision,
> = {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectVersion?: string
  expectedSubjectHash: string
  idempotencyKey: string
  closure: TClosure
}

type CloseWorkItemResult = {
  workItemId: string
  workItemStatus: "CLOSED"
  closureRecordId: string
  reasonCode: string
  replacementWorkItemId?: string
  closureEvidenceReference: string
  subjectVersion?: string
  subjectHash: string
}

type TransferWorkItemEnvelope<TTransfer> = {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectVersion?: string
  expectedSubjectHash: string
  idempotencyKey: string
  transfer: TTransfer
}

type TransferWorkItemResult = {
  originalWorkItemId: string
  originalWorkItemStatus: "TRANSFERRED"
  transferRecordId: string
  successorWorkItemId: string
  successorWorkItemStatus: "UNCLAIMED" | "PENDING"
}
```

- `claimToken` 只由领取或续租 mutation 的 `WorkItemLease` 响应返回，并只存于当前会话内存，不进查询 View、URL、日志或分析事件。任务内动作结果最多返回不含 token 的 `WorkItemLeaseState`；正式完成、关闭和转交响应也不回显 token。
- `WorkItemActionEnvelope.action` 只承载查询、重放、保存证据、暂挂等**不终结任务**的强类型动作；它的 `idempotencyKey` 只标识本次任务动作，不得兼作外部接口原动作或业务事实的幂等键。
- 任务内动作成功必须返回服务端正式非终结状态 `workItemStatus: "PENDING" | "IN_PROGRESS"`；可同时返回动作证据和不含 token 的 `WorkItemLeaseState`。该结果不等于任务完成，前端不能仅因动作成功自动移到下一项；只有用户显式执行 `DEFER` 等处理器动作且其契约要求切换时，才在固定结果后更新 `currentWorkItemId`。
- `CompleteWorkItemEnvelope` 只用于处理器注册的正式业务完成动作。决策表单由各 `handlerKey` 定义，正式业务结果与任务 `COMPLETED` 必须同一事务返回；它不能返回 `CLOSED`，前端也不再单独调用“标记已完成”。
- `CloseWorkItemEnvelope` 只用于服务端确认可关闭的重复、误派或已有替代任务场景；所有分支强制结构化 `reasonCode` 与 `closureEvidenceReference`，重复/替代分支还必须给出 `replacementWorkItemId`。审批、确认、结果未知、未完成补偿以及处理器禁止关闭的任务一律拒绝；关闭只追加任务关闭证据，不写正式业务结论。
- `TransferWorkItemEnvelope` 只用于转交：同一事务把原任务置为 `TRANSFERRED`、使原租约失效、追加转交记录并创建一个 `UNCLAIMED/PENDING` 后继任务；转交不完成任务、不写正式业务结论，也不允许直接覆盖责任人。
- 超时或网络中断不改本地业务状态；使用同一幂等键查询最终结果，确认后再移到下一项。

### 8.3 前端边界

- 只格式化时间、优先级、状态和类型文案；不改任务状态或完成条件。
- “超期多久”可用服务端时钟基准做显示计算，不用于改排正式优先级。
- 对象摘要是查询投影，正式决策提交前必须由服务端重读当前事实。
- 任务完成数、超期数和角色池数由服务端聚合，不从已加载页面求和。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 页头、左队列和右处理区同构 Skeleton | 应用壳可导航 | 查询成功后原位替换 |
| 刷新 | 保留旧队列和当前项，显示轻量刷新 | 已领取任务可续编辑，提交时重验 | 成功更新快照，失败可重试 |
| 无待办 | “本筛选项已处理完” + 返回 W01 | 清除筛选、查看已完成 | 新任务到达或刷新 |
| 筛选无结果 | 保留工具栏和当前筛选摘要 | 清除筛选 | 回默认有效待办 |
| 无数据范围 | 不显示虚假 0 条；说明当前角色无范围 | 查看当前角色 | 权限更新后重查 |
| 列表失败 | 无缓存时整页失败；有缓存时保留并标记陈旧 | 重试；缓存对象仅查看 | 查询成功 |
| 摘要分区失败 | 只替换该分区，队列与租约保留 | 重试分区、打开对象 | 分区查询成功 |
| 任务已被他人领取 | 处理器只读，显示领取人与租约到期 | 下一项、查看对象 | 租约到期后重新领取 |
| 租约丢失 | 保留本地输入，显示“不能提交” | 重新领取、复制本地备忘 | 重新领取并确认版本未变 |
| 对象版本冲突 | `ConflictResolutionDialog` 对比任务针对版本与当前事实 | 刷新、跳到替代任务 | 服务端关闭/转交旧任务 |
| 保存失败 | 决策输入保留，错误靠近主动作 | 重试 | 使用同一幂等键再提交 |
| 任务内动作成功 | 固定展示动作证据和不含 token 的租约状态（如有），明确标注任务仍为 `PENDING/IN_PROGRESS` | 继续处理、正式完成、暂挂或转交 | 不自动下一项；重新查询任务和对象版本 |
| 任务内动作结果不确定 | 保留输入和本次动作幂等身份，任务继续显示 `PENDING/IN_PROGRESS`，不渲染完成 | 查询本次动作结果、用同一幂等键重试 | 得到确定动作结果后继续处理，不能自动完成 |
| 正式动作成功 | 固定结果、业务对象号、处理时间和下一步 | 打开结果对象、下一项 | 结果已固定，不靠 toast |
| 结果不确定 | 停在当前项，不渲染成功或移到下一项 | 查询最终结果、幂等重试 | 得到确定结果 |
| 字段级隐藏 | 任务身份和字段标签保留，敏感金额/技术摘要掩码；缺关键字段权时正式动作禁用 | 查看其它授权字段、转交有权责任人 | 权限版本更新后重查 |
| 权限收回 | 清除敏感字段和租约，保留任务身份 | 返回可访问视图 | 权限恢复后重查 |

## 10. 响应式、键盘与无障碍

### 10.1 响应式

| 视口 | 布局变化 | 必须保留 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 左 34% 队列 + 右 66% 处理，两区独立滚动 | 当前位置、对象、截止、原因、决策与租约 | 无 |
| 1280×800 | 左区缩至 32%，摘要字段紧凑 | 当前项与主动作 | 队列条目影响摘要限两行 |
| 1024×768 | 左队列可收起为窄栏，右区全宽；工具栏换行 | 队列位置、对象标题、决策、结果 | 次要摘要折叠 |
| 768×1024 | 上方任务选择器 + 下方处理器；筛选进面板 | 当前任务身份、截止、主动作、结果不确定入口 | 不同时展示完整队列和处理表单 |
| 375×812 | 单列，只保证待办阅读与经业务标记允许的简单确认 | 对象、原因、截止、确认结果 | 复杂分摊、表格编辑和 diff 引导至桌面工作面；不伪造可提交表单 |

### 10.2 键盘与焦点

- Tab 顺序：页头刷新 → 责任范围 → 搜索/筛选 → 队列条目 → 处理表单 → 决策区。
- `j/k` 可在未聚焦输入框时切换下/上一项；必须同时提供可见按钮，不以快捷键作为唯一途径。
- 队列切换后焦点落到新对象标题，`aria-live=polite` 播报“第 N/M 项、任务类型、对象”。
- 正式动作确认层关闭后焦点返回触发按钮；成功移到下一项时落到新对象标题。
- 驳回或转交表单校验失败时，焦点先到 `ValidationSummary`，再可直达首个错误字段。
- 超期、租约丢失、版本冲突和结果未知均使用文字 + tone，不只靠颜色。
- 桌面触控和移动按钮的可点区域不小于 44×44px。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 今日工作台 | W01 | `scope`、`family`、`due`、来源 `workItemId` 焦点 | 返回 W01 恢复原筛选 |
| 客户、合同、销售单 | W03 / W04 / W05 | `businessObjectId`、`workItemId`、只读来源 | 对象页签关闭后回当前任务 |
| 采购二次确认 | W07 | `currentWorkItemId=workItemId`、`queueContextId` | W07 处理后按同一上下文恢复 W02 行焦点 |
| 采购/履约对象 | W08 / W09 | 业务对象稳定 ID、来源 `workItemId` | 返回仍保留队列位置 |
| 票款与复核 | W11 / W12 / W13 | 客户/供应商主体、对应单据、任务 | W11/W12 只形成回款、付款、发票等领域事实或纠错申请并返回原处理器重算指纹；只有 W13 或对应 W02 handler 的任务绑定决定，才通过共享信封与业务结果同事务完成/转交 `work_item` |
| 同步、映射与错误 | W17 / W21 / W29 | 差异/错误任务身份、原对象 | 处理完等待正式任务查询更新 |

跨工作面只传递稳定身份、`queueContextId` 和返回焦点，不传递金额、审批结论或权限结果作为正式事实。

## 12. 验收清单

### 12.1 任务效率

- [ ] 从 W01 任务条目到 W02 当前处理器不超过一次点击。
- [ ] 同类任务通过、驳回或暂挂后可直接处理下一项，不回普通列表。
- [ ] 全部类型视图可找任务，正式处理时收敛到单类型或兼容处理器组。
- [ ] 当前条目能回答对象、原因、影响、截止、责任人和下一步。
- [ ] 完成队列有明确结束态和返回 W01 入口。

### 12.2 数据、权限与并发

- [ ] 同一任务不能被两个用户在有效租约内同时处理。
- [ ] 提交同时校验领取人、租约、对象版本、内容指纹和岗位分离。
- [ ] 查询、重放、保存证据和暂挂等任务内动作使用 `WorkItemActionEnvelope`，成功后任务仍为正式 `PENDING/IN_PROGRESS`，不会误触发完成或自动下一项。
- [ ] 任务完成与业务事实变化是同一事务，无独立“标记完成”伪动作。
- [ ] 正式完成只能返回 `COMPLETED`；重复、误派或已有替代任务的关闭必须使用 `CloseWorkItemEnvelope` 并强制原因、替代引用/关闭证据，返回 `CLOSED` 但不改业务事实。
- [ ] 转交使用 `TransferWorkItemEnvelope`，原任务转交、原租约失效和 `UNCLAIMED/PENDING` 后继任务创建原子完成，不直接覆盖责任人或写业务结论。
- [ ] 审批、确认、结果未知和补偿任务无人工关闭入口。
- [ ] 权限收回后当前处理器不残留敏感快照或租约令牌。

### 12.3 状态与终端

- [ ] 网络超时不自动跳到下一项，可用同一幂等键查询最终结果。
- [ ] 租约丢失和版本冲突都保留本地输入但阻止提交。
- [ ] 第 9 节全部状态通过组件或浏览器验收。
- [ ] 1440、1280、1024、768、375 五档视口符合第 10.1 节。
- [ ] 仅用键盘可完成筛选、领取、打开对象、做决定和继续下一项。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | 主管“团队视图”是否可直接转交，还是只可请求原处理人暂挂？ | 管理动作权限和有效租约处理 | 各部门负责人 | 无有效租约时可转交；有效租约时需明确的管理接管动作与原因 |
| Q2 | 375px 窄屏允许的“简单确认”任务类型白名单是哪些？ | 移动端可见动作与安全验收 | 业务负责人 + 安全负责人 | 只允许无表格编辑、无敏感值揭示、无多对象分摊的任务，白名单由服务端下发 |

待确认事项确认后，应把结论写回对应章节并从本表移除；不得长期保留“建议”与正式规则并存。

## 14. 业务依据

- `erp-ui-design.md` §3.4 TaskTabs、§4.4 M3、§11 状态与并发、§13 W02。
- `erp-phase-1.md` §5.1：统一待办覆盖采购确认、财务审核、票款复核、资质到期、履约超期和同步失败。
- `erp-phase-1.md` §4.6.2：正式待办在业务事务内同步更新。
- `erp-phase-2.md` §15–§16：卡券审批、供应商结算、接口错误等增量任务及角色职责。
- `erp-data-model.md` §6.1 `work_item`：固定任务类型、责任人、租约、转交、关闭证据和完成审计。
- `erp-data-model.md` §4.6：状态固定、角色和数据范围配置化。
- `erp-ui-flows.md` §4、§6、§9、§10：各类处理任务使用连续队列，对象中心与队列上下文分离保留。
