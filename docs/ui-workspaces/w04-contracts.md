# W04 · 合同

> 状态：草稿
> 页面模式：M2 高密度查询列表 + M4 对象中心
> 主要路由：`/sales/contracts`；对象路由 `/sales/contracts/:contractId`
> 主要角色：销售、销售领导；财务与历史单据参与者按权限只读
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

- 在高密度列表中找到某个客户的当前、将到期或历史合同。
- 通过 detail 半屏读完合同身份、客户、结算主体、付款条件、开票要求、有效期、附件和关联销售摘要。
- 需要作业时进入稳定合同对象中心，维护草稿、查看版本 diff、打开关联销售单；只有 `contractRevisionPolicy` 已配置且服务端明确允许时才能创建新修订。
- 从有效合同直接发起 W05 销售单，不再重新搜客户和合同。

### 1.2 业务目标

- 以 `contract` 稳定身份和不可变 `contract_revision` 管理合同当前内容及历史。
- 确保销售单选择具体合同版本，并保存当时结算、付款与开票快照；合同后续修订不改历史销售单。
- 一份合同可关联多张销售单，但合同不汇总成另一套应收、回款或履约账本。
- 将有效期、停用和将到期表达清楚，防止新销售单静默引用无效合同。

### 1.3 不在本工作面完成

- 不在 W04 创建客户主体或调整客户归属；进入 W03。
- 不在合同页修改已生效销售单、应收、回款或发票。
- 不从某份合同关联单据的当前金额反推“合同金额”；当前业务模型没有定义该正式事实。
- 不在 W04 新建可配置审批流。合同如何取得生效资格由业务规则确认，UI 只执行服务端返回的允许动作。
- 不在合同附件中长期保存未经安全检查的文件或把附件当作结构化条款的唯一来源。

## 2. 用户、权限与数据范围

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 负责销售 | 我的合同 | 当前负责客户合同与已参与历史单据所引用版本 | 新建、维护草稿、提交生效、从合同建销售单 |
| 协作销售 | 协作客户合同 | 有效协作期内可见；历史按参与权 | 按 `allowedActions` 查看或维护 |
| 销售领导 | 团队合同 | 本团队客户合同 | 查看、按授权生效/终止、建销售单 |
| 财务 | 销售单/往来深链 | 票款职责范围内合同结算与开票快照 | 只读核对，不修改销售条件 |
| 历史单据参与者 | 历史销售单合同链接 | 该历史单据所固定的合同版本快照 | 只读，不因当前客户归属变化而扩大范围 |

权限规则：

- 合同可见范围以客户归属和 `document_participant` 历史参与权为基础，由服务端过滤。
- 财务可见结算、付款与开票条件，但不因查看权自动获得合同编辑权。
- 附件列表与文件下载分别鉴权；下载使用短时链接并记录审计。
- 动作不以前端角色名决定，使用 `allowedActions` 和 `actionBlockers`。合同状态不允许某动作时，按钮可见禁用并说明原因。
- `contractRevisionPolicy` 未配置期间必须 fail-closed：服务端不得返回 `REVISE`，已生效合同只读查看，前端不得开放或通过深链绕过“创建新修订”入口。
- 页面打开期间权限收回时，清除未保存的敏感输入和附件下载 URL，对象身份与返回上下文保留。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 查询合同 | 侧栏“合同” | `/sales/contracts`，列表筛选写入 URL | 保留原任务页签 |
| 从客户中心进入 | W03 合同摘要 | `/sales/contracts?customerId=...`；打开某合同时聚焦对象页签 | 返 W03 原客户子区 |
| 单击列表行 / Enter | 合同列表 | 开启 detail 半屏，URL 可保留 `preview={contractId}` | 关闭后焦点回原行 |
| 打开合同中心 | detail 底栏、全局搜索 | `/sales/contracts/:contractId`，新建或聚焦页签 | 关闭后回来源页签 |
| 新建合同 | 列表或 W03 | 先建立服务端草稿身份，再以同一合同页签进入编辑态 | 放弃时返来源，未形成正式事实的草稿可逻辑删除 |
| 从合同建销售单 | 合同中心主动作 | W05 新任务页签，携带 `customerId`、`contractId`和当前 `contractRevisionId` | 销售单保存/提交后可回合同中心 |
| 刷新对象中心 | 任意合同子区 | 恢复稳定 ID 和 `section`；Dialog/Sheet 不恢复 | 当前合同 |

