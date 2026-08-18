# W23 · 销售单执行投影

> 状态：已定<br>
> 页面模式：M2 高密度查询列表 + M4 对象中心<br>
> 主要路由：`/commerce/execution-projections`、`/commerce/execution-projections/:projectionId`<br>
> 主要角色：运营、销售、系统管理员 / 运维<br>
> 最后更新：2026-08-17

## 1. 定位与目标

### 1.1 用户目标

用户进入此工作面，应能回答：

1. 哪些已生效卡券销售版本仍等待商城接收、接收失败或结果未知？
2. 某个执行投影对应哪张 ERP 销售单、哪个不可变销售版本和哪次商城确认？
3. 商城实际收到的执行字段是什么，为什么当前版本被阻断？
4. 当前应等待自动重试、查询最终结果、人工重试，还是进入错误中心处理？
5. 从运营监控列表能否直接回到 W05 销售单“协同”子区理解完整业务上下文？

### 1.2 业务目标

- `sales_order_projection` 只是 ERP 已生效卡券销售版本向商城下发的执行投影，不是第二张销售单，也不是可独立编辑的业务单据。
- 每个 ERP 销售单版本向一个目标商城恰好对应一份不可变投影修订。
- 投影只包含商城执行所需字段，不泄露成交金额、配赠、税率、开票、应收和玩法规则。
- 销售单生效与商城执行接收是两个连续但独立的阶段；投递失败不回退销售事实、销售版本或应收。
- 单张销售单的协同状态优先在 W05 内看懂；W23 服务于跨单监控、失败治理和批量处理。

### 1.3 不在本工作面完成

- 不创建、修改、作废销售单或销售变更单；进入 W05。
- 不允许用户手工改投影字段、替换销售版本或伪造商城确认。
- 不展示或维护商城玩法、卡号、卡密、手机号绑定、余额或制卡进度。
- 不把失败投影重新生成成“修正版”；业务内容变化必须先在 W05 形成新的销售版本。
- 不在此关闭接口差异或编辑原始报文；人工异常闭环进入 W29。
- W23 不得提供已废止的“开始处理”、移动确认或完成 `work_item`；W01 的错误待办统一进入 W29，W23 只执行投递对象级查询、重试和升级动作。
- 不处理商品发布；进入 W22。

## 2. 用户、权限与数据范围

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 运营 | 失败 / 待确认视图 | 被授权商城和卡券业务范围 | 查看执行字段、查询结果、重试、转错误处理 |
| 销售 | W05 销售单协同子区 | 本人负责或协作客户的销售单 | 查看接收状态和失败说明，不能重写投影 |
| 销售领导 | 从销售单中心进入 | 授权团队销售单 | 只读查看审批生效后的投影结果 |
| 系统管理员 / 运维 | W23 全局异常视图 | 获授权商城与组织 | 批量处理失败、查询结果、进入 W29 |
| 管理层 / 财务 | 按需只读 | 授权范围 | 查看接收进度；不会因财务权限看到投影外字段 |

### 2.1 权限与字段边界

| 情况 | 页面行为 |
| --- | --- |
| 无 W23 模块权限 | 不展示独立列表入口；有 W05 权限时仍可在销售单协同子区查看该单摘要 |
| 有模块权限但无销售数据范围 | 显示无数据范围，不返回全量投影后前端过滤 |
| 有销售单查看权限但无重试权限 | 内容可读；动作禁用并展示服务端阻塞原因 |
| 外部引用展示策略未配置或无相应字段权限 | 客户名称按字段策略掩码；商城客户身份与执行基线一律只显示受控短引用，且不提供完整值复制 |
| 有财务字段权限 | W23 仍不展示成交金额、税率、应收等非投影字段，权限不会扩大对象边界 |
| 权限在页面打开时被收回 | 清除缓存中的客户和错误摘要，内容切为无权限态 |

所有操作由 `allowedActions` 和 `actionBlockers` 决定。前端不能因为状态显示为“失败”就默认用户有重试权限。

对象级 `QUERY_RESULT` / `ESCALATE` 按错误类别授权，由服务端写入 `allowedActions` / `actionBlockers`；不决定 W29 任务责任或完成权限。`RETRY` 仅当服务端判定错误可重试且角色具备 `RETRY` 权限时开放，映射差异禁止对象级重试：

