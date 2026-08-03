# W05 · 销售单（统一）

> 状态：草稿
> 页面模式：M2 高密度查询列表 + M4 对象中心 + M5 建单/销售变更
> 主要路由：`/sales/orders`；对象路由 `/sales/orders/:salesOrderId`
> 主要角色：销售、运营、财务；采购、仓储、管理层按职责协同或只读
> 最后更新：2026-08-03（建单合同仅选择 + 旁加号复用上传 Dialog）

## 1. 定位与目标

### 1.1 用户目标

- 销售在一个工作面查询、新建和跟进卡券与非卡券销售单，不记两套路由和两套编号。
- 经办人在对象中心一屏判断当前商业版本、主责系统、履约、回款和开票三轨进度，以及下一责任方。
- 协同角色从待办、客户、合同、采购、履约、票款或分析下钻后，直接落到同一张销售单的对应子区。
- 创建者在一个 M5 会话内选择已有有效合同（无合同时可旁加号复用上传 Dialog 原地归档并刷新列表），再完成销售内容、金额、证据和提交，不依靠多个孤立页面拼单。
- 销售领导与运营可在销售单对象中心嵌入处理固定的卡券销售审批任务；即使不返回 W02，也沿用同一任务、领取租约和正式完成信封。
- 采购二次确认驳回后，销售在对象中心只看到三条固定出路：改品/改价后重提、照原条件申请低毛利承接、确认不做并作废。

### 1.2 业务目标

- 卡券与非卡券共用 `sales_order` 稳定身份、统一编号、版本和工作面；业务性质在创建后终身不变。
- 将“由哪个系统创建”与“当前哪个系统主责”分开表达；任何时点一张销售单只有一个写入主责。
- 锁定正式版本快照，后续客户、合同、基础资料或成本变化不得覆盖历史销售事实。
- 把生效闸门、审批、采购履约、客户验收、票款和执行投影串成可追溯闭环，但不在 W05 复制各领域的正式表单。
- 采购驳回后的每次重提均形成新的不可变提交和内容指纹；照原条件承接必须先完成已注册的 `LOW_MARGIN_MANAGER_CONFIRMATION`，上级通过后才创建新的 `PROCUREMENT_CONFIRMATION`。

### 1.3 不在本工作面完成

- 不把卡券和实物/服务拆成两套销售单实体、列表、编号或一级菜单。
- 不在 ERP 修改一期仍由商城主责的卡券商业字段；W05 只展示同步事实、映射和版本时间线。
- 不在 W05 完成采购二次确认、采购单审核、出入库、回款核销、卡券票款复核或接口补偿。
- 不将 `work_item`、审批记录、执行投影或商城同步副本当作第二张销售单。
- 不以“开票完成”作为关闭销售单的必要条件；也不绕过应收结清或履约完成直接关闭。
- 不展示卡号、卡密、商城用户手机号、完整外部凭据或未授权银行信息。

## 2. 用户、权限与数据范围

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 销售经办 | W01 / W05 | 本人负责、协作或历史参与的客户销售单 | 新建、编辑草稿、提交；采购驳回后从三条固定出路中处理；发起销售变更单、登记验收 |
| 销售经理 | W02 / W05 | 授权团队销售单，以及分配给本人或有权领取的 `CARD_SALES_MANAGER_APPROVAL`、`LOW_MARGIN_MANAGER_CONFIRMATION` | 查看、在共享任务处理器中通过/驳回低毛利或卡券审批、风险协同、查看销售变更影响 |
| 运营 | W02 / W05 | 授权卡券单、分配给本人或有权领取的 `CARD_SALES_OPERATION_APPROVAL` 及商城协同对象 | 在共享任务处理器中通过/驳回、查看发布/投影、处理业务映射 |
| 财务 | W05 / W11 / W13 | 授权公司的票款及税务相关销售单 | 查看商业快照、进入客户往来或票款复核 |
| 采购 | W07 / W08 链入 | 需要确认或采购的非卡券销售单 | 只读商业依据、处理二次确认、创建采购单 |
| 仓储 | W09 链入 | 与授权仓库作业相关的销售内容 | 只读履约上下文，不编辑销售商业字段 |
| 管理层 | W15 / W16 下钻 | 授权组织或团队范围 | 只读经营与单据事实 |

权限规则：

- 服务端同时返回模块权限、动作权限、字段权限、数据范围版本与 `allowedActions/actionBlockers`；前端不以角色名推断正式动作。
- 无模块权限时隐藏导航；直接访问显示无权限页。有模块权限但无数据范围时不能显示“0 张销售单”。
- 商城主责的卡券单即使用户有编辑角色，商业字段也必须只读，并说明“当前由商城主责”。
- 对象状态导致不可做时保留动作位置并禁用；无动作权限时按全局权限契约隐藏或只读。
- 权限或数据范围被收回时，清除表单、附件下载 URL、敏感摘要和对象缓存，不保留可提交草稿令牌。
- 金额、利润、客户联系方式和外部身份按字段权限分别控制；隐藏字段不得通过导出、URL、埋点或错误文案泄露。
- 对象中心只能在当前用户可查看对应 `work_item` 时嵌入审批处理器；审批动作取任务返回的 `allowedActions/actionBlockers`，不能因用户可查看销售单就推导其可审批。低毛利承接也必须绑定正式 `LOW_MARGIN_MANAGER_CONFIRMATION`，不得退化为销售单上的经理状态按钮。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 侧栏查询 | 销售 > 销售单 | `/sales/orders`，复用固定列表页签 | 保留筛选、滚动和选中行 |
| 打开对象 | 列表 detail、全局搜索、关联对象 | `/sales/orders/:salesOrderId`，相同稳定 ID 只聚焦已有页签 | 关闭后回来源触发点 |
| 新建销售单 | W05、W03、W04 | 进入 M5；合同仅选择已有有效版本，旁加号复用 `ContractUploadDialog` | 取消后回原客户/合同或列表；上传 Dialog 取消不形成合同 |
| 处理卡券销售审批 | W01 / W02 或对象中心待处理提示 | `/sales/orders/:salesOrderId?section=approval&workItemId=...&queueContextId=...`，嵌入 W02 注册的同一处理器 | 完成后刷新主状态；从队列进入时按 `queueContextId` 返回/继续 |
| 处理采购驳回 | W07 固定驳回结果 / W01 提示 | `/sales/orders/:salesOrderId?section=procurement-rejection&rejectedConfirmationId=...`；只携稳定驳回身份，页面重查当前可用出路 | 完成重提/低毛利申请/作废后留在固定结果，或进入相应任务 |
| 处理低毛利承接确认 | W01 / W02 或对象中心待处理提示 | `/sales/orders/:salesOrderId?section=procurement-rejection&workItemId=...&queueContextId=...`，嵌入已注册 `LOW_MARGIN_MANAGER_CONFIRMATION` 处理器 | 决定后刷新驳回处理链；通过时可打开新 W07 任务，驳回时返回销售处理 |
| 采购确认 | W07 | 对象路由携带 `section=commercial&workItemId` | 关闭后回原队列位置 |
| 登记客户验收 | W05 履约子区 | 同一页签切到 `section=acceptance`，复用 W06 | 完成后回履约摘要 |
| 登记回款/开票 | 票款子区 | 打开 W11 会话并携带销售单、应收和客户 ID | 返回 W05 票款子区并重查 |
| 查看执行投影 | 协同子区 | W05 展示当前状态；完整历史打开 W23 新页签 | 返回聚焦协同子区 |
| 分析下钻 | W15 / W16 / W28 | 打开对象页签并携带只读来源上下文 | 关闭后恢复分析筛选 |

列表页签身份为 `workspace:sales-orders:{viewerScopeId}`，对象页签身份为 `sales-order:{salesOrderId}`。编辑、审批、验收和只读中心是同一对象页签中的模式与子区，不建立平行路由树。URL 只保存稳定 ID、子区和可恢复筛选，不保存金额、权限、审批结论或 `claimToken`。从 W02 进入与在 W05 直接展开审批子区只改变承载位置，不改变 `workItemId`、处理器或接口契约。

## 4. 页面布局

### 4.1 1440×900 列表与 detail

