# W27 · API 供应商结算

> 状态：已定稿
> 页面模式：M2 高密度查询列表 + M4 对象中心
> 主要路由：`/supplier-api/settlements`、`/supplier-api/settlements/:statementId`
> 主要角色：财务；采购协同，管理层只读
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

- 财务按供应商和结算期间核对已完成、已取消、已退款的 API 供应商订单，清楚看到 ERP 计算金额、供应商账单金额和差异。
- 在同一结算单中心逐项处理差异、完成复核并确认结算，不需要导出后在线下改完再覆盖系统结果。
- 结算确认后能直接查看形成的应付，并进入 W12 完成付款、进项发票和核销。

### 1.2 业务目标

- 从供应商订单完成、取消、供应商退款等不可变历史事实及结算草稿冻结的来源快照汇总最终结算价、运费、服务费和退款，形成可审计周期结算单；W26 当前投影只辅助阅读和导航，不能成为正式取数依据。
- 差异处理只追加处理结论和成本差额，不修改供应商订单、原成本或供应商账单原值。
- 结算确认在同一事务追加最终成本差额、形成供应商结算单应付并锁定正式结果。
- 坚持经办与复核岗位分离；同一人不能准备并复核同一结算单。

### 1.3 不在本工作面完成

- 不创建或修改供应商订单、取消、退款和履约结果；进入 W26 查看原订单。
- 不维护供应商连接、商品映射和供给价格；分别进入 W20、W21。
- 不在结算单直接登记付款或进项发票；确认后的应付进入 W12 同一核销内核。
- 不用结算差异覆盖历史订单成本；差额必须形成追加 `cost_entry`。
- 不处理商城消费金额或支付分摊差异；这些进入 W25 或 W29。

## 2. 用户、权限与数据范围

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 财务经办 | 待对账 | 授权供应商和账期 | 结算期间策略已配置时建立/刷新草稿并提交复核；否则只读核对与补充证据 |
| 财务复核 | 待复核 | 授权供应商和账期 | 复核、驳回、确认结算；不得是本单经办人 |
| 采购 | 差异协同 | 负责供应商的业务差异 | 查看订单与差异、提供供应商确认依据；不能确认结算 |
| 财务负责人 / 管理层 | 汇总与异常 | 授权组织范围 | 只读结算进度、差异金额和应付衔接 |
| 系统管理员 | 任务失败入口 | 授权接口范围 | 处理账单同步和后台任务异常；不能代替财务复核 |

补充规则：

- 无模块权限隐藏入口；无数据范围显示专用空态，不以 0 单据伪装。
- 服务端按供应商、组织、会计责任范围和角色过滤；前端不得加载后裁剪。
- 订单级成本、差异原因、发票和付款入口分别受字段权限控制。
- `preparedBy` 与 `reviewedBy` 必须由服务端做岗位分离校验；前端禁用态只用于解释。
- 采购只能追加供应商证据和业务意见，不能选择正式差异结论或改变成本基线；正式差异结论由财务经办登记，最终结算由另一名财务复核确认。
- 已确认结算单永久只读；后续发现问题走差额、退款、冲正或新结算处理，不开放“重新编辑”。
- 页面期间内权限被收回时，清除成本、账单附件和应付缓存，切换无权限态。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 浏览结算 | 侧栏“API 供应商结算” | `/supplier-api/settlements?view=pending` | 恢复筛选、分页和滚动位置 |
| 财务待办 | W01 / W02 `SUPPLIER_SETTLEMENT_REVIEW` | 打开对象中心，只携带 `workItemId`、`statementId`和 `queueContextId`；目标页重新查询当前主题版本和领取状态 | 返回待办队列原位置 |
| 复核任务领取 | 复核记录 Tab | 财务复核且非经办人；任务未领取（`CLAIM_REVIEW`） | 「领取任务」按钮 | 领取后显示领取人，可确认/驳回 | 领取失败保留待领取态 |
| 从供应商订单钻取 | W26 成本与结算 | 聚焦已有结算页签或新建 | 返回 W26 原订单 |
| 从供应商往来钻取 | W12 应付来源 | 打开已确认结算单只读中心 | 返回 W12 原应付会话 |
| 列表核对 | W27 单击行 / Enter | 打开 `detail` 半屏，URL 增加 `preview={id}` | 关闭焦点回原行 |
| 打开中心 | detail / 行操作 | `/supplier-api/settlements/:statementId` | 返回列表筛选不丢失 |

