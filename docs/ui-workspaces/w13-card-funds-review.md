# W13 · 卡券票款复核

> 状态：草稿
> 页面模式：M3 连续处理队列（内嵌复用 W11 的回款/发票核销能力）
> 主要路由：`/finance/card-funds-review`
> 主要角色：财务
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

- 财务连续处理卡券销售单期初票款任务，在一屏内核对同步成交额、当前应收、净已收、净已开票、证据和复核结论。
- 若存在历史回款或发票，直接在当前队列上下文登记正式事实并完成多对多核销；若确认没有历史票款，明确“从 0 起”并留下正式证据。
- 后续商城同步金额或票款分配变化时，处理新的差额复核，清楚比较上一有效复核与当前事实，不沿用旧结论。
- 正式完成或驳回得到可验证终态后可自动进入下一项；暂挂后任务仍为 `PENDING/IN_PROGRESS`，只提供手动继续浏览，不把导航冒充任务完成。

### 1.2 业务目标

- 对期初卡券应收执行逐单人工复核，禁止估算、批量置为已结清或直接编辑已收/已开票汇总。
- 用 `receivable_funds_review` 追加式链保存每次 `OPENING` 或 `SYNC_DELTA` 复核，权威性由链尾通过记录与当前 `subject_hash` 一致共同决定。
- 复用 W11 的 `customer_receipt`、`invoice` 及其分配，确保卡券与实物服务使用同一票款内核。
- 复核未完成或旧指纹失效时，W11/W15 明确标记应收指标不可靠，不以 0 值冒充已核实事实。

### 1.3 不在本工作面完成

- 不修改商城主责卡券销售单的成交额、客户、合同、类目、状态或历史同步版本。
- 不直接输入“累计已收金额”或“累计已开票金额”覆盖汇总；真实历史记录必须形成正式回款、发票和核销分配。
- 不用 Excel 导出、修改、导回作为主路径；W13 必须可逐项连续处理。
- 不把期初复核与后续同步差额复核合并为同一记录，也不复制旧通过结论。
- 不在本工作面处理基础资料映射；映射未完成的商城快照不能形成应收，也不应进入正常复核队列。
- 不计算卡券实际成本或利润；一期卡券成本未覆盖不等于零成本。

## 2. 用户、权限与数据范围

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 财务复核人 | W13 | 分配给本人或有权领取的卡券票款任务 | 领取、登记票款、确认从 0 起、通过、驳回、暂挂 |
| 财务经办 | W11/W13（按配置） | 有权登记历史回款和发票的往来主体 | 登记与核销；是否可同时完成复核由岗位分离策略决定 |
| 财务负责人 | W02/W13 | 授权范围内全部复核任务和进度 | 转交/查看；正式动作仍按任务权限 |
| 销售 | W05 | 本人销售单的复核状态摘要 | 只读，不进入 W13 财务队列 |
| 管理层 | W15 | 复核完成率与可靠性标识 | 只读，不见登记和决策入口 |

权限与租约规则：

- 无 W13 模块权限时隐藏导航；直接访问显示无权限页。
- 队列服务端按角色、组织和往来范围返回任务；前端不得获取全部任务后过滤。
- 任务领取复用 W02 原子租约，成功返回的令牌统一命名为 `claimToken`；它只存当前会话内存，不进 URL、日志、分析事件或查询缓存。
- 已被他人有效领取时显示领取人/到期时间，处理动作禁用，允许按权限只读查看。
- 正式完成使用 `CompleteWorkItemEnvelope<CardFundsReviewDecision>`，同时校验 `claimToken`、`leaseVersion`、任务 `subject_hash`、当前领域版本、复核链尾和岗位分离。
- 登记回款/发票与复核是否允许同一人完成由服务端 `allowedActions/actionBlockers` 返回；前端不按角色名自行判断。
- 银行、发票和证据字段按 W11 敏感字段规则处理；权限收回或租约丢失时立即清除揭示值并禁止提交。
- 有队列权限但当前没有可处理数据时显示已完成空态；无数据范围与“任务已处理完”必须区分。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 财务默认进入 | W01/W02/侧栏 | `/finance/card-funds-review?type=all&scope=mine`，定位首条可处理任务 | 工作台/待办页签保留 |
| 打开指定任务 | W01/W02/W11/W05 状态入口 | 将来源 `workItemId` 写入 `currentWorkItemId`，并携带 `queueContextId`，聚焦已有 W13 页签 | 返回原来源页签 |
| 打开销售单 | 当前对象摘要 | 新开/聚焦 W05 稳定销售单页签；W13 租约按规则续期 | 回 W13 恢复当前项和滚动位置 |
| 登记历史回款/发票 | 当前复核项 | 在 W13 内容区展开同一 Allocation 组件；复杂深挖可聚焦 W11 会话，队列页签保留 | 完成后回当前项并重新查询指纹 |
| 完成并自动下一项 | 决策区 | URL 更新 `currentWorkItemId`，保留类型、范围、排序和自动下一项偏好 | 队列末尾进入完成空态 |
| 刷新/浏览器后退 | 任意任务 | 恢复队列筛选、位置和当前任务；服务端重新校验租约和版本 | 不恢复敏感揭示状态 |