```text
┌ PageHeader：销售单                              [新建销售单] [导出] ┐
├ MetricStrip：待处理 | 进行中 | 待收款 | 履约异常 | 商城协同异常       ┤
├ ListToolbar：搜索 | 业务性质 | 主责 | 主状态 | 三轨进度 | 负责人 | 日期 ┤
├─────────────────────────────────────────────────────────────────────┤
│ 固定：销售单号/客户 │ 业务性质 │ 主责 │ 主状态 │ 履约/回款/开票 │ 操作 │
│ 至少 6–8 条紧凑行；金额明确含税；服务端分页与排序                  │
└─────────────────────────────────────────────────────────────────────┘
                              ┌ detail 半屏 ──────────────────────────┐
                              │ 身份、当前版本、商业摘要、三轨进度   │
                              │ blocker、下一责任方、近期时间线      │
                              │ [打开中心] [打印预览] [允许动作]      │
                              └───────────────────────────────────────┘
```

### 4.2 对象中心

遵循 `erp-ui-design.md` §4.5.1：`PageHeader object-chrome` + `DocumentHeader density="compact"`；跨工作面入口（回款/履约）用 header 次要按钮，不用整幅说明条占首屏。

```text
┌ PageHeader object-chrome：销售 › 销售单 › XS-…          [返回] [变更] ─┐
├ DocumentHeader compact：客户名 [主状态]                                │
│  XS-2026-0088 · 版本 · 主责轨          [主动作] [登记回款] [去处理]    │
│  多轨：履约 | 回款 | 开票                                              │
├ ProgressRail：提交/确认或审批 → 生效 → 履约 → 应收结清 → 已关闭      ┤
├ 锚点：商业内容 | 采购/履约 | 客户验收 | 票款 | 协同 | 版本与审计     ┤
├──────────────────────────────────────────────────────────────────────┤
│ 左：当前正式商业快照、明细、金额、证据                                │
│ 右：下一步、阻断原因、履约/回款/开票摘要、责任人                      │
│ 审批任务存在时：只读冻结提交 + 任务身份/租约 + [通过] [驳回]           │
│ 采购驳回时：原提交/指纹 + 原因 + [改品改价重提][低毛利承接][作废]      │
│ 下：关联采购/作业/验收/票款/投影 + 版本 diff + 审计时间线              │
└──────────────────────────────────────────────────────────────────────┘
```

### 4.3 M5 建单与销售变更

实现页：`erp-client/features/sales-orders/sales-order-create-page.tsx`。  
目标密度：**一屏头信息 + 可滚明细 + 与主栏一体的合计 footer**；避免多层 Card 标题、长 Section 说明与独立全宽底栏。

```text
┌ PageHeader density=compact：新建销售单 · 状态未创建                 ┐
├ 主栏（单张 rounded-xl border card）          │ 右侧摘要（xl+ sticky）│
│  ├ 单据头：合同选择 + 客户/结算只读摘要        │  合同 / 客户 / 结算     │
│  │   · 有效合同：ContractCombobox + [+] 加号  │  业务性质 / 明细行数   │
│  │     （[+] 打开 ContractUploadDialog，归档  │  含税预估 / 下一步     │
│  │      后刷新可选列表并自动选中新合同）       │                       │
│  │   · 商业约定：性质/负责人/场景/付款/期限/税率（大屏 3 列 2 行） │   │
│  ├ 销售明细：EditableLineItemTable（单元格横排）│                       │
│  │   · 备注 rows=2                            │                       │
│  └ StickyTotalBar（card footer：无独立圆角/阴影，贴主栏底）            │
│       含税 / 不含税 / 税额 + 流程说明 + [取消][草稿][提交]            │
└──────────────────────────────────────────────────────────────────────┘
```

布局硬约束：

- 不渲染分区锚点（「单据头 | 销售明细」nav）；主栏内用 `border-b` 分节即可。
- 合计条必须放在**主栏 card 内部**底部，与单据头/明细同一外框；不得横跨右侧摘要成为全宽独立条。
- 右侧「本单摘要」仅 `xl` 及以上显示；窄屏只保留主栏。
- 不在建单页堆叠「销售单草稿」类第二标题；流程说明并入合计 `note`，不另起 info Alert。

建单字段控件约定：

- 有效合同：仅 `ContractCombobox` 选择；选择框旁 **加号** 打开与 W04 共用的 `ContractUploadDialog`，原地归档后刷新可选列表并自动选中新合同。
- 客户 / 结算主体 / 合同精确版本由所选合同快照**只读摘要条**带出（不渲染 disabled 输入框；建单页不再内嵌随单上传表单）。
- 负责销售：`OwnerCombobox`；付款条件码表 `SelectField`。
- 非卡券明细商品/SKU：`ProductCombobox`（公司商品池/启用商品）。
- **单位**：随所选 SKU 的**基础单位**自动带出并**只读展示**；建单页禁止 `SelectField` 改单位。卡券明细单位固定为「张」。未选 SKU 时显示「—」。
- **履约方式（仓发 / 直发等）**：建单页**不可选择、不展示下拉**。明细列仅保留可编辑的**交付日期**；正式履约方式由 W07 采购二次确认写入。前端提交可带契约占位值，服务端以确认结论为准。
- 含税小计列使用 `MoneyValue` **不传** `taxBasis`（表头已写「含税小计」，行内不再叠「含税」Badge）。合计栏金额标签已区分含税/不含税，口径以标签为准。
- 禁止用自由 `Input` 手输客户名、结算主体名、负责销售、SKU 编码（卡券类目名称除外）。

### 4.4 区域规则

| 区域 | 目的 | 主组件 | 关键规则 |
| --- | --- | --- | --- |
| 列表 | 高频查单与分流 | `BusinessTableFrame` `DataTable` | 身份列和操作列固定，业务性质与主责分列 |
| detail | 不离开列表读完主事实 | `QuickPreviewSheet size="detail"` | 只读，不塞完整编辑器 |
| 对象头/进度 | 判断“这是什么、到哪了、谁负责” | `DocumentHeader` `ProgressRail` | 履约、回款、开票分轨显示 |
| 商业内容 | 当前正式版本与版本快照 | `DocumentSummary` `LineItemsTable` | 历史版本不可覆盖 |
| 关联作业 | 串采购、履约、验收和票款 | `RelatedDocumentList` | 只展示摘要，正式动作进入对应 W |
| 卡券销售审批 | 在对象上下文内高效处理领导/运营任务 | W02 `handlerKey` 对应的 `ApprovalDecisionPanel` | 只读冻结提交；必须显示任务类型、领取人、租约、前置与正式结果；不能退化为对象状态按钮 |
| 采购驳回处理 | 给销售固定且穷尽的三条出路，并承载低毛利上级任务 | `ProcurementRejectionResolutionCard` + W02 `LOW_MARGIN_MANAGER_CONFIRMATION` 处理器 | 原驳回提交只读；每次重提生成新提交/新指纹；旧 W07 任务只作历史引用 |
| 协同 | 商城同步、主责与执行投影 | `SyncStatus` `ProjectionStatus` | 不提供第二销售单编辑入口 |
| 编辑区 | 新建草稿或销售变更工作副本 | TanStack Form + `StickyTotalBar` | 正式版本不可直接编辑；服务端算金额与资格 |

## 5. 展示内容与字段

### 5.1 列表与对象身份

