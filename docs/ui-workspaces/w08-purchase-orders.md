# W08 · 采购单

> 状态：已定稿
> 页面模式：M2 高密度列表 + M4 对象中心 + M5 编辑工作区
> 主要路由：`/procurement/orders`；对象 `/procurement/orders/:purchaseOrderId`
> 主要角色：采购经办、当前审批人、财务经办；仓储/销售按关联对象只读
> 最后更新：2026-08-17
> 权威合同：`../approval-workflow-contract.md` §4.4.3、§16.4

## 1. 定位与目标

### 1.1 用户目标

- 销售单生效后，采购在本页选源（供应商供给、最新成本、供货数量、预计交期、履约方式），按供应商、采购类型、付款条件和履约责任拆成采购单，补齐采购费用并提交审批。
- 当前审批人在 W01 或本页通用审批区核对不可变采购提交，完成通过或驳回。
- 任一有权用户在采购单中心一次看清采购版本、关联销售、应付/付款/进项发票、履约和变更现状，不需要去多个菜单拼接事实。
- 列表单击后在 detail 半屏内读完主事实；只有编辑、审批、付款、履约或变更等作业才进入对象中心/对应工作面。

### 1.2 业务目标

- 采购单同时承担采购申请、通用审批对象和供应商采购依据；正式内容由不可变提交和生效版本承载。
- 一张采购单只属于一张实物与服务销售单、一个供应商、一种采购类型、一套付款条件和一个履约责任。
- 已发布定义的末节点通过时形成采购生效版本和应付原始分录；审批与实际付款保持独立事实。
- 生效后的供应商、商品、数量、金额或履约责任变化通过采购变更单及新版本表达，不覆盖已发生履约、付款或发票事实。

### 1.3 不在本工作面完成

- 不进入已废止的 W07 确认队列；选源只在本页创建采购单时完成，行引用已生效销售明细，不引用 `ProcurementConfirmationLine`。
- 不依赖未注册的“采购建单任务”：建单从 W05 已生效销售单进入。采购单提交后走 `PurchaseOrder` 已发布审批定义，任务类型为 `DocumentApproval`。
- 不在采购单编辑器中修改销售单、销售提交或客户承诺。
- 不在审批时修改采购内容；当前审批人只能通过或驳回不可变提交。
- 不把付款、进项发票和核销做成采购单字段编辑；正式处理进入 W12，W08 只展示进度和入口。
- 不在采购单直接登记入库、仓发、代发、电子交付或服务事实；正式履约进入 W09。
- 不直接改已生效采购单；后续变化从本中心发起采购变更。

## 2. 用户、权限与数据范围

### 2.1 角色与动作

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 采购经办 | W08 列表 / W05 已生效销售单 / W01 | 本人负责或采购责任域内采购单 | 选源并创建、编辑草稿、提交审批、在允许时撤回后修改、发起采购变更、去交付 |
| 采购负责人 | W08 负责范围 | 本采购团队范围 | 查看、对象级分派采购草稿责任；代办按权限，不自动越过经办约束 |
| 当前审批人 | W01 `DocumentApproval` → W08 审批视图 | 本人当前开放任务且具备对象读取权 | 只读核对、通过、驳回 |
| 财务经办 | W12 / W08 票款子区 | 财务数据范围内已生效采购单 | 查看应付、登记付款/进项发票并核销 |
| 仓储 | W09 / W08 履约子区 | 与仓储入库/仓发相关采购单 | 查看必要采购与预占上下文；不看无关供应商资金信息 |
| 销售 | W05 关联采购 | 自己负责销售单的关联采购进度 | 只读采购/履约进度；成本按字段权限隐藏 |
| 管理层 / 审计 | 分析或 W19 下钻 | 授权范围 | 只读 |

### 2.2 权限表达

| 情况 | W08 行为 |
| --- | --- |
| 无采购单模块权限 | 不展示导航、命令和快捷入口；直接 URL 显示无权限 |
| 有模块权限但数据范围为空 | 显示“当前数据范围暂无采购单”，不返回全公司 0 指标或敏感字段 |
| 有对象查看权但无动作权 | 对象中心可读；动作保留并禁用，`actionBlocker` 说明角色/状态/门禁 |
| 销售或仓储无成本字段权限 | 保留“采购金额/供应商账户”等标签，值掩码或整组最小化；不在客户端返回原值 |
| 当前审批人也是提交经办人 | “通过/驳回”禁用并明确岗位分离原因；不得只靠前端判断 |
| 功能能力尚未启用 | 入口隐藏，不用永久灰按钮表达未来路线图 |
| 页面期间权限收回 | 停止保存等写动作，清除成本、银行、联系人等敏感缓存，切无权限态 |

数据查询由服务端按组织、采购责任域、销售对象参与权、审批任务责任、财务范围和字段权限裁剪。导出下载时必须再次校验当前权限和遮罩规则。

### 2.3 编辑权、审批处理权与岗位分离