| 错误类别 | 运营 | 系统管理员 / 运维 | 规则 |
| --- | --- | --- | --- |
| 业务拒绝 | 可 `QUERY_RESULT` 或 `ESCALATE` | 不得替代运营执行业务拒绝闭环，除非同时具备运营授权范围 | 业务拒绝由运营查询或升级 |
| 鉴权 / 连接 | 禁止对本类错误执行 `QUERY_RESULT` / `ESCALATE`；可只读查看 | 可 `QUERY_RESULT` 或 `ESCALATE` | 鉴权与连接类错误由运维查询或升级 |
| 映射差异 | 禁止 `RETRY`；必须直接 `ESCALATE` 到 W29 对应责任队列 | 禁止 `RETRY`；必须直接 `ESCALATE` 到 W29 对应责任队列 | 映射差异不得在 W23 重放或覆盖 |

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 查看跨单投影列表 | 侧栏“执行投影” | `/commerce/execution-projections`；筛选写入 URL | 返回恢复筛选、分页与滚动 |
| 从销售单查看协同状态 | W05 “协同”子区 | 默认留在 `sales-order:{salesOrderId}` 页签内展示当前投影摘要 | 不强制新开 W23 |
| 查看投影完整历史 | W05 “查看投影历史”或列表行 | `/commerce/execution-projections/:projectionId?revision=:revisionId` | 返回 W05 时聚焦协同子区 |
| 处理投递对象 | W23 失败指标、W05 协同摘要或 W29 证据链接 | 打开 W23 投影对象并定位 `deliveryId`；不携带任务处理上下文 | 返回原筛选、W05 或 W29 上下文 |
| 处理正式错误待办 | W01 | 统一打开 W29 并携带 `workItemId`；不直接路由 W23 | 由 W29 完成或转交后回 W01 原任务位置 |
| 批量处理 | W23 失败筛选 | 选择条件进入 URL；选择快照不直接写 URL | 完成后保留原筛选 |
| 刷新浏览器 | 任意详情 | 恢复稳定投影、选中修订和投递项 | 当前投影 |

W23 对象页签身份为 `sales-projection:{projectionId}`，标题为 `投影 · {销售单号}`。W05 和 W23 可以分别存在任务页签，但同一 W23 投影重复打开只聚焦原页签。选中历史修订或投递尝试不改变页签身份。

## 4. 页面布局

### 4.1 列表页（1440×900）

```text
┌ PageHeader：执行投影                    数据更新时间 09:36 [刷新] [批量处理]
├ MetricStrip：[待发送] [发送/重试中] [待确认超时] [失败/转人工] [已确认]
├ ListToolbar：SavedView | 搜索 | 商城 | 接收状态 | 来源 | 发生时间 | 更多筛选
├ SelectionScopeBar（显式选择失败项后）：已选 N 项 [影响预览]
├ BusinessTableFrame
│ 销售单（固定） | 来源 | 客户 | 商城 | 接收状态 | 商城已确认版
│ 最近尝试 | 失败原因 | 操作（固定）
│ （ERP 版本 / 数据版本收敛到详情抽屉「概览」，列表不并列三个 vN 列）
└ 服务端分页 / BackgroundJobProgress（批量任务存在时）
```

### 4.2 对象中心（1440×900）

```text
┌ DocumentHeader compact：销售单号 · ERP版本 · 投影版本 · 目标商城
│ 销售事实：已生效    投影投递：重试中    商城确认：待确认      [查询结果] [更多]
│ （列表页壳仍为 PageHeader page；侧栏/Sheet 对象头用 compact，不叠第二套工作面 title）
├ 关键提示：投影不是销售单副本；失败不回退销售事实或应收
├ 锚点：概览 | 执行内容 | 投递历史 | 版本对应 | 差异与错误 | 审计
├ 概览：来源销售版本、数据来源、当前商城已确认版、等待时长和责任人
├ 执行内容：客户外部身份、卡券类目、履约期限、面额、数量、卡形态、生效时间
├ 投递历史：状态、尝试、下次动作、商城确认、脱敏失败摘要
├ 版本对应：ERP 销售版本 ↔ 投影版本 ↔ 商城确认版本
└ 差异与错误：对账差异、人工任务及 W29 入口
```