TaskTab 身份为 `queue:card-funds-review:{queueContextId}`，标题为 `卡券票款复核`。同一队列上下文只保留一个页签；当前项变化不改变页签身份。URL 至少保存任务类型、范围、排序、当前项和自动下一项偏好，但不保存金额、证据、权限结论或 `claimToken`。

## 4. 页面布局

### 4.1 1440×900 基准布局

```text
┌ SequentialProcessBar：第 3/28 项 · 期初/差额 · 筛选摘要 · 租约   [上一项] [下一项] ┐
├ 对象身份：销售单号 · 当前版本 · 客户 · 往来主体 · 主责商城 · 复核类型             ┤
├─────────────────────────────────────────┬────────────────────────────────────┤
│ 左侧主区约 64%                          │ 右侧证据与历史约 36%                │
│ [同步成交额] [当前应收] [净已收] [净已开]│ 上一复核链尾 / 当前指纹状态          │
│ 差额任务：上一基线 vs 当前事实 diff      │ 银行/发票/正式核对证据               │
│ 正式回款与发票明细                       │ 同步版本、任务原因、审计时间线         │
│ [登记历史回款] [登记历史发票]             │ [打开销售单] [打开客户往来]            │
├─────────────────────────────────────────┴────────────────────────────────────┤
│ 结论区：证据/备注 · [无历史票款，从0起] [复核通过] [驳回] [暂挂]                │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 4.2 区域说明

| 区域 | 目的 | 主组件 | 固定规则 |
| --- | --- | --- | --- |
| 连续处理条 | 位置、筛选、租约和前后项导航 | `SequentialProcessBar` | 顶部 sticky；对象切换后播报位置 |
| 对象身份 | 确认正在复核哪张正式销售单和哪个子账 | `DocumentHeader density="compact"`（与 M4 对象头密度一致） | 销售单号、版本、复核类型不可随滚动消失 |
| 票款事实 | 核对当前正式应收、回款和发票 | `MetricStrip` + 紧凑明细 | 金额全由服务端返回；复核前后不本地改写 |
| 差异区 | 解释为何产生差额复核 | `BusinessDiffPanel` | 仅 `SYNC_DELTA` 展示；左右均为受控事实投影 |
| 登记区 | 复用 W11 表单和多对多核销 | `AllocationWorkspace` | 保持队列条和对象身份可见；不得变成另一个无上下文菜单 |
| 证据与历史 | 支撑当前结论并追溯复核链 | 附件、`AuditTimeline` | 当前证据与历史证据分开，不覆盖旧记录 |
| 决策区 | 完成、驳回或暂挂 | `FormalActionConfirmDialog` / `FormalActionResult` | 底部 sticky；先固定显示结果再切下一项 |

### 4.3 两类任务

| 任务类型 | 复核对象 | 页面重点 | 禁止 |
| --- | --- | --- | --- |
| `CARD_FUNDS_REVIEW` / `OPENING` | 期初卡券应收，已收和已开票初始为 0 | 查明上线前真实回款和开票；登记事实或确认从 0 起 | 估算、批量结清、直接改汇总 |
| `CARD_FUNDS_DELTA_REVIEW` / `SYNC_DELTA` | 后续销售版本、应收分录或净分配变化后的当前事实 | 比较上一有效复核与当前事实；解释变化并形成新链尾 | 复制旧 `subject_hash` 或沿用旧通过结论 |

## 5. 展示内容与字段

### 5.1 对象与金额

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 身份 | `salesOrderNo` | 卡券销售单 | `sales_order` | ERP 与商城共用的正式单号 | 有销售单查看权 |
| 身份 | `sourceRevisionNo` / `sourceSnapshotAt` | 当前同步版本 / 快照时间 | 当前销售版本及商城快照 | 显示版本和业务时间，不展示原始报文秘密 | 按同步字段权限 |
| 身份 | `customerName` | 经营归属客户 | `receivable_account.customer_id` 投影 | 不作为核销相等键 | 按客户字段权限 |
| 身份 | `counterpartyPartyName` | 收款/开票往来主体 | `counterparty_party_id` | W11 核销主体 | 财务可见 |
| 身份 | `reviewType` / `reviewNo` | 期初复核 / 同步差额复核 | 当前任务 + 复核链 | `reviewNo` 仅正式形成后显示 | 财务可见 |
| 金额 | `syncedGrossAmount` | 同步成交额 | 当前卡券销售版本 | 含税，服务端已确认 | 金额权限 |
| 金额 | `receivableGrossTotal` | 当前应收 | `receivable_account.gross_total` | 含税，同步汇总 | 金额权限 |
| 金额 | `settledTotal` / `openTotal` | 净已收 / 开放应收 | 有效回款分配汇总 | `APPLY - REVERSE`，前端不重算 | 金额权限 |
| 金额 | `invoicedTotal` / `openInvoiceableTotal` | 净已开票 / 剩余可开票 | 有效销项票分配汇总 | 与回款轨道独立 | 金额权限 |

“同步成交额”和“当前应收”可能因追加差额分录而需要解释，但页面不得自行假设两者始终相等；服务端返回差异原因和来源分录。

### 5.2 正式票款与证据

| 内容 | 必须显示 | 数据来源 / 规则 |
| --- | --- | --- |
| 回款摘要 | 回款单号、到账时间、含税金额、净分配到当前应收、其它同主体分配摘要、冲正状态 | W11 `customer_receipt` + `receipt_allocation`；只读当前正式事实 |
| 发票摘要 | 发票号码、蓝/红、日期、含税/不含税/税额、净分配到当前子账、红冲状态 | W11 `invoice` + `sales_invoice_allocation` |
| 当前证据 | 正式文档、受控证据引用、备注、提供人和时间 | 完成复核时证据不能为空；敏感值受控揭示 |
| 复核历史 | 复核号、类型、结果、复核人/时间、当时指纹、前驱/被替代关系 | `receivable_funds_review` 追加式链；历史不可编辑删除 |
| 任务信息 | 原因、业务影响、优先级、到期、领取人和租约 | `work_item`；内部代码映射为业务文案 |

### 5.3 差额复核对比

`SYNC_DELTA` 的 `BusinessDiffPanel` 由服务端返回结构化对比，至少区分：

- 销售版本或同步成交额变化；
- 新增/冲减应收分录；
- 回款及其有效分配变化；
- 发票及其有效分配变化；
- 上一有效复核指纹失效时间与触发事实。

前端只渲染字段、旧值、新值、来源对象和发生时间；不得用当前余额反推或重建历史快照。

## 6. 搜索、筛选、排序与默认视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 任务类型 | 全部有效任务 | `type=all|opening|delta` | 切换后重新建立队列上下文 |
| 责任范围 | 我的任务 | `scope=mine|role_pool` | 角色池任务需先原子领取 |
| 客户/单号搜索 | 空 | `q` | 服务端搜索销售单号、客户和往来主体 |
| 时限 | 全部有效 | `due=all|today|overdue` | 到期判断使用已确认业务时区 |
| 复核状态 | 待处理 | `status` | 正常队列不混入已完成历史；历史另行查询 |
| 排序 | 服务端队列顺序 | `sort` | 优先级、到期和业务排序规则由服务端固定返回 |
| 自动下一项 | 开启 | `autoNext` + 本地偏好 | 只控制正式完成/驳回得到终态后的导航；暂挂、转交和结果未知不自动移动 |

- 队列总数和当前位置由服务端 `queueContextId` 快照/稳定游标返回，不能用当前页数组下标冒充。
- 按客户或账龄组织的具体默认优先级见待确认事项 Q1；未确认前使用统一待办的优先级和到期规则。
- 暂挂后当前项仍为 `PENDING/IN_PROGRESS`；当前位置、手动下一项和恢复入口必须明确展示，不触发完成成功语义。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 领取任务 | 队列条 | 任务待领取、当前用户有责任角色 | 无 | 复用 W02 `ClaimWorkItemCommand`，获得 `claimToken`、`leaseVersion`、任务指纹并显示到期时间 | 被他人领取时转只读并可跳下一项 |
| 登记历史回款 | 票款事实区 | 有 W11 回款登记权；往来主体明确 | 按 W11 正式提交确认 | 形成回款及分配；返回后由服务端重查金额、指纹和当前有效任务 | 保留 W11 草稿；结果未知不完成复核 |
| 登记历史发票 | 票款事实区 | 有 W11 发票登记权；号码和主体校验通过 | 按 W11 正式提交确认 | 形成发票及分配；返回后由服务端重查指纹和当前有效任务 | 重复票号定位已有事实；不创建副本 |
| 无历史票款，确认从 0 起 | `OPENING` 决策区 | 当前净已收/净已开均为 0；用户已核对；服务端允许；证据完整 | 强确认销售单、应收与“未发现历史票款” | 用共享完成信封提交；不创建虚假回款/发票，同事务形成通过复核链尾、`workflow_action` 并完成任务 | 指纹/领域版本变化时阻断；保留证据，刷新事实后重审 |
| 复核通过 | 决策区 | 领取有效；证据完整；当前指纹、账户版本、票款事实版本与任务一致；登记事实已正式过账 | `FormalActionConfirmDialog` | 用共享完成信封追加新复核链尾、`workflow_action`、更新可重建缓存并完成任务，固定展示复核号后可自动下一项 | 失败保留当前项；结果未知查询原幂等操作，不跳下一项 |
| 驳回复核 | 决策区 | 固定 `completion_action` 允许；领取有效；原因和证据必填 | 确认驳回影响 | 用共享完成信封追加本次 `REJECTED` 复核链尾和 `workflow_action` 并完成当前任务；不创建、不转交驳回后继任务。结果区显示“后继流程未配置” blocker 和人工协作说明 | 结果未知停留当前项；不得猜测任务类型、责任池或 handler 并补建驳回后继任务 |
| 暂挂 | 决策区 | 当前任务允许；原因按规则填写 | 轻确认 | 如需持久化，使用 W02 `WorkItemActionEnvelope<CardFundsReviewHoldAction>`；返回 `PENDING/IN_PROGRESS` 及动作记录/新租约，不形成复核事实、不自动下一项 | 失败停留；显示任务仍归属谁；可手动浏览下一项 |
| 转交 | 任务更多菜单 | 有转交权限；未作出复核结论；目标责任范围有效 | 确认原租约失效和责任变化 | 复用 W02 `TransferWorkItemEnvelope<CardFundsReviewTransfer>`，原任务转交与 `UNCLAIMED/PENDING` 后继任务创建原子完成；不形成 `receivable_funds_review` | 结果未知不改变当前归属；查询原幂等操作 |
| 打开销售单/客户往来 | 右栏 | 有目标模块权限 | 无 | 聚焦目标页签，W13 保留 | 权限不足时留在 W13 并说明 |
| 查询最终结果 | 结果不确定区 | 已有操作号和原幂等键 | 无 | 得到正式终态后更新结果/移动下一项 | 仍未知时保留联系支持和追踪号 |

任何正式复核都不能只更新 `work_item`，也不能先写复核事实再单独“标记完成”。同一事务必须追加 `receivable_funds_review`、`workflow_action`，更新可重建查询缓存，并完成当前任务。Q5 未确定固定后继任务类型、责任池和 handler 前，`REJECTED` 只终结本次复核；跟进需求只显示配置 blocker 与协作说明。纯转交不代表已复核，按 W02 转交信封处理且不得写复核事实。

## 8. 数据契约

### 8.1 队列查询

```ts
type CardFundsReviewQueueQuery = {
  queueContextId?: string
  currentWorkItemId?: string
  type: "all" | "opening" | "delta"
  scope: "mine" | "role_pool"
  q?: string
  due?: "all" | "today" | "overdue"
  status?: "pending" | "completed"
  sort?: string
}

