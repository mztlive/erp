# W06 · 客户验收

> 状态：草稿
> 页面模式：挂载于 W05 对象中心 + M5 简化作业
> 主要路由：`/sales/orders/:salesOrderId?section=acceptance`
> 主要角色：销售经办；销售经理、采购、仓储、财务按权限只读
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

- 销售在当前销售单内核对已发货、已电子交付或已完成服务的事实，并登记客户实际验收结果。
- 一次验收可以覆盖同一销售明细的多个履约批次；同一履约批次也可以分多次验收。
- 发生短少、拒收或服务不通过时，原位看清影响并进入后续异常处理，不需要再去无上下文列表寻找原单。
- 登记完成后能明确知道本次验收单号、各明细剩余待验收量及销售单履约进度是否变化。

### 1.2 业务目标

- 以正式 `customer_acceptance`、`customer_acceptance_line` 和 `acceptance_fulfillment_allocation` 记录验收事实。
- 非卡券销售明细只在累计净有效验收数量达到当前有效履约数量时判定履约完成。
- 保留履约事实、验收事实和反向事实之间的完整来源关系，不以覆盖、删除或直接改状态代替纠错。
- W06 作为 W05 的履约作业模式，不创建与销售单割裂的独立验收业务入口。

### 1.3 不在本工作面完成

- 不处理卡券销售单的发卡、绑定、激活或消费验收；卡券履约完成只按销售单履约期限判断。
- 不修改 W09 已过账的入库、仓发、代发、电子交付或服务履约事实。
- 不因短少、拒收或服务不通过直接扣库存、改应收、改采购单或回退已发生履约。
- 不在验收表单中完成销售退货、采购退货、退款、红票或库存调整；这些动作从结果区进入原单的“变更与异常”流程。
- 不提供独立顶层验收列表作为唯一入口。当前普通验收待办类型与粒度尚未登记，跨单查找先使用 W05 销售单筛选；只有完成 Q2 并在统一模型注册后，才允许从 W01/W02 汇聚到本工作面。

## 2. 用户、权限与数据范围

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 负责销售 / 协作销售 | 当前从 W05 履约子区进入；注册正式验收任务后可从 W01/W02 进入 | 自己负责或协作客户的销售单及授权履约事实 | 新建验收草稿、保存、过账、发起异常处理 |
| 销售经理 | W05 履约子区 | 本团队销售单 | 查看；有经办授权时可登记，不因经理身份自动获得写权限 |
| 采购 | W09 或 W05 关联履约 | 与本人采购履约职责相关的销售单 | 查看验收结果与后续补履约要求 |
| 仓储 | W09 或 W05 关联履约 | 与本人仓储作业相关的销售单 | 查看仓发验收结果；不得替销售登记客户结论 |
| 财务 | W05 票款/履约子区 | 财务数据范围内的销售单 | 查看验收进度；不得用验收结果直接改应收或发票 |
| 管理层 | 分析下钻 | 授权汇总及对象 | 只读 |

### 2.1 权限表达

| 情况 | 界面行为 |
| --- | --- |
| 无 W05 销售单模块权限 | 不展示入口；直接访问路由显示无权限页 |
| 有 W05 权限但无该销售单数据范围 | 显示无数据范围，不返回客户、地址、附件或履约明细 |
| 可看销售单但无验收写权限 | 完整显示有权事实；“登记验收”可见但禁用，展示 `actionBlocker` |
| 销售单不是实物与服务业务 | 不渲染验收编辑器；卡券单显示其履约期限口径说明 |
| 当前没有可验收履约量 | 显示“当前没有待验收的履约事实”，保留历史与去 W09 的入口 |
| 敏感交付信息无字段权限 | 保留字段标签，使用 `SensitiveValue` 掩码；附件按独立权限控制 |
| 操作期间权限被收回 | 立即停止自动保存和正式提交，清除敏感缓存；草稿留在服务端待有权用户继续 |

服务端必须按销售单参与者、当前客户负责人/协作销售及部门数据范围过滤。前端不得先查询全量履约事实再隐藏无权行。

### 2.2 编辑占用与任务租约