### 4.3 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 三段状态轨 | 区分 ERP 生效、消息投递和商城确认 | `DocumentHeader density="compact"` `StatusTrackSummary` | 顶部固定 |
| 本页只读提示 | 防止用户把执行信息当成可改单据 | `Alert` | 存在失败或用户有写权限时固定可见 |
| 执行内容 | 读取商城真正应接收的白名单字段 | `DocumentSection` | 否 |
| 版本对应 | 证明一对一来源与当前确认版本 | `RevisionTimeline` | 否 |
| 投递历史 | 查看每次尝试和下一步 | `AsyncSectionState` / 历史表 | 否 |
| 正式结果 | 固定显示查询、重试和批量任务结果 | `FormalActionResult` | 动作后主区顶部 |

## 5. 展示内容与字段

### 5.1 身份与状态

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 身份 | `salesOrderId` / `salesOrderNo` | 销售单 | `sales_order` | 稳定身份；列表固定列 | 按销售单数据范围 |
| 版本 | `salesOrderRevisionNo` | ERP 销售版本 | `sales_order_projection_revision.sales_order_revision_id` | 不可变版本号 | 可见投影即可见 |
| 版本 | `projectionRevisionNo` | 执行投影版本 | 投影修订 | 与 ERP 版本一对一 | 可见 |
| 来源 | `projectionSource` | 版本来源 | 迁移时点当前 ERP 销售版本 / ERP 销售版本 | 迁移时点版本明确标注，不声称新销售版本 | 可见 |
| 商城 | `targetMallName` | 目标商城 | 稳定投影 | 按商城范围 | 可见 |
| 销售事实 | `salesOrderCommercialStatus` | 销售单状态 | W05 查询投影 | 只读摘要；失败时仍显示已生效 | 按销售权限 |
| 接收 | `deliveryStatus` | 商城接收状态 | `sales_order_projection_delivery` | 待发送、发送中、重试中、已确认、失败、转人工 | 可见 |
| 确认 | `currentAckedRevisionNo` | 商城最后确认版本 | `current_acked_revision_id` | 无确认时明确“尚未确认” | 可见 |
| 延迟 | `pendingDuration` / `nextAttemptAt` | 等待时长 / 下次处理 | 服务端投递查询 | 服务端基于公司时钟返回 | 可见 |

### 5.2 执行投影白名单字段

| 字段 | 用户文案 | 数据来源 | 展示规则 |
| --- | --- | --- | --- |
| `customerExternalIdentity` | 商城客户引用 | 投影修订 | 外部引用展示策略未配置时只显示短引用且不可复制完整值；配置后仍按服务端字段策略裁剪 |
| `voucherCategoryExternalIdentity` | 商城卡券类目 | 投影修订 | 同时展示 ERP 类目名称与商城稳定映射 |
| `voucherExpiryAt` | 履约期限 | 投影修订 | 销售单表头级一个值 |
| `faceValue` | 面额 | 投影修订唯一明细 | 金额格式化；它是执行字段，不展示成交金额 |
| `cardCount` | 数量 | 投影修订唯一明细 | 必须来自恰好一条卡券明细 |
| `cardForm` | 卡形态 | 投影修订唯一明细 | 电子卡 / 实体卡 |
| `effectiveAt` | ERP 生效时间 | 投影修订 | 绝对时间 + 工作时区 |

W23 在任何角色下都不得增加玩法规则、成交金额、配赠、税率、开票要求或应收字段。需要理解完整商业事实时进入 W05。

### 5.3 投递与错误

| 字段 | 用户文案 | 数据来源 | 说明 |
| --- | --- | --- | --- |
| `attemptCount` / `lastAttemptAt` | 尝试次数 / 最近尝试 | 投递 | 尝试次数不等于版本数 |
| `nextAttemptAt` | 下次自动处理 | 投递 | 不可重试时为空并给出原因 |
| `mallAckAt` | 商城确认时间 | 投递 | 只有明确确认后展示 |
| `mallExecutionBaseline` | 商城执行基线 | 投递 | 展示策略未配置时只返回短引用且禁止复制完整值；配置后仅按服务端字段策略向明确授权的排障角色开放完整值；不展示商城玩法内容 |
| `errorCode` / `errorSummary` | 失败原因 | 脱敏错误摘要 | 不显示密钥、原始报文或堆栈 |
| `reconciliationStatus` | 版本核对 | 对账差异 | 差异只生成任务，不覆盖任一侧事实 |
| `workItemId` / `errorTaskId` | 待办 / 错误对象 | W29 关联 | 仅显示稳定引用、责任方、状态和 W29 入口；W23 不得提供已废止的“开始处理”或完成任务 |