- 采购草稿编辑使用采购对象专属的 `draftEditToken`、草稿责任人与 `lock_version`；TaskTabs 打开也不等于取得编辑权。
- 审批从 W01 的 `DocumentApproval` 任务进入；任务创建即指定到人。决定只走通用 `submit_decision`。不得提供已废止的“开始处理”或“退回团队”能力。
- 编辑权与审批处理权不能复用；采购提交形成不可变 `purchase_order_submission` 后，采购编辑结束，审批任务引用该提交。
- 服务端在决定事务内重验当前审批人不是该采购提交经办人，并执行配置化岗位分离规则。
- 草稿编辑权不进入 URL、日志、长期缓存或埋点；其用途和服务端校验域严格隔离。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 浏览采购单 | 侧栏 / 全局搜索 | `/procurement/orders`；筛选写入 URL | 浏览器后退恢复筛选和列表位置 |
| 列表核对 | 单击行 / Enter | 当前列表上打开 `detail` 半屏，不新建页签 | 关闭回原行焦点 |
| 打开对象中心 | detail 底栏 / 行内“详情” | `/procurement/orders/:purchaseOrderId`，新建或聚焦稳定对象页签 | 返回列表保留查询 |
| 从 W05 建单/查看 | 销售单“采购与履约”子区 | 携带已生效销售单 ID；已有采购单聚焦对象，无则进入选源与拆单 | 返回 W05 履约子区 |
| 处理审批 | W01 `DocumentApproval` | 聚焦对象页签并打开 `section=approval&workItemId=...` | 完成后返回 W01 下一项 |
| 撤回后修改草稿 | W08 负责范围 / 对象中心审批摘要 | 只有服务端允许撤回且撤回成功后，才在同一对象页签进入 `mode=edit`；不创建 `/edit` 平行路由 | 保存/提交后回对象阅读态 |
| 生效后发起变更 | 对象中心“变更与异常” | 同页签进入采购变更工作副本，携带基准版本 | 退出回原采购版本中心 |
| 刷新 | 对象、编辑或审批态 | 恢复采购单 ID、子区、模式、提交/任务稳定 ID；重新取处理权和权限 | 当前页签 |

TaskTabs 身份为 `purchase-order:{purchaseOrderId}`；新草稿在服务端取得稳定采购单 ID 后使用相同身份。草稿阶段标题必须采用 `PO · 采购草稿 + 短 ID`（不得预分配正式采购单号）；首次提交审批成功时服务端形成不可复用正式 `purchaseNo`，页签仅更新标题文案，不得改变页签身份。

列表预览、纸质预览和确认 Dialog 不创建任务页签。脏草稿关闭页签必须确认；审批视图只读采购内容，不产生脏编辑状态。

## 4. 页面布局

### 4.1 M2 列表（1440×900）

布局与筛选密度遵循 `docs/ui-filter-design.md`（M2：工具条在 frame 内；主筛 ≤3；枚举 ≥5 用 Combobox / 高级筛选）。

```text
┌ PageHeader：采购单                              [导出] [新建采购单] ───┐
├ MetricStrip：全部 | 草稿 | 审批中 | 待履约 | 先款门禁阻塞（真筛选）│
├ BusinessTableFrame ───────────────────────────────────────────────────┤
│ title · 筛选摘要 description                                          │
│ ListToolbar 第1层：搜索 | 状态(Combobox) | 供应商 | …（主筛≤3）       │
│            actions：共 N 条 | 清除筛选                                 │
│ 第2层：高级筛选（类型/履约责任/交期/进度…）| FilterChip（深链）      │
│ 采购单号 | 状态 | 供应商 | 来源销售单 | 类型 | 含税金额 | 付款 | 履约 | 操作│
│ PO…      | 审批中| …      | XS…       | 实物 | ¥…      | 未付 | 未开始| 详情│
│ …                                                                  │
└ 分页 · 结果数量 · 数据更新时间 ──────────────────────────────────────────┘
```

1440×900 必须露出 6–8 条有效行；采购单号和行级主动作固定，金额等宽右齐。指标可点击时使用按钮语义和筛选摘要。状态枚举项多，**禁止** 6～7 段 Toggle 横排；与指标重叠的粗筛以指标为准，工具条用 Combobox 或高级筛选承接细筛。

### 4.2 detail 半屏预览

```text
┌ PO… · v2 · 已生效 · 供应商 · 来源 XS… ─────────────────────────────┐
├ 左 38%（独立滚动）             │ 右 62%（独立滚动）                 │
│ 审批/付款/开票/履约四轨         │ 采购明细：商品/服务成本、物流费用 │
│ 采购类型、付款条件、履约责任    │ 数量、含税单价、税率、税额、小计 │
│ 先款门禁与应付摘要              │ DocumentTotals                    │
│ 关联销售、履约和变更数量        │ 销售分配摘要                      │
├ [关闭] [打印预览] [打开中心 / 去审批 / 去交付] ─────────────────────┤
└──────────────────────────────────────────────────────────────────────┘
```

detail 必须能读完当前版本主事实；不得只展示三五个摘要字段再强迫打开中心。纸质版式使用宽 Dialog 的 `PaperDocument`，不塞入窄 Sheet。

### 4.3 M4 对象中心

遵循 `erp-ui-design.md` §4.5.1：导航壳 `object-chrome`，对象身份仅 `DocumentHeader compact`（标题优先供应商快照，单号/版本/来源销售在 meta）。

```text
┌ PageHeader object-chrome：采购 › 采购单 › PO…     [返回] [付款] [履约] ─┐
├ DocumentHeader compact：供应商快照 [主状态]                              │
│  PO… · v2 · 来源销售…     类型/履约轨 · [主动作]                         │
│ 供应商 · 来源销售单 · 采购类型 · 履约责任    [去供应商往来] [更多 ▾]  │
├ 概览 | 明细与分配 | 履约 | 应付与票款 | 变更与异常 | 审计 ──────────┤
│ PrepaymentGate / 审批摘要 / 数据更新时间                                 │
│ DocumentSection × N                                                   │
└───────────────────────────────────────────────────────────────────────┘
```