| 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- |
| `salesOrderNo` | 销售单号 | `sales_order` 稳定身份 | 唯一编号，所有版本不变 | 对象可见即显示 |
| `customer` | 客户 | 正式销售版本客户快照 + 当前主体引用 | 中心优先显示正式快照，可钻当前客户 | 按客户数据范围 |
| `businessType` | 业务性质 | `sales_order.business_type` | `VOUCHER` / `GOODS_SERVICE`，创建后不可变 | 全部可见 |
| `originSystem` | 创建来源 | `sales_order.origin_system` | ERP / 商城，仅说明来源 | 管理员可见技术标识 |
| `ownerSystem` | 当前主责 | 主责登记 | 商城 / ERP，与创建来源分开 | 全部显示可理解文案 |
| `status` | 主状态 | 正式状态机投影 | 服务端枚举 + tone | 全部可见 |
| `revisionNo` | 商业版本 | 当前正式修订 | `v{n}`，草稿另标 | 可见对象均可见 |
| `fulfillmentProgress` | 履约 | 正式履约/验收/到期投影 | 与回款、开票独立 | 按作业范围裁剪明细 |
| `collectionProgress` | 回款 | W11 应收与核销投影 | 含税金额和开放余额 | 需票款字段权限 |
| `invoiceProgress` | 开票 | W11 发票分配投影 | 不参与关闭门槛 | 需发票字段权限 |
| `grossAmount` | 含税金额 | 正式销售版本服务端金额 | 币种 + 两位小数 | 金额字段权限 |
| `ownerUser` | 负责销售 | 当前归属与单据参与事实 | 当前负责人与历史经办分开 | 组织范围控制 |

### 5.2 商业内容与明细

| 类型 | 必须展示 | 数据来源 / 口径 | 可写边界 |
| --- | --- | --- | --- |
| 两类共有 | 合同、客户、合同精确版本、负责人、业务场景、有效期、币种、税务与含税/不含税/税额 | 初始销售草稿、销售变更工作副本或不可变销售版本；合同按 ID 查询已归档有效版本（建单前可经共用上传 Dialog 先归档） | 仅 ERP 主责可编辑初始草稿或销售变更工作副本；正式版本永远只读 |
| 非卡券 | 一条或多条公司商品池、数量、**SKU 基础单位（只读）**、单价、税率、**交付日期**；履约方式不在建单页选择 | W14 精确公司商品池修订；服务端金额和资格；单位取商品基础单位 | 生效前经 W07 二次确认（此处写正式履约方式）；生效后走正式变更，不回旧确认 |
| 卡券 | 卡券类目、面值、张数、卡形态、履约期限和商城外部身份；单位固定「张」 | 每个销售版本恰好一条卡券明细；不含玩法、卡号或卡密 | 一期商城主责只读；二期 ERP 主责必须经卡券销售变更单的运营确认和财务复核 |

业务性质不允许通过销售变更改变。尝试从卡券单复制为非卡券时，必须创建新销售单并保留来源关系，不能复用原稳定身份。

销售选品查询和商品导出只包含公司商品池资料、销售可见价、可售区域和必要图文。
供应商身份、来源报价、采购确认成本、进项税率、费用和 MOQ 不进入销售接口、导出或错误文案。

### 5.3 进度、关闭与协同

| 分区 | 展示字段 | 数据来源 | 关键解释 |
| --- | --- | --- | --- |
| 生效链 | 当前节点、责任人、提交版本/指纹、决定与原因 | W07 二次确认或固定 `CARD_SALES_MANAGER_APPROVAL` / `CARD_SALES_OPERATION_APPROVAL` + `sales_order_review` / `workflow_action` | 待办负责租约与路由；审批事实及销售生效事务决定正式状态 |
| 履约 | 采购、作业、验收/到期、异常与完成时间 | W08/W09/W06 正式事实；卡券用履约期限 | 非卡券验收完成；卡券不因消费完提前完成 |
| 票款 | 应收、已核销回款、开放余额、开票进度 | W11 正式事实 | 应收结清是关闭条件；开票未完成不阻塞关闭 |
| 主责 | 创建系统、当前主责、迁移批次、唯一切换时间 | W17/W24 主责事实 | 二期仅允许商城→ERP 迁移一次，不提供回退动作 |
| 执行投影 | 当前投影版本、商城确认、错误摘要、最后更新 | W23 投影及投递状态 | 失败不回退已生效销售版本或应收 |
| 消费汇总（二期） | 消费金额、退款、余额恢复、消费事实水位和“查看消费订单”入口 | W25 正式商城事实聚合；经营口径另进 W28 | 只表达消费进度，不使卡券提前履约或增加关闭条件 |
| 版本审计 | 修订、结构化 diff、正式动作、来源对象 | 销售修订与审计事件 | 不回填当前基础资料覆盖旧快照 |

### 5.4 采购驳回后的固定出路

`ProcurementRejectionResolutionCard` 固定展示被驳回的 `submissionNo`、不可变 `submissionId`、`subjectHash` 摘要、采购确认身份、驳回原因/说明、处理人和时间，以及当前销售草稿相对该提交的结构化差异。卡片只提供以下三条互斥出路，服务端不返回第四种通用“再次提交”：

| 出路 | 页面输入 | 正式结果 | 必须阻止 |
| --- | --- | --- | --- |
| 改品或改价后重提 | 修改后的商品/服务或销售价格、客户重新确认依据 | 冻结新的 `sales_order_submission`、递增提交号和新 `subjectHash`，原子创建新的 `PROCUREMENT_CONFIRMATION` | 内容未发生改品/改价时不得冒充此路径；不得复用旧提交、旧指纹或旧 W07 任务 |
| 照原条件承接低毛利 | 保持原商业条件、低毛利承接理由和必要证据 | 先冻结新的不可变提交/新指纹并原子创建唯一 `LOW_MARGIN_MANAGER_CONFIRMATION`；此时不创建采购确认任务 | 销售或经理不能直接把销售单送回 W07；上级尚未通过时不得生效、形成应收或采购创建依据 |
| 不做并作废 | 结构化作废原因和说明 | 追加作废 `workflow_action` 并把生效前销售单置为作废；不创建任何后继任务 | 有有效低毛利任务或其它正式处理进行中时不得并发作废；不得删除历史提交和驳回记录 |

低毛利上级处理区必须展示“商业条件与被驳回提交一致”的服务端校验、采购最新成本/预计毛利证据、销售承接理由、新提交号/新指纹和原驳回链。上级通过只代表同意承担低毛利，不能替代采购确认；通过事务完成当前 `LOW_MARGIN_MANAGER_CONFIRMATION` 后，才为同一新提交创建一个新的 `PROCUREMENT_CONFIRMATION`。上级驳回则完成低毛利任务但不创建采购任务，销售单回到三条出路处理卡片。

## 6. 搜索、筛选、排序与默认视图

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| Saved View | 我的进行中 | `view=mine_active` | 只保存偏好，不扩大数据范围 |
| 搜索 | 空 | `q` | 服务端搜单号、客户编号/名称、合同号；不搜敏感全文 |
| 业务性质 | 全部 | `businessType` | 卡券/非卡券共享列表 |
| 主责系统 | 全部 | `ownerSystem` | 与创建来源分开筛选 |
| 主状态 | 进行中 | `statuses` | 多选，使用服务端枚举 |
| 三轨进度 | 全部 | `fulfillment/collection/invoice` | 各自筛选，不合并为模糊“完成度” |
| 负责人 | 当前用户 | `ownerUserId` | 经理可选授权团队 |
| 日期 | 最近 90 天 | `createdFrom/createdTo` | 业务时区由服务端解释 |
| 异常 | 全部 | `exceptionType` | 履约、票款、映射、投影分别编码 |
| 排序 | 待处理、最近更新 | `sort` | 服务端稳定排序与游标分页 |

