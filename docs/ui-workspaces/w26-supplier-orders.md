# W26 · 供应商订单

> 状态：草稿
> 页面模式：M2 高密度查询列表 + M4 对象中心
> 主要路由：`/supplier-api/orders`、`/supplier-api/orders/:supplierOrderId`
> 主要角色：采购、客服；运营、财务和系统管理员按职责只读或协同
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

- 采购和客服能从一张供应商子订单看清：由哪笔商城支付触发、向哪个供应商提交、当前履约/取消/退款三条进度分别是什么。
- 对拒单、超时、结果未知、取消失败和退款失败，直接进入正确的查询或人工补偿路径，不回列表拼接上下文。
- 能追溯商城商品明细、固定供给版本、供应商外部单号、动作历史、成本快照和售后请求。

### 1.2 业务目标

- 承载 `T` 及之后支付成功事实触发的自动供应商履约，保持商城支付事实与供应商执行结果可分别追溯。
- 把履约主线、取消进度和退款进度作为正交事实展示，避免用单一状态覆盖“已完成但部分退款”等真实情况。
- 让结果未知遵循“先查询原结果，再决定是否重放”的安全路径；任何重放沿用原幂等键。
- 让异常处理追加查询、取消、退款或补偿记录，不删除商城支付、消费、供应商订单或既有成本事实。

### 1.3 不在本工作面完成

- 不修改商城支付、消费分摊或关键事实；查看原事实进入 W25。
- 不修改外部商品映射、固定供应关系和发布版本；分别进入 W21、W22。
- 不直接修订结算成本或形成应付；周期结算进入 W27，付款和进项票进入 W12。
- 不在订单中心直接改接口消息、签名、密钥或对账差异；技术错误和跨系统差异进入 W29。
- 不允许脱离商城售后请求创建任意取消或退款动作。

## 2. 用户、权限与数据范围

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 采购 | 我的异常 / 全部有权订单 | 负责供应商、供给关系或被指派订单 | 查看、查询供应商结果、转人工、协同供应商 |
| 客服 | 售后待处理 | 与本人服务范围相关的商城订单和供应商订单 | 查看售后进度、发起已存在请求的后续处理、记录协同说明 |
| 运营 | 异常协同 | 负责商品或商城范围 | 查看支付到履约链路、协同商城，不修改供应商事实 |
| 财务 | 成本与退款只读 | 有权供应商及结算范围 | 查看成本、供应商退款和结算归属 |
| 系统管理员 / 运维 | 接口异常入口 | 授权连接和环境 | 查看消息摘要、进入 W29 查询或重放；不代替业务确认 |
| 管理层 | 无默认入口 | 经授权的汇总与只读订单 | 只读，不展示员工履约敏感字段 |

权限规则：

- 无 W26 模块权限时不展示入口；直接访问路由显示无权限页。
- 数据范围由服务端按供应商、商城、组织和责任任务过滤，前端不得加载全量后再隐藏。
- 收货人、手机号和地址默认掩码；只有履约处理所需角色可短时揭示，揭示和下载均审计。
- 成本、进项税率和结算信息使用独立字段权限；无权限时保留字段标签与布局、隐藏值。
- `allowedActions` 和 `actionBlockers` 由服务端按订单状态、供应商能力、售后请求、租约和岗位权限计算。
- 页面打开期间权限被收回时，立即清除已揭示敏感值和缓存详情，切换为无权限或无数据范围状态。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 浏览订单 | 侧栏“供应商订单” | `/supplier-api/orders?view=actionable` | 恢复筛选、分页和滚动位置 |
| 从商城消费钻取 | W25 供应商履约区 | 打开 `/supplier-api/orders/:supplierOrderId?from=mall-order&sourceId=:mallOrderId` | 关闭后聚焦 W25 原订单 |
| 从异常任务处理 | W01 / W02 / W29 | 打开订单中心并携带 `workItemId`、`queueContextId` | 返回原队列上下文 |
| 从结算明细钻取 | W27 | 打开订单中心并聚焦“成本与结算” | 返回 W27 原结算明细 |
| 单击列表行 | W26 列表 | 打开 `detail` 半屏预览，URL 写入 `preview={id}` | 关闭后焦点回原行 |
| 打开中心 | detail 底栏或行操作 | 以稳定订单 ID 聚焦或新建任务页签 | 返回列表仍保留预览来源 |