| 子区 | 内容 | 主入口 |
| --- | --- | --- |
| 概览 | 供应商快照、来源销售、采购类型、付款条件、履约责任、关键日期、金额 | 编辑草稿 / 查看提交 |
| 明细与分配 | 商品/服务成本行、物流费用行、进项税与销售明细分配 | 草稿时编辑；生效后只读 |
| 履约 | 入库/发货/电子/服务事实、预占和剩余量 | 去 W09 定位任务 |
| 应付与票款 | 应付、已付、待付、进项发票与核销进度 | 去 W12 同供应商会话 |
| 变更与异常 | 采购变更、采购退货、付款/发票纠错关系 | 从原单发起 |
| 审计 | 提交、轮次驳回、节点决定、版本、任务结果的业务审计 | 只读 |

### 4.4 M5 编辑工作区

- 顶部固定显示“采购草稿 / 撤回后待修改”、来源销售单和选源覆盖摘要。
- 表头编辑供应商、采购类型、付款条件和履约责任；这些字段是拆单维度，改变任一项时必须提示拆单影响，不能静默搬行。
- 明细表区分“商品/服务成本”与“物流费用”两类行。
- 商品/服务行必须选择已生效销售明细，并在本页选源：供应商供给、最新成本、供货数量、预计交期和履约方式。提交时服务端重验供给仍有效。
- 物流费用独立行、独立税率；一件代发含包装/发货费用时不得重复添加物流费用。
- 右侧/底部固定显示含税、不含税、税额合计与销售分配守恒；前端只显示服务端规范化结果。
- 自动保存 + `⌘S` 显式保存；提交后形成不可变采购提交，编辑区转只读审批等待态。

### 4.5 通用审批视图

审批人由 W01 连续处理，W08 提供采购对象和通用审批区：

- 采购提交头、行、销售分配全部只读。
- 左/上区域显示供应商、采购类型、含税/不含税金额、税额和选源结果。
- 右/下区域显示服务端返回的节点名称、付款条件、先款门禁、费用行、应付影响和校验摘要。
- 当前审批人只有“通过”“驳回”“打开单据”能力，没有字段编辑器；不得提供已废止的“退回团队”能力。
- 成功先展示固定结果，再由 W01 进入下一项；结果不确定时停留当前对象。

## 5. 展示内容与字段

### 5.1 列表与单据头

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 身份 | `purchaseNo` | 采购单号 | `purchase_order.purchase_no` | 稳定业务号；草稿阶段禁止预分配正式号，展示为「采购草稿 + 短 ID」；仅当首次提交审批成功时分配不可复用正式编号 | 对象可见者 |
| 身份 | `revisionNo` | 当前版本 | `purchase_order_revision.revision_no` | 草稿无正式版本时明确“草稿” | 同上 |
| 状态 | `status` | 主状态 | `purchase_order.status` | 固定状态机文案 | 同上 |
| 主体 | `supplierSnapshot` | 供应商 | 当前提交/版本快照 | 历史版本不跟随基础资料改名 | 销售/仓储可按权限只见简称 |
| 来源 | `salesOrderNo` | 来源销售单 | `purchase_order.sales_order_id` + 注册表投影 | 可钻取 W05 | 按来源对象权限 |
| 分类 | `purchaseType` | 采购类型 | `purchase_order.purchase_type` | 实物 / 虚拟 / 线下服务 | 同上 |
| 分类 | `fulfillmentResponsibility` | 履约责任 | `purchase_order.fulfillment_responsibility` | 入仓 / 供应商直发 / 电子交付 / 线下服务 | 同上 |
| 金额 | `grossAmount` / `netAmount` / `taxAmount` | 含税 / 不含税 / 税额 | 当前提交或版本行汇总 | 定点小数；标签明示口径 | 成本权限控制 |
| 进度 | `paymentProgress` | 付款进度 | 应付/付款核销同步投影 | 未付 / 部分 / 已付 | 财务全量；其它角色受限 |
| 进度 | `invoiceProgress` | 进项票进度 | 进项发票核销同步投影 | 未收 / 部分 / 完成 | 同上 |
| 进度 | `fulfillmentProgress` | 履约进度 | 正式履约同步投影 | 未开始 / 部分 / 完成 | 关联角色 |
| 日期 | `expectedDate` | 最近预计交期 | 当前版本行服务端汇总 | 最早未完成交期；不由当前页行临时求值 | 同上 |

### 5.2 表头商务字段

| 字段 | 用户文案 | 数据来源 / 提交去向 | 规则 |
| --- | --- | --- | --- |
| `supplierId` / `supplierRevisionId` | 供应商 / 供应商版本 | 采购主表、提交/版本快照 | 一张采购单唯一供应商；资质能力提交时重验 |
| `purchaseType` | 采购类型 | 采购主表/提交 | 实物、虚拟、线下服务；拆单维度 |
| `paymentTermCode` / `paymentTermSnapshot` | 付款条件 | 采购主表、提交/生效快照 | 含先款/后款及门禁金额或比例 |
| `fulfillmentResponsibility` | 履约责任 | 采购主表/提交 | 入仓、供应商直发、电子、服务；拆单维度 |
| `salesOrderId` | 来源销售单 | 采购主表 | 一张采购单只属于一张销售单，不可跨单合并 |
| `submittedAt` / `submittedBy` | 提交时间 / 经办人 | `purchase_order_submission` | 用于审批与岗位分离 |
| `submissionVersionLabel` | 提交版本摘要 | 采购提交版本 | 审批决定针对不可变内容 |
| `effectiveAt` | 生效时间 | `purchase_order_revision.effective_at` | 最终审批通过事务返回 |