筛选、排序、游标、detail ID 和对象子区可恢复；表单输入不进入 URL。指标与列表使用同一查询水位和权限版本。导出只覆盖当前筛选、当前字段权限和服务端快照，大范围导出使用后台任务。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 新建销售单 | 页头、W03、W04 | `CREATE`；客户有效；服务端允许业务性质；必须引用已有有效合同当前修订 | 创建草稿无需正式确认 | 固定所选合同版本；新合同须先经上传 Dialog 归档 | 失败保留选择；结果未知查原请求 |
| 保存草稿 | M5 | 编辑租约、草稿版本、字段校验 | 无 | 返回新 `draftVersion`、指纹与保存时间 | 保留输入，处理冲突后重试 |
| 提交非卡券确认 | M5 | ERP 主责、非卡券、校验通过 | 展示客户、合同、金额、履约摘要 | 冻结提交版本并形成 W07 任务 | 不乐观生效；同幂等键查结果 |
| 改品/改价后重提 | 采购驳回处理卡 | 当前为采购驳回后的销售处理态；无有效后继任务；草稿相对被驳回提交至少有商品/服务或销售价格变化；客户已重新确认 | 展示旧/新结构化 diff、原驳回原因和“将创建全新采购确认任务” | `ResolveProcurementRejectionCommand` 原子冻结新提交/新指纹并创建唯一新 `PROCUREMENT_CONFIRMATION`；旧提交、确认和任务不变 | 冲突时刷新旧/新 diff；结果未知按原操作查询，不重复生成提交或任务 |
| 申请照原条件低毛利承接 | 采购驳回处理卡 | 当前为采购驳回后的销售处理态；无有效后继任务；服务端确认商业条件与被驳回提交一致；承接理由完整 | 展示服务端返回的毛利风险等级/是否低于阈值、承接理由和“须由销售上级确认，尚不会回采购”；不展示原始采购成本或可反推成本的精确毛利值 | 原子冻结新提交/新指纹并创建唯一 `LOW_MARGIN_MANAGER_CONFIRMATION`；不创建 `PROCUREMENT_CONFIRMATION` | 内容变化时引导改品/改价路径；结果未知查询原操作，不允许直接进入 W07 |
| 领取低毛利上级确认 | W02 或驳回处理卡 | 固定任务 `LOW_MARGIN_MANAGER_CONFIRMATION`；当前用户在销售上级责任池 | 无 | 复用 W02 `ClaimWorkItemCommand`，`claimToken` 仅存会话内存 | 他人领取时只读展示；租约失效后重新领取并重查新提交/指纹 |
| 上级通过低毛利承接 | W02 或驳回处理卡 | 有效 `LOW_MARGIN_MANAGER_CONFIRMATION` 租约；新提交/任务指纹、订单版本与原驳回链一致 | 展示低毛利理由、受控风险结论及“通过后仍须采购再次确认”；采购成本继续保密 | 同事务写上级通过事实、完成低毛利任务并为该新提交创建唯一新 `PROCUREMENT_CONFIRMATION`；销售单不生效 | 结果未知不本地进入 W07；同幂等键查最终结果，禁止手工补建采购任务 |
| 上级驳回低毛利承接 | W02 或驳回处理卡 | 有效低毛利任务租约；结构化驳回原因完整 | 展示将退回销售继续选择固定出路 | 同事务写驳回事实并完成低毛利任务；不创建采购确认任务，返回销售处理态 | 结果未知保留当前任务；确定结果后销售重新选择，不能复用已完成低毛利任务 |
| 不做并作废 | 采购驳回处理卡 | 当前为采购驳回后的销售处理态；无有效低毛利/采购后继任务；销售单尚未生效 | 展示不可恢复作废影响和历史保留说明 | 原子追加作废审计并将销售单置为作废，不创建后继任务 | 版本冲突重查；结果未知按原操作恢复，不重复写作废事实 |
| 提交卡券审批 | M5 | 二期能力启用、ERP 主责、卡券 | 展示版本与审批链 | 冻结版本，进入销售领导/运营审批 | 不生成第二销售单；失败保留草稿 |
| 领取卡券销售审批 | W02 或 W05 审批子区 | 固定任务为 `CARD_SALES_MANAGER_APPROVAL` 或 `CARD_SALES_OPERATION_APPROVAL`；当前用户在责任池 | 无 | 复用 W02 `ClaimWorkItemCommand`，取得仅存会话内存的 `claimToken`、`leaseVersion` 和任务指纹 | 被他人领取时保留对象只读；可回队列或刷新租约 |
| 销售领导通过 | W05 嵌入处理器 | `CARD_SALES_MANAGER_APPROVAL`；有效领取；销售单为 `PENDING_SALES_LEAD`；冻结提交及指纹一致 | 展示客户、合同、卡券内容、金额及“将进入运营审批” | 同事务追加领导 `sales_order_review` 和 `workflow_action`、完成当前任务、把销售单转为 `PENDING_OPERATIONS` 并创建唯一运营任务 | 冲突后刷新冻结提交；结果未知固定显示操作追踪，不本地进入运营节点 |
| 运营通过 | W05 嵌入处理器 | `CARD_SALES_OPERATION_APPROVAL`；有效领取；领导审批已通过；销售单为 `PENDING_OPERATIONS`；冻结提交及指纹一致 | 展示商城执行条件及“通过后销售单立即生效” | 同事务追加运营 `sales_order_review` 和 `workflow_action`、完成任务、形成首个正式销售版本和应收、置为 `EFFECTIVE` 并写执行投影 outbox | 结果未知不本地生效；用原幂等键查询，商城接收失败另进投影异常且不回退销售事实 |
| 驳回卡券销售审批 | W05 嵌入处理器 | 当前固定审批任务有效；`REJECT` 可见；结构化原因必填 | 展示冻结提交和“退回销售处理；修改后从领导审批重启” | 同事务追加对应阶段的驳回 `sales_order_review` 和 `workflow_action`、完成当前任务并使提交/销售单进入服务端返回的退回销售状态；不创建下阶段任务 | 结果未知停留当前任务；不得创建工作副本或重复驳回记录 |
| 发起销售变更单 | 对象头 | 正式单 ERP 主责；负责销售；同一基准版本无进行中变更 | 说明历史版本和既有履约/票款不被覆盖 | 创建 `sales_change_order` 与工作副本；非卡券进入采购影响确认，卡券进入运营执行影响确认，之后均经财务复核 | 原正式版本继续有效；创建结果未知时查询原请求 |
| 登记验收 | 履约子区 | 非卡券、W06 动作允许 | 按验收契约 | 追加验收事实并刷新履约 | 结果未知不本地完成 |
| 登记回款/开票 | 票款子区 | 有 W11 权限和往来主体 | 在 W11 正式确认 | 形成票款事实并回 W05 重查 | W05 不缓存拟核销结果 |
| 查看关闭条件 | 对象头 / 进度轨 | 对象可见 | 无 | 展示全部明细履约与应收结清两项证据；两项满足后由服务端自动进入已关闭，开票可继续 | 查询失败保留上次结论并标陈旧，不提供人工“关闭”按钮 |
| 打印预览 | detail/对象头 | 存在已确认版本 | 无 | 展示服务端 `PaperDocument` | 投影失败不影响对象事实 |
| 导出 | 列表 | `EXPORT`、有结果范围 | 大范围确认行数与口径 | 后台生成 7 天有效授权文件 | 任务失败可重试，不在浏览器拼全量 |

普通对象正式动作携带稳定对象 ID、期望版本/内容指纹、操作 ID 和幂等键。采购驳回的三条出路必须使用下述强类型命令；低毛利上级决定与卡券销售审批一样，无论承载在 W02 还是 W05，都必须使用 W02 共享的 `CompleteWorkItemEnvelope`，不能改用 `SalesOrderFormalCommand`、通用对象状态命令或单独“完成任务”请求。超时或断网后先查询原结果；结果未知期间不切换主状态、不跳下一步、不生成新幂等键。

## 8. 数据契约

### 8.1 列表查询

```ts
type SalesOrderListQuery = {
  view?: string
  q?: string
  businessTypes?: Array<"VOUCHER" | "GOODS_SERVICE">
  ownerSystems?: Array<"MALL" | "ERP">
  statuses?: string[]
  fulfillmentStates?: string[]
  collectionStates?: string[]
  invoiceStates?: string[]
  ownerUserIds?: string[]
  createdFrom?: string
  createdTo?: string
  exceptionTypes?: string[]
  sort: string[]
  cursor?: string
  pageSize: number
}

type SalesOrderListResult = {
  rows: SalesOrderListRow[]
  nextCursor?: string
  totalCount: number
  metrics: SalesOrderMetric[]
  permissionVersion: string
  sourceAsOf: string
  queriedAt: string
}
```

### 8.2 对象中心查询