TaskTab 身份为 `supplier-settlement:{statementId}`。相同结算单重复打开只聚焦。当前子区和差异筛选进入 URL；未提交说明属于脏状态，关闭时确认。刷新后恢复结算单、当前子区和正式差异筛选，不恢复打开中的附件预览或确认对话框。

## 4. 页面布局

### 4.1 桌面布局

```text
┌ PageHeader：API 供应商结算                  [数据水位] [新建结算草稿]
├ MetricStrip：待处理 | 有差异 | 待复核 | 已确认金额
├ ListToolbar：视图 | 供应商 | 结算期间 | 状态 | 差异类型 | 搜索
├ BusinessTableFrame
│ 结算单号 | 供应商 | 期间 | ERP 金额 | 账单金额 | 差异 | 状态 | 经办/复核 | 操作
│ 差异侧栏项展示类型、状态、待举证/阻断与差异金额
└ 分页

对象中心
┌ PageHeader object-chrome：API 供应商结算 › 结算单号        [返回] ─┐
├ DocumentHeader compact：供应商 [状态] · 结算单号 · 期间  [提交复核/确认]
├ 金额摘要：订单 | 运费 | 服务费 | 退款 | ERP金额 | 供应商金额 | 差异
├ 概览 | 结算明细 | 差异处理 | 复核记录 | 应付与票款 | 审计
├ 明细 / 差异工作区
└ FormalActionResult：确认编号、应付编号、成本调整结果与下一步
```

### 4.2 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 页头与指标 | 识别结算水位和待处理规模 | `PageHeader` `MetricStrip` `DataFreshness` | 页头固定 |
| 结算列表 | 按供应商和账期扫读结算状态 | `DataTable` `BusinessTableFrame` | 单号左固定、操作右固定 |
| detail 预览 | 阅读金额、差异和复核摘要 | `QuickPreviewSheet size="detail"` | 浮层 |
| 对象头与汇总 | 锁定供应商、期间和金额口径 | `PageHeader object-chrome` + `DocumentHeader density="compact"` `DocumentTotals` | 中心滚动时吸顶 |
| 明细区 | 核对供应商订单、费用、退款 | `DataTable` | 表头吸顶 |
| 差异区 | 在同一详情完成分类、证据和处理 | `BusinessDiffPanel`（列头/计数用新 prop） | 有差异时默认打开；选中差异写入 `diff=` URL 参数，刷新/分享不丢上下文 |
| 正式结果 | 固定呈现成本调整和应付结果 | `FormalActionResult` | 动作后保持可见 |

## 5. 展示内容与字段

### 5.1 列表与结算头

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 身份 | `statementNo` | 结算单号 | `supplier_settlement_statement` | 正式稳定编号 | 有对象权限可打开 |
| 主体 | `supplierName` | 供应商 | 供应商结算快照 | 不受后续名称变化影响 | 按供应商范围 |
| 期间 | `periodStart/end` | 结算期间 | 结算单 | 半开/闭区间展示规则由接口固定；工具栏提供期间自/至筛选控件（`periodFrom`/`periodTo`），筛选摘要含期间口径 | 财务、采购可见 |
| 金额 | `erpAmount` | ERP 计算金额（含税） | 结算明细汇总 | 订单 + 运费 + 服务费 − 供应商退款 | 成本权限 |
| 金额 | `supplierAmount` | 供应商账单金额（含税） | 外部账单版本 | 缺失时显示「账单未同步 · 刷新试算后以 ERP 金额预填」；仅当供应商商务资料明确「无账单」且已登记人工对账证据时，允许以 ERP 计算额作为确认口径；否则禁止确认结算 | 成本权限 |
| 差异 | `differenceAmount` | 差异金额（含税） | 服务端汇总 | `supplierAmount - erpAmount`，并展示方向文案；差异金额与侧栏差异项金额视觉强调（琥珀色加粗 + 方向） | 成本权限 |
| 状态 | `status` | 草稿 / 待对账 / 有差异 / 待复核 / 已确认 / 已作废 | 结算单状态 | 固定状态映射 | 对象权限 |
| 岗位 | `preparedBy/reviewedBy` | 经办 / 复核 | 正式审计 | 未分配时明确“待复核人” | 仅展示必要姓名 |
| 复核记录 | `actionLabel`/`reasonCode` | 提交/驳回/确认 | 追加式复核记录 | 原因码使用中文映射（证据不足/金额仍不一致等） | 对象权限 |
| 审计 | `auditAction`/`auditNo` | 动作中文名 / 审计号 | 审计事件 | 动作码走 `AUDIT_ACTION_LABEL` 中文映射；审计号为 `AUD-*` 业务流水号，不含工作面编号；不展示 `ssh_*`/`sh_*` 哈希，统一为「数据版本 vN」 | 对象权限 |
| 结果 | `payableNo` | 形成应付 | `payable_account` | 仅确认后出现 | 有 W12 权限可钻取 |