对象中心 TaskTab 身份为 `supplier-fulfillment-order:{supplierOrderId}`。同一订单重复打开只聚焦原页签；外部单号补齐或标题变化不改变页签身份。刷新恢复当前订单和锚点子区，不恢复临时确认层。脏状态只用于尚未提交的协同说明；关闭时按全局脏页签规则确认。

## 4. 页面布局

### 4.1 桌面布局

列表工作面：

```text
┌ PageHeader：供应商订单                     [数据水位] [导出]
├ MetricStrip：待提交 | 结果未知 | 履约异常 | 售后待处理
├ ListToolbar：视图 | 搜索 | 供应商 | 三轨状态 | 支付时间 | 高级筛选
├ BusinessTableFrame
│ 订单号 | 商城单号 | 供应商 | 履约状态 | 取消 | 退款 | 外部单号 | 更新时间 | 操作
└ 分页
                                      ┌ detail 半屏预览 ───────────┐
                                      │ 来源、明细、三轨进度       │
                                      │ 最近动作、异常原因、下一步 │
                                      └ [关闭] [打开订单中心] ────┘
```

对象中心：

```text
┌ DocumentHeader：供应商订单号 · 供应商 · 综合状态 · 外部单号
│ 履约：[已接单]  取消：[无]  退款：[部分退款]       [查询结果] [更多]
├ 异常 / 结果未知 / 能力不足提示
├ 概览 | 商品明细 | 履约与物流 | 售后 | 成本与结算 | 动作与审计
├ 当前子区内容
└ FormalActionResult（最近正式动作的固定结果）
```

### 4.2 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 页头与指标 | 呈现处理水位并一键过滤 | `PageHeader` `MetricStrip` `DataFreshness` | 页头固定于内容顶部 |
| 列表工具栏 | 高频查询和 Saved View | `ListToolbar` | 表格滚动时保持可见 |
| 订单表格 | 扫描身份、三轨状态和下一步 | `BusinessTableFrame` `DataTable` | 订单号左固定，操作右固定 |
| detail 预览 | 不离开列表核对主事实 | `QuickPreviewSheet size="detail"` | 浮层 |
| 对象头 | 锁定订单身份、状态、供应商 | `DocumentHeader` `StatusTrackSummary` | 中心滚动时吸顶 |
| 异常提示 | 解释为何不能继续及正确去向 | `InterfaceErrorResolutionPanel` / `Alert` | 有异常时可见 |
| 锚点子区 | 组织来源、履约、售后、成本和审计 | `DocumentSection` `RelatedDocumentList` | 子导航吸顶 |

## 5. 展示内容与字段

### 5.1 列表与预览

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 身份 | `fulfillmentOrderNo` | 供应商订单号 | `supplier_fulfillment_order` | ERP 稳定单号 | 有对象查看权可打开 |
| 来源 | `mallOrderNo` / `paidAt` | 商城订单 / 支付时间 | `mall_order` / 支付事实 | 支付发生时间，不用 ERP 接收时间 | 按商城订单范围 |
| 供应商 | `supplierName` | 供应商 | 供应商快照 | 使用下单时快照 | 采购/客服可见 |
| 外部结果 | `externalOrderNo` | 供应商单号 | 供应商接单结果 | 未取得时明确“尚未返回” | 不展示外部鉴权信息 |
| 履约轨 | `fulfillmentStatus` | 已接收 / 提交中 / 已接单 / 明确拒绝 / 结果未知 / 履约中 / 已发货 / 已完成 / 异常 | `supplier_fulfillment_order.fulfillment_status` 与状态历史 | 严格映射 `RECEIVED`、`SUBMITTING`、`ACCEPTED`、`REJECTED`、`RESULT_UNKNOWN`、`FULFILLING`、`SHIPPED`、`COMPLETED`、`EXCEPTION`；不得把取消退款折入本字段 | 按对象权限 |
| 取消轨 | `cancelStatus` | 未发起 / 处理中 / 已取消 / 失败 / 待人工 | 取消动作及结果 | 与履约轨正交 | 售后权限 |
| 退款轨 | `refundStatus` | 未发起 / 处理中 / 部分 / 全部 / 失败 / 待人工 | 供应商退款事实 | 部分退款不回退履约状态 | 成本金额按字段权限 |
| 异常 | `errorClass` / `actionBlocker` | 异常原因 / 下一步 | 错误任务与服务端动作判断 | 业务语言，不展示堆栈 | 技术摘要仅管理员可见 |
| 时间 | `lastBusinessAt` | 最近业务变化 | 状态历史 / 正式动作 | 发生时间 + 数据水位 | 全员有权对象可见 |