```ts
type SalesOrderCenterView = {
  identity: {
    salesOrderId: string
    salesOrderNo: string
    businessType: "VOUCHER" | "GOODS_SERVICE"
    originSystem: "MALL" | "ERP"
    ownerSystem: "MALL" | "ERP"
  }
  status: string
  lockVersion: number
  currentRevision: SalesOrderRevisionView
  progress: {
    activation: ProcessSummary
    fulfillment: ProcessSummary
    collection: ProcessSummary
    invoice: ProcessSummary
  }
  procurementAndFulfillment: RelatedDocumentSummary[]
  procurementRejectionResolution?: ProcurementRejectionResolutionView
  acceptance?: AcceptanceSummary
  receivable: ReceivableSummary
  activeCardSalesApproval?: {
    workItemId: string
    workItemType: "CARD_SALES_MANAGER_APPROVAL" | "CARD_SALES_OPERATION_APPROVAL"
    completionAction: string
    subjectVersion: string
    subjectHash: string
    workItemStatus: WorkItemStatus
    claimedBy?: UserSummary
    leaseVersion?: number
    leaseExpiresAt?: string
    frozenSubmission: SalesOrderSubmissionView
    allowedActions: Array<"CLAIM" | "APPROVE" | "REJECT">
    actionBlockers: ActionBlocker[]
  }
  collaboration?: ExecutionProjectionSummary
  ownershipMigration?: OwnershipMigrationSummary
  revisionTimeline: RevisionSummary[]
  auditTimeline: AuditEventView[]
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
  fieldPermissions: Record<string, "full" | "masked" | "hidden">
  sourceAsOf: string
  projectionUpdatedAt?: string
  queriedAt: string
}

type ProcurementRejectionResolutionView = {
  rejectedProcurementConfirmationId: string
  rejectedProcurementWorkItemId: string
  rejectedSubmissionId: string
  rejectedSubmissionNo: number
  rejectedSubjectHash: string
  rejectReasonCode: string
  rejectComment: string
  rejectedByLabel: string
  rejectedAt: string
  reviewStatus: "REJECTED" | "PENDING_LOW_MARGIN_MANAGER"
  latestDecisionStage:
    | "PROCUREMENT_CONFIRMATION"
    | "LOW_MARGIN_MANAGER_CONFIRMATION"
  draftDifference: {
    changedItemOrService: boolean
    changedSalesPrice: boolean
    commercialTermsUnchanged: boolean
    diffSummary: StructuredDiffItem[]
  }
  fixedResolutions: [
    "RESUBMIT_CHANGED_TERMS",
    "REQUEST_LOW_MARGIN_ACCEPTANCE",
    "VOID_AFTER_REJECTION",
  ]
  lowMarginSubmission?: {
    submissionId: string
    submissionNo: number
    subjectHash: string
    acceptanceReason: string
    commercialTermsMatchRejectedSubmission: boolean
  }
  activeLowMarginManagerTask?: {
    workItemId: string
    workItemType: "LOW_MARGIN_MANAGER_CONFIRMATION"
    completionAction: "DECIDE_LOW_MARGIN_ACCEPTANCE"
    subjectVersion: string
    subjectHash: string
    workItemStatus: WorkItemStatus
    claimedBy?: UserSummary
    leaseVersion?: number
    leaseExpiresAt?: string
    allowedActions: Array<"CLAIM" | "APPROVE" | "REJECT">
    actionBlockers: ActionBlocker[]
  }
  allowedActions: Array<
    | "RESUBMIT_CHANGED_TERMS"
    | "REQUEST_LOW_MARGIN_ACCEPTANCE"
    | "VOID_AFTER_REJECTION"
  >
  actionBlockers: ActionBlocker[]
}
```

卡券正式修订必须由服务端保证恰好一条卡券明细；非卡券修订至少一条有效销售明细。关联分区可以延迟加载，但必须返回各自水位，不能用失败分区覆盖主对象。`activeCardSalesApproval` 和 `activeLowMarginManagerTask` 都只是 W02 正式任务在对象中心的投影，不返回 `claimToken`，也不建立第二套审批状态；`workItemStatus` 直接复用 W02 `WorkItemStatus`，不接受本地字符串或同义枚举。`procurementRejectionResolution` 只在当前销售单确为采购驳回后的生效前处理态时返回；旧 W07 任务身份用于历史追溯，不允许作为新动作的任务信封。

### 8.3 提交

```ts
/** 建单仅引用已归档有效合同的当前修订；新合同先经 W04 UploadContractPdf。 */
type SalesOrderContractInput = {
  contractId: string
  contractRevisionId: string
}

type CreateSalesOrderCommand = {
  contract: SalesOrderContractInput
  businessType: "VOUCHER" | "GOODS_SERVICE"
  lines: SalesOrderLineInput[]
  paymentTerms: string
  fulfillmentDeadline: string
  idempotencyKey: string
}

type SaveSalesOrderDraftCommand = {
  salesOrderId: string
  expectedLockVersion: number
  expectedDraftVersion: number
  businessType: "VOUCHER" | "GOODS_SERVICE"
  customerId: string
  contractRevisionId: string
  lines: SalesOrderLineInput[]
  idempotencyKey: string
}

type SalesOrderFormalCommand = {
  salesOrderId: string
  action: "SUBMIT_INITIAL"
  expectedLockVersion: number
  subjectHash: string
  operationId: string
  idempotencyKey: string
}

type ProcurementRejectionResolutionBase = {
  salesOrderId: string
  rejectedProcurementConfirmationId: string
  rejectedSubmissionId: string
  expectedRejectedSubjectHash: string
  expectedSalesOrderLockVersion: number
  operationId: string
  idempotencyKey: string
}

type ResolveProcurementRejectionCommand =
  ProcurementRejectionResolutionBase &
  (
    | {
        action: "RESUBMIT_CHANGED_TERMS"
        expectedDraftVersion: number
        expectedDraftHash: string
        customerReconfirmationEvidenceIds: [string, ...string[]]
      }
    | {
        action: "REQUEST_LOW_MARGIN_ACCEPTANCE"
        expectedDraftVersion: number
        expectedDraftHash: string
        lowMarginAcceptanceReason: string
        evidenceReferenceIds: string[]
      }
    | {
        action: "VOID_AFTER_REJECTION"
        voidReasonCode: string
        comment: string
        expectedDraftVersion?: never
        expectedDraftHash?: never
      }
  )

type ProcurementRejectionResolutionBusinessResult =
  | {
      outcome: "CHANGED_TERMS_RESUBMITTED"
      salesOrderId: string
      newSubmissionId: string
      newSubmissionNo: number
      newSubjectHash: string
      workflowActionId: string
      salesOrderReviewStatus: "PENDING_PROCUREMENT_CONFIRMATION"
      newProcurementWorkItemId: string
      newProcurementWorkItemStatus: "UNCLAIMED" | "PENDING"
    }
  | {
      outcome: "LOW_MARGIN_MANAGER_CONFIRMATION_CREATED"
      salesOrderId: string
      newSubmissionId: string
      newSubmissionNo: number
      newSubjectHash: string
      workflowActionId: string
      salesOrderReviewStatus: "PENDING_LOW_MARGIN_MANAGER"
      lowMarginManagerWorkItemId: string
      lowMarginManagerWorkItemStatus: "UNCLAIMED" | "PENDING"
    }
  | {
      outcome: "VOIDED_AFTER_PROCUREMENT_REJECTION"
      salesOrderId: string
      workflowActionId: string
      salesOrderCommercialStatus: "VOIDED"
    }

type ResolveProcurementRejectionResult =
  | ({
      operationId: string
      status: "COMMITTED"
      committedAt: string
    } & ProcurementRejectionResolutionBusinessResult)
  | {
      operationId: string
      status: "NOT_COMMITTED" | "RESULT_UNKNOWN"
      reasonCode?: string
      nextActions: string[]
    }

type LowMarginManagerConfirmationDecisionBase = {
  workItemType: "LOW_MARGIN_MANAGER_CONFIRMATION"
  salesOrderId: string
  rejectedProcurementConfirmationId: string
  lowMarginSubmissionId: string
  expectedSalesOrderLockVersion: number
}

type LowMarginManagerConfirmationDecision =
  LowMarginManagerConfirmationDecisionBase &
  (
    | {
        decision: "APPROVE"
        comment?: string
      }
    | {
        decision: "REJECT"
        reasonCode: string
        comment: string
      }
  )

type CompleteLowMarginManagerConfirmationCommand =
  CompleteWorkItemEnvelope<LowMarginManagerConfirmationDecision>

type LowMarginManagerConfirmationBusinessResult =
  | {
      outcome: "LOW_MARGIN_APPROVED_AND_PROCUREMENT_RESUBMITTED"
      salesOrderId: string
      lowMarginSubmissionId: string
      subjectHash: string
      workflowActionId: string
      salesOrderReviewStatus: "PENDING_PROCUREMENT_CONFIRMATION"
      newProcurementWorkItemId: string
      newProcurementWorkItemStatus: "UNCLAIMED" | "PENDING"
    }
  | {
      outcome: "LOW_MARGIN_REJECTED_TO_SALES"
      salesOrderId: string
      lowMarginSubmissionId: string
      subjectHash: string
      workflowActionId: string
      salesOrderReviewStatus: "REJECTED"
    }

type CompleteLowMarginManagerConfirmationResult =
  CompleteWorkItemResult<LowMarginManagerConfirmationBusinessResult>

type SalesChangeFormalCommand = {
  salesChangeOrderId: string
  salesOrderId: string
  baseRevisionId: string
  salesChangeSubmissionId: string
  action: "SUBMIT_IMPACT_REVIEW" | "REJECT" | "ACTIVATE"
  expectedLockVersion: number
  subjectHash: string
  operationId: string
  idempotencyKey: string
  reasonCode?: string
}

type CardSalesApprovalDecisionBase = {
  salesOrderId: string
  salesOrderSubmissionId: string
  expectedSalesOrderLockVersion: number
  expectedSubmissionNo: number
  comment?: string
}

type CardSalesApprovalDecision = CardSalesApprovalDecisionBase &
  (
    | {
        workItemType: "CARD_SALES_MANAGER_APPROVAL"
        expectedReviewStatus: "PENDING_SALES_LEAD"
        reviewDecision: "APPROVE"
      }
    | {
        workItemType: "CARD_SALES_MANAGER_APPROVAL"
        expectedReviewStatus: "PENDING_SALES_LEAD"
        reviewDecision: "REJECT"
        reasonCode: string
      }
    | {
        workItemType: "CARD_SALES_OPERATION_APPROVAL"
        expectedReviewStatus: "PENDING_OPERATIONS"
        reviewDecision: "APPROVE"
      }
    | {
        workItemType: "CARD_SALES_OPERATION_APPROVAL"
        expectedReviewStatus: "PENDING_OPERATIONS"
        reviewDecision: "REJECT"
        reasonCode: string
      }
  )

type CompleteCardSalesApprovalCommand =
  CompleteWorkItemEnvelope<CardSalesApprovalDecision>

type CardSalesApprovalBusinessResult =
  | {
      outcome: "MANAGER_APPROVED"
      salesOrderId: string
      salesOrderReviewId: string
      workflowActionId: string
      salesOrderStatus: "PENDING_OPERATIONS"
      nextWorkItemId: string
      nextWorkItemStatus: "UNCLAIMED" | "PENDING"
    }
  | {
      outcome: "OPERATIONS_APPROVED_AND_EFFECTIVE"
      salesOrderId: string
      salesOrderReviewId: string
      workflowActionId: string
      salesOrderStatus: "EFFECTIVE"
      salesOrderRevisionId: string
      receivableAccountId: string
      executionProjectionOperationId: string
    }
  | {
      outcome: "REJECTED_TO_SALES"
      salesOrderId: string
      salesOrderReviewId: string
      workflowActionId: string
      salesOrderStatus: string
    }

type CompleteCardSalesApprovalResult =
  CompleteWorkItemResult<CardSalesApprovalBusinessResult>
```