- 当前没有权威注册的普通客户验收 `work_item_type`，Q2 的任务粒度也未确认；因此 W01/W02 验收入口、`workItemId` 路由和 `WORK_ITEM` 正式提交默认 fail-closed，只允许从 W05 直接对象过账。
- 不得用 `BUSINESS_EXCEPTION` 或页面私有码伪装普通验收任务。只有固定类型与粒度写入 `erp-data-model.md` 注册表且 API 契约同步后，才能启用 W01/W02。
- 启用后，页面携带 `workItemId`，按 W02 的 `work_item` 协议原子领取、续租和提交校验；正式提交必须完整使用 `CompleteWorkItemEnvelope`，不得在验收过账后再单独调用“完成待办”。
- 从 W05 直接进入且没有待办时，验收草稿使用对象编辑租约或服务端草稿责任人约束；不能用 TaskTabs 的“当前打开”冒充业务锁。
- 租约只保护当前处理权，不改变履约或验收业务状态。
- 租约丢失时保留本地未保存输入供复制，禁止继续正式过账；重新领取成功后必须重取事实和版本再校验。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 销售单内登记 | W05“履约”子区的“登记验收” | 同一销售单页签切到 `/sales/orders/:salesOrderId?section=acceptance` | 取消后回 W05 履约阅读态 |
| 待办处理（目标契约，当前禁用） | W01/W02 的已注册客户验收任务 | 注册表与 API 生效后，同一路由追加 `workItemId`、可选 `salesLineId`；此前直接访问显示配置 blocker，不降级成直接过账 | 完成后回来源队列或定位下一待办 |
| 履约完成后进入 | W09 固定结果面板 | 聚焦已存在的 W05 销售单页签并打开验收子区 | 返回时 W09 队列上下文保留 |
| 查看历史验收 | W05 履约时间线 | URL 追加 `acceptanceId`，打开当前页签内详情 | 关闭详情回原滚动位置 |
| 纠正误录 | 历史验收详情“冲正误录” | 同页打开反向事实确认层 | 成功后仍停留履约时间线 |
| 刷新 | 任意编辑状态 | 恢复销售单、子区、草稿 ID 和来源任务；不恢复临时 Dialog | 当前 W05 页签 |

TaskTabs 身份始终是 `sales-order:{salesOrderId}`，不是验收草稿 ID。重复打开同一销售单只聚焦原页签；验收编辑中显示脏状态，关闭销售单页签前必须确认。

URL 只传稳定身份和显示状态，不传客户名称、验收数量、权限结论或履约状态作为事实。

## 4. 页面布局

### 4.1 1440×900 基准布局

```text
┌ DocumentHeader：销售单号 · 客户 · 主状态 · 履约/回款/开票进度 ───────────┐
├ 概览 | 明细 | 履约与验收（选中）| 票款 | 变更与异常 | 审计 ─────────────┤
│ 数据水位 · 待验收 3 批 / 128 件                    [退出登记] [保存草稿] │
├────────────────────────────────────────┬───────────────────────────────┤
│ 可验收履约事实 约 62%                   │ 本次验收 约 38%（sticky）      │
│ 筛选：全部明细 / 仅待验收               │ 验收日期 · 总体结果            │
│                                         │                               │
│ 明细 A · 已履约 100 · 已验收 60         │ 明细 A                         │
│  □ 仓发 FH… 可验收 20                   │ 通过 20 / 短少 0 / 拒收 0      │
│  □ 代发 DF… 可验收 20                   │ 来源分配：FH… 20               │
│                                         │                               │
│ 明细 B · 服务已完成 1 · 待验收 1        │ 明细 B                         │
│  □ 服务履约 FW…                         │ 通过 / 服务不通过 · 原因 · 凭证│
├────────────────────────────────────────┴───────────────────────────────┤
│ ValidationSummary             [取消] [保存草稿] [确认并过账验收]       │
└────────────────────────────────────────────────────────────────────────┘
```

未进入编辑模式时，W05 履约子区展示“履约事实 → 验收事实 → 异常/补履约”的只读时间线，以及“登记验收”主入口。

### 4.2 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 销售单头 | 保留客户承诺、主状态和多轨进度上下文 | `DocumentHeader` `StatusTrackSummary` | 顶部 sticky |
| 子区导航 | 在同一对象中心切换履约、票款和异常 | W05 锚点导航 | 顶部 sticky |
| 待验收摘要 | 展示服务端汇总与数据水位 | `MetricStrip` `DataFreshness` | 否 |
| 履约事实池 | 选择可被本次验收分配的有效履约事实 | `BusinessTableFrame` + 分组行 | 桌面左栏独立滚动 |
| 本次验收 | 编辑验收头、结果数量、原因、附件和分配 | M5 简化作业表单 | 桌面右栏 sticky / 独立滚动 |
| 校验与动作栏 | 汇总守恒、版本冲突和正式过账 | `ValidationSummary` `FormalActionConfirmDialog` | 底部 sticky |
| 历史时间线 | 查看验收、反向事实及剩余量变化 | `AuditTimeline` `RelatedDocumentList` | 阅读态展示 |