type CardFundsReviewItemView = {
  workItem: {
    workItemId: string
    workItemType: "CARD_FUNDS_REVIEW" | "CARD_FUNDS_DELTA_REVIEW"
    completionAction: string
    subjectVersion: string
    subjectHash: string
    workItemStatus: WorkItemStatus
    dueAt?: string
    claimedBy?: UserSummary
    leaseVersion?: number
    leaseExpiresAt?: string
    allowedActions: Array<
      "CLAIM" | "CONFIRM_ZERO" | "APPROVE" | "REJECT" | "HOLD" | "TRANSFER"
    >
    actionBlockers: Array<{ action: string; code: string; message: string }>
  }
  salesOrder: { id: string; orderNo: string; revisionNo: number; snapshotAt: string }
  account: {
    id: string
    accountSeq: number
    domainVersion: string
    customerId: string
    counterpartyPartyId: string
    reviewStatus: string
    grossTotal: string
    settledTotal: string
    openTotal: string
    invoicedTotal: string
    openInvoiceableTotal: string
  }
  reviewChain: {
    tailReviewId?: string
    chainVersion: string
    nextReviewNo: number
    items: Array<ReviewHistoryItem>
  }
  currentSalesOrderRevisionId: string
  fundsFactVersion: string
  receiptFacts: Array<ReceiptSummary>
  invoiceFacts: Array<InvoiceSummary>
  difference?: ReviewDifference
}
```

金额使用定点十进制字符串或等价安全类型，不使用 JavaScript 浮点数。`workItemStatus` 直接复用 W02 `WorkItemStatus`，不接受本地字符串或同义枚举；`domainVersion`、`chainVersion`、`fundsFactVersion` 和任务 `subjectVersion` 都是服务端不透明并发令牌，前端不得递增或互相替代。Query Key 包含用户、角色、权限/范围版本、队列上下文、任务 ID 和 `subjectVersion`。当前项、邻接项和队列计数由服务端返回；`claimToken` 仅由领取结果返回，不属于查询投影。

### 8.2 正式复核提交

```ts
type CardFundsReviewDecisionBase = {
  receivableAccountId: string
  expectedAccountSeq: number
  expectedAccountDomainVersion: string
  expectedReviewChainTailId?: string
  expectedReviewChainVersion: string
  expectedNextReviewNo: number
  expectedSalesOrderRevisionId: string
  expectedFundsFactVersion: string
  reviewType: "OPENING" | "SYNC_DELTA"
  evidenceDocumentIds: string[]
  evidenceReferences: string[]
  comment?: string
}