`CreateSalesOrderCommand.contract` 仅包含已有合同身份：`contractId` + `requestedContractRevisionId`。服务端重验合同当前可选资格与精确当前修订。新合同不在建单命令内嵌上传；须先调用 W04 `UploadContractPdf`（UI 为共用 `ContractUploadDialog`），归档成功后再引用。

初始提交响应固定返回操作号、销售单号、冻结提交、当前状态、形成的待办以及下一步。销售变更必须保存 `sales_change_order`、不可变提交、基准版本和同一内容指纹；实物与服务由采购确认履约影响，卡券由运营确认商城执行影响，之后均由财务复核，生效事务才形成新销售版本和应收差额。`SalesChangeFormalCommand` 只处理销售变更单，不得用于初始卡券销售审批。

`ResolveProcurementRejectionCommand` 的三个分支必须以服务端当前驳回事实强判别，并各自在一个事务内完成：

1. 共同锁定销售单、被驳回的不可变提交、`procurement_confirmation` 和当前销售草稿，校验销售单尚未生效、旧 W07 任务已 `COMPLETED`、驳回指纹一致且没有有效后继任务；
2. `RESUBMIT_CHANGED_TERMS` 由服务端比较旧提交与当前草稿，至少存在商品/服务或销售价格变化且有客户重新确认依据时，才冻结递增提交号的新 `sales_order_submission`、计算新 `subjectHash`、追加 `workflow_action` 并创建唯一新 `PROCUREMENT_CONFIRMATION`；
3. `REQUEST_LOW_MARGIN_ACCEPTANCE` 必须确认当前商业条件与旧提交完全一致，再把本次承接理由和提交身份纳入新的审批对象，冻结新提交并计算不复用旧值的 `subjectHash`，追加动作并创建唯一 `LOW_MARGIN_MANAGER_CONFIRMATION`；此事务绝不创建采购确认任务；
4. `VOID_AFTER_REJECTION` 必须确认无有效低毛利/采购后继任务，再追加作废动作并把生效前销售单置为作废；历史提交、确认、任务和理由保留不变。

`CompleteLowMarginManagerConfirmationCommand` 完全复用 W02 完成信封。服务端必须在同一事务校验领取人、租约、低毛利任务类型与唯一 `completionAction`、任务版本/指纹、新提交、原驳回链、销售单版本、商业条件未变及岗位分离。上级通过时写确认事实和 `workflow_action`、完成当前低毛利任务，并为同一 `lowMarginSubmissionId` / 新 `subjectHash` 创建唯一 `PROCUREMENT_CONFIRMATION`；上级驳回时写驳回事实并完成低毛利任务，但不创建采购任务。两个分支都不使销售单生效，也不形成应收或采购创建依据。

旧的 `PROCUREMENT_CONFIRMATION` 和 `LOW_MARGIN_MANAGER_CONFIRMATION` 一旦完成便永久留痕，任何路径都不能重新开启、改绑新提交或复用其 `workItemId`。服务端以本次业务操作幂等键返回同一 `ResolveProcurementRejectionResult`，并以 W02 信封幂等键返回同一低毛利完成结果；同时对“提交 + 固定任务类型”的有效关系做唯一约束。超时或断网只查询原 operation/idempotency identity；`RESULT_UNKNOWN` 期间不得新增提交、生成第二个上级任务或手工补建采购任务。

`CompleteCardSalesApprovalCommand` 完全复用 W02 信封：`workItemId`、`claimToken`、`leaseVersion`、期望任务版本/指纹和 `idempotencyKey` 位于共享外层，销售单、冻结提交、领域版本和审批结论位于 `decision`。服务端必须校验固定任务类型、注册处理器及唯一 `completionAction`；W05 不根据对象状态自行构造可见动作。

领导或运营作出决定时，任务租约校验、`sales_order_review`、`workflow_action`、任务完成和销售状态迁移必须在同一事务中完成。领导通过还要原子创建唯一 `CARD_SALES_OPERATION_APPROVAL`；运营通过还要原子形成首个不可变销售版本、应收和执行投影 outbox。驳回不创建下一审批节点，后续销售修改和重新提交必须从领导审批重新开始。前端不得只根据 HTTP 成功自行推进流程。

### 8.4 缓存、新鲜度与前端边界