### 5.2 结算明细与差异

| 区域 | 字段 | 用户文案 | 事实来源 | 规则 |
| --- | --- | --- | --- | --- |
| 订单明细 | 供应商订单号、外部单号、商品、数量、完成/取消/供应商退款事实 | 结算订单 | `supplier_settlement_item` 冻结快照 + `supplier_order_status_history` / `supplier_refund_fact` 等不可变历史 | 明细表含「数量」列；完成、取消、退款均依据不可变事实及本次来源快照，不由 W26 当前状态或查询投影猜金额 |
| 金额构成 | 订单结算价、运费、服务费、退款、ERP 计算额 | ERP 金额构成 | 结算明细 | 含税与不含税列分开，不混加 |
| 供应商账单 | 外部账单号、版本、明细金额 | 供应商账单 | 账单同步 | 原值只读，不允许为“消差”直接改写；版本展示为「第 N 版」 |
| 差异 | 漏单、重复、金额、退款、状态等 | 差异类型 | `supplier_settlement_difference` | 每条差异显示左右证据和金额方向 |
| 差异状态 | 待处理 / 供应商认可 / ERP 认可 / 已补偿 / 关闭 | 差异结论状态 | `supplier_settlement_difference.status` | 5 值固定枚举、单轨，结论即状态；“待举证/阻塞”不占状态值 |
| 处理 | 供应商认可、ERP 认可、已补偿、关闭（无需调整） | 处理结论 | 追加式处理记录（`resolution` / `resolved_by` / `resolved_at`） | 需原因、证据、处理人和时间；默认组合为 ERP 认可 + 接受供应商账单（语义一致）；弹窗明示「结论一经登记不可撤回」；原因码展示中文映射 |
| 成本 | 调整前累计、结算确认额、待追加差额 | 成本影响 | 服务端试算 | 确认前为预览，确认后引用正式 `cost_entry` |
| 应付 | 应付含税金额、到期日、核销状态 | 结算应付 | `payable_account/entry` | 确认后生成，不能在 W27 编辑核销 |

“ERP 认可差异”不等于改写原供应商订单：它表示财务经办接受供应商账单金额，结算确认时以追加成本差额表达。关闭差异必须有“无需业务调整”的受控原因和证据。采购提供的书面确认、外部工单或业务说明只是证据，不会自行改变差异状态、结算金额或成本基线。

差异状态固定为 5 值单轨枚举：待处理、供应商认可、ERP 认可、已补偿、关闭，与 `docs/erp-data-model.md` §6.20 一致；`resolution`（`resolved_by` / `resolved_at`）是追加式处理记录，与状态并存不冲突。“待举证”（`requiresProcurementEvidence`）与“阻塞”（`blocking`）是标志而非状态：待举证差异在状态旁以“需采购举证”徽标展示，阻塞差异以“阻塞”徽标展示，二者均不占用状态值。

草稿刷新时服务端按 `sourceAsOf` 冻结本次纳入的不可变订单完成/取消/退款历史、外部账单版本及各明细金额，生成 `sourceSnapshotHash`。提交复核的 `subjectHash` 必须覆盖该来源快照、结算明细和差异结论；W26 的列表/详情投影即使延迟或重建，也不能改变已冻结试算或正式确认结果。

## 6. 搜索、筛选、排序与默认视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| Saved View | `pending` | `view=pending` | 默认含待对账、有差异、待复核；可切“我经办”“我复核”“已确认”；Tab 切换清除 `status`/`differenceType`，与指标卡行为一致 |
| 供应商 | 全部有权 | `supplierId=` | 服务端稳定 ID 过滤 |
| 结算期间 | 必须使用供应商版本化结算期间策略返回的时区与完整可选周期（当前及上一周期）；未配置时不预填 | `periodFrom=` / `periodTo=` | 必须按策略时区和边界筛选，禁止用创建时间代替；策略缺失或过期时列表仅可显式查询历史，禁止新建结算草稿 |
| 状态 | 待处理集合 | `status=` | 多选固定状态；筛选摘要中的视图/状态使用中文映射（如「我经办」「有差异」），不展示枚举原值 |
| 差异类型 | 全部 | `differenceType=` | 过滤包含该类未解决差异的结算单 |
| 搜索 | 空 | `q=` | 精确优先匹配结算单号、外部账单号、供应商名称 |
| 排序 | 无排序控件 | — | 列表不提供排序控件；顺序由服务端稳定返回，结算单 ID 为尾键 |
| 分页 | 50 条 | `page=`（`pageSize` 固定 50 不入 URL） | 分页以 URL 为唯一事实源，本地不持有分页副本，避免双写漂移 |
| 清除筛选 | — | — | 工具栏常驻「清除筛选」清全部筛选参数并回第 1 页，`view` 回 `pending`（保持原清除语义）；空态筛选无结果时同样提供 |