列表页签身份为 `contracts:list:{userId}`；合同对象页签身份为 `contract:{contractId}`，标题为 `合同 · {contractNo}`。编辑是同一对象页签内模式，不使用平行 `/edit` 路由。未保存编辑显示脏状态并拦截关闭。

## 4. 页面布局

### 4.1 M2 列表（1440×900）

```text
┌ PageHeader：合同                                  [新建合同] ─────┐
├ MetricStrip：有效 | 30天内到期 | 已到期 | 草稿                 ┤
├ ListToolbar：Saved View | 搜索 | 客户 | 状态 | 有效期 | 负责人 | 导出 ┤
├──────────────────────────────────────────────────────────────┤
│ 合同编号↑ | 客户 | 结算主体 | 有效期 | 状态 | 版本 | 销售单 | 负责人 | 操作 │
│ 36px 紧凑数据行，1440×900 首屏至少 6–8 条                        │
└──────────────────────────────────────────────────────────────┘
```

- 合同编号和行级主动作列固定。
- 单击行空白或 Enter 打开 detail 半屏，不离开列表；明确“打开中心”才创建对象页签。
- 指标卡全部是筛选按钮，使用 `aria-pressed`、选中态和当前筛选摘要。

### 4.2 detail 半屏预览

```text
┌ 合同 HT-2026-0088 [生效] v3 · 客户东方企业 ─────────┐
├───────────────────┬──────────────────────────────────────┤
│ 左 38%              │ 右主阅读区                            │
│ · 签订/有效期      │ · 结算主体与付款条件                 │
│ · 客户与负责人     │ · 开票要求结构化摘要                 │
│ · 附件数与版本       │ · 关联销售单和当前业务进度           │
├───────────────────┴──────────────────────────────────────┤
│ [关闭] [纸质预览]                         [打开合同中心] │
└──────────────────────────────────────────────────────────┘
```

detail 可读完合同主事实，但不负责编辑条款、版本 diff 或终止作业。纸质阅读使用宽 Dialog + `PaperDocument`，不把完整纸质投影塞进半屏。

### 4.3 M4 合同对象中心

```text
┌ DocumentHeader：HT-2026-0088 [生效] v3  东方企业 ─────────┐
│ 2026-01-01 至 2027-12-31   [新建销售单] [打印预览] [更多] │
├ 锚点：概览 | 结算与开票 | 附件 | 关联销售单 | 版本与审计         ┤
├──────────────────────────────────────────────────────────────┤
│ 客户、结算主体、签订日、有效期、负责人、当前版本                │
│ 付款条件快照 · 开票要求快照 · 结构化条款摘要                         │
│ 合同附件与文件安全状态                                      │
│ 关联销售单、所用合同版本、主状态和金额摘要                       │
│ RevisionTimeline + BusinessDiffPanel + AuditTimeline               │
└──────────────────────────────────────────────────────────────┘
```

### 4.4 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 列表页头/工具栏 | 新建、定位和导出授权合同 | `PageHeader` `ListToolbar` | 顶部 |
| 合同表格 | 高密度查询与扫描 | `BusinessTableFrame` `DataTable` | 表头 sticky |
| 半屏预览 | 列表内读主事实 | `QuickPreviewSheet size="detail"` | 覆盖右侧 |
| 对象头/锚点 | 合同身份、状态、版本和子区导航 | `DocumentHeader` | sticky |
| 结算与开票 | 展示当前结构化约定 | `DocumentSummary` | 否 |
| 附件 | 查看、下载和追溯合同文件 | 附件列表 | 否 |
| 关联销售单 | 追溯哪张单使用哪个合同版本 | `RelatedDocumentList` | 否 |
| 版本与审计 | 对比修订内容和正式动作 | `RevisionTimeline` `BusinessDiffPanel` `AuditTimeline` | 否 |