### 5.3 明细、金额与销售分配

| 字段 | 用户文案 | 数据来源 | 规则 |
| --- | --- | --- | --- |
| `lineType` | 商品/服务成本 / 物流费用 | 采购提交/版本行 | 两类行分开展示和计税 |
| `salesOrderLineId` | 来源销售明细 | 已生效销售单行 | 首次商品/服务行必填；物流费用为空 |
| `supplierOfferingRevisionId` | 选中供给 | 本页选源 | 必须为业务时点有效且当前可供 |
| `itemSnapshot` | 商品/服务、规格、单位 | 销售提交、SKU/采购结构化快照 | 历史版本不追随基础资料变化 |
| `quantity` / `baseUnitCode` | 采购数量 / 单位 | 采购提交/版本行 | 商品行 >0，最多 6 位；费用行无数量 |
| `unitCostGross` | 含税采购单价 | 采购提交/版本行 | 最多 4 位小数 |
| `inputTaxRate` | 进项税率 | 采购提交/版本行 | 与销项税率分离 |
| `grossAmount` / `netAmount` / `taxAmount` | 行含税 / 不含税 / 税额 | 服务端规范化计算 | 各行先舍入到分，表头汇总舍入后行 |
| `expectedDeliveryDate` | 预计交期 | 本页选源 → 采购提交/版本 | 改动规则由服务端校验 |
| `salesAllocation` | 对应销售明细与数量 | 提交行 / `purchase_line_sales_allocation` | 两端同销售单；采购分配不超采购量和销售承诺量 |
| `logisticsFeeReason` | 物流费用说明 | 费用行受控字段 | 代发已含费用时阻塞重复登记 |

### 5.4 审批、票款与履约摘要

| 字段 | 用户文案 | 数据来源 | 规则 |
| --- | --- | --- | --- |
| `approvalHistory` | 审批记录 | 审批实例 + `workflow_action` | 显示轮次、节点、处理人、时间、决定和意见 |
| `payableOpenAmount` | 应付未结 | `payable_account` 同步投影 | 最终审批通过后形成；含税金额 |
| `paidAllocatedAmount` | 已付并核销 | 有效已过账付款净核销 | 仅附件/付款申请不算付款完成 |
| `purchaseInvoiceAllocatedAmount` | 已收票并核销 | 进项发票核销 | 与付款分轨 |
| `prepaymentGate` | 履约付款门禁 | 生效版本付款条件快照 + 当前有效付款 | 服务端返回 satisfied/blocker；页面不自行放行 |
| `relatedFulfillment` | 入库/代发/电子/服务进度 | W09 正式事实投影 | 已发生事实不因变更回退 |
| `stockReservationSummary` | 销售预占 | W10 正式预占投影 | 仅入仓合格数量沿采购销售分配形成 |
| `changeSummary` | 采购变更与纠错 | `document_relation` / 采购变更 | 显示当前进行中变更与历史版本 |

## 6. 搜索、筛选、排序与默认视图

### 6.1 默认列表

- 采购用户默认保存视图“待我推进”：草稿、审批中、待履约且责任在本人范围的采购单。
- 审批人从 W01 的 `DocumentApproval` 任务进入，不把 W08 默认列表变成第二套审批队列。
- 列表服务端分页、筛选和排序；默认 36px 行高，稳定 `rowId=purchaseOrderId`。
- 单击行打开 detail，Enter 等价；行内“去审批/去交付”等明确作业按钮直达目标，不先开无用详情。

### 6.2 筛选契约

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 保存视图 | 角色默认 | `view` | 保存筛选、排序、列设置，不保存权限结论 |
| 搜索 | 空 | `q` | 采购单号、来源销售单号、供应商名称；服务端查询 |
| 主状态 | 有效全部 | `status` 可多选 | 草稿、审批中、已生效、部分执行、已完成、已作废 |
| 供应商 | 全部 | `supplierId` | 业务对象选择器，只返回有权供应商 |
| 采购类型 | 全部 | `purchaseType` | 实物 / 虚拟 / 线下服务 |
| 履约责任 | 全部 | `responsibility` | 入仓 / 直发 / 电子 / 服务 |
| 付款条件 | 全部 | `paymentGate` | 先款 / 后款 / 门禁阻塞 |
| 交期 | 有效全部 | `due=upcoming|overdue` | 使用服务端最近未完成交期 |
| 进度 | 全部 | `paymentProgress` / `fulfillmentProgress` | 多轨独立过滤 |
| 排序 | 最近更新 | `sort` | 更新时间、采购单号、交期、金额；服务端稳定次排序 |

筛选与分页写 URL；detail 打开状态可写 `preview`，浏览器后退恢复。列表结果数变化通过 `aria-live=polite` 播报。筛选激活时工具栏常驻「清除筛选」：清 `q/status/metric` 及高级筛选条件并回第 1 页，保留 `sort`/`view`；空态清除复用同一逻辑且**不**卸载筛选区。搜索 300ms 防抖 + Enter + `/` 聚焦。  
主工具条常驻 ≤3 个筛选项；上表其余维度进「高级筛选」Popover（见 `ui-filter-design.md` §3）。

### 6.3 导出和批量边界