### 5.2 对象中心

| 子区 | 必须展示 | 事实来源 | 特别规则 |
| --- | --- | --- | --- |
| 概览 | 商城订单、支付事实键摘要、履约链归属、供应商、连接环境、固定供给版本 | `mall_order`、`supplier_fulfillment_order` | 明确提示“商城支付已发生，当前处理供应商履约” |
| 商品明细 | 商品快照、数量、外部商品、发布版本、供给版本、下单成本快照 | `supplier_fulfillment_item` | 一条商城明细只属于一个供应商子订单，不显示可改供应商 |
| 履约与物流 | 接单、发货、完成时间，承运商、物流号，状态历史 | 供应商回调和状态历史 | 重复/乱序回调不覆盖历史 |
| 售后 | 商城售后请求、申请范围、供应商动作、商城退款、余额恢复、供应商退款进度 | 售后请求与三类结果事实 | 三类事实分别展示，不用一个“已退款”掩盖缺口 |
| 成本与结算 | 当前累计成本、成本来源、成本差额、所属结算单和应付入口 | `cost_entry`、`cost_allocation`、W27 投影 | 金额按含税/不含税分别标注 |
| 动作与审计 | 下单/查询/取消/退款动作、幂等键尾部摘要、尝试次数、结果分类、操作人 | `supplier_order_action`、`integration_attempt`、审计 | 不展示密钥、完整报文或敏感地址 |

收货地址只在“履约与物流”按需揭示，不出现在列表、导出默认列、URL、分析埋点或浏览器持久缓存中。

## 6. 搜索、筛选、排序与默认视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| Saved View | `actionable` | `view=actionable` | 默认显示结果未知、异常和售后待处理；可切“全部”“最近完成” |
| 搜索 | 空 | `q=` | 精确优先匹配 ERP 订单号、商城订单号、供应商外部单号 |
| 供应商 | 全部有权供应商 | `supplierId=` | 服务端按稳定 ID 过滤 |
| 履约状态 | 可操作集合 | `fulfillmentStatus=` | 支持多选，不与取消/退款状态混成一个枚举 |
| 取消 / 退款 | 全部 | `cancelStatus=` / `refundStatus=` | 两个独立筛选器 |
| 结果未知快捷筛选 | 默认包含 | `fulfillmentStatus=RESULT_UNKNOWN` | 只是履约状态筛选预设；不维护独立 `resultUnknown` 字段或第二套状态 |
| 支付时间 | 最近 30 天 | `paidFrom=` / `paidTo=` | 以支付事实发生时间为准 |
| 排序 | 优先级降序、异常发生时间升序 | `sort=` | 服务端稳定排序，订单 ID 作为尾排序键 |
| 分页 | 50 条 | `page=` / `pageSize=` | 服务端分页；最大页大小由接口约定 |