## 5. 展示内容与字段

### 5.1 列表与身份

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 列表 | `contractNo` | 合同编号 | `contract.contract_no` | 稳定、唯一 | 合同可见则可见 |
| 列表 | `customer` | 客户 | 当前客户修订查询 | 列表显当前名称；历史版本显快照 | 按客户范围 |
| 列表 | `settlementParty` | 结算主体 | 当前 `contract_revision` | 显示快照名称 | 销售/财务可见 |
| 列表 | `status` | 草稿 / 生效 / 终止 / 到期 | `contract.status` | 文字 + tone，不在前端自定义状态 | 全部可见 |
| 列表 | `validFrom/validTo` | 有效期 | 当前 `contract_revision` | 业务日期，将到期加文字提示 | 全部可见 |
| 列表 | `revisionNo` | 当前版本 | `contract.current_revision_id` | `v{n}` | 可见合同均可见 |
| 列表 | `salesOrderCount` | 关联销售单 | 服务端关联聚合 | 数量不从当前页求和 | 只统计用户有权单据 |
| 列表 | `owner` | 负责销售 | 客户当前归属/合同参与人投影 | 必须标明是当前客户负责人还是合同历史参与人 | 按组织权限 |

### 5.2 合同中心

| 子区 | 字段 | 数据来源 | 展示规则 | 权限规则 |
| --- | --- | --- | --- | --- |
| 概览 | 合同编号、客户、状态、版本、签订日、有效期 | `contract` + 当前 `contract_revision` | 日期按业务时区；状态由服务端返回 | 合同可见即显示身份；客户按数据范围 |
| 结算与开票 | 结算主体、付款条件、开票要求 | 当前合同修订的结构化快照 | 不只展示一个附件名；销售单使用时再固定具体版本 | 销售/财务字段权限分别控制 |
| 附件 | 文件名、类型、版本关联、上传者/时间、安全检查状态 | `file_asset` 及合同版本关联查询 | 失败/隔离文件不允许下载或作为生效依据 | 需附件列表权；下载另验短时权限 |
| 关联销售单 | 销售单号、业务性质、使用合同版本、主状态、三轨进度、含税金额 | W05 关联单据查询 | 金额只作单据摘要，不汇总为未定义的合同金额 | 按每张销售单范围裁剪；金额需字段权 |
| 版本 | 修订号、有效区间、变更原因、结构化 diff | `contract_revision` + 审计 | 历史修订不可编辑；销售单链接的版本高亮 | 需版本查看权；敏感 diff 掩码 |
| 审计 | 创建、修订、生效、终止、附件下载等 | 审计事件 | 敏感值只记“已变更”和摘要 | 按审计动作与对象范围裁剪 |

## 6. 搜索、筛选、排序与默认视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| Saved View | 我的有效合同 | `view=mine_active` | 用户偏好不改变服务端权限 |
| 搜索 | 空 | `q=` | 服务端搜合同编号、客户名称/编号；不搜未授权附件全文 |
| 客户 | 全部授权客户 | `customerId=` | 从 W03 进入时预设 |
| 状态 | 生效 | `status=draft|effective|terminated|expired` | 多选，状态值使用服务端枚举 |
| 有效期 | 全部 | `valid=active|expiring_30d|expired|range` | 业务日期边界由服务端计算 |
| 负责人 | 当前用户 | `ownerUserId=` | 管理者可选团队成员 |
| 关联销售 | 全部 | `usage=unused|used|has_active_orders` | 数量与状态由服务端聚合 |
| 排序 | 将到期优先、有效期结束升序 | `sort=valid_to_asc|updated_desc|contract_no` | 服务端排序和分页 |
| 列设置 | 保留身份、状态、有效期、操作 | 用户偏好 | 不允许隐藏固定列 |