对象中心明细默认显示全部结算项；差异子区默认仅显示未解决。明细和差异分别使用 URL 子筛选，浏览器后退能恢复。批量导出按服务端冻结选择快照生成，不因期间内新订单自动扩大范围。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 新建结算草稿 | 列表页头 | 财务经办；服务端已返回该供应商有效的结算期间策略、时区、边界和版本；所选策略周期未被其他有效结算覆盖 | 展示供应商、期间（策略返回的自然月）；禁用时以 Tooltip 说明原因（如「仅财务经办可新建」「策略未配置」） | 按稳定 `requestId` 生成草稿、冻结来源快照并形成明细试算，打开对象中心；不展示策略内部 ID@版本与时区 | 策略缺失/过期返回 `PERIOD_POLICY_UNCONFIGURED` 或版本冲突，不创建草稿；结果未知查询原请求 |
| 刷新明细试算 | 草稿页头 | 财务经办；草稿/待对账，尚未提交复核 | 展示新增、移除、事实水位和金额变化影响 | 按稳定 `requestId` 更新可变草稿试算版本与 `sourceSnapshotHash`，不改原订单或事实 | 结果未知查询原请求；失败保留上次试算和水位，不新建刷新请求 |
| 提供采购协同证据 | 差异证据区 | 被指派采购；结算未确认 | 无金额确认；说明证据用途 | 只追加供应商证据或业务意见和审计，不改变差异结论、试算金额或成本基线 | 保存失败保留证据输入，按原请求重试 |
| 处理差异 | 差异行 | 财务经办；结算未确认；采购证据已按需齐备 | 选择受控结论，金额影响时展示预览 | 追加财务处理记录并刷新待确认成本差额 | 版本冲突保留输入，重载当前差异；结果未知查询原操作 |
| 提交复核 | 页头主动作 | 全部差异必须已有完整处理结论，且其金额影响已进入试算；未知金额差异一律阻断提交；明细金额核对一致 | `FormalActionConfirmDialog` 展示冻结来源快照时间、金额、差异及复核人；锁定字段不重复 | 按服务端 `sourceAsOf` 冻结提交版本与来源快照，创建唯一待办 | 校验失败定位差异/明细 |
| 驳回复核 | 复核区 | 当前复核人且指纹一致 | 原因必填（中文原因选择） | 退回经办并保留复核记录 | 失败停留，原因保留 |
| 确认结算 | 复核区主动作 | 财务复核；非经办人；无未解决阻断差异；指纹一致；外部账单已同步，或供应商商务资料明确「无账单」且已登记人工对账证据 | 展示最终成本差额、应付金额和不可逆影响；状态图为实际状态流转 | 同事务追加成本差额、形成应付、更新已确认；固定返回应付编号 | 超时进入结果不确定，按操作 ID 查询，不重复确认 |
| 查看/处理应付 | 结果区 | 已确认且有 W12 权限 | 无 | 打开 W12，预选结算单应付 | 无权限时保持编号只读 |
| 作废草稿 | 更多 | 草稿未形成正式事实、无在途复核 | 原因必填 | 草稿作废，保留审计 | 版本变化时重新校验 |
| 导出 | 列表/明细 | 授权范围和字段权限 | 预览范围、字段、遮罩 | 创建后台导出任务 | 失败报告可下载，正式事实不受影响 |

已确认结算不提供“撤回确认”或“编辑金额”。后续账单变化、新退款或发现差错使用追加差额和后续结算/纠错链处理，原确认结果保持可审计。

## 8. 数据契约

### 8.1 查询