### 4.3 分组与选择规则

- 履约事实先按销售明细分组，再按实际发生时间升序排列；同一明细的最早未验收事实默认在前。
- 默认只显示“净成功履约量 > 净已验收分配量”的事实；用户可切换查看全部历史。
- 选择一个履约事实后，只能分配到相同 `sales_order_line_id` 的验收行。
- UI 可提供“按最早批次自动分配”，但分配结果必须可见、可改，并由服务端重新校验；不得把按 SKU 猜测的结果直接过账。
- 本次未选择的履约事实不受影响。

## 5. 展示内容与字段

### 5.1 销售单与待验收摘要

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 单据头 | `salesOrderNo` | 销售单号 | `sales_order` / 当前版本投影 | 稳定业务号 | 对象可见者可见 |
| 单据头 | `customerName` | 客户 | 销售版本客户快照 | 不追随主数据静默变化 | 按字段权限掩码 |
| 单据头 | `commercialStatus` | 主状态 | `sales_order` | 使用固定状态文案 | 全部对象查看者 |
| 单据头 | `fulfillmentProgress` | 履约进度 | 同步维护的销售履约投影 | 未开始 / 部分履约 / 已完成 | 不由当前页面求值 |
| 摘要 | `eligibleFulfillmentCount` | 待验收批次 | 服务端验收工作区投影 | 当前有效、仍有净可验收量的批次数 | 与列表同权限快照 |
| 摘要 | `eligibleQuantity` | 待验收数量 | 服务端按有效履约与验收分配汇总 | 基础单位，不跨单位相加；多单位时分组显示 | 同上 |
| 摘要 | `nearestDueDate` | 最近履约期限 | 当前有效销售版本明细 | 业务日期；超期有文字提示 | 同上 |
| 摘要 | `factsUpdatedAt` | 数据更新时间 | 正式履约/验收查询水位 | 绝对时间 | 全部对象查看者 |

### 5.2 履约事实池

| 字段 | 用户文案 / 表现 | 数据来源 | 说明 |
| --- | --- | --- | --- |
| `salesLineNo` / `itemSnapshot` | 明细行号、商品/服务、规格 | 当前销售版本明细快照 | 不用当前 SKU 名称覆盖历史快照 |
| `fulfillmentFactType` | 仓发 / 代发 / 电子交付 / 服务履约 | `delivery`、`electronic_delivery`、`service_fulfillment` | 采购入库不是客户验收来源 |
| `fulfillmentNo` | 履约单号 | 对应正式履约事实 | 可打开 W09 只读详情 |
| `occurredAt` | 发货 / 交付 / 服务时间 | 对应履约事实 | 页面按业务时区格式化 |
| `netSuccessfulQuantity` | 有效履约数量 | 服务端扣除冲正后的正式事实 | 前端不自行解释冲正链 |
| `netAcceptedAllocatedQuantity` | 已验收数量 | `acceptance_fulfillment_allocation` 净 `APPLY - REVERSE` | 由服务端计算 |
| `eligibleQuantity` | 本次最多可验收 | 服务端守恒结果 | 不得为负；提交时重验 |
| `carrier` / `trackingNo` | 物流信息 | `delivery` | 电子/服务不显示空占位 |
| `result` / `evidence` | 履约结果与凭证 | 电子/服务履约事实、附件 | 附件按权限与安全扫描状态开放 |

### 5.3 本次验收表单