## 6. 搜索、筛选、排序与默认视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 搜索 | 空 | `q` | 精确销售单号、投影编号；客户名按权限模糊搜索 |
| 目标商城 | 全部有权商城 | `mall` | 服务端过滤 |
| 接收状态 | 待处理 + 失败 | `deliveryStatus` | 运营默认关注未确认；销售从 W05 进入不套此默认 |
| 投影来源 | 全部 | `source` | 迁移时点当前 ERP 销售版本 / ERP 销售版本 |
| 等待时长 | 全部 | `latency` | 正常、接近超时、已超时；SLA 分级、阈值与自动重试上限按错误类别由服务端配置并返回，前端禁止本地推算 |
| 版本差异 | 全部 | `reconciliation` | 只查看 ERP 当前与商城确认不一致项 |
| 负责人 | 当前角色范围 | `owner` | 自动重试、运营协同、人工错误责任队列 |
| 时间窗口 | 近 90 天或未闭环 | `timeWindow` | 默认视图必须聚焦近 90 天内或未闭环项；全历史可追溯查询，不得从系统删除已确认历史 |
| 排序 | 风险优先 | `sort=risk.desc,createdAt.asc` | 结果未知/转人工 → 失败 → 超时 → 待发送 → 已确认 |

- 单击列表行打开 `detail` 半屏，可读完整白名单字段、来源版本、最新投递和错误摘要。
- 指标筛选必须有选中态和当前筛选摘要，结果数量由服务端返回。
- `ListToolbar` 位于 `BusinessTableFrame` 的 toolbar 槽（批量选择条进 selectionBar 槽）；`filterSummary` 展示在表格 `description`，有筛选时写筛选摘要、无筛选时写默认说明。
- 搜索支持 `/` 聚焦 + 防抖 300ms + Enter 兜底；空态区分无数据（不引导清除筛选）与筛选无结果（带「清除筛选」）。
- 禁止提供“当前筛选全部”批量；批量操作必须仅接收用户逐项显式勾选且仍在当前授权结果中的稳定 ID，并受服务端保守数量上限约束。批量执行前必须冻结本次显式选择的稳定 ID、对象版本与授权状态；不得以当前筛选条件代替显式选择集。
- 结果未知项只允许查询最终结果，禁止直接纳入批量重试。
- 已确认项默认不参与批量重试；服务端执行时逐项跳过状态已变化、权限收回或版本不匹配项。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 查看销售单 | 列表 / 对象中心 | 有 W05 对象权限 | 无 | 聚焦 W05 同一销售单并定位“协同” | 无权限时保留投影摘要，不泄露商业字段 |
| 查询最终结果 | 发送超时 / 结果未知 | `QUERY_RESULT`；存在可查询原请求 | 无 | 更新明确已确认、明确失败或仍未知 | 仍未知停留当前项并提供 W29 入口 |
| 重试投递 | 失败项 | `RETRY`；错误可重试、没有并发发送、版本仍有效 | 展示销售版本、商城和原投递身份 | 沿原投影修订继续投递，固定显示操作编号 | 超时先查询；不得生成新投影修订 |
| 批量查询结果 | 选择栏 | `BULK_QUERY`；仅逐项显式选择且服务端数量上限内；禁止“当前筛选全部” | `BatchImpactPreview` | 后台任务逐项查询并汇总 | 显示成功/仍未知/跳过/失败；请求含全筛选选择时服务端必须拒绝 |
| 批量重试 | 选择栏 | `BULK_RETRY`；仅逐项显式选择、明确可重试且服务端数量上限内；禁止结果未知项与“当前筛选全部” | 预览状态、商城和数量 | 后台任务沿各项原投递身份执行 | 逐项保留原因，不改变销售事实；请求含全筛选选择或结果未知项时服务端必须拒绝或跳过 |
| 升级到 W29 | 对象中心 | `ESCALATE`；已超自动上限或明确人工错误 | 说明责任队列与影响 | 按稳定身份创建或复用 W29 错误对象及 `work_item`，只返回入口 | 重复请求返回既有对象与任务；后续领域动作和完成均在 W29，W23 不提供已废止的“开始处理”能力 |
| 复制投影摘要 | 执行内容 | 有字段查看权限；外部引用展示策略已配置 | 无 | 复制服务端白名单字段；完整稳定引用只有策略显式允许时才进入结果 | 策略未配置时只可复制不含完整外部引用的摘要；被掩码字段不进入剪贴板 |