```ts
type SupplierSettlementListQuery = {
  view: "pending" | "prepared_by_me" | "review_by_me" | "confirmed"
  supplierIds?: string[]
  periodFrom?: string
  periodTo?: string
  statuses?: string[]
  differenceTypes?: string[]
  q?: string
  sort: string
  page: number
  pageSize: number
}

type SupplierSettlementListRow = {
  statementId: string
  statementNo: string
  supplierId: string
  supplierName: string
  periodStart: string
  periodEnd: string
  status: string
  erpAmountGross: string
  supplierAmountGross?: string
  differenceAmountGross?: string
  unresolvedDifferenceCount: number
  updatedAt: string
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
}

type SupplierSettlementListResult = {
  rows: SupplierSettlementListRow[]
  pageInfo: { page: number; pageSize: number; total: number }
  totals: SettlementListTotalsView
  permissionVersion: string
  sourceAsOf: string
  queriedAt: string
  periodPolicy:
    | {
        state: "CONFIGURED"
        policyId: string
        policyVersion: string
        timezone: string
        selectablePeriods: Array<{ periodStart: string; periodEnd: string; label: string }>
      }
    | {
        state: "UNCONFIGURED"
        policyId?: never
        policyVersion?: never
        timezone?: never
        selectablePeriods?: never
        blocker: ActionBlocker
      }
}

type SupplierSettlementDetailQuery = {
  statementId: string
  workItemId?: string
  itemStatus?: string[]
  differenceStatuses?: string[]
  differenceTypes?: string[]
  itemPage: number
  itemPageSize: number
  differencePage: number
  differencePageSize: number
}

type SupplierSettlementDetailView = {
  statement: {
    id: string
    statementNo: string
    supplierId: string
    supplierName: string
    periodStart: string
    periodEnd: string
    externalBillNo?: string
    externalBillVersion?: string
    erpAmountGross: string
    supplierAmountGross?: string
    differenceAmountGross?: string
    status: string
    preparedBy?: ActorView
    reviewedBy?: ActorView
    lockVersion: number
    subjectHash?: string
    sourceAsOf: string
    sourceSnapshotAt: string
    sourceSnapshotHash: string
  }
  totals: SettlementTotalsView
  itemsPage: Page<SupplierSettlementItemView>
  differenceSummary: DifferenceSummaryView
  payable?: PayableLinkView
  workItem?: {
    workItemId: string
    workItemType: "SUPPLIER_SETTLEMENT_REVIEW"
    businessObjectType: "SUPPLIER_SETTLEMENT_STATEMENT"
    businessObjectId: string
    subjectVersion: string
    completionAction: string
    claimedBy?: ActorView
  }
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
  freshness: {
    immutableFactsAsOf: string
    externalBillAsOf?: string
    w26ProjectionUpdatedAt?: string
    queriedAt: string
  }
}
```

列表总数、金额指标和差异统计由服务端在同一数据水位与权限版本计算，前端不能用当前页求和。明细分页必须能定位具体供应商订单和结算项目；差异摘要和总额在分页外由服务端返回。

### 8.2 提交

```ts
type SettlementDraftCommand =
  | {
      action: "CREATE"
      supplierId: string
      periodStart: string
      periodEnd: string
      periodPolicyId: string
      expectedPeriodPolicyVersion: string
      statementId?: never
      expectedLockVersion?: never
      expectedSourceSnapshotHash?: never
      requestId: string
      idempotencyKey: string
    }
  | {
      action: "REFRESH"
      statementId: string
      expectedLockVersion: number
      expectedSourceSnapshotHash: string
      supplierId?: never
      periodStart?: never
      periodEnd?: never
      periodPolicyId?: never
      expectedPeriodPolicyVersion?: never
      requestId: string
      idempotencyKey: string
    }

type SettlementDifferenceEvidenceCommand = {
  statementId: string
  differenceId: string
  expectedDifferenceVersion: number
  evidenceReferenceIds: string[]
  opinionCode?: string
  comment?: string
  requestId: string
  idempotencyKey: string
}

type SettlementDifferenceCommand = {
  statementId: string
  differenceId: string
  expectedLockVersion: number
  expectedDifferenceVersion: number
  resolution:
    | "SUPPLIER_ACCEPTED"
    | "ERP_ACCEPTED"
    | "COMPENSATED"
    | "CLOSED_NO_ADJUSTMENT"
  reasonCode: string
  evidenceReferenceIds: string[]
  operationId: string
  idempotencyKey: string
}

type SettlementObjectCommand =
  | {
      action: "SUBMIT_REVIEW"
      statementId: string
      expectedLockVersion: number
      subjectHash: string
      refreshCutoffPolicyId: string
      expectedRefreshCutoffPolicyVersion: string
      operationId: string
      idempotencyKey: string
      reasonCode?: never
      comment?: string
    }
  | {
      action: "VOID_DRAFT"
      statementId: string
      expectedLockVersion: number
      subjectHash: string
      refreshCutoffPolicyId?: never
      expectedRefreshCutoffPolicyVersion?: never
      operationId: string
      idempotencyKey: string
      reasonCode: string
      comment?: string
    }

// 直接复用 W02 WorkItemActionCommand；expectedSubjectVersion 对应提交版本。
type SettlementReviewCommand = WorkItemActionCommand<{
  statementId: string
  expectedLockVersion: number
  action: "REJECT" | "CONFIRM"
  operationId: string
  reasonCode?: string
  comment?: string
}>
```