type CardFundsReviewDecision = CardFundsReviewDecisionBase &
  (
    | {
        reviewResult: "APPROVED"
        conclusion: "NO_HISTORY_FROM_ZERO" | "RECORDED_FACTS_RECONCILED"
      }
    | {
        reviewResult: "REJECTED"
        conclusion: "REJECTED"
        reasonCode: string
      }
  )

type CompleteCardFundsReviewCommand =
  CompleteWorkItemEnvelope<CardFundsReviewDecision>

type CardFundsReviewBusinessResultBase = {
  receivableFundsReviewId: string
  receivableAccountId: string
  reviewNo: number
  accountReviewStatus: string
  workflowActionId: string
  operationId: string
  completedAt: string
}

type CardFundsReviewBusinessResult = CardFundsReviewBusinessResultBase &
  (
    | {
        reviewResult: "APPROVED"
        conclusion: "NO_HISTORY_FROM_ZERO" | "RECORDED_FACTS_RECONCILED"
      }
    | {
        reviewResult: "REJECTED"
        conclusion: "REJECTED"
        followUpConfiguration: {
          status: "BLOCKED"
          blockerCode: "REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED"
          collaborationMessage: string
          requiredRegistration: Array<"WORK_ITEM_TYPE" | "OWNER_POOL" | "HANDLER_KEY">
        }
      }
  )