列表首屏在 1440×900 下至少显示 6 条有效数据行。刷新保留旧行；浏览器后退恢复筛选、分页和当前预览。导出使用服务端选择快照，执行时重验权限和字段遮罩。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 查询原结果 | 结果未知提示 / 主动作 | `QUERY_RESULT` 可用，供应商具备查询能力 | 无破坏性确认 | 追加查询证据；任务入口同时追加任务处理记录，并保持当前任务为 `PENDING/IN_PROGRESS` 非终结状态；固定展示已受理、已拒绝、明确无结果或仍未知 | 停留当前订单；保留查询编号、当前任务和重试入口 |
| 安全重放下单 | 查询结果后的动作 | 查询已明确“无结果”且服务端确认可安全重试 | 展示原订单、供应商及影响；正式确认 | 使用原 `fulfillmentOrderNo` 对应的供应商幂等键重放并追加结果证据；任务入口同时追加任务处理记录，并保持当前任务为 `PENDING/IN_PROGRESS` 非终结状态 | 结果未知不判成功、不跳转；继续“查询原结果” |
| 暂挂 / 本轮跳过 | 正式任务处理器 | 当前租约有效；选择结构化原因 | 无破坏性确认 | 使用非终结动作信封记录原因；同一任务保持 `PENDING/IN_PROGRESS`，仅按服务端结果续租或释放租约并移动本轮队列游标 | 失败时停留当前任务，保留原因；刷新任务与队列快照后可用同一幂等键恢复 |
| 提交取消 / 退款（领域动作） | 售后子区 | 存在有效商城售后请求、商品能力支持且动作未重复 | 展示请求范围和已发生支付事实 | 以服务端固定幂等键提交并追加领域动作 / 结果；无论是否存在同对象任务，都不顺带完成任务 | 超时转结果未知；不得新建另一请求、更换幂等键或把任务标为完成 |
| 确认可验证终态并完成任务 | 正式任务处理器 | 已取得可验证的下单、取消或退款终态；完成动作与任务处理器登记值一致 | 展示终态证据、对象版本和任务影响 | 同一事务重验终态证据并完成任务，固定展示业务结果和任务结果 | 证据仍未知或版本变化时保持同一任务为 `PENDING/IN_PROGRESS`，刷新证据后重提 |
| 转人工 | 异常提示 | 自动路径不可用且任务允许显式转交 | 说明目标责任、原因与业务影响 | 原任务置为 `TRANSFERRED`、原租约失效并原子创建 `UNCLAIMED/PENDING` 后继正式任务；订单事实不变 | 原任务和租约保持不变，保留输入并允许使用同一动作幂等键恢复 |
| 记录协同说明 | 对象中心 | 有协同权限，订单版本未变化 | 无 | 追加审计说明，不改变状态 | 保留输入并提示版本冲突 |
| 揭示敏感地址 | 履约区 | 有敏感字段权限且当前处理确需 | 短时揭示确认 | 限时显示并记录审计 | 权限变化立即隐藏 |
| 导出 | 列表页头 | 有导出权限和当前数据范围 | `BatchImpactPreview` 展示范围、字段和过期时间 | 创建后台任务，结果 7 天内下载 | 部分失败报告逐项原因，不扩大范围 |

任何动作都不得把 `RESULT_UNKNOWN` 直接改成成功。明确业务拒绝不自动重试；供应商无查询能力时进入 W29 人工异常。订单或售后已产生正式结果时，重复操作返回原结果而不是再次推进状态。查询 / 重放的接口成功只表示证据和处理记录已追加，不表示任务完成；只有另行确认可验证终态，或通过显式转交 / 替换动作原子创建合规的 `UNCLAIMED/PENDING` 后继任务，当前任务才可完成、转交或替换。

## 8. 数据契约

### 8.1 查询

```ts
type SupplierFulfillmentStatus =
  | "RECEIVED"
  | "SUBMITTING"
  | "ACCEPTED"
  | "REJECTED"
  | "RESULT_UNKNOWN"
  | "FULFILLING"
  | "SHIPPED"
  | "COMPLETED"
  | "EXCEPTION"

type SupplierOrderListQuery = {
  view: "actionable" | "all" | "recent_completed"
  q?: string
  supplierIds?: string[]
  fulfillmentStatuses?: SupplierFulfillmentStatus[]
  cancelStatuses?: string[]
  refundStatuses?: string[]
  paidFrom?: string
  paidTo?: string
  sort: string
  page: number
  pageSize: number
}

type SupplierOrderListRow = {
  orderId: string
  orderNo: string
  mallOrderId: string
  mallOrderNo: string
  supplierId: string
  supplierName: string
  fulfillmentStatus: SupplierFulfillmentStatus
  cancelStatus: string
  refundStatus: string
  paidAt: string
  updatedAt: string
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
}

type SupplierOrderListResult = {
  rows: SupplierOrderListRow[]
  pageInfo: { page: number; pageSize: number; total: number }
  metrics: SupplierOrderMetricView[]
  permissionVersion: string
  sourceAsOf: string
  queriedAt: string
}

type SupplierOrderDetailQuery = {
  orderId: string
  workItemId?: string
}

type SupplierOrderDetailView = {
  order: {
    id: string
    orderNo: string
    mallOrderId: string
    mallOrderNo: string
    fulfillmentChain: "ERP_AUTOMATED"
    supplierId: string
    supplierName: string
    externalOrderNo?: string
    fulfillmentStatus: SupplierFulfillmentStatus
    cancelStatus: string
    refundStatus: string
    lockVersion: number
  }
  items: SupplierOrderItemView[]
  afterSales: AfterSalesTrackView[]
  costs: AuthorizedCostView
  actions: SupplierActionView[]
  workItem?: {
    workItemId: string
    workItemType: "INTEGRATION_RESULT_UNKNOWN" | "BUSINESS_EXCEPTION"
    businessObjectType: "SUPPLIER_FULFILLMENT_ORDER"
    businessObjectId: string
    subjectVersion?: string
    subjectHash: string
    completionAction: string
    allowedTaskActions: string[]
    claimedBy?: ActorView
    leaseVersion?: number
    leaseExpiresAt?: string
  }
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
  freshness: { updatedAt: string; state: "fresh" | "stale" }
}
```