- TanStack Query Key 包含用户、角色、权限/数据范围版本、列表全部筛选；对象 Key 包含 `salesOrderId` 和可见分区。
- 正式销售当前版本、主状态、履约、应收开放余额与回款/开票进度属于同步查询事实；提交成功后按服务端结果定向失效。
- W01/W15/W16/W28 等经营摘要允许不超过 1 分钟异步，W05 只显示其水位，不拿分析投影覆盖正式销售事实。
- 关联执行投影显示 `projectionUpdatedAt`；陈旧或失败时明确分区风险，不回退销售版本或应收。
- 前端只格式化金额、日期、数量、版本和服务端状态文案；金额、税额、关闭资格、审批资格、履约完成和主责判定均由服务端计算。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 页头、列表/中心分区与 6–8 行同构 Skeleton | 应用壳导航可用 | 查询完成原位替换 |
| 刷新 | 保留旧数据并显示各分区水位 | 查看；正式动作提交时重验 | 成功更新，失败标陈旧 |
| 无销售单 | 区分尚无记录、筛选无结果和无数据范围 | 有权时新建、清除筛选 | 条件变化后重查 |
| 查询失败 | 无缓存使用 `BusinessFailureState`；有缓存保留旧值 | 重试；陈旧对象只读 | 查询成功 |
| 分区失败 | 主对象保留，只替换采购/票款/协同失败区 | 重试分区、打开对应 W | 分区恢复 |
| 字段级隐藏 | 标签与结构保留，值掩码或不返回 | 其它授权动作 | 权限更新后重查 |
| 商城主责只读 | 页头显示主责和同步水位，编辑禁用 | 查看版本、去映射/错误工作面 | 正式迁移后重查 |
| 草稿保存中/失败 | 显示保存状态，输入不丢，不重复提交 | 重试、复制非敏感摘要 | 保存成功 |
| 校验失败 | `ValidationSummary` + 行级错误 | 修正并重提 | 校验通过 |
| 编辑租约丢失 | 本地输入只读保留 | 重新领取、复制安全内容 | 取得新租约并重验 |
| 采购确认已驳回、待销售处理 | 固定展示旧提交/指纹、驳回原因、草稿差异和三条互斥出路 | 改品/改价重提、申请照原条件低毛利承接、不做并作废 | 任一正式动作确定提交后刷新处理链 |
| 改品/改价重提成功 | 固定展示新提交号/新指纹、旧驳回链和新 `PROCUREMENT_CONFIRMATION` 身份 | 打开 W07 新任务、留在销售单 | W07 只处理新任务；旧任务继续显示已完成驳回 |
| 低毛利上级确认待处理 | 新提交只读，展示新指纹、承接理由、成本/毛利证据和正式任务状态 | 有权上级领取并通过/驳回；销售只读等待 | 通过后创建新采购确认，驳回后回销售三条出路 |
| 低毛利上级通过 | 固定展示上级确认事实、已完成低毛利任务和新 `PROCUREMENT_CONFIRMATION` | 打开 W07 新任务 | 采购基于新提交独立确认，不继承旧驳回任务决定 |
| 低毛利上级驳回 | 固定展示驳回事实；无采购确认后继任务 | 销售重新选择固定三条出路 | 新动作必须生成新提交/新任务或作废，不复用已完成低毛利任务 |
| 低毛利上级决定 `RESULT_UNKNOWN` | 保留新提交、意见和原任务位置，不显示已完成或新采购任务 | 按 W02 信封幂等键查询同一决定结果 | 明确通过/驳回后再刷新；未知期间不得补建任务或改走其它出路 |
| 采购驳回处理结果不确定 | 保留旧提交、草稿和原 operation/idempotency identity，不渲染新提交/任务/作废 | 查询原结果、同幂等键重试 | 得到 `COMMITTED` / `NOT_COMMITTED` 后再刷新 |
| 采购驳回后已作废 | 页头和处理卡固定显示作废原因、旧提交与驳回历史 | 只读查看与审计 | 不恢复草稿或创建后继任务 |
| 卡券审批任务待领取/他人领取 | 冻结提交只读；显示任务类型、领取人和到期时间 | 有权时领取；否则返回队列/查看对象 | W02 原子领取返回 `claimToken` 后才显示正式决定动作 |
| 卡券审批租约丢失 | 保留只读提交与未提交意见，隐藏/禁用通过和驳回 | 重新领取并重查任务与销售单版本 | 新租约、任务指纹和领域版本均一致后恢复 |
| 版本/指纹冲突 | 结构化展示基线与当前事实 diff | 刷新、重做或放弃 | 基于新版本确认 |
| 待确认/审批 | 当前冻结版本只读，进度显示责任节点；卡券领导/运营任务可在对象中心嵌入同一 W02 处理器 | 查看任务；仅任务返回的动作可执行 | 共享完成信封返回正式决定事务 |
| 卡券审批成功 | 固定显示审批阶段、决定、审批记录、销售新状态和下一任务/生效对象 | 打开下一任务、销售版本或执行投影 | 用户确认结果后继续；不只显示 toast |
| 卡券审批 `RESULT_UNKNOWN` | 固定显示原幂等操作的追踪号；销售状态和任务位置均不乐观推进 | 查询原操作最终结果、联系支持 | 取得 `CompleteWorkItemResult` 后才刷新/离开 |
| 正式动作成功 | `FormalActionResult` 固定展示对象、版本、时间与下一步 | 打开关联对象、继续处理 | 用户明确关闭结果 |
| 正式结果不确定 | 不乐观推进状态，固定显示操作号 | 查询原结果、同幂等键重试 | 得到可验证终态 |
| 履约阻断 | 履约分区显示原因、影响明细与责任方 | 进入 W08/W09/W06 | 正式事实修复后重查 |
| 应收未结清 | 显示“系统关闭条件未满足”和开放余额；页面不存在人工关闭动作 | 进入 W11 | 应收结清后由服务端重新判定并自动关闭 |
| 投影/同步陈旧 | 协同分区显示水位与错误，不污染主对象 | 进入 W17/W23/W29 | 投影追平 |
| 后台导出 | `BackgroundJobProgress` 显示快照、进度和任务号 | 查看任务、取消未开始项 | 完成后下载或重试 |
| 权限收回 | 清除敏感缓存和编辑状态，转无权限 | 返回有权工作面 | 权限恢复后重查 |

## 10. 响应式、键盘与无障碍

### 10.1 响应式

| 视口 | 布局变化 | 必须保留 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 列表至少 6–8 行；detail 约 38/62；中心双列 | 单号、客户、业务性质、主责、三轨进度、主动作 | 无 |
| 1280×800 | 次要筛选收起，detail 覆盖更多列表 | 固定身份/操作列，金额与三轨状态 | 次要来源列进入列设置 |
| 1024×768 | 导航图标态，中心单列，detail 覆盖式 | 对象身份、当前版本、下一步和 blocker | 时间线与次要摘要折叠 |
| 768×1024 | 导航抽屉，列表横滚，编辑明细卡片化 | 左身份列、右操作列、总计与正式结果 | 筛选进入面板，分区单列 |
| 375×812 | 单列只读摘要与简单结果查看 | 单号、客户、业务性质、主责、三轨进度、错误和下一步 | 不提供复杂建单、销售变更、明细表格和正式审批；保留任务并引导桌面 |

### 10.2 键盘与焦点

- `/` 聚焦列表搜索；方向键或 `j/k` 移动行；Enter 打开 detail；Esc 关闭并恢复原行焦点。
- 对象锚点使用真实链接/Tab 语义并标 `aria-current`；分区更新不抢焦点。
- M5 中 `⌘S` 保存草稿，`⌘↵` 仅打开正式确认层，不绕过确认与服务端校验。
- 校验失败先聚焦摘要，再可跳到首个错误字段；版本冲突、结果未知和权限收回均用文字说明。
- 业务性质、主责、状态和三轨进度不能只靠颜色；触控目标至少 44×44px。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 客户 / 合同 | W03 / W04 | 客户 ID、合同及精确修订 ID | 返回刷新关联摘要 |
| 统一待办 / 卡券审批 / 低毛利确认 / 二次确认 | W02 / W07 | 销售单、不可变提交、任务和队列上下文；采购驳回返回稳定确认身份 | 决定后刷新主状态，不本地完成任务；新 W07 任务只由改品/改价重提事务或低毛利上级通过事务创建 |
| 采购单 / 履约 | W08 / W09 | 销售明细、分配、采购或作业稳定 ID | 返回履约子区并重查 |
| 客户验收 | W06 | 非卡券销售明细和作业证据 | 同一 W05 页签回履约摘要 |
| 客户往来 / 票款复核 | W11 / W13 | 客户、应收、销售单和任务 ID | 返回票款子区并重查三轨 |
| 商城同步与映射 | W17 | 商城来源身份、同步版本、差异任务 | 修复后形成/更新同一销售身份 |
| 执行投影 / 主责迁移 | W23 / W24 | 销售版本、投影或迁移批次 ID | 返回协同区，不传主责结论快照 |
| 商城消费订单 | W25 | 销售单稳定 ID、消费事实水位 | 返回协同区；消费汇总不改履约/关闭判定 |
| 经营分析 | W15 / W16 / W28 | 客户、期间、销售单稳定 ID | 返回保留分析筛选和焦点 |
| 接口错误 | W29 | 原操作、投影/同步错误任务 | 修复后 W05 重查正式事实 |