筛选和排序由服务端执行，结果数使用 `aria-live=polite`。列表页码/游标、筛选、排序和预览 ID 写入 URL，刷新和后退可恢复。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 新建合同 | 列表页、W03 | `CREATE_CONTRACT`、客户启用 | 无 | 创建服务端草稿身份，打开同一对象页签编辑 | 未创建空对象；保留客户选择 |
| 保存草稿 | 合同编辑态 | `EDIT_DRAFT`、当前编辑权和 `lockVersion` | 无，自动保存 + 显式保存 | 返回新 `draftVersion` 与保存时间 | 保留输入，重试或解决冲突 |
| 将当前合同内容生效 | 对象头主动作 | `ACTIVATE` 由服务端返回，客户/结算主体有效，必需结构化条款与附件证据齐全 | `FormalActionConfirmDialog` 展示客户、版本、有效期和条款摘要 | 生成不可变合同修订并固定结果 | 校验失败回字段；结果未知查最终结果 |
| 创建新修订 | 已生效合同“修订” | `contractRevisionPolicy` 已配置且服务端返回 `REVISE`；未配置时入口禁用或隐藏，只读查看 | 展示适用策略、必需证据，并说明历史销售快照不改 | 按已配置策略在同一合同页签建立可编辑工作副本 | 策略缺失或失效时不创建工作副本，原生效版保持只读 |
| 终止合同 | 更多动作 | `TERMINATE`，使用影响由服务端返回 | 必须填原因并预览进行中销售单影响 | 合同转终止，新销售不可选，历史单据快照不变 | 阻断时列出需先处理对象，合同不变 |
| 上传附件 | 附件子区 | `UPLOAD_ATTACHMENT`，文件类型/大小合法 | 无 | 安全检查后绑定当前草稿/修订，显示检查状态 | 隔离失败文件，其它输入不丢 |
| 打印预览 | detail / 对象头 | 合同可见且存在已确认修订 | 无 | 宽层显示服务端给定的纸质投影 | 投影失败不影响对象中心 |
| 新建销售单 | 合同头 | 合同当前有效、客户启用、`CREATE_SALES_ORDER` | 无 | 打开 W05，固定当前 `contractRevisionId` | 条件变化时留 W04 并显示 blocker |
| 导出 | 列表工具栏 | `EXPORT`，当前筛选有数据 | 当前页/当前筛选全部须确认 | 创建服务端选择快照与后台任务，结果保留 7 天 | 部分失败给脱敏结果，不直接前端拼 CSV |

## 8. 数据契约

### 8.1 列表查询

```ts
type ContractListQuery = {
  view?: string
  query?: string
  customerId?: string
  statuses?: Array<"DRAFT" | "EFFECTIVE" | "TERMINATED" | "EXPIRED">
  valid?: "active" | "expiring_30d" | "expired" | { from: string; to: string }
  ownerUserId?: string
  usage?: "unused" | "used" | "has_active_orders"
  sort: "valid_to_asc" | "updated_desc" | "contract_no"
  cursor?: string
  pageSize: number
}

type ContractListRow = {
  contractId: string
  contractNo: string
  customer: { customerId: string; customerNo: string; displayName: string }
  settlementParty: { partyId: string; displayName: string }
  status: string
  revisionNo: number
  signedAt?: string
  validFrom: string
  validTo: string
  salesOrderCount: number
  activeSalesOrderCount: number
  ownerLabel: string
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
}

type ContractListResult = {
  rows: ContractListRow[]
  nextCursor?: string
  totalCount: number
  permissionVersion: string
  sourceAsOf: string
  relatedSalesOrdersAsOf: string
  queriedAt: string
}
```

服务端返回分页、排序、数量、筛选摘要和同一查询水位。Query Key 包含用户、角色、权限/数据范围版本、查询参数。列表刷新保留旧行，不闪成空白。

### 8.2 对象查询