Query Key 至少包含当前用户、角色、权限版本、数据范围版本、筛选、排序、分页和对象版本。列表、详情、售后和成本可以拆 Query，但对象头必须显示各区数据水位；局部失败不得把整个中心清空。

### 8.2 提交

```ts
type SupplierOrderObjectInvestigationCommand = {
  orderId: string
  expectedLockVersion: number
  action: "QUERY_RESULT" | "REPLAY"
  operationId: string
  targetSupplierActionId: string
  idempotencyKey: string
}

type SupplierOrderTaskInvestigationCommand =
  WorkItemActionEnvelope<{
    type: "QUERY_RESULT" | "REPLAY"
    orderId: string
    expectedOrderLockVersion: number
    targetSupplierActionId: string
    operationId: string
  }> & { expectedSubjectVersion: string }

type SupplierOrderInvestigationEvidence = {
  evidenceId: string
  targetSupplierActionId: string
  outcome: "VERIFIED_TERMINAL" | "VERIFIED_NO_RESULT" | "RESULT_UNKNOWN"
  recordedAt: string
}

type SupplierOrderTaskInvestigationResult =
  WorkItemActionResult<SupplierOrderInvestigationEvidence>

type DeferSupplierOrderTaskAction = {
  type: "DEFER"
  orderId: string
  reasonCode: string
  comment?: string
  queueContextId: string
}

type DeferSupplierOrderTaskCommand =
  WorkItemActionEnvelope<DeferSupplierOrderTaskAction>

type DeferSupplierOrderTaskResult =
  WorkItemActionResult<{
    reasonCode: string
    queueContextId: string
    leaseDisposition: "RENEWED" | "RELEASED"
    nextQueueCursor?: string
  }>

type SupplierOrderAfterSalesCommand = {
  orderId: string
  expectedLockVersion: number
  action: "CANCEL" | "REFUND"
  operationId: string
  idempotencyKey: string
  afterSalesRequestId: string
  reasonCode?: string
  comment?: string
}

type SupplierOrderTaskCompletionCommand =
  CompleteWorkItemEnvelope<{
    type: "CONFIRM_VERIFIED_TERMINAL_RESULT"
    orderId: string
    expectedOrderLockVersion: number
    verifiedSupplierActionResultId: string
    resolution:
      | "ORDER_ACCEPTED"
      | "ORDER_REJECTED"
      | "ORDER_COMPLETED"
      | "CANCELLED"
      | "REFUNDED"
  }> & { expectedSubjectVersion: string }

type SupplierOrderTaskTransferCommand =
  TransferWorkItemEnvelope<{
    type: "TRANSFER_MANUAL"
    orderId: string
    targetOwnerRole: string
    targetOwnerUserId?: string
    reasonCode: string
    comment?: string
  }> & { expectedSubjectVersion: string }
```