- `CREATE` 必须携带服务端当前供应商结算期间策略及版本，并严格选择其返回的完整周期（含时区与边界）；策略缺失、过期或客户端自行拼接期间时 fail-closed，禁止新建草稿。`REFRESH` 必须携带结算单、当前锁版本和上一次 `sourceSnapshotHash`，且不能改供应商或期间。两者从不可变履约/取消/供应商退款历史及外部账单版本生成冻结来源快照；结果未知时按原 `requestId` / 幂等键查询或续跑，不能创建第二张草稿或第二次刷新。
- `SettlementDifferenceEvidenceCommand` 只供采购或协同角色追加受控证据/意见，不改变差异状态、结算金额和成本。`SettlementDifferenceCommand` 只供财务经办登记正式结论，使用差异自身版本和追加处理记录，不修改左右证据或历史成本。
- 差异处理结果未知时先按 `operationId` / 幂等键查询同一操作；确认既有操作失败后也必须沿用原键恢复，不能重复追加处理结论。
- `SUBMIT_REVIEW` 按服务端 `sourceAsOf` 冻结来源快照、结算明细和差异结论，并在同一事务创建唯一 `SUPPLIER_SETTLEMENT_REVIEW`；复核任务唯一，不得重复创建。仅当全部差异均已有完整处理结论且金额影响已进入试算时允许提交；未知金额差异必须阻断提交，禁止“带未知金额差异”进入复核。
- `CONFIRM` 必须同时校验岗位分离、当前指纹、差异状态和供应商/期间重复覆盖约束。外部账单已同步时按账单与试算结果确认；仅当供应商商务资料明确「无账单」且已登记人工对账证据时，允许仅按 ERP 计算额确认；否则账单缺失必须阻断确认。
- `REJECT` 和 `CONFIRM` 只能使用独立的 `SettlementReviewCommand`，复用 W02 `WorkItemActionCommand`（`kind="WORK_ITEM_ACTION"`），携带 `workItemId`、`expectedSubjectVersion` 和 `decision`；服务端校验当前领取人、结算单版本及任务唯一 `completionAction`。正式结算状态变化与该 `work_item` 完成在同一事务，任一失败均不留下半完成；任务完成本身不能单独追加成本或形成应付。
- `SUBMIT_REVIEW` 与 `VOID_DRAFT` 使用独立 `SettlementObjectCommand`，不接受任务命令。普通对象入口即使读到已有复核任务，也必须先领取任务再用 `SettlementReviewCommand` 决策，不能绕过任务直接确认。
- 正式确认返回成本调整事实、应付账户、应付分录、确认时间和下一步，不只返回成功布尔值。
- 网络失败时先用 `operationId` 查询原结果；结果不确定期间不得重复提交。

### 8.3 前端边界

- 前端只格式化金额和差异方向，不计算 ERP 结算总额、应付或成本差额。
- 是否允许进入复核、差异是否阻断、岗位是否冲突、订单是否重复覆盖均以服务端结果为准。
- 供应商账单原值、订单事实、成本历史和应付事实不得由页面覆盖。
- 正式结算金额、`sourceSnapshotHash` 和 `subjectHash` 只取不可变供应商订单完成/取消/退款历史、外部账单版本和冻结结算明细；W26 当前投影只用于显示及钻取，投影延迟不能阻断或改变正式计算。
- 含税、不含税、税额使用服务端已舍入结果，禁止在浏览器用浮点重算。

### 8.4 缓存、新鲜度与失效