### 7.1 禁止的动作

- W23 不提供“新建投影”“编辑投影”“修改接收状态”“改销售版本”或“重新生成内容”。
- 投影由已生效销售版本自动形成。内容错误时返回 W05 走销售变更单，形成新的销售版本和新的投影修订。
- W23 的对象级重试使用原幂等键“ERP 销售单号 + ERP 销售单版本 + 目标商城”；需要人工重放时升级到 W29，由 W29 沿原幂等键和正式任务契约处理。
- 商城接收失败时，界面不得提供回退销售单、删除应收或删除投影的动作。
- 对账差异只能进入 W29 核对和追加处理记录，不能在 W23 选择一侧直接覆盖另一侧。
- `ESCALATE` 只登记升级证据，并由服务端按固定类型唯一创建 `work_item`；W23 不得提供已废止的“开始处理”、转交、关闭或完成任务动作。

## 8. 数据契约

### 8.1 列表查询

```ts
type ExecutionProjectionListQuery = {
  q?: string
  mallIds?: string[]
  deliveryStatuses?: string[]
  sources?: Array<"MIGRATION_BASELINE" | "ERP_SALES_REVISION">
  latencyBand?: "normal" | "near_sla" | "over_sla"
  reconciliationStatus?: string
  ownerScope?: string
  sort: string
  page: number
  pageSize: number
}

type ExecutionProjectionRow = {
  projectionId: string
  projectionRevisionId: string
  projectionRevisionNo: number
  projectionSource: "MIGRATION_BASELINE" | "ERP_SALES_REVISION"
  salesOrderId: string
  salesOrderNo: string
  salesOrderRevisionId: string
  salesOrderRevisionNo: number
  salesOrderCommercialStatus: string
  customerLabel: string
  targetMallId: string
  targetMallName: string
  currentAckedRevisionNo?: number
  delivery: {
    deliveryId: string
    status: string
    attemptCount: number
    lastAttemptAt?: string
    nextAttemptAt?: string
    mallAckAt?: string
    errorSummary?: string
  }
  reconciliationStatus?: string
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
}

type ExecutionProjectionListResult = {
  rows: ExecutionProjectionRow[]
  pageInfo: { page: number; pageSize: number; total: number }
  metrics: ExecutionProjectionMetricView[]
  permissionVersion: string
  sourceFactsAsOf: string
  projectionUpdatedAt: string
  deliveryStatusUpdatedAt: string
  queriedAt: string
}
```

列表总数、指标和行数据必须使用同一权限/数据范围版本与查询更新时间。前端不得用当前页行数计算待确认或失败总量。

### 8.2 对象中心查询

```ts
type ExecutionProjectionView = {
  identity: {
    projectionId: string
    salesOrderId: string
    salesOrderNo: string
    targetMallId: string
    targetMallName: string
  }
  selectedRevision: {
    projectionRevisionId: string
    revisionNo: number
    projectionSource: "MIGRATION_BASELINE" | "ERP_SALES_REVISION"
    salesOrderRevisionId: string
    salesOrderRevisionNo: number
    customerExternalIdentity: string
    voucherCategoryExternalIdentity: string
    voucherExpiryAt: string
    faceValue: string
    cardCount: string
    cardForm: string
    effectiveAt: string
  }
  currentAckedRevisionId?: string
  revisionLinks: Array<{
    salesOrderRevisionId: string
    projectionRevisionId: string
    deliveryStatus: string
    mallAckAt?: string
  }>
  deliveries: Array<{
    deliveryId: string
    status: string
    attemptCount: number
    lastAttemptAt?: string
    nextAttemptAt?: string
    mallAckAt?: string
    mallExecutionBaseline?: string
    errorCode?: string
    errorSummary?: string
    workItemId?: string
    errorTaskId?: string
  }>
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
  fieldPermissions: Record<string, "full" | "masked" | "hidden">
  objectVersion: string
  sourceFactsAsOf: string
  projectionUpdatedAt: string
  deliveryStatusUpdatedAt: string
  queriedAt: string
}
```