```ts
type ContractCenterView = {
  contractId: string
  contractNo: string
  status: string
  lockVersion: number
  customer: ObjectReference
  currentRevision: {
    revisionId: string
    revisionNo: number
    settlementParty: ObjectReference
    paymentTermSnapshot: PaymentTermView
    invoiceRequirementSnapshot: InvoiceRequirementView
    validFrom: string
    validTo: string
    signedAt?: string
    effectiveAt?: string
  }
  attachments: ContractAttachmentView[]
  relatedSalesOrders: RelatedSalesOrderSummary[]
  revisionTimeline: ContractRevisionSummary[]
  auditTimeline: AuditEventView[]
  contractRevisionPolicy?: {
    policyVersion: string
    mode: "DIRECT_REVISION" | "CHANGE_REQUEST"
    requiredEvidenceCodes: string[]
  }
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
  sourceAsOf: string
  relatedSalesOrdersAsOf: string
  queriedAt: string
}
```

该视图不固定 API 路径，但必须保留稳定对象 ID、当前修订 ID、乐观版本、服务端动作判定和分区水位。`contractRevisionPolicy` 缺失时 `allowedActions` 必须排除 `REVISE`，任何创建修订请求也由服务端拒绝；前端不能自行选择 `DIRECT_REVISION`。附件、关联单据和审计可分区延迟加载；关联销售聚合陈旧时显示其 `as-of`，不得冒充合同正式字段的当前值。

### 8.3 提交

```ts
type SaveContractDraftCommand = {
  contractId: string
  expectedLockVersion: number
  draftVersion: number
  customerId: string
  settlementPartyId: string
  paymentTerm: PaymentTermInput
  invoiceRequirement: InvoiceRequirementInput
  validFrom: string
  validTo: string
  signedAt?: string
  attachmentIds: string[]
  idempotencyKey: string
}

type ActivateContractCommand = {
  contractId: string
  expectedLockVersion: number
  expectedDraftVersion: number
  contentHash: string
  idempotencyKey: string
}
```

- 自动保存用 `draftVersion` 条件更新；保存成功返回新版本与内容指纹。
- 生效提交指向不可变内容指纹，不读取仍可变的客户默认值作为正式条款。
- 超时不先行标记生效/终止；使用同一幂等键查询最终结果。
- 当前合同修订被销售单引用后，该销售单保存 `contract_revision_id` 和关键结构化快照，合同新版本不替换旧引用。

### 8.4 前端边界

- 可格式化日期、状态、名称和金额摘要；不计算合同状态或使用资格。
- “30 天内到期”数量由服务端按业务时区聚合，不用已加载行求和。
- 付款条件和开票要求可在前端做必填/格式即时校验，正式有效性与销售可用性以服务端结果为准。
- 纸质投影的字段、状态与条款完全使用服务端已确认数据，组件不重算或拼凑正式文本。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 列表或对象中心同构 Skeleton | 应用壳导航可用 | 查询成功原位替换 |
| 刷新 | 保留旧列表/合同内容，轻量标记 | 已有数据可查看；写入重验 | 成功更新水位 |
| 无合同 | “当前范围尚无合同” | 有权时新建合同 | 创建成功 |
| 筛选无结果 | 保留筛选摘要和清除入口 | 清除筛选 | 恢复默认视图 |
| 无数据范围 | 不显示 0 条，说明当前角色无合同范围 | 查看当前角色 | 范围更新后重查 |
| 列表失败 | 无缓存整表失败；有缓存保留并标记陈旧 | 重试；陈旧行仅查看 | 查询成功 |
| 对象/分区失败 | 主对象失败显稳定错误态；附件/关联单据失败只替换分区 | 重试对应查询 | 分区成功 |
| 合同到期/终止 | 页头状态和原因明确，历史版本可读 | 查看历史；新销售单禁用并说明 | 选其他有效合同 |
| 字段级隐藏 | 保留标签与结构，敏感条款/附件不返回或掩码 | 其他动作保持可用 | 权限更新后重查 |
| 修订策略未配置 | 已生效合同保持只读，显示“修订规则尚未配置” | 查看版本与审计；不得创建修订工作副本 | 服务端配置 `contractRevisionPolicy` 并重新返回 `REVISE` 后开放 |
| 保存中 | `DraftSaveIndicator`，防止重复保存/生效 | 继续编辑不丢输入 | 成功返回新草稿版本 |
| 保存失败 | 保留输入，错误靠近底部主动作 | 重试、导出本地备忘 | 同一草稿版本重试 |
| 校验失败 | `ValidationSummary` + 字段错误 | 修正字段 | 校验通过后重提交 |
| 版本冲突 | `ConflictResolutionDialog` 对比服务端新版本 | 刷新后重做、放弃本地修改 | 用新 `lockVersion` 重提 |
| 正式动作成功 | 固定显示合同号、修订号、生效/终止时间和下一步 | 新建销售单、查看版本 | 对象中心刷新 |
| 正式动作结果不确定 | 不本地标记生效/终止，固定显示查结果入口 | 查询最终结果、幂等重试 | 服务端返回确定结果 |
| 后台导出 | `BackgroundJobProgress` 显示筛选快照、字段遮罩、进度和任务号 | 查看任务、取消未开始项 | 完成后下载；失败按原快照重试 |
| 权限收回 | 清除表单、附件下载 URL 和敏感快照 | 返回有权列表 | 权限恢复后重查 |