- 列表 Query Key 包含用户、角色、权限/数据范围版本、供应商、期间、状态、差异筛选、排序和分页。
- 对象 Query Key 包含 `statementId` 以及明细、差异各自的筛选与分页；两区失败或失效互不清空，汇总使用服务端统一 `sourceAsOf`。
- 草稿试算、差异处理、提交复核或确认结算成功后，按返回的结算单版本定向失效列表、对象、明细和差异；结果未知时不乐观改缓存。
- 确认形成应付后只保存 W12 稳定引用，W12 余额和核销状态到达目标页重新查询。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 列表或对象中心结构 Skeleton | 应用壳导航 | 原位替换 |
| 结算期间策略未配置 | 列表可按显式历史范围只读查询；新建入口显示 `PERIOD_POLICY_UNCONFIGURED` 并禁用 | 仅查看历史、进入供应商配置；禁止新建草稿 | 服务端返回时区、完整周期、边界和版本后才可新建 |
| 来源快照过期 | 草稿与证据可读，提示重新刷新明细试算 | 补充证据、刷新明细试算 | 重新试算后再提交复核 |
| 刷新 | 保留旧数据并显示来源水位 | 可查看；正式动作服务端重验 | 成功更新；失败保留旧值 |
| 空数据 | “当前范围没有结算单” | 期间策略已配置且有权限时新建草稿 | 选择供应商与策略返回的完整期间；策略缺失时先完成配置 |
| 筛选无结果 | 显示当前筛选摘要 | 清除筛选 | 回默认待处理视图 |
| 无数据范围 | 专用无范围空态 | 查看角色/申请权限 | 范围变化后重查 |
| 查询失败 | `BusinessFailureState`；有缓存时不清空 | 重试、返回来源 | 查询恢复 |
| 单据不存在（404） | 「结算单不存在或已作废」空态 + 返回列表，隐藏无效重试 | 返回列表或检查分享链接 | 选择有效单据 |
| 详情加载中 | 骨架 + 「正在加载结算单」提示 | 等待 | 查询完成 |
| 角色/演示切换 | 保留旧内容局部刷新（`keepPreviousData`），不整页骨架 | 继续阅读 | 查询完成 |
| 演示权限下拉（详情页） | 不渲染：详情查询只消费 statementId+role，demoFlag 控件对详情无效果 | — | — |
| 差异选中锚定 | 选中差异写入 `diff=` URL；刷新/分享不丢失 | 深链直达 | 重新选择 |
| 数据陈旧 | 标注账单/订单/投影各自水位 | 刷新；正式提交前强制重验 | 追平后解除 |
| 字段级隐藏 | 成本或应付值掩码，结构不跳动 | 其余授权动作 | 权限变化后重查 |
| 草稿取数中 | `BackgroundJobProgress` 显示订单扫描进度 | 查看已完成批次，不确认 | 任务完成或失败报告 |
| 草稿创建 / 刷新结果未知 | 保留原 `requestId`、上次快照和当前页面，不显示新试算为成功 | 查询原请求、沿原键续跑 | 取回唯一草稿/刷新结果；禁止另建请求 |
| 保存失败 | 保留差异结论和说明 | 重试保存 | 成功后更新版本 |
| 差异处理结果未知 | 不乐观改变差异状态或成本预览，固定显示原操作号 | 查询原操作 | 取回唯一追加记录或确认失败后沿原键恢复 |
| 校验失败 | 顶部摘要 + 具体明细/差异定位 | 修正后重提 | 所有阻断清除 |
| 动作门禁 | Alert 说明阻断原因（不展示错误码原值）；阻断按钮用 `GuardedBusinessAction` 展示原因 | 解除阻断后重试 | 阻断条件清除 |
| 版本冲突 | 显示新订单/退款/账单版本导致的变化 | 重新试算或放弃旧提交 | 新版本重新确认 |
| 正式动作成功 | 固定结果显示确认号、成本调整和应付编号 | 去 W12 / 留在本单 | 不依赖 toast |
| 正式动作结果不确定 | 本地状态不切换，显示“查询最终结果” | 查询原操作 | 得到终态后刷新 |
| 任务处理权 / 提交版本冲突 | 显示当前复核任务领取人或提交快照变化，保留阅读位置 | 刷新任务、重新领取；不能改走普通对象确认 | 当前任务、提交版本和岗位分离重新满足后再操作 |
| 权限收回 | 清除成本、附件和应付缓存 | 返回有权模块 | 权限恢复后重查 |

## 10. 响应式与键盘

| 视口 | 布局变化 | 保留内容 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 列表 + detail；对象中心金额摘要单行 | 单号、供应商、期间、三金额、状态、主动作 | 无 |
| 1280×800 | 汇总卡紧凑；表格允许横向滚动 | ERP/供应商/差异金额及口径 | 经办复核列并为一列 |
| 1024×768 | 图标侧栏；明细与差异区单列 | 差异证据、正式动作和岗位提示 | 次要时间移详情 |
| 768×1024 | 导航抽屉；固定单号和操作列；筛选入面板 | 金额汇总、未解决差异和状态 | 订单构成次要列默认隐藏 |
| 375×812 | 单列只读；允许复核阅读与简单驳回 | 结算身份、金额、差异、岗位分离提示 | 不提供新建、差异批量处理、确认结算、导出 |