### 8.3 提交与后台任务

```ts
type ProjectionDeliveryCommand = {
  projectionId: string
  projectionRevisionId: string
  deliveryId: string
  action: "QUERY_RESULT" | "RETRY" | "ESCALATE"
  expectedObjectVersion: string
  requestId: string
}

type ProjectionDeliveryResult = {
  operationId: string
  deliveryId: string
  result: "ACKED" | "FAILED" | "STILL_UNKNOWN" | "RETRY_SCHEDULED" | "ESCALATED"
  workItemId?: string
  errorTaskId?: string
  occurredAt: string
  nextAction?: string
}
```

- `RETRY` 只引用既有投影修订和投递，不提交任何投影业务字段。
- `QUERY_RESULT`、`RETRY`、`ESCALATE` 都是投递对象级命令，不携带任务完成决定或任务处理上下文。
- `ESCALATE` 使用稳定升级身份创建或复用 W29 错误对象，并由服务端从已注册类型中选择 `INTEGRATION_RESULT_UNKNOWN` 或 `BUSINESS_EXCEPTION` 创建正式 `work_item`；错误对象、升级证据与任务引用同一事务落地。W23 只读取返回的 `workItemId` / `errorTaskId` 并打开 W29。
- 批量动作必须只冻结本次显式选择的稳定 ID、对象版本与授权状态，再以唯一 `requestId` 注册 `background_job`；请求禁止表达“当前筛选全部”。不建设全筛选批量选择能力。逐项结果可为成功、仍未知、跳过或失败；结果未知项仅允许查询，禁止直接重试。
- 请求超时后按 `requestId` / `operationId` 查询结果；前端不自行判定成功。
- 投递是后台过程；页面刷新不得新建投递或推进状态。

### 8.4 前端边界

- 前端只做执行字段格式化和服务端状态文案映射，不从销售单重新组装投影。
- 不比较金额、税率或应收，也不把 W05 当前字段拼进历史投影。
- “商城版本落后 N 版”、等待时长、SLA 分类、下次动作时间与自动重试上限必须由服务端按错误类别配置后返回；前端可展示，禁止本地推算正式状态、超时或转人工时点。
- 原始请求/响应只允许展示脱敏摘要；任何密钥、卡号、卡密和完整客户外部凭据都不进入浏览器。
- `mallExecutionBaseline` 与其它外部稳定引用：策略未配置时必须只返回短引用且禁止复制完整值；配置后仅向服务端字段策略明确授权的排障角色开放完整值。

### 8.5 缓存、新鲜度与失效