- 导出当前页可直接按当前响应生成受权限裁剪文件；导出“当前筛选全部”必须创建服务端 `bulk_selection_snapshot` 和后台任务。
- 导出字段清单、遮罩规则和下载审计必须固化；下载时再次鉴权，结果 7 天到期。
- 不提供通用批量审批通过、批量作废或批量变更；审批决定必须逐任务校验责任、岗位分离和版本。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 新建采购单 | 页头 / W05 已生效销售单 | `CREATE_PURCHASE_ORDER`；销售单已生效且仍有未覆盖的销售明细；不要求采购建单 `work_item` | 创建前确认选源与拆单建议 | 创建稳定采购对象与草稿并进入同一对象页签 | 供给失效或不可供时不得创建 |
| 分派采购草稿责任 | 列表 / 对象头 | 采购负责人；状态为草稿；目标经办人在责任域内 | 展示当前/目标经办人和未完成草稿影响 | 对象级更新草稿责任人与审计；不得创建、不得开始处理、不得完成或转交 `work_item` | 失败保持原责任人，刷新对象版本后按同一幂等键恢复 |
| 保存草稿 | 编辑区自动保存 / `⌘S` | 编辑权有效、`lockVersion` 匹配 | 无 | 返回新版本、规范化金额和校验摘要 | 输入保留；冲突时显示差异 |
| 提交审批 | 编辑区主动作 | 草稿完整；分行来源、数量分配、资质、金额、付款条件通过校验 | 展示供应商、含税/不含税/税额、费用、付款门禁和已绑定流程 | 形成不可变采购提交，递增 `approval_subject_version`，启动审批并创建第一节点任务；首次提交成功时分配不可复用正式 `purchaseNo` | 失败保留草稿；不确定时查询提交结果 |
| 审批通过 | W01 / W08 通用审批区 | `APPROVE`；当前责任人为本人、岗位分离、提交版本有效 | 展示节点、提交摘要和最终通过影响 | 通用 `submit_decision`；中间节点进入下一节点，末节点原子形成采购版本与应付 | 不确定时停留当前项，不本地生效 |
| 审批驳回 | W01 / W08 通用审批区 | `REJECT`；原因必填 | 展示“不改变采购单，下一轮从入口节点重新开始” | 单据保持 `IN_APPROVAL`，完成当前任务并创建下一轮入口任务 | 失败或结果未知时停留当前项 |
| 撤回审批 | 对象中心“更多” | 原提交人，或具备采购单类型运行管理权且填写原因的应急运行管理员；服务端 `allowed_actions` 允许对应撤回 | 展示将回到草稿；管理员路径标明应急代办；运行中任务将关闭，人员失效受阻实例无开放任务 | 调用采购单强类型撤回命令；原实例取消，采购单回 `DRAFT` | 非人员 blocker 转受阻取消；失败保持审批中并刷新当前事实 |
| 作废草稿 | 对象中心“更多” | 草稿、无下游正式事实且 `VOID` 可用 | 强确认并说明影响 | 状态转已作废并留审计 | 失败保持草稿，不从列表本地删除 |
| 打印预览 | detail / 对象头 | 有打印字段权限 | 无 | 宽层 `PaperDocument` 展示服务端正式投影 | 失败提示重试，不用旧版本冒充当前 |
| 去登记付款/发票 | 应付与票款子区 / `PrepaymentGate` | W12 权限且采购已生效 | 无 | 打开 W12 供应商会话，预选当前应付 | 返回 W08 时刷新付款/票据进度 |
| 去交付 | 对象头 / 履约子区 | W09 权限、采购已生效、服务端门禁允许对应动作 | 无 | 打开 W09 并定位采购/任务 | 门禁不满足时留在 W08 并给 W12 正确入口 |
| 发起采购变更 | 变更与异常子区 | 已生效、`CREATE_CHANGE` 可用、无冲突进行中变更 | 展示基准版本与已发生事实 | 创建采购变更工作副本，同对象页签进入变更模式 | 创建失败留在当前版本；不得直接解锁原单 |
| 导出 | 工具栏 | 有导出和字段权限 | 全筛选导出确认范围/更新时间 | 创建后台任务或下载当前页 | 可在后台任务中心重试/下载 |

### 7.1 采购提交与审批边界

- 建单分组规则已固定：只允许合并同一销售单、同一有效销售提交/版本，且供应商、采购类型、付款条件、履约责任四个拆单维度完全相同的选源分行；任一维度不同必须拆单，不同销售提交/版本不得跨版本拼单。
- 提交事务冻结完整采购头、行、销售分配，形成新的 `submission_no`，并递增 `approval_subject_version`；审批不得读取可变主表，字符串业务提交号不得充当 `subject_version`。
- 每个节点决定事务锁定当前任务、实例、执行和提交，并再次校验责任、DataScope、对象读取权、岗位分离和版本。
- 只有末节点通过才原子形成 `purchase_order_revision`、版本行、销售分配、应付原始分录、工作流动作和任务完成结果。
- 驳回不改变采购单和 `subject_version`，实例轮次加一并回到入口节点。采购若要改单必须先撤回审批；再次提交启动新实例并递增 `approval_subject_version`。不得创建 `PURCHASE_ORDER_REVIEW` 专用任务类型。
- 驳回原因使用通用审批决定合同，不得新增采购专用决定 DTO、原因状态机或返回草稿分支。
- 实际付款不会改变审批结果；履约与付款、开票分别推进独立进度。

### 7.2 生效后变更边界

- 生效字段锁定，编辑入口替换为“发起采购变更”。
- 采购变更以当前 `baseRevisionId` 创建完整目标提交，并按 `PurchaseChangeOrder` 已发布定义审批；节点执行引用同一冻结提交。
- 已入库、已发货、已付款和已开票事实不回退；差异通过退货、冲正、退款、成本调整等反向/追加事实处理。
- W08 只提供变更入口、版本时间线和关联处理单，不在原版本表单内直接覆写。
- 变更的节点数量、顺序和审批人只由已发布定义决定；W08 禁止硬编码影响阈值、审批层级或额外状态机。