键盘顺序：页头 → 指标 → 筛选 → 表格 → detail → 对象子导航 → 明细/差异 → 正式动作。表格 Enter 打开 detail，差异行可键盘展开左右证据。确认层关闭后焦点回原动作；正式确认成功后焦点落到结果标题。金额口径、差异方向、状态和岗位阻断均有文字与读屏标签。详情页提供快捷键提示：d 直达差异处理。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 供应商订单 | W26 | 供应商订单、结算明细、当前差异 | 返回原明细行和差异筛选 |
| 供应商往来 | W12 | 供应商、结算单应付、来源类型 | 完成付款/发票后回 W27 刷新核销摘要 |
| API 连接 | W20 | 供应商、连接、外部账单版本 | 返回保持结算页签 |
| 接口错误与对账 | W29 | 账单消息、同步任务、差异证据 | 解决后重新试算，不直接改结算事实 |
| 卡券经营分析 | W28 | 期间、供应商、成本调整来源 | 返回分析筛选不丢失 |
| 工作台 / 待办 | W01 / W02 | `workItemId`、`statementId`、`queueContextId`；不跨页传递 `subjectHash` 等可变事实 | 目标页重查主题版本与领取状态；处理完成后原任务刷新 |

## 12. 验收清单

### 12.1 业务闭环

- [x] 财务能在一个对象中心完成汇总核对、差异处理、提交复核和确认结算。
- [ ] 已完成、已取消和已退款订单均按正式事实进入结算，不由当前状态反推历史金额。
- [ ] 正式取数、金额和提交指纹只依赖不可变履约/取消/供应商退款历史、外部账单版本与冻结来源快照；W26 投影仅用于展示。
- [x] 未解决的阻断差异不能确认结算，处理结论均有证据和审计。
- [x] 采购只能追加供应商证据和意见；财务经办登记正式差异结论，另一名财务复核确认结算。
- [x] 经办和复核不能为同一人，前后端均有明确反馈。
- [x] 结算确认同事务追加成本差额并形成唯一应付，结果显示应付编号。
- [x] 确认后付款、进项发票和核销进入 W12，不在 W27 复制一套财务流程。

### 12.2 数据与正式动作

- [x] 供应商账单原值、订单、原成本和已确认结算均不可被页面覆盖。
- [x] ERP 金额、供应商金额和差异方向使用服务端舍入结果，含税/不含税标注清楚。
- [ ] 同一供应商、同一结算范围不会被两个已确认结算单重复覆盖。
- [x] 版本变化会使旧提交失效；不会静默确认过期试算。
- [ ] 结果不确定时查询原操作，不重复形成成本和应付。
- [ ] 新建/刷新草稿、差异处理均有独立命令和稳定请求/操作键；UNKNOWN 恢复不会生成第二草稿、重复刷新或重复差异结论。
- [x] 新建草稿必须引用供应商当前结算期间策略及版本；策略缺失、过期或期间不匹配时 fail-closed，不接受任意自然日范围。
- [ ] 提交复核按服务端 `sourceAsOf` 冻结来源快照，并创建唯一复核任务。
- [ ] `REJECT` / `CONFIRM` 只接受 W02 统一动作命令，校验正式 `work_item` 的当前领取人、对象版本和完成动作，并与业务状态变化同事务；对象级命令不含可选任务字段。

### 12.3 体验、权限和响应式

- [x] 1440×900 首屏显示至少 6 条结算单，身份和操作列固定。
- [x] 无模块权限、无数据范围、无结算单和筛选无结果可区分。
- [ ] 成本、账单附件和应付按字段权限控制，权限收回后无缓存泄漏。
- [ ] §9 状态和 §10 五档视口全部验收。
- [ ] 键盘可完成核对、打开差异、提交复核和查询正式结果。

## 13. 业务依据

- `erp-phase-2.md` §12.1–§12.2：成本逐步确认链、周期结算、差异和第一期应付衔接。
- `erp-phase-2.md` §13.3、§16、§17.5：对账范围、财务职责、岗位分离和结算验收。
- `erp-data-model.md` §6.9：供应商结算单作为应付和进项发票可核销来源。
- `erp-data-model.md` §6.20、§8.4：结算单/明细/差异字段、确认事务与追加成本差额。
- `erp-ui-design.md` §4.3、§4.5、§5.5、§11：M2/M4、对象中心、供应商往来和正式动作状态。
- `erp-ui-flows.md` §11.3：周期结算“汇总 → 差异确认 → 形成应付 → W12 核销”的完整路径。