- 列表 Query Key 包含用户、角色、权限/数据范围版本和全部筛选；对象 Key 包含 `projectionId`、选中修订和可见分区。
- 页面同时显示销售来源事实、投影生成和投递状态三个更新时间；任一数据陈旧时只标记对应分区，不用新投递状态覆盖旧投影版本。
- 查询原结果、重试或后台批量任务有确定结果后，按返回的投影/投递 ID 定向失效；结果未知期间保留旧值并标记风险。
- W05 当前销售版本变化只触发服务端生成新投影；前端不得从 W05 缓存拼装或直接写入 W23 缓存。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 按列表 / 对象中心成稿结构显示 Skeleton | 应用壳可用 | 查询完成原位替换 |
| 刷新 | 保留旧状态轨和历史，显示轻量刷新 | 阅读、打开 W05；正式动作前重验 | 成功更新时间，失败保留缓存 |
| 空数据 | “当前没有执行投影” | 返回销售单或清除范围 | 销售版本生效后自动出现 |
| 筛选无结果 | 展示当前状态/商城筛选摘要 | 清除筛选 | 恢复默认视图 |
| 无数据范围 | 不显示全局 0 指标 | 查看角色 / 申请范围 | 权限更新后重查 |
| 查询失败 | 有缓存保留并标陈旧；无缓存显示失败态 | 重试 | 查询恢复 |
| 数据陈旧 | 展示投递更新时间和最后查询时间 | 刷新、查询最终结果 | 服务端状态追平 |
| 字段级隐藏 / 引用策略未配置 | 客户和外部引用按策略掩码；未配置时一律短引用 | 其它有权动作可用；完整引用复制禁用 | 权限或策略更新后重查 |
| 待发送 / 发送中 | 状态轨显示当前阶段和下次检查时间 | 查看销售单；禁止重复重试 | 后台推进 |
| 自动重试中 | 保留原失败原因、尝试次数与下次时间 | 按权限查询结果 | 自动成功或转人工 |
| 投递失败 | 销售事实轨仍显示已生效；投递轨失败 | 对象级重试、升级到 W29 | 原幂等投递成功，或在 W29 处理正式任务 |
| 结果未知 | 不显示成功、不移动到已确认筛选 | 对象级查询最终结果、升级到 W29 | 明确结果后更新，或在 W29 处理正式任务 |
| 已升级人工 | 显示只读 `workItemId`、W29 错误对象和责任队列 | 打开 W29；W23 不得提供已废止的“开始处理”或完成 | W29 返回后刷新投递明确结果 |
| 商城已确认 | 固定展示确认时间和商城执行基线 | 查看 W05 / 历史 | 新销售版本形成下一投影 |
| 版本差异 | 对应关系区显示 ERP 与商城版本，不自动选边 | 打开 W29 差异任务 | 人工核对闭环 |
| 后台批量任务 | 显示进度与逐项结果 | 查看、下载合规结果 | 原任务继续查询 |
| 权限收回 | 清除客户和错误摘要，切无权限态 | 返回有权模块 | 恢复后重查 |

## 10. 响应式、键盘与无障碍

| 视口 | 布局变化 | 保留内容 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 列表 6–8 行；对象中心三轨和版本对应同屏 | 销售单、接收状态、商城确认版、失败原因、主动作（版本收敛到详情） | 无 |
| 1280×800 | 次要列进入列设置；详情右区变窄 | 身份、三版本、状态、错误摘要 | 负责人、最近尝试时间可隐藏 |
| 1024×768 | 图标侧栏；详情覆盖；工具栏换行 | 三段状态轨、白名单字段、查询结果动作 | 完整历史折叠 |
| 768×1024 | 导航抽屉；表格横滚；对象中心单列 | 销售单身份与操作列固定；失败与结果未知文案 | 版本对应改卡片；筛选进面板 |
| 375×812 | 紧凑只读卡片 | 销售单、投影版、商城状态、失败入口 | 不提供批量、重试和复杂治理；可查询简单结果或进入 W29 |

- `/` 聚焦列表搜索，方向键移动，Enter 打开详情预览。
- 三段状态轨提供可读文本和阶段位置；读屏器不得只听到颜色或图标名称。
- 查询/重试结果使用固定 `FormalActionResult` 和 `aria-live=polite`；结果未知使用 `role=alert`，但不反复播报轮询状态。
- 从 W05 返回后焦点恢复到“查看投影历史”触发源；从 W29 返回后聚焦原错误区。
- 确认框关闭后焦点返回原按钮；批量任务启动后焦点落到任务结果标题。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 今日工作台 / 待办 | W01 | 正式错误待办只携带 `workItemId` 打开 W29，不直接进入 W23 | W29 完成或转交后回 W01 原任务位置 |
| 销售单统一中心 | W05 | `salesOrderId`、`salesOrderRevisionId`、`section=collaboration` | W05 是单单据主入口；返回保留选中投影 |
| 商品发布 | W22 | 无直接内容写入；仅目标商城协同状态 | 返回保留原投影筛选 |
| T 切换 | 商城停止建单、ERP 全面服务 | 切换时点当前版本（第一份投影修订） | 切换时点版本只读；不在 W23 修改 |
| 商城消费订单 | W25 | 原销售单身份；消费不按投影版本归集 | 返回保留消费订单上下文 |
| 接口错误与对账 | W29 | `deliveryId`、`errorTaskId`、对账差异、原幂等键 | 处理后回 W23 刷新明确结果 |

W23 与 W05 的边界固定：W05 展示一张销售单的当前协同全貌，W23 展示跨销售单的投递运营面。两处读取同一投影事实，不形成两套写入口。