## 8. 数据契约

本节定义 UI 所需语义，不固定 HTTP 路径。

### 8.1 列表查询

```ts
type PurchaseOrderListQuery = {
  q?: string
  viewId?: string
  statuses?: string[]
  supplierId?: string
  purchaseType?: "PHYSICAL" | "VIRTUAL" | "SERVICE"
  fulfillmentResponsibility?: "WAREHOUSE" | "SUPPLIER_DIRECT" | "ELECTRONIC" | "SERVICE"
  paymentGate?: "PREPAY" | "POSTPAY" | "BLOCKED"
  due?: "upcoming" | "overdue"
  paymentProgress?: string
  fulfillmentProgress?: string
  sort: string
  page: number
  pageSize: number
}

type PurchaseOrderListRow = {
  purchaseOrderId: string
  purchaseOrderNo?: string
  salesOrderId: string
  salesOrderNo: string
  supplierId: string
  supplierName: string
  status: string
  purchaseType: "PHYSICAL" | "VIRTUAL" | "SERVICE"
  fulfillmentResponsibility: string
  paymentGate: "SATISFIED" | "BLOCKED" | "NOT_APPLICABLE"
  grossAmount: string
  paymentProgress: string
  fulfillmentProgress: string
  updatedAt: string
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
}

type PurchaseOrderListResult = {
  rows: PurchaseOrderListRow[]
  pageInfo: { page: number; pageSize: number; total: number }
  metrics: Array<{ key: string; label: string; count: number; visible: boolean }>
  freshness: { updatedAt: string; state: "fresh" | "refreshing" | "failed" }
  allowedActions: string[]
}
```

列表 Query Key 包含用户、角色、权限/数据范围版本、所有筛选、排序和分页。列表、指标和导出范围使用同一服务端权限快照，不以当前页行数求和。

### 8.2 对象中心查询

```ts
type PurchaseOrderCenterView = {
  identity: {
    purchaseOrderId: string
    purchaseNo?: string
    status: string
    approvalSubjectVersion: number
    lockVersion: number
    currentSubmissionId?: string
    currentRevisionId?: string
    revisionNo?: number
  }
  header: {
    salesOrderId: string
    salesOrderNo: string
    supplierId: string
    supplierSnapshot: string
    purchaseType: string
    fulfillmentResponsibility: string
    paymentTermSnapshot: PaymentTermView
  }
  progress: {
    payment: string
    invoice: string
    fulfillment: string
    prepaymentGate: { state: "SATISFIED" | "BLOCKED" | "NOT_APPLICABLE"; message: string }
  }
  currentContent: {
    source: "DRAFT" | "SUBMISSION" | "REVISION"
    version: number
    lines: PurchaseOrderLineView[]
    totals: { gross: string; net: string; tax: string }
  }
  allocations: PurchaseSalesAllocationView[]
  payableSummary?: PayableSummaryView
  fulfillmentSummary: FulfillmentSummaryView
  changes: RelatedChangeView[]
  workflow: WorkflowActionView[]
  approvalWorkItem?: {
    workItemId: string
    taskVersion: string
    workItemType: "DOCUMENT_APPROVAL"
    subjectVersion: number
    ownerUser: ActorView
  }
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
  fieldVisibility: Record<string, "full" | "masked" | "hidden">
}
```

采购当前状态、应付开放余额、付款/发票核销和履约进度事务内同步维护；经营分析类指标可异步，但不能驱动 W08 正式动作。

### 8.3 草稿保存与提交

```ts
type AssignPurchaseOrderDraftOwnerCommand = {
  purchaseOrderId: string
  expectedLockVersion: number
  ownerUserId: string
  reasonCode: string
  idempotencyKey: string
}

type SavePurchaseOrderDraftCommand = {
  purchaseOrderId: string
  expectedLockVersion: number
  draftEditToken: string
  salesOrderId: string
  supplierId: string
  purchaseType: string
  paymentTermCode: string
  fulfillmentResponsibility: string
  lines: Array<{
    lineId: string
    lineType: "ITEM_SERVICE" | "LOGISTICS_FEE"
    salesOrderLineId?: string
    supplierOfferingRevisionId?: string
    quantity?: string
    unitCostGross?: string
    inputTaxRate: string
    expectedDeliveryDate?: string
    salesAllocations: Array<{ salesOrderSubmissionLineId: string; allocatedQuantity: string }>
  }>
}

type SubmitPurchaseOrderCommand = {
  purchaseOrderId: string
  expectedLockVersion: number
  expectedDraftContentHash: string
  draftEditToken: string
  idempotencyKey: string
}
```

`AssignPurchaseOrderDraftOwnerCommand` 是对象级草稿责任分派：只更新草稿责任人与审计，校验对象版本、负责人范围和幂等键；不得创建、不得开始处理、不得完成或转交 `work_item`。`SavePurchaseOrderDraftCommand` 只保存采购对象草稿；`draftEditToken` 只证明采购草稿编辑权，不形成第二套任务责任协议。保存返回新 `lockVersion`、规范化头行、服务端金额和校验摘要；草稿阶段禁止返回正式 `purchaseNo`。`SubmitPurchaseOrderCommand` 冻结提交、递增 `approvalSubjectVersion`、启动审批并创建第一节点任务；成功返回稳定 `submissionId`、`submissionNo`、`approvalSubjectVersion`、审批实例与任务身份；首次提交成功时必须一并返回不可复用正式 `purchaseNo`。新建和保存时服务端按 §7.1 固定拆单键重验，禁止跨销售提交/版本拼单。