| 字段 | 用户文案 | 数据来源 / 提交去向 | 校验 |
| --- | --- | --- | --- |
| `acceptedAt` | 客户验收时间 | `customer_acceptance.accepted_at` | 不晚于允许的业务时间上限；时区明确 |
| `overallResult` | 总体验收结果 | `customer_acceptance.result` | 通过 / 短少 / 拒收 / 服务不通过；由明细结果约束 |
| `salesOrderLineId` | 销售明细 | `customer_acceptance_line.sales_order_line_id` | 必须属于当前销售单当前有效范围 |
| `acceptedQuantity` | 验收通过数量 | `customer_acceptance_line.accepted_quantity` | 非负，最多 6 位小数 |
| `shortQuantity` | 短少数量 | `customer_acceptance_line.short_quantity` | 非负；大于 0 时原因必填 |
| `rejectedQuantity` | 拒收 / 不通过数量 | `customer_acceptance_line.rejected_quantity` | 非负；大于 0 时原因必填 |
| `reason` | 客户反馈 / 原因 | `customer_acceptance_line.reason` | 短少、拒收、服务不通过时必填；禁止写技术日志 |
| `evidenceAttachmentId` | 客户签收或验收凭证 | `customer_acceptance_line.evidence_attachment_id` | 类型、大小与安全状态由附件服务校验 |
| `allocations` | 对应履约批次 | `acceptance_fulfillment_allocation` | 同销售明细；各结果量由有效来源分配覆盖且守恒 |
| `comment` | 内部备注 | 受控备注字段 | 不放联系人手机号等无关敏感信息 |

### 5.4 历史与结果

| 字段 | 用户文案 | 数据来源 | 规则 |
| --- | --- | --- | --- |
| `acceptanceNo` | 验收单号 | `customer_acceptance.acceptance_no` | 过账后形成且唯一 |
| `status` | 草稿 / 已过账 / 已冲正 | `customer_acceptance.status` | 不新增 UI 私有状态 |
| `recordedBy` / `recordedAt` | 登记人 / 记录时间 | 公共事实字段 | 与客户验收时间分开展示 |
| `reversalOfAcceptanceId` | 冲正原验收 | `customer_acceptance.reversal_of_acceptance_id` | 双向链接原事实与反向事实 |
| `remainingQuantity` | 剩余待验收 | 服务端正式投影 | 不是验收单自身字段 |
| `followUpActions` | 后续处理 | 服务端 `allowedActions` / `actionBlockers` | 例如补履约、创建退货/拒收处理单 |

## 6. 搜索、筛选、排序与默认视图

W06 没有跨销售单全文搜索；对象查找由 W05 列表或全局搜索完成。

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 子视图 | `pending` | `acceptanceView=pending|history` | 待验收事实 / 验收历史切换 |
| 销售明细 | 全部 | `salesLineId` | 从 W09 进入时定位明细；任务注册后也可从 W02 定位；无权 ID 不泄露存在性 |
| 履约类型 | 全部 | `fulfillmentType` | 只过滤当前销售单内的仓发、代发、电子、服务 |
| 是否仅剩余 | 是 | `remainingOnly=1` | 关闭后显示已完全验收历史事实 |
| 历史排序 | 最近过账优先 | `acceptanceSort` | 待验收事实仍按发生时间升序，避免遗漏早批次 |

筛选变化保留草稿选择；若隐藏了已选来源，表单顶部提示“当前有 N 个已选来源未显示”，不得静默删除分配。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 开始登记验收 | W05 履约子区 | `CREATE_ACCEPTANCE` 可用；存在有效可验收量 | 无 | 创建或恢复当前用户草稿，进入编辑态 | 创建失败停留阅读态并重试 |
| 选择/自动分配履约 | 履约事实池 | 来源与销售明细一致、仍有可验收量 | 无 | 仅更新草稿 | 版本变化时重取并标出冲突行 |
| 保存草稿 | 动作栏 / 自动保存 | 草稿编辑租约有效，`draftVersion` 匹配 | 无 | 返回新版本与保存时间 | 保留输入；冲突时不覆盖服务端草稿 |
| 确认并过账验收 | 底部主动作 | `POST_ACCEPTANCE` 可用；守恒、附件和版本校验通过 | `FormalActionConfirmDialog` 展示单据、各结果数量及异常影响 | 形成正式验收、分配和工作流审计；固定结果显示验收单号与剩余量 | 失败保留草稿；结果不确定停留并查询最终结果 |
| 创建后续异常处理 | 短少/拒收/服务不通过结果区 | 服务端返回对应 `allowedAction` | 展示原验收与影响范围 | 进入 W05“变更与异常”预填原销售单、验收单与影响行 | 创建失败不回退验收事实，可从结果区重试 |
| 冲正误录验收 | 历史详情 | `REVERSE_ACCEPTANCE` 可用；原验收已过账且未被完整冲正 | 强确认原事实、影响分配与理由 | 新增反向验收及 `REVERSE` 分配；原验收保留 | 不确定时查询反向事实；不得本地标记已冲正 |
| 打开履约详情 | 来源行 | 有 W09 与对象权限 | 无 | 聚焦 W09 只读详情或已有页签 | 无权限时留在 W06 并说明 |
| 退出编辑 | 页头 | 有未保存或已保存草稿时 | 脏草稿二次确认 | 返回 W05 履约阅读态；服务端草稿按选择保留或放弃 | 放弃失败时不得假装草稿已删除 |