## 10. 响应式、键盘与无障碍

### 10.1 响应式

| 视口 | 布局变化 | 必须保留 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 列表紧凑 36px，detail 打开时仍保留身份列；中心双列摘要 | 合同号、客户、状态、有效期、主动作；首屏 6–8 行 | 无 |
| 1280×800 | detail 覆盖更多列，列表可横滚 | 固定身份/操作列、合同主事实 | 销售单数可移列设置 |
| 1024×768 | 导航图标模式，detail 覆盖；中心单列 | 合同身份、有效期、结算条款、主动作 | 工具栏换行，次要列默认隐藏 |
| 768×1024 | 导航抽屉，筛选进面板，detail 上下分区；表格横滚 | 合同号、客户、状态、有效期、行动作 | 结算主体和负责人移入行展开 |
| 375×812 | 列表改紧凑卡片，只读合同摘要和简单结果 | 合同号、客户、状态、有效期、查看附件摘要 | 不提供复杂合同编辑、版本 diff、列设置和导出 |

### 10.2 键盘与焦点

- 列表行可聚焦；Enter 打开 detail，Esc 关闭并回原行，“打开中心”是独立可聚焦动作。
- 表格排序头播报当前排序方向；筛选改变后结果数通过 `aria-live=polite` 播报。
- detail 左右分区有明确标题和独立滚动区；焦点不在打开时跳过抽屉标题。
- 编辑态 Tab 顺序按表头字段 → 条款 → 附件 → 校验摘要 → 主动作；失败先聚焦校验摘要。
- 正式确认层关闭焦点回触发按钮；成功结果使用标题并可被读屏器读到。
- 状态、到期、校验失败与版本冲突均使用文字 + tone，不只靠颜色。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 今日工作台/待办 | W01 / W02 | `workItemId`、`contractId`、来源焦点 | 完成合同作业后回原任务 |
| 客户中心 | W03 | `customerId`、客户页签和子区 | 合同关闭后回客户合同摘要 |
| 销售单 | W05 | `customerId`、`contractId`、`contractRevisionId` | 新建/查看销售单后可回合同原页签 |
| 客户往来 | W11 | 客户 ID；合同只作查询参考 | 财务会话不反向修改合同快照 |
| 导入与期初 | W18 | 导入行、客户/合同映射和差异任务 | 导入形成正式合同后以稳定 ID 打开 W04 |

跨工作面不传递客户名称、条款或合同状态作为正式事实；目标页根据稳定 ID 和具体修订 ID 重新查询。

## 12. 验收清单

### 12.1 列表与阅读