- W26 直接引用 W02 的 `WorkItemActionEnvelope`、`CompleteWorkItemEnvelope` 和 `TransferWorkItemEnvelope`；其字段、校验和完成语义以 W02 为准，不在本工作面另造一套可选任务字段。
- `SupplierOrderObjectInvestigationCommand` 只供非任务对象入口查询 / 重放；`SupplierOrderTaskInvestigationCommand` 只供正式任务入口。服务端不得因客户端漏传任务信封而把任务动作降级为普通对象动作。
- 任务内 `QUERY_RESULT` / `REPLAY` 必须完整校验 `workItemId`、`claimToken`、`leaseVersion`、`expectedSubjectVersion`、`expectedSubjectHash`、订单当前版本和本次任务动作的 `idempotencyKey`；`targetSupplierActionId` 必须引用原供应商动作。
- `WorkItemActionEnvelope.idempotencyKey` 只标识本次查询 / 重放任务动作。`REPLAY` 的外部调用仍由服务端沿用原供应商动作幂等键，两者不得混用；查询不产生新的业务订单或替换动作。
- 查询 / 重放任务动作必须在同一事务追加查询或重放证据与任务处理记录，并返回 `WorkItemActionResult`。即使已取得可验证终态，动作结果也只能是 `workItemStatus: "PENDING" | "IN_PROGRESS"`，可返回续租后的新租约、任务版本和新对象指纹；前端不得自动下一项。
- 若查询 / 重放后仍为 `RESULT_UNKNOWN`，必须保持同一 `workItemId` 为 `PENDING/IN_PROGRESS`，续租或更新租约版本，并保留下一次查询入口；不得完成、关闭、转交或偷换成新任务。
- “暂挂 / 本轮跳过”只使用 `DeferSupplierOrderTaskCommand`。服务端记录结构化原因后返回非终结动作结果：任务仍为 `PENDING/IN_PROGRESS`，不得写入不存在的 `paused` 状态；租约是否续期或释放以及下一游标均以服务端结果为准，客户端只能在同一 `queueContextId` / 队列快照内移动本轮游标。
- `SupplierOrderAfterSalesCommand` 是取消 / 退款领域直接动作：它只校验售后请求、订单版本和服务端固定幂等键，追加供应商动作及结果，不读取或改变 `work_item`。任务处理器不得把这类对象命令包装成“提交即完成”。
- 只有已经取得可验证终态时，任务处理器才可用 `SupplierOrderTaskCompletionCommand`；服务端必须重新校验 `verifiedSupplierActionResultId`、订单版本、任务租约、主体版本 / 指纹和处理器登记的 `completionAction`，并在同一事务固定业务结果与任务 `COMPLETED` 结果。证据仍未知时拒绝完成并保持原任务为 `PENDING/IN_PROGRESS`；重复/误派等关闭场景只能另用 W02 `CloseWorkItemEnvelope`，不得复用本完成命令。
- 显式转交只使用 `SupplierOrderTaskTransferCommand`。服务端在同一事务将原任务置为 `TRANSFERRED`、使原租约失效、追加转交记录并创建符合任务类型、责任范围和对象指纹约束的 `UNCLAIMED/PENDING` 后继任务；不得用转交伪造业务终态或直接覆盖责任人。
- mutation 返回 `operationId` / 动作记录、业务或证据结果、对象新版本和任务结果（如适用）；网络断开时按对应信封的幂等键查询同一次动作，不得换键重提。
- 版本、租约或指纹冲突不静默覆盖，显示当前状态与用户操作目标，由用户重新领取或确认仍适用的动作。

### 8.3 前端边界