短少、拒收和服务不通过的过账只记录客户验收结论。界面不得把它表述成“已退货”“已退款”“已扣库存”或“已减少应收”。

## 8. 数据契约

本节约定页面所需语义，不固定具体 HTTP 路径。

### 8.1 查询

```ts
type CustomerAcceptanceWorkspaceQuery = {
  salesOrderId: string
  salesLineId?: string
  acceptanceView: "pending" | "history"
  fulfillmentType?: "WAREHOUSE_SHIP" | "SUPPLIER_DIRECT" | "ELECTRONIC" | "SERVICE"
  remainingOnly: boolean
  acceptanceSort?: "occurred_asc" | "posted_desc"
  workItemId?: string
}
```

```ts
type CustomerAcceptanceWorkspaceView = {
  salesOrder: {
    id: string
    salesOrderNo: string
    businessType: "GOODS_SERVICE"
    customerLabel: string
    commercialStatus: string
    fulfillmentProgress: string
    collectionProgress: string
    invoiceProgress: string
    lockVersion: number
  }
  freshness: { factsUpdatedAt: string; state: "fresh" | "refreshing" | "failed" }
  metrics: {
    eligibleFulfillmentCount: number
    eligibleQuantityByUnit: Array<{ unitCode: string; quantity: string }>
    overdueLineCount: number
  }
  salesLines: Array<{
    salesOrderLineId: string
    lineNo: number
    itemSnapshot: string
    unitCode: string
    requiredQuantity: string
    netAcceptedQuantity: string
    fulfillmentFacts: AcceptanceEligibleFact[]
  }>
  draft?: AcceptanceDraft
  history: AcceptanceHistoryItem[]
  permissions: {
    allowedActions: string[]
    actionBlockers: Array<{ action: string; code: string; message: string }>
    fieldVisibility: Record<string, "full" | "masked" | "hidden">
  }
  workItem?: {
    workItemId: string
    workItemType: string
    subjectVersion?: string
    subjectHash: string
  }
  lease?: {
    claimedByLabel?: string
    claimedByCurrentUser: boolean
    expiresAt?: string
    leaseVersion?: number
    hasValidClaim: boolean
    renewAllowed: boolean
  }
}
```

Query Key 至少包含用户、当前角色、权限版本、数据范围版本、销售单稳定 ID、当前版本、子视图和筛选。当前配置不接受 `workItemId`；只有 Q2 对应注册表/API 生效后才返回 `workItem` / `lease` 安全投影。查询 View 永不返回原始 `claimToken`；令牌只由 W02 Claim / 续租 mutation 返回并仅存当前会话内存，不进入 URL、持久化 Query 缓存、日志或埋点。正式履约与验收事实事务内同步可见；不得用一分钟级分析投影驱动过账。

### 8.2 草稿保存

```ts
type SaveAcceptanceDraftCommand = {
  acceptanceDraftId: string
  expectedDraftVersion: number
  salesOrderId: string
  acceptedAt: string
  lines: Array<{
    salesOrderLineId: string
    acceptedQuantity: string
    shortQuantity: string
    rejectedQuantity: string
    reason?: string
    evidenceAttachmentId?: string
    allocations: Array<{
      fulfillmentFactType: string
      fulfillmentLineId: string
      allocatedQuantity: string
    }>
  }>
}
```

自动保存使用 TanStack Form 的服务端确认值与 `expectedDraftVersion` 条件更新。失败时保留用户输入并显示最近成功保存时间；不能用后到响应覆盖新输入。

### 8.3 正式过账与冲正