- [ ] 1440×900 下页头、指标、工具栏和分页同时存在时至少露出 6–8 条 36px 数据行。
- [ ] 合同编号与行主动作固定，横向滚动不丢对象身份。
- [ ] 单击行打开 detail 后可读完客户、结算/开票、有效期、附件与关联销售摘要，无需再点中心才读主事实。
- [ ] 纸质预览使用宽 Dialog/打印页，不与 detail 半屏混用。
- [ ] 所有可见工具栏控件可操作；未接入能力隐藏或显示不可用原因。

### 12.2 版本、数据与权限

- [ ] 合同编号唯一，对象页签以稳定 `contractId` 为身份。
- [ ] 销售单保存具体 `contractRevisionId` 和关键快照，合同新修订不替换历史引用。
- [ ] 一份合同可关联多张销售单，但 UI 不自行创造“合同金额”事实。
- [ ] 合同到期/终止后不进新销售单选择器，历史销售快照和合同版本仍可追溯。
- [ ] 附件列表、下载和字段值分别鉴权，下载链接短时且有审计。
- [ ] 列表导出使用服务端选择快照与下载重新鉴权，不用前端当前页拼出全量结果。

### 12.3 写入、状态与终端

- [ ] 自动保存、显式保存和生效动作共用已确认的草稿版本与内容指纹。
- [ ] `contractRevisionPolicy` 缺失时已生效合同只读，`REVISE` 不出现在允许动作中，深链或直接请求也不能创建修订工作副本。
- [ ] 保存失败、版本冲突和结果未知都不丢用户输入，不乐观改正式状态。
- [ ] 正式动作成功固定展示合同号、修订号、时间和下一步，不只靠 toast。
- [ ] 第 9 节全部状态与第 10.1 节五档视口通过验收。
- [ ] 键盘可完成列表查找、detail 预览、打开中心、编辑与提交校验。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | 合同内容从草稿变为生效的责任岗位和必需证据是什么？ | 生效主动作、权限、确认内容和服务端校验 | 销售负责人 + 合规/管理负责人 | 不在 UI 自建审批流；由服务端 `allowedActions` 和证据清单控制生效 |
| Q2 | 合同生效后是允许直接创建新修订，还是需先建立变更申请？ | 修订入口、审计对象与页签脏状态 | 业务负责人 | 未确认前 fail-closed，只读查看且不创建修订工作副本；确认后由服务端配置 `contractRevisionPolicy`，只有策略存在且返回 `REVISE` 才开放 |
| Q3 | 存在进行中销售单时是否允许终止合同？ | 终止影响预览、服务端 blocker 和历史履约 | 销售 + 财务负责人 | 允许停止新业务，已生效销售继续按原合同快照履约与结算，确认层明示影响 |
| Q4 | 合同纸质投影需要展示哪些签章位和对外版式？ | `PaperDocument` 字段、布局和打印验收 | 业务 + 行政/合规 | 服务端返回已确认打印视图，前端不从附件自行提取条款 |

待确认事项确认后，应把结论写回对应章节并从本表移除；不得长期保留“建议”与正式规则并存。

## 14. 业务依据

- `erp-ui-design.md` §4.3 M2、§4.5 M4、§4.6 M5、§12 纸质投影、§13 W04。
- `erp-phase-1.md` §4.3：一份合同对应一张或多张销售单，销售单记录客户、合同、结算主体、付款和开票要求。
- `erp-phase-1.md` §5.1：客户、合同主数据在建设范围。
- `erp-phase-1.md` §11.1：销售、协作销售、团队、财务与历史参与权数据范围。
- `erp-data-model.md` §6.4 `contract` / `contract_revision`：编号、客户、状态、版本、结算快照、有效期和一对多销售关系。
- `erp-data-model.md` §4.4–§4.5：不可变版本和单据快照、文件与审计安全。
- `erp-ui-flows.md` §2：销售从客户/合同进入 W05 建单，合同与销售单在任务页签中可追溯。