- 前端只格式化金额、时间、状态文案和三轨进度，不推导供应商状态终态。
- 前端不得从回调次数、HTTP 状态或 toast 推断下单成功。
- 成本累计、退款净额、能力支持、可安全重试、售后关闭条件全部采用服务端结果。
- 外部报文、密钥、完整地址、手机号不得进入 URL、日志、埋点或长期客户端缓存。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 与列表/中心成稿一致的 Skeleton | 应用壳导航 | 查询完成原位替换 |
| 刷新 | 保留旧数据，显示分区刷新和水位 | 可查看；正式动作提交时服务端重验 | 成功更新水位；失败保留旧值 |
| 空数据 | “当前范围没有供应商订单” | 调整时间或进入商城消费 | 新事实到达后刷新 |
| 筛选无结果 | 展示筛选摘要 | 清除筛选 | 恢复默认视图 |
| 无数据范围 | 专用无范围空态，不显示虚假 0 指标 | 查看当前角色 / 申请权限 | 范围变化后重查 |
| 查询失败且无缓存 | `BusinessFailureState` | 重试、返回来源 | 重试成功 |
| 局部失败 | 对应子区失败，其余中心保持可读 | 重试该区 | 局部恢复 |
| 数据陈旧 | 标注更新时间；动作前必须重验 | 刷新、查看历史 | 新查询追平 |
| 字段级隐藏 | 标签保留、敏感值掩码 | 其余授权动作 | 权限恢复后重查 |
| 提交中 | 锁定当前正式动作，禁止重复点击 | 取消不可中断已发送请求 | 返回正式结果或结果未知 |
| 正式动作成功 | `FormalActionResult` 固定展示供应商订单、履约/取消/退款轨结果、时间和下一步 | 返回 W25、进入 W27、继续处理 | 用户明确关闭结果 |
| 任务内查询 / 重放成功 | 固定展示本次证据、动作记录号、新租约与“任务仍待处理”；即使取得终态也不自动完成 | 继续查询、确认可验证终态、暂挂或转交 | 同一任务保持 `PENDING/IN_PROGRESS`；刷新任务版本和对象指纹 |
| 暂挂成功 | 显示结构化原因、任务仍待处理、租约处置与本轮下一项 | 返回当前任务或继续同一队列快照 | 同一任务保持 `PENDING/IN_PROGRESS`；只移动本轮游标，不产生暂停状态 |
| 结果未知 | 固定警示，不改变本地订单状态 | 查询原结果；不能直接再次下单 | 得到可验证终态或转人工 |
| 明确无结果 | 显示供应商查询证据与安全重放判断 | 允许时使用原幂等键重放 | 重放结果固定展示 |
| 版本冲突 | 对比当前三轨状态和原操作目标 | 重新加载、放弃旧动作 | 重新确认当前可用动作 |
| 任务租约 / 指纹冲突 | 保留订单事实和用户输入，显示当前领取人、租约或对象版本变化 | 返回任务刷新、重新领取；不能转普通对象操作绕过 | 新租约与当前指纹一致后重提 |
| 后台导出 | `BackgroundJobProgress` 显示筛选快照、字段遮罩、进度和任务号 | 查看任务 | 完成后下载；失败按原快照重试 |
| 权限收回 | 清除敏感缓存并切无权限态 | 返回有权工作面 | 权限恢复后重查 |

## 10. 响应式与键盘

| 视口 | 布局变化 | 保留内容 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 侧栏展开；列表 + detail 半屏；中心全宽 | 三轨状态、订单身份、异常和主动作 | 无 |
| 1280×800 | detail 覆盖更多列表；工具栏一行半 | ERP/商城/外部身份与主动作 | 次要时间列移入列设置 |
| 1024×768 | 图标侧栏；detail 覆盖式；中心子区两列改单列 | 三轨状态、供应商、异常原因 | 动作历史摘要折叠 |
| 768×1024 | 导航抽屉；表格横向滚动；detail 上下分区 | 订单号左固定、操作右固定、状态文字 | 成本列和次要时间默认隐藏 |
| 375×812 | 订单卡片单列；只读摘要和简单“查询结果” | 订单身份、三轨状态、异常、查询结果 | 不允许重放、取消、退款、导出和敏感地址揭示 |

键盘顺序：页头 → 指标 → 工具栏 → 表格行 → 行动作 → detail / 中心子区。`/` 聚焦列表搜索，方向键移动行，Enter 打开 detail，Esc 只关闭最上层浮层。detail 关闭焦点回原行；切换订单中心后焦点落到对象标题并播报三轨状态。状态不只靠颜色，结果未知和正式动作结果使用 `aria-live=polite`。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 商城消费订单 | W25 | 商城订单、支付事实、商品明细 ID | 返回聚焦原商品的供应商履约区 |
| API 供应商连接 | W20 | 供应商、连接环境、能力代码 | 返回订单异常子区 |
| 外部商品与供给 | W21 / W22 | SKU、外部商品、供给/发布版本 | 返回保持订单页签 |
| API 结算 | W27 | 结算单、结算明细、供应商订单 | 返回原结算行 |
| 供应商往来 | W12 | 供应商、结算单应付 | 返回成本与结算子区 |
| 接口错误与对账 | W29 | 消息、错误任务、原动作、订单 ID | 解决后刷新订单；W29 队列上下文保留 |
| 卡券经营分析 | W28 | 供应商订单、消费来源和成本口径 | 返回分析筛选不丢失 |

跨工作面只传稳定 ID 和筛选上下文，不传状态、金额、地址或权限结论作为事实。

## 12. 验收清单

### 12.1 业务与操作