```ts
type PostCustomerAcceptanceDecision = {
  acceptanceDraftId: string
  expectedDraftVersion: number
  expectedSalesOrderLockVersion: number
}

type DirectPostCustomerAcceptanceCommand = {
  submissionMode: "DIRECT_OBJECT"
  decision: PostCustomerAcceptanceDecision
  idempotencyKey: string
}

// 完整复用 W02 §8.2；字段不得裁减或另起同义名。
type WorkItemPostCustomerAcceptanceCommand = {
  submissionMode: "WORK_ITEM"
  completion: CompleteWorkItemEnvelope<PostCustomerAcceptanceDecision>
  // completion 内含：
  // workItemId, claimToken, leaseVersion,
  // expectedSubjectVersion?, expectedSubjectHash,
  // idempotencyKey, decision
}

type PostCustomerAcceptanceCommand =
  | DirectPostCustomerAcceptanceCommand
  | WorkItemPostCustomerAcceptanceCommand

type ReverseCustomerAcceptanceCommand = {
  acceptanceId: string
  expectedAcceptanceVersion: number
  reasonCode: string
  reasonText: string
  idempotencyKey: string
}
```

当前只启用从 W05 直接登记的 `DIRECT_OBJECT` 分支。`WorkItemPostCustomerAcceptanceCommand` 是 Q2 决策并在 `erp-data-model.md` 固定注册类型与粒度后的目标契约；注册前服务端必须拒绝 `WORK_ITEM` 分支，前端不得显示 W01/W02 验收入口，也不得把失败请求降级成直接过账。启用后，从 W01/W02 正式待办进入只允许 `WORK_ITEM` 分支，服务端必须校验 `workItemId`、`claimToken`、`leaseVersion`、任务针对的 `expectedSubjectVersion` / `expectedSubjectHash` 以及验收自身版本，不能降级成仅传租约 ID 的命令。

过账事务必须重新校验当前有效履约事实、净验收上限、销售明细归属、对象版本和权限。`WORK_ITEM` 分支在同一事务中原子写验收头行、`APPLY` 分配、工作流审计、销售履约投影并完成该 `work_item`；任一部分失败则全部不提交，前端不得补调“完成待办”。`DIRECT_OBJECT` 分支不创建或完成虚构待办。冲正写新反向事实与 `REVERSE` 分配，不能更新原行。

重复提交同一幂等键必须返回同一正式结果。网络超时后先按幂等键查询结果；未确认前不关闭草稿、不推进本地进度、不自动进入下一任务。

### 8.4 前端边界

- 前端只格式化数量、日期、状态和掩码字段。
- “可验收量”“净验收量”“销售明细是否履约完成”完全采用服务端结果。
- 前端可展示草稿守恒预检查，但不能代替服务端按正式事实和反向分配校验。
- 前端不得根据验收结果修改库存、应收、采购或销售主状态。
- 附件上传完成不代表验收已过账；只有正式结果中的验收 ID/单号可作为成功依据。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 销售单头、左右栏和底栏等尺寸 Skeleton | 应用壳导航可用 | 查询完成原位替换 |
| 刷新 | 保留旧事实，标记正在刷新；编辑器不重置 | 可继续编辑；正式提交前等待重验 | 成功合并未改字段，失败保留旧数据 |
| 无可验收量 | 阅读态显示原因及历史 | 去 W09 查看履约、查看历史 | 新履约过账后重查 |
| 无验收历史 | 待验收区正常；历史区显示空态 | 开始登记 | 首次过账后出现记录 |
| 无数据范围 | 不展示客户、明细与数量 | 返回销售单列表、查看权限范围 | 权限更新后重查 |
| 验收任务类型/粒度未登记 | W01/W02 不展示验收入口；直接携带 `workItemId` 时显示配置 blocker，不返回任务或敏感对象 | 去 W05 按销售单直接登记 | Q2 确认、统一模型/API 注册并上线后启用目标契约 |
| 查询失败且无缓存 | `BusinessFailureState` | 重试、返回 W05 | 重试成功 |
| 查询失败但有缓存 | 保留旧内容并标“刷新失败” | 查看；正式过账前必须重验 | 重试成功 |
| 数据陈旧 | 展示事实水位；禁用正式过账 | 刷新 | 获取当前版本后恢复 |
| 保存中 | 保存指示；防止重复保存 | 继续编辑非冲突字段 | 服务端确认新草稿版本 |
| 保存失败 | 输入不丢，错误靠近保存区 | 重试、复制输入 | 重试成功 |
| 校验失败 | 顶部摘要 + 行内错误，焦点到首错 | 修正、保存草稿 | 校验通过 |
| 版本冲突 | 显示变更行 diff，不覆盖任何一方 | 刷新并重做分配、放弃本地变更 | 基于新版本保存 |
| 租约即将到期 | 倒计时 warning | 续租、保存草稿 | 续租成功 |
| 租约丢失 | 保留本地输入只读，提交禁用 | 重新领取、复制输入 | 重取事实和版本 |
| 正式动作成功 | `FormalActionResult` 固定显示验收单号、数量、剩余量和下一步 | 查看结果、创建异常处理、返回履约区 | 用户明确继续 |
| 正式结果不确定 | 停留当前草稿，不显示成功进度 | 查询最终结果、同幂等键重试 | 查到同一结果或明确失败 |
| 字段级隐藏 | 标签保留、值掩码；守恒仍由服务端完成 | 其余授权动作 | 权限更新后重查 |
| 权限收回 | 清除敏感值并切无权限态 | 返回有权工作面 | 权限恢复后重查 |