W23 与 W29 的任务边界固定：W23 只执行 `QUERY_RESULT`、`RETRY`、`ESCALATE` 三类投递对象动作；`ESCALATE` 可以创建待办，W01 只负责入口路由，领域动作和强类型完成统一由 W29 的任务处理器承接；W23 不得提供已废止的“开始处理”能力。

## 12. 验收清单

### 12.1 定位与页面

- [x] 用户在 W05 协同子区即可看懂单张销售单当前投影和商城接收状态，无需先进入 W23。
- [x] W23 能一次筛选出结果未知、失败、转人工、已超时和版本差异项。
- [x] 1440×900 下列表露出 6–8 条有效数据行，销售单身份列和操作列固定。
- [x] 对象中心一屏区分销售事实、投影投递和商城确认三条状态轨。
- [x] 历史投影始终显示其来源销售版本，不被 W05 当前版本覆盖。

### 12.2 非第二写者边界

- [x] W23 没有新建、编辑、删除投影或修改商城确认状态的入口。
- [x] 投影字段只来自已生效销售版本的服务端投影修订，前端不重新组装。
- [x] 成交金额、配赠、税率、开票、应收和玩法规则在任何角色下都不进入投影内容。
- [ ] 内容变化必须在 W05 走销售变更单，形成新销售版本后自动产生新投影。
- [x] 接收失败不会回退销售版本、应收或旧版已完成执行事实。

### 12.3 重试、异常与后台任务

- [ ] 一个 ERP 销售版本与目标商城只存在一份投影修订。
- [ ] 自动、人工和批量重试均使用原投递身份，不产生新投影修订。
- [x] 结果未知先查询，未明确前不显示成功、不跳过、不进入已确认统计。
- [x] 批量动作禁止“当前筛选全部”，仅冻结显式选择快照，逐项重验权限、状态和版本；结果未知项只查询、不直接重试。
- [x] 对账差异只创建 / 打开 W29 任务，不覆盖 ERP 或商城事实。
- [x] 正式结果固定显示操作编号、对象、时间和下一步，不只用 toast。
- [x] W01 错误待办统一打开 W29；W23 不得提供已废止的“开始处理”或完成。
- [x] `ESCALATE` 重复请求返回既有或新建的 `workItemId` / `errorTaskId`，后续处理只在 W29 发生。
- [x] SLA 状态、超时分级与下次动作时间由服务端按错误类别返回；前端不本地推算转人工时点。

### 12.4 权限、状态与响应式

- [ ] 销售只能看到其负责/协作客户范围，运营和运维按商城与组织范围查看。
- [ ] 业务拒绝由运营查询或升级；鉴权/连接由运维查询或升级；映射差异禁止对象级重试，必须升级 W29。
- [ ] 权限收回后不残留客户外部身份、错误摘要或缓存的敏感字段。
- [ ] 外部引用策略未配置时只显示短引用且不可复制完整值；完整值仅对策略授权排障角色开放。
- [ ] 默认列表聚焦近 90 天或未闭环项；全历史仍可追溯查询。
- [ ] §9 状态均完成组件或浏览器验收。
- [ ] 1440、1280、1024、768、375 五档视口符合 §10。
- [ ] 键盘可完成搜索、预览、打开 W05、查询结果和读取批量任务结果。

## 13. 业务依据

- `erp-phase-2.md` §8.1–§8.4：销售审批生效、执行投影字段、接收阻断、变更和商城执行边界。
- `erp-phase-2.md` §13：投影幂等、消息可靠性、版本对账与监控指标。
- `erp-phase-2.md` §15 P2-P04、§16、§17.2：W23 落点、角色职责和验收场景。
- `erp-data-model.md` §6.16：稳定投影、不可变修订、投递字段、一对一版本关系和禁止字段。
- `erp-data-model.md` §7.7、§8.4、§9.4：投递状态、结果未知、事务不变量和对账边界。
- `erp-mall-data-mapping.md` §3.6、§10.3：原投递身份、投影白名单与商城确认。
- `erp-ui-design.md` §3.4–§3.5、§4.3、§4.5、§6、§9–§11、§15：任务页签、响应式、M2/M4、二期协同与通用状态。
- `erp-ui-flows.md` §10.2：销售单协同子区优先呈现，W23 处理跨单批量失败。