- [ ] 一屏能同时看清商城支付已发生、供应商订单身份及履约/取消/退款三条进度。
- [ ] 已完成但部分退款能被正确表达，不因单一综合状态丢失事实。
- [ ] 结果未知的唯一主路径是先查询原结果，不能直接再次下单。
- [ ] 只有明确无结果且服务端确认可安全重试时才开放重放，并沿用原幂等键。
- [ ] 取消和退款必须引用既有商城售后请求，重复提交不重复调用供应商。
- [ ] 供应商拒单或履约异常不删除商城支付、消费、订单或成本事实。
- [ ] 三类退款相关事实能分别看见缺口和责任方。
- [ ] 履约主状态只使用九个正式枚举；“结果未知”快捷筛选等价于 `fulfillmentStatus = RESULT_UNKNOWN`，没有独立状态源。
- [ ] 任务内查询 / 重放使用 W02 的非终结动作信封，完整校验领取、租约、对象版本 / 指纹和本次动作幂等键；成功后同一任务仍为 `PENDING/IN_PROGRESS`，不会自动下一项。
- [ ] 暂挂使用 W02 非终结动作信封并记录结构化原因；任务不完成、不转交、不写 `paused`，租约与同一队列快照的本轮游标只按服务端结果更新。
- [ ] 取消 / 退款领域命令不读写任务；只有可验证终态能通过正式完成信封终结任务，转交则必须原子创建合规的 `UNCLAIMED/PENDING` 后继任务。

### 12.2 数据、权限与安全

- [ ] 下单时发布版本、固定供给、商品和成本快照不受后续主数据变化影响。
- [ ] 地址、手机号、成本和技术摘要按字段权限独立控制；权限收回后无缓存泄漏。
- [ ] 列表、detail、中心和导出均使用相同的数据范围版本。
- [ ] 业务页和日志不展示密钥、完整请求报文或未脱敏响应。
- [ ] 正式动作返回固定结果；超时可按 `operationId` / 幂等键查询，不靠 toast 猜状态。

### 12.3 体验与状态

- [ ] 1440×900 首屏至少显示 6 条有效订单，身份和操作列固定。
- [ ] §9 全部状态和 §10 五档视口完成验证。
- [ ] 键盘可完成搜索、开预览、查询原结果和返回来源。
- [ ] detail 关闭、中心返回和权限变化后的焦点行为符合契约。
- [ ] 页面文案使用业务语言，不出现表名、HTTP 状态、堆栈或组件名称。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | 采购与客服对“转人工”任务的默认责任边界如何按异常类型分配？ | 默认视图、责任人和 SLA | 采购负责人 + 客服负责人 | 供应商接单/履约归采购，员工售后沟通归客服，系统异常归管理员 |
| Q2 | 敏感收货信息单次揭示的有效时长是多少？ | 安全交互与重新鉴权 | 安全负责人 + 客服负责人 | 5 分钟，离开对象页或权限变化立即清除 |
| Q3 | 供应商结果未知在无查询能力时的人工确认需要哪些最低证据？ | 异常能否解决及审计 | 采购 + 运维 + 财务 | 外部工单/书面回复、核对时间、经办人和外部单号至少齐备 |
| Q4 | 默认“可操作”视图是否包含长时间无状态变化但未超 SLA 的订单？ | 待办水位和噪声 | 采购负责人 | 未超 SLA 不进入异常指标，仅保留全部订单视图 |

确认后把结论写回对应章节并移除本表项，不让“当前建议”长期充当正式规则。

## 14. 业务依据

- `erp-phase-2.md` §3.5–§3.6：固定供应关系、支付事实与供应商下单失败补偿边界。
- `erp-phase-2.md` §6.3、§10、§11：错误分类、供应商订单、状态、取消退款和售后责任。
- `erp-phase-2.md` §13：两层幂等、结果未知先查询、人工重放沿用原幂等键和周期对账。
- `erp-data-model.md` §6.19：`supplier_fulfillment_order`、三轨状态、动作和供应商退款事实。
- `erp-data-model.md` §7.6、§8.4、§9.4：供应商履约状态机、正式事务和结果未知断言。
- `erp-ui-design.md` §4.3、§4.5、§6、§11：M2/M4、第二期对象中心和通用状态契约。
- `erp-ui-flows.md` §11：消费订单到供应商订单的钻取，以及“支付已发生，处理履约异常”的界面语言。