## 10. 响应式、键盘与无障碍

### 10.1 响应式

| 视口 | 布局变化 | 必须保留 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | W05 头 + 62/38 双栏；两栏独立滚动；底栏固定 | 销售单身份、待验收来源、分配、结果数量、主动作 | 无 |
| 1280×800 | 双栏改 58/42；明细说明压缩 | 单号、明细、可验收量、错误摘要 | 历史摘要折叠，次要履约备注进详情 |
| 1024×768 | 单列分步区域：事实池在上、本次验收在下；底栏固定 | 当前选中来源摘要、守恒结果、保存与过账 | 销售单多轨进度收为紧凑条 |
| 768×1024 | 导航抽屉；单列；履约事实改紧凑卡片 | 稳定单号、商品/服务、剩余量、验收结果 | 批量自动分配入口折叠；历史附件延后加载 |
| 375×812 | 只读待验收与简单“整批通过”场景；复杂分配转桌面提示 | 任务身份、来源、数量、结果查看 | 多来源分配、短少/拒收复杂登记、冲正不在手机完成 |

### 10.2 键盘与焦点

- Tab 顺序：销售单子导航 → 筛选 → 履约来源 → 验收字段 → 附件 → 校验摘要 → 正式动作。
- 履约来源选择使用真实 checkbox；行内展开使用 `aria-expanded`。
- 数量输入必须带单位和关联标签，错误通过 `aria-describedby` 关联。
- `⌘S` 保存草稿；校验通过时 `⌘↵` 打开正式确认，不直接绕过确认层提交。
- 关闭附件、详情或确认 Dialog 后焦点回到触发按钮。
- 过账成功后焦点移到固定结果标题；返回阅读态后落到新验收时间线条目。
- 数量、结果、错误和成功不得只依赖颜色表达；触控目标至少 44×44px。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 今日工作台 / 统一待办（目标，当前禁用） | W01 / W02 | 注册后传 `workItemId`、销售单 ID、可选销售明细 ID；注册前不生成或展示普通验收任务 | 事实与任务同事务完成后刷新来源队列；当前从 W05 直接进入 |
| 销售单对象中心 | W05 | 同一销售单页签、`section=acceptance` | 退出编辑回 W05 履约阅读态 |
| 履约作业 | W09 | 履约事实类型/ID、销售单 ID、来源任务 | W09 页签保留；验收后其事实不被修改 |
| 变更与异常 | W05 子区 | 销售单 ID、验收 ID、影响行与结构化原因 | 完成异常建单后回验收结果区 |
| 库存台账 | W10 | 仓发相关 SKU/仓库稳定 ID | 只读钻取，返回保留验收草稿 |
| 客户往来 | W11 | 销售单/应收稳定 ID | 验收不自动改变应收；返回 W06 上下文 |

跨工作面只传稳定身份；数量、状态、敏感地址和权限由目标工作面重新查询。

## 12. 验收清单

### 12.1 流程与布局

- [ ] 当前用户从 W05 进入无需再次搜索销售单；只有正式类型与粒度注册后才开放 W01/W02 待办入口。
- [ ] W06 没有平行顶层路由和脱离销售单上下文的唯一入口。
- [ ] 一次验收可分配多个履约批次，同一履约批次可被多次验收。
- [ ] 1440×900 下销售单身份、至少两条履约来源、本次验收摘要和主动作同屏可见。
- [ ] 短少、拒收和服务不通过结果明确说明“仅记录验收事实”，不暗示库存/票款已处理。