type CompleteCardFundsReviewResult =
  CompleteWorkItemResult<CardFundsReviewBusinessResult>

type CardFundsReviewHoldAction = {
  kind: "HOLD"
  reasonCode: string
  note?: string
}

type HoldCardFundsReviewCommand =
  WorkItemActionEnvelope<CardFundsReviewHoldAction>

type HoldCardFundsReviewResult =
  WorkItemActionResult<{ heldAt: string; resumeHint?: string }>

type CardFundsReviewTransfer = {
  targetOwnerRole: string
  targetOwnerUserId?: string
  reasonCode: string
  note?: string
}

type TransferCardFundsReviewCommand =
  TransferWorkItemEnvelope<CardFundsReviewTransfer>
```

正式结论固定使用 `CompleteCardFundsReviewCommand`。共享外层只放 `workItemId`、`claimToken`、`leaseVersion`、任务期望版本/指纹和本次完成操作的 `idempotencyKey`；应收账户、当前复核链尾、复核类型与结论、证据及全部领域版本都放在 `decision`。W13 不再定义领取令牌别名，也不把对象字段提升成另一套命令外层。

服务端在同一个正式事务中：

1. 锁定并校验固定任务类型、`completion_action`、领取人、`claimToken` 摘要、`leaseVersion`、任务状态和任务版本/指纹；
2. 锁定应收账户及当前复核链尾，核对 `decision` 中的账户身份、账户/链版本和预计下一复核号；
3. 重新取得当前销售版本、应收分录、净回款分配和净发票分配，核对全部领域版本；
4. 规范化计算当前 `subject_hash`，并与任务、共享信封和当前事实三方校验；
5. 校验证据非空、岗位分离、结论组合和复核号连续；`NO_HISTORY_FROM_ZERO` 只允许 `OPENING + APPROVED` 且净已收/净已开均为 0；
6. 以 `review_no + 1` 追加 `receivable_funds_review`，并在适用时唯一引用 `supersedes_review_id`；
7. 追加同一决定的 `workflow_action`，更新 `receivable_account.review_status` 等可重建同步缓存，并完成当前正式任务；`REJECTED` 分支不得创建或转交后继任务；
8. 返回共享 `CompleteWorkItemResult`，其中业务结果固定包含复核事实、`workflowActionId`、操作号和完成时间；`REJECTED` 还返回固定配置 blocker 与人工协作说明。

复核事实、`workflow_action`、查询缓存和当前任务完成不得分成多次提交。Q5 未决期间，服务端不得从自由文本、当前角色或前端参数推导后继任务；只有先注册固定 `work_item_type`、责任池与 `handlerKey` 并更新本契约后，未来的驳回事务才可按该正式规则生成新任务。重复点击和超时重试使用同一幂等键。若网络中断，UI 停留当前项并查询操作号；没有正式终态前不得本地标为已复核、不得自动下一项。

暂挂如需写入任务动作，固定使用 `HoldCardFundsReviewCommand`。成功只返回 `WorkItemActionResult`，任务仍为 `PENDING/IN_PROGRESS`，可以返回续租后的新租约，但不写 `receivable_funds_review`、不产生正式复核 `workflow_action`、不自动下一项。纯转交使用 W02 `TransferWorkItemEnvelope`：原任务置为 `TRANSFERRED`、原租约失效、后继任务置为 `UNCLAIMED/PENDING`；它不代表票款已经复核，也不得生成复核链记录。

在复核完成前登记回款、分配或发票会改变当前规范化指纹。W13 返回后必须向服务端查询“当前有效复核任务”：若原任务仍有效，则采用服务端返回的新上下文；若系统已用正式后继任务替代原任务，则按 `replacementWorkItemId` 定位并重新领取。前端不得直接改写原任务 `subject_hash`，也不得继续拿旧租约完成新指纹的复核。

### 8.3 前端边界

- 前端只格式化服务端金额和差异，不计算正式应收、净已收、净已开票或复核有效性。
- “无历史票款，从 0 起”只形成复核结论，不创建 0 元回款单、0 元发票或手工汇总字段。
- 历史回款/发票登记完全复用 W11 数据与提交契约；W13 不建第二套票款表单模型。
- 旧复核不可更新、删除或复制；当前有效复核由服务端按链尾和当前指纹派生。
- 新的同步差额、回款/分配或发票/分配变化使旧指纹失效时，页面只展示新任务和差异，不自动继承旧结论。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 队列条、对象身份、金额、证据区等高 Skeleton | 应用壳导航可用 | 原位替换并聚焦对象标题 |
| 切换下一项 | 保留队列条，主区显示轻量 Skeleton | 上一项结果仍可读 | 新项加载后播报位置 |
| 队列已完成 | “当前筛选项已处理完”及处理数量 | 返回工作台、切换筛选 | 新任务到达或手动刷新 |
| 筛选无结果 | 显示筛选摘要 | 清除筛选 | 返回有效队列 |
| 无数据范围 | 不显示“已处理完” | 查看角色/申请范围 | 范围更新后重查 |
| 已被他人领取 | 对象可按权限只读，动作禁用并显示租约 | 跳下一项、刷新租约 | 对方释放/到期后重新领取 |
| 租约即将到期/丢失 | 明确倒计时或失效提示；保留安全输入 | 续租；丢失后禁止提交 | 重新领取并重校验指纹 |
| 查询失败且无缓存 | `BusinessFailureState`，保留队列身份但不显示金额结论 | 重试、返回 W02 | 查询恢复 |
| 查询失败有缓存 | 保留旧对象并标陈旧 | 只读；正式提交禁用 | 重试成功 |
| 票款登记保存失败 | W11 区域保留输入和分配 | 修正/重试 | 正式结果确认后刷新当前项 |
| 指纹变化/版本冲突 | 展示“复核对象已变化”和结构化 diff | 刷新当前任务 | 重新核对最新事实 |
| 正式动作失败 | 固定错误区说明当前项仍在队列 | 修正、同幂等操作重试 | 成功或暂挂 |
| 暂挂成功 | 显示动作记录和任务仍为 `PENDING/IN_PROGRESS`，不使用完成态视觉 | 恢复处理、手动浏览下一项 | 以返回的新租约或重新领取恢复 |
| 转交成功 | 显示原任务 `TRANSFERRED`、后继任务 `UNCLAIMED/PENDING` 和转交记录 | 返回队列、查看后继任务 | 不显示复核号或复核结论 |
| 正式动作成功 | `FormalActionResult` 固定展示 `CompleteWorkItemResult` 中的复核号、结论、`workflowActionId`、证据时间和下一项 | 打开销售单/往来、下一项 | 当前任务由票款复核事务完成 |
| 驳回完成且后继未配置 | 固定展示本次 `REJECTED` 复核号、当前任务已完成，以及 `REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED` blocker 和人工协作说明 | 复制协作摘要、查看 Q5 配置责任方、继续下一项 | 不创建/转交驳回后继；固定类型、责任池和 handler 注册并发布后才允许未来驳回生成新任务 |
| 正式结果不确定 | 固定结果区显示原幂等操作号；任务、链尾和账户复核状态都不乐观改变 | 查询最终结果、联系支持 | 得到可验证 `CompleteWorkItemResult` 后才移动 |
| 字段级隐藏/权限收回 | 敏感值掩码或清除；必要字段缺失时动作禁用 | 返回有权页面 | 权限恢复后重查 |

## 10. 响应式与键盘

| 视口 | 布局变化 | 保留内容 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 队列条 sticky，主区 64/36，两栏同屏 | 位置、销售单、四类金额、证据、决策 | 无 |
| 1280×800 | 右栏收窄，历史默认折叠 | 对象身份、指纹状态、证据和主动作 | 次要同步元数据折叠 |
| 1024×768 | 主区约 60/40；登记区可覆盖展开 | 队列条、金额、当前证据、决策 sticky | 历史进入折叠面板 |
| 768×1024 | 单列：事实 → 差异 → 证据 → 决策；队列条简化 | 当前/总数、对象、金额、证据、结果 | 明细表改卡片，复杂登记建议桌面 |
| 375×812 | 只读摘要与服务端允许的简单“从 0 起”确认 | 任务身份、净已收/已开、证据摘要、最终结果 | 不提供复杂回款/发票多对多核销；转桌面并保留任务 |

键盘顺序：队列导航 → 对象身份 → 票款事实 → 登记入口 → 证据 → 决策。方向键或 j/k 按统一约定切换项但不在表单输入中触发；`⌘↵` 仅打开正式确认；Esc 不关闭队列页签。切换项后焦点落新对象标题并播报“第 N/M 项”；Dialog 关闭返回原动作；结果不确定时焦点落固定结果区。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 今日工作台 / 统一待办 | W01 / W02 | `workItemId`、`queueContextId`、筛选摘要 | 队列完成后返回原工作台/待办 |
| 销售单 | W05 | 销售单稳定 ID、当前任务 ID | 返回 W13 保留当前项并重查版本 |
| 客户往来 | W11 | 应收账户、往来主体、回款/发票模式、来源队列 | 登记后回当前项并重算 `subject_hash` |
| 客户经营质量 | W15 | 客户和复核状态筛选 | 只读下钻；完成后分析投影按水位刷新 |
| 商城同步与映射 | W17 | 销售单、同步版本或差额任务 | 映射/同步修复后由服务端创建或替换正式复核任务 |
| 导入与期初 | W18 | 期初批次、销售单来源身份 | 导入只创建正式应收和任务，不传“已收/已开”结论 |

跨工作面只传稳定身份与队列上下文。销售版本、票款金额、指纹、权限和复核有效性必须在目标页重新查询。

W13 复用 W02 的领取、`WorkItemActionEnvelope`、`CompleteWorkItemEnvelope` 和 `TransferWorkItemEnvelope`；只在 `CardFundsReviewDecision` 中定义本领域账户、链尾、结论、证据和并发版本，不复制任务信封或任务状态机。

## 12. 验收清单

- [x] `OPENING` 与 `SYNC_DELTA` 明确区分，后续任务不会复用或覆盖期初复核。
- [x] 一屏看清同步成交额、当前应收、净已收、净已开票、证据和当前指纹状态。
- [ ] 历史回款/发票通过 W11 正式事实及多对多分配登记，不存在累计金额覆盖字段。
- [x] “从 0 起”不会创建 0 元回款/发票，且必须有明确证据和强确认。
- [x] 完成时重新计算并三方校验 `subject_hash`；变化时阻断而非静默通过。
- [x] 复核链递增、单根不分叉，旧记录不可编辑删除，当前缓存可从链重建。
- [x] 所有正式结论使用 `CompleteWorkItemEnvelope<CardFundsReviewDecision>`；账户、链尾、结论和领域版本全部位于 `decision`，领取令牌只使用 `claimToken`。
- [ ] `receivable_funds_review`、`workflow_action`、查询缓存和当前任务完成在同一事务形成，不存在独立“标记完成”。
- [x] Q5 未决时，`REJECTED` 只形成驳回复核事实并完成当前任务；结果固定显示配置 blocker/协作说明，前后端均不能猜测或创建驳回后继任务。
- [x] 处理成功先展示固定复核号/结果再自动下一项；结果不确定时不移动。
- [ ] 领取、续租、他人占用、暂挂、转交、驳回和从 W05/W11 返回均不丢队列上下文；暂挂后任务仍为 `PENDING/IN_PROGRESS`。
- [x] 复核未完成时 W11/W15 能识别指标不可靠，不以 0 值冒充已核实。
- [ ] 五档视口、键盘、焦点恢复和读屏队列位置播报通过验收。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | W13 默认按客户聚合、应收账龄还是统一待办优先级排序？ | 连续处理效率和逾期风险 | 财务负责人 | 未确认前沿用统一待办优先级 + 到期顺序，不在前端重排 |
| Q2 | 登记历史回款/发票的财务经办人与最终复核人是否必须分离？ | `allowedActions`、任务转交和人员配置 | 财务负责人 + 内控负责人 | 由服务端岗位分离策略明确，UI 不自行放宽 |
| Q3 | “无历史票款，从 0 起”接受哪些证据类型，是否允许受控备注替代文档？ | 提交校验和附件组件 | 财务负责人 + 审计负责人 | 证据不得为空；具体白名单由后端规则返回 |
| Q4 | 375px 移动端是否允许正式“从 0 起”确认？ | 移动端权限和风险控制 | 财务 + 安全负责人 | 默认仅只读；只有服务端明确允许时开放简单确认 |
| Q5 | 驳回复核完成当前任务后，是否需要创建哪一类固定后继任务、进入哪个责任池？ | 固定 `work_item_type`、责任池、`handlerKey`、队列位置和结果文案 | 财务负责人 | 未确认并注册固定类型、责任池与 handler 前禁止因驳回生成/转交后继任务；当前只返回 `REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED` blocker 和人工协作说明。确认后先更新任务注册表与本契约，再允许未来驳回事务创建新任务 |

## 14. 业务依据

- `erp-phase-1.md` §5.3、§8.7、§9.1、§9.4、§11：期初票款置 0 后逐单复核、统一票款内核和分析可靠性提示。
- `erp-data-model.md` §6.1 `work_item`、§6.8 `receivable_funds_review`：固定任务类型、租约、指纹、复核链和证据不变量。
- `erp-data-model.md` §7.5、§8.3、§9.3、§12：资金状态、票款事务、独立事实和同步/异步投影边界。
- `erp-ui-design.md` §4.4 M3、§4.6.2 M5、§5.5、§11：连续处理、同屏核销、正式结果和结果不确定。
- `erp-ui-flows.md` §3、§5、§6：卡券同步到应收、W11 同一核销会话和 W13 单屏连续复核。