### 8.4 通用审批决定

采购单审批决定只走通用 `submit_decision`（`work_item_id`、`APPROVE|REJECT`、原因、`expected_task_version`、幂等键）。不得另传选源字段或下一审批人。最终通过才形成采购生效版本和应付。

不得提供已废止的“退回团队”、领取或“开始处理”能力。页面只展示服务端 `allowed_actions` 中的通过/驳回/撤回。

服务端在当前事务重验权限、岗位分离、当前责任人、提交版本、采购对象版本、选源有效性和销售分配。中间节点通过只推进实例；最终通过原子形成采购正式版本和应付。驳回不改变采购单，轮次加一并回到入口节点。采购若要改单必须先撤回。任一业务写入或任务完成失败均整体回滚，前端不得补发独立“标记完成”。

### 8.5 幂等与结果不确定

- 新建采购对象、提交审批、撤回、作废和采购变更创建分别使用独立幂等键，不能跨动作复用；节点决定使用通用审批幂等键。
- 同一动作同一幂等键重复请求返回同一业务对象和结果。
- 超时后按幂等键查询，不依据旧列表状态猜测成功。
- 结果未知时固定保留当前提交/草稿与任务上下文，提供查询最终结果；不得自动进入 W09 或 W12。

### 8.6 前端边界

- 前端只格式化金额、税率、数量、日期、状态和受控差异；正式金额、税额、分配守恒、门禁与状态完全采用服务端结果。
- 前端可预提示拆单维度变化，但不能自动跨采购单搬行或合并不同销售单。
- `allowedActions` 和 `actionBlockers` 每次对象/任务查询返回；组件不硬编码角色或状态邻接。
- TanStack Query 管理列表、中心、任务和缓存失效；TanStack Form 管理采购草稿/驳回表单。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 列表初载 | 页头、指标、工具栏、8 行表格和分页 Skeleton | 应用壳导航可用 | 原位替换 |
| 列表刷新 | 保留旧行 + 轻指示 | 可打开缓存详情；正式作业重验 | 成功更新时间，失败保留旧行 |
| 列表无数据 | 区分无记录、筛选无结果、无数据范围 | 新建（有依据时）、清筛选、查看范围 | 条件变化后重查 |
| 对象初载 | 单据头和各子区 Skeleton | 返回列表 | 查询成功 |
| detail 加载失败 | 列表保留，抽屉显示失败态 | 重试、关闭 | 重试成功 |
| 页面查询失败 | 无缓存显示 `BusinessFailureState`；有缓存保留旧列表/对象并标陈旧 | 重试；陈旧内容只读 | 查询恢复 |
| 数据陈旧 | 显示当前版本/更新时间；正式动作禁用 | 刷新 | 获取当前事实后恢复 |
| 草稿保存中 | 保存指示，不重置输入 | 编辑非冲突字段 | 返回新版本 |
| 草稿保存失败 | 输入保留、错误靠近保存区 | 重试、复制输入 | 重试成功 |
| 草稿校验失败 | 顶部摘要 + 行错误 | 修正、保存 | 校验通过 |
| 编辑权丢失 | 本地输入只读保留 | 复制输入 | 重取草稿与版本 |
| 提交结果不确定 | 不切审批中状态 | 查询最终结果 | 确认提交或明确失败 |
| 审批中 | 中心显示不可变提交、当前节点和处理人 | 查看、打开本人审批任务 | 节点决定完成 |
| 任务已由他人负责 | 审批区只读，显示当前处理人 | 查看、打开单据 | 任务完成后重查 |
| 审批版本冲突 | 决策禁用，显示当前事实已变化 | 刷新当前任务、下一项 | 当前事实加载成功 |
| 审批通过成功 | 中间节点显示下一节点；最终通过显示版本、应付和付款门禁 | 下一项；最终通过后可去 W09/W12 | 用户继续 |
| 审批驳回成功 | 固定显示原因和下一轮入口节点 | 下一项、查看中心、按允许动作撤回 | 下一轮任务已形成 |
| 审批刚驳回 | 采购单仍为审批中；展示最近驳回 | 撤回后改草稿、等待下一轮、或作废 | 不得渲染为已废止的“团队待处理”状态 |
| 正式动作结果不确定 | 停留当前对象/任务，不宣告结果 | 查询最终结果 | 确定结果 |
| 后台导出 | `BackgroundJobProgress` 显示筛选快照、进度和逐项结果 | 查看任务、取消未开始项 | 完成后下载或按原任务重试 |
| 先款门禁阻塞 | `PrepaymentGate` 展示有效已付与缺口 | 去 W12；履约动作禁用 | 有效付款核销后重查 |
| 字段级隐藏 | 标签保留、值掩码；列宽稳定 | 其它有权动作 | 权限变化后重查 |
| 权限收回 | 清除敏感缓存，转无权限态 | 返回有权模块 | 权限恢复后重查 |

## 10. 响应式、键盘与无障碍

### 10.1 响应式

| 视口 | 布局变化 | 必须保留 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 侧栏展开；M2 至少 6–8 行；detail 38/62；中心全宽 | 采购单身份、主状态、审批摘要、供应商、金额、付款/履约、行级主动作 | 无 |
| 1280×800 | 表格横滚；detail 覆盖更多列表 | 固定采购单号和操作列；中心关键进度 | 次要说明列进入列设置 |
| 1024×768 | 图标侧栏；detail 覆盖式；编辑表头与明细单列 | 身份、供应商、金额口径、门禁、提交/审批动作 | 关联摘要折叠 |
| 768×1024 | 导航抽屉；表格横滚；detail 上下分区；编辑行卡片化 | 身份列和操作列固定；错误、总计与主动作 | 次要税务说明按行展开 |
| 375×812 | 保证列表阅读、detail 主事实和简单审批结果查看 | 单号、状态、供应商、含税总额、门禁/结果 | 不提供复杂采购建单、行分配、列设置或正式审批 |