### 12.2 数据、权限与正式动作

- [ ] 页面所有字段能追溯到销售版本、履约事实、验收事实、正式投影或权限结果。
- [ ] 可验收量和履约完成采用服务端净事实，前端不按表头状态推断。
- [ ] Q2 未关闭且统一模型/API 未注册前，W01/W02 验收入口与 `WORK_ITEM` 提交 fail-closed；不得用 `BUSINESS_EXCEPTION` 或页面私有码代替。
- [ ] 注册后，从 W01/W02 进入的验收过账完整使用 W02 `CompleteWorkItemEnvelope`，逐行校验同销售明细、净数量上限、任务主体版本/哈希、权限、租约和幂等键。
- [ ] 注册后的验收事实、`APPLY` 分配与正式待办完成同事务；当前从 W05 直接登记不创建或完成虚构待办。
- [ ] 查询 View 只返回领取人、到期、版本和 `hasValidClaim` 等安全租约投影；原始 `claimToken` 仅由 Claim / 续租 mutation 返回并留在会话内存。
- [ ] 已过账验收不可编辑，误录通过新反向事实与 `REVERSE` 分配纠正。
- [ ] 任务租约与对象编辑租约边界清晰，租约不新增业务状态。
- [ ] 权限收回后客户、地址、附件和本地敏感快照被清理。

### 12.3 状态、恢复与无障碍

- [ ] §9 全部状态有组件测试或浏览器验证。
- [ ] 1440、1280、1024、768、375 五档视口符合 §10.1。
- [ ] 正式成功固定展示验收单号；超时不会误报成功或重复过账。
- [ ] 刷新、后退和从 W09 返回均恢复销售单、子区与草稿；注册任务入口后还需恢复来源任务。
- [ ] 键盘可完成来源选择、数量填写、保存和正式确认；读屏能听到错误与结果。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | 哪些验收结果必须上传客户签收单、邮件或服务确认附件？ | 字段必填、移动端能力、审计证据 | 销售负责人 + 法务/内控 | 过账前按业务类型配置证据规则，拒收与服务不通过默认必传 |
| Q2 | 普通客户验收使用哪个固定 `work_item_type`，并按销售单、销售明细还是一次履约事实生成？ | 统一注册表、W01/W02 数量、租约粒度、主体版本/哈希与完成条件 | 架构负责人 + 销售 + 采购 + 产品 | 固定注册一种类型并按销售单聚合任务，进入后按明细和履约事实分配；决策写回统一模型/API 前入口保持 fail-closed |
| Q3 | 短少/拒收过账后自动创建异常处理草稿，还是只生成待办由销售确认创建？ | 正式事务边界、结果区下一步 | 销售 + 仓储 + 财务 | 先生成强提醒待办，用户确认影响范围后再创建处理单 |
| Q4 | 误录验收冲正是否需要销售经理复核或岗位分离？ | `REVERSE_ACCEPTANCE` 权限与队列 | 内控 + 销售负责人 | 高影响冲正进入复核队列，普通经办不能单人完成 |
| Q5 | 手机端是否允许带附件的整批验收通过？ | 375px 能力边界与附件上传 | 销售负责人 + 安全负责人 | 只在单一来源、无差异且证据齐全时开放简单通过 |

确认后把结论写回正式章节并移除对应问题，不保留“建议规则”和正式规则并存。尤其 Q2 未写回 `erp-data-model.md` 固定注册表和 API 前，W01/W02 验收入口不得上线。

## 14. 业务依据

- `erp-phase-1.md` §4.3.1：非卡券明细以客户验收通过为履约完成，履约期限只用于超期预警。
- `erp-phase-1.md` §6.2、§6.3、§7.1–§7.3：客户验收单、履约顺序及短少/拒收后续处理责任。
- `erp-phase-1.md` §9.3：全部明细履约完成且应收结清后销售单关闭，开票不阻塞关闭。
- `erp-data-model.md` §6.7：`customer_acceptance`、验收行与履约分配的数据结构及守恒规则。
- `erp-data-model.md` §7.1、§8.2：固定销售状态机和客户验收过账事务不变量。
- `erp-ui-design.md` §3.4、§4.5–§4.6、§5.3–§5.4、§9–§11：对象中心、编辑作业、浮层、状态与响应式规则。
- `erp-ui-flows.md` §2、§7：验收挂在 W05 履约子区，W09 事实作为来源，不走孤立列表。