实物/服务主链为 W03/W04 → W05 → W07 → W08 建单与财务审核 → W09 → W06；W07 驳回则回到 W05 固定三路：改品/改价后以新提交直接进入新 W07 任务，照原条件承接先进入 W02/W05 的 `LOW_MARGIN_MANAGER_CONFIRMATION` 且仅上级通过后进入新 W07 任务，不做则作废。任何回路都不复用旧 W07 任务或旧提交指纹。W11 从销售单生效后即可并行处理票款，最终与履约事实共同驱动服务端自动关闭。一期卡券为商城 → W17 → W05 → W13/W11；二期 ERP 卡券为 W05 → W02 审批 → W23。跨工作面只传稳定身份和返回上下文。

二期初始卡券审批与低毛利上级确认都在 W02 队列和 W05 对象中心共用各自注册的 `handlerKey`、`workItemId`、领取租约、`CompleteWorkItemEnvelope` 及最终结果查询；在 W05 嵌入并不把任务完成权转移给销售单对象接口。低毛利处理跨 W 只传销售单、驳回确认、新提交和任务稳定身份，目标工作面重新查询指纹、租约、成本/毛利证据与权限。

## 12. 验收清单

### 12.1 业务与数据

- [x] 卡券与非卡券在同一列表、对象中心、编号和版本体系，业务性质创建后不可修改。
- [x] 创建来源与当前主责分列；任一时点只有一个写入主责。
- [x] 一期商城主责卡券商业字段只读；二期迁移只改主责、不换身份、单号或销售版本。
- [x] 每个卡券销售版本恰好一条卡券明细，且页面不出现玩法、卡号、卡密或手机号。
- [x] 非卡券以验收完成履约；卡券以履约期限到期完成，不因已消费完提前完成。
- [x] 履约完成且应收结清才能关闭；开票未完成不阻塞关闭。
- [x] 二期卡券单协同子区展示消费汇总与 W25 入口，但消费、退款或余额恢复不增加第三个关闭条件。
- [x] 新建销售单仅选择已有有效合同；选择框旁加号复用 `ContractUploadDialog`，无已有合同时不阻断进入 M5。
- [x] 上传 Dialog 归档成功后刷新可选合同列表并可自动选中；销售单创建与合同上传幂等分离。
- [x] 建单页紧凑布局：主栏单卡（单据头 + 明细 + 合计 footer 一体）、无分区锚点 nav、右侧摘要仅宽屏；字段与 §4.3 一致。
- [x] 非卡券明细单位随 SKU 基础单位只读带出，不可码表改写；建单页不提供仓发/直发选择，仅可填交付日期。
- [x] 含税小计行内不叠「含税」Badge（`MoneyValue` 不传 `taxBasis`）。
- [x] 历史销售版本保存精确合同/基础资料修订和关键快照，不被当前值覆盖。
- [ ] 正式销售单没有直接编辑或人工关闭入口；商业变化必须通过销售变更单并按业务类型完成影响确认和财务复核。
- [ ] `CARD_SALES_MANAGER_APPROVAL` 与 `CARD_SALES_OPERATION_APPROVAL` 在 W02/W05 共用处理器和完成信封；对象中心不存在绕过任务的审批按钮。
- [ ] 领导通过原子形成运营任务；运营通过原子形成首个销售版本、应收和投影 outbox；审批事实、`workflow_action` 与任务完成不分离。
- [x] 采购驳回后页面只提供改品/改价重提、照原条件申请低毛利承接、不做并作废三条固定出路；不存在通用重提或恢复旧 W07 任务入口。
- [ ] 改品/改价重提必须校验确有相应变化和客户重新确认依据，并原子形成递增提交号、新 `subjectHash` 与唯一新 `PROCUREMENT_CONFIRMATION`。
- [ ] 照原条件承接必须校验商业条件未变，先原子形成新提交/新指纹及唯一 `LOW_MARGIN_MANAGER_CONFIRMATION`；上级通过前不得创建采购确认、使销售生效或形成应收。
- [ ] 低毛利上级通过与低毛利任务完成、唯一新 `PROCUREMENT_CONFIRMATION` 创建处于同一事务；上级驳回不创建采购任务，且已完成低毛利任务不能复用。
- [ ] “不做”仅在生效前且无有效后继任务时原子作废，完整保留旧提交、采购驳回和任务历史。

### 12.2 交互、权限与恢复

- [x] 1440×900 首屏展示筛选、至少 6–8 行、固定身份/操作列和 detail 主事实。
- [x] 同一销售单从任意来源重复打开只聚焦一个 TaskTab；关闭后恢复来源焦点。
- [ ] 所有操作有权限、前置、确认、成功结果和失败恢复；正式成功不只靠 toast。
- [ ] 租约丢失、版本冲突、结果未知、权限收回和分区失败均不丢安全输入或伪造状态。
- [ ] 卡券审批 `claimToken` 仅存会话内存，`RESULT_UNKNOWN` 时不移动任务、不切换销售状态且可用原幂等键恢复。
- [ ] 采购驳回三路及低毛利决定的重复点击、超时恢复和结果查询使用原 operation/idempotency identity，不重复创建提交或任务。
- [ ] 查询、对象、导出均受权限/数据范围版本约束，目标页重新查询金额和状态。
- [ ] 1440、1280、1024、768、375 五档视口与键盘焦点恢复通过验证。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | 非卡券销售变更的影响预览需要把哪些采购分配、库存预占和应收差额固定展示为首屏项？ | 变更单布局与影响摘要 | 销售 + 采购 + 财务 | 完整影响由服务端计算；只追加版本和差额事实，不回退已发生履约/票款 |
| Q2 | 销售提交后在何种条件下允许撤回二次确认或二期审批？ | 待办关闭、租约与状态矩阵 | 销售 + 采购 + 运营 | 未领取且未产生正式决定时由服务端返回撤回能力 |
| Q3 | 卡券单同时存在退款/余额恢复异常时，协同子区如何排序和突出责任人？ | W05/W25/W29 异常摘要与视觉优先级 | 运营 + 财务 | 异常只作协同提示；关闭仍只取正式履约与应收事实，不能新增第三个条件 |
| Q4 | 移动端允许哪些低风险销售动作？ | 375px 能力与权限策略 | 产品 + 业务负责人 | 默认只读；正式建单、销售变更和审批在桌面完成 |

待确认事项未决时采用保守禁用或只读规则，不把建议写成正式状态机。

## 14. 业务依据

- `erp-phase-1.md` §4.4、§7：销售单统一模型、非卡券二次确认、采购驳回后的改品/改价重提、照原条件低毛利上级确认、不做作废三条固定出路，以及采购履约；其余章节约束客户验收、票款三轨、一期商城主责卡券同步及关闭规则。
- `erp-phase-2.md`：ERP 卡券建单审批、执行投影、主责一次性迁移、消费回流与供应商履约边界。
- `erp-data-model.md`：`sales_order` 稳定身份与修订、卡券单一明细不变量、主责事实、`work_item`、应收与履约投影。
- `erp-ui-design.md` §3–§5、§11、§13：TaskTabs、M2/M4/M5、统一销售单工作面、权限与状态契约。
- `erp-ui-flows.md` §2–§4、§10–§12：实物/服务、一期卡券、审批与执行投影、主责迁移和经营下钻完整路径。