### 10.2 键盘与焦点

- `/` 聚焦 W08 列表搜索；`j/k` 或方向键移动行；Enter 打开 detail。
- detail 关闭后焦点回原行；打开对象中心时目标标题获得焦点。
- 编辑区 `⌘S` 保存，校验通过时 `⌘↵` 打开提交确认；不绕过确认。
- 表格金额头声明排序，金额单元格带明确含税/不含税上下文供读屏读取。
- 审批决定自动下一项后焦点落新采购单标题并播报队列位置。
- 验证摘要、门禁、版本冲突和正式结果使用 `aria-live` 适度播报，不把整个页面重复朗读。
- 状态和进度有文字/图标/tone；触控目标至少 44×44px。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 我的工作台 | W01 | 审批任务携带 `workItemId` 和提交 ID；普通建单从已生效销售单进入 | 节点决定完成后返回 W01 下一项 |
| 销售单 | W05 | 已生效销售单 ID、采购单 ID、来源子区 | 返回 W05 采购/履约子区，进度重查 |
| 履约处理 | W09 | 采购单/版本/行、履约责任、来源 `workItemId` | 返回 W08 履约子区并刷新事实 |
| 库存台账 | W10 | 入仓采购、仓库、预占稳定 ID | 只读钻取，返回采购中心 |
| 供应商往来 | W12 | 供应商、应付、采购单、付款门禁来源 | 返回刷新付款/发票/门禁，不传旧金额结论 |
| 基础资料 | W14 | 供应商及能力/资质、SKU、仓库 | 返回编辑时重验版本，不静默替换快照 |
| 权限与审计 | W19 | 单据、提交、工作流动作、任务、请求追踪号 | 只读返回原对象 |

第二期 API 供应商履约订单属于 W26，不并入 W08 采购单；W08 继续承载第一期实物与服务销售的人工采购链路。

## 12. 验收清单

### 12.1 列表、预览与中心

- [x] 1440×900 M2 首屏至少显示 6–8 条有效行，采购单号和行级动作固定。
- [x] 单击采购单在 detail 半屏读完状态、供应商、来源销售、明细、金额、票款和履约主事实。
- [x] 对象中心同屏可到应付/付款、进项发票、履约、变更和关联销售，无需跨三个菜单拼现状。
- [x] 编辑、查看、审批不建立三套平行路由；同一采购对象保持一个 TaskTab 身份。

### 12.2 业务、数据与权限

- [x] 一张采购单严格限制为一张销售单、一个供应商、一种采购类型、一套付款条件和一个履约责任。
- [ ] 同一销售单、同一有效销售提交/版本且四个拆单维度完全一致的选源分行按固定规则合并；任一维度不同必须拆单，不跨销售提交/版本拼单。
- [ ] 首次商品/服务行逐行引用已生效销售明细，并在本页完成选源；物流费用行来源与计税边界正确。
- [x] W05 建单入口只消费已生效销售单，不要求未注册的采购建单 `work_item`，也不得引用已废止的 W07。
- [x] 含税、不含税、税额按服务端舍入结果展示，销售/仓储无权时不泄露成本。
- [ ] 审批引用不可变提交，当前审批人没有编辑器且满足岗位分离；正式决定只使用通用 `submit_decision`。
- [ ] 页面不得出现领取、已废止的“开始处理”或已废止的“退回团队”能力；审批任务创建即指定到人。
- [ ] 节点通过或驳回在同一事务写执行、实例、`workflow_action` 和当前 `work_item` 结果；驳回同事务创建下一轮入口执行和任务，采购内容不变；实际付款独立。
- [x] 生效后变化只走采购变更，不覆盖已发生付款、发票或履约事实。

### 12.3 操作、状态与响应式

- [ ] 保存、提交、审批决定、撤回、作废和变更创建各自具备防重复、冲突和结果不确定处理。
- [x] `PrepaymentGate` 使用服务端有效付款净核销结果，四类采购履约入口均不能绕过。
- [ ] §9 全部状态完成组件测试或浏览器验证。
- [ ] 1440、1280、1024、768、375 五档视口符合 §10.1。
- [x] 键盘可完成列表核对、草稿保存/提交和审批导航；焦点返回正确。

## 13. 业务依据

- `erp-phase-1.md` §6.2、§7.1–§7.4：采购单职责、拆单维度、费用、通用审批、付款条件与履约路径。
- `erp-phase-1.md` §6.3–§6.5、§10：生效后采购变更、退货/纠错与固定状态边界。
- `erp-phase-1.md` §9.2：一张采购单支持多次付款和同供应商多对多付款/进项票核销。
- `erp-data-model.md` §6.6：采购主表、不可变提交/版本、版本行、销售分配和采购变更数据契约。
- `erp-data-model.md` §6.7、§7.4、§8.1：付款门禁、采购固定状态机和最终审批事务不变量。
- `erp-ui-design.md` §4.3–§4.6、§5.2、§9–§12：M2 detail、对象中心、编辑、采购连续路径和正式结果。
- `erp-ui-flows.md` §1.1、§2、§2.5：采购单核对、通用审批、履约/票款衔接及先款后货门禁。
