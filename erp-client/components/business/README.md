# ERP 业务组件目录

本目录把 [UI 工作面与七种页面模式](../../../docs/erp-ui-design.md) 落成可复用的业务展示层。
组件实现以该设计文档与 [关键流程交互说明](../../../docs/erp-ui-flows.md) 为依据，不复制业务
状态机、金额计算、权限判断或网络请求。

业务组件以本目录导出为准；选择器约定见下文「可搜索 Combobox 约定」。共享码表（付款条件、单位、承运方等）见 `lib/business-options.ts`。

## 分层边界

| 层                    | 负责                                                                   | 不负责                                   |
| --------------------- | ---------------------------------------------------------------------- | ---------------------------------------- |
| `components/ui`       | Base UI / shadcn 原语、主题 token、可访问性和基础交互                  | ERP 对象、单据状态和业务动作             |
| `components/business` | 跨领域复用的 ERP 页面模式、业务语义和受控动作插槽                      | 请求、路由、表单业务状态、权限与正式计算 |
| `features/*` 与页面   | TanStack Query、TanStack Form、路由参数、API DTO 适配和 mutation       | 重复实现通用页面结构                     |
| ERP 后端              | `allowedActions`、`actionBlockers`、金额与数量口径、状态机、并发和幂等 | 让浏览器猜测正式结果                     |

业务组件只接收已经授权、已经计算的展示数据。字段编辑器由页面通过 TanStack Form 注入，
读取和提交由 feature 通过 TanStack Query 完成。

## 页面模式与组件清单

| 页面模式           | 组件                                                                                                                                                                                                                                                                            | 文件                                                            | 覆盖场景                                                                                                                                                                                                                                                               |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 应用壳             | `ErpAppShell`、`GlobalTopbar`、`MaintenanceBanner`                                                                                                                                                                                                                              | `shell.tsx`                                                     | 全局导航、维护与主责迁移冻结                                                                                                                                                                                                                                           |
| 页面公共区         | `PageScaffold`、`PageHeader`（`page` / `object-chrome`）、`PageActions`、`MetricStrip`、`MetricItem`（`detailMode`）、`MetricFilterItem`（`status` 指标徽章）、`DataFreshness`、`surfacePanelClassName` / `surfaceInsetClassName`                                               | `page.tsx`                                                      | 内容区脚手架与浮起表面；**固定模式见** `docs/erp-ui-design.md` §2.5；一级 `page` 不展示面包屑；M4 用 object-chrome + 面包屑                                                                                                                                            |
| 高密度列表         | `DataTable`、`DataTableViewOptions`、`DataTablePagination`                                                                                                                                                                                                                      | `data-table.tsx`                                                | 服务端分页、排序、筛选、显隐、固定、调宽、跨页稳定选择和键盘行导航                                                                                                                                                                                                     |
| 列表编排           | `ListToolbar`、`SelectionScopeBar`、`StatusMatrix`、`BusinessTableFrame`、`QuickPreviewSheet`                                                                                                                                                                                   | `list.tsx`                                                      | 常驻筛选、选择范围、多轨状态、加载/空态/失败、右侧快速预览                                                                                                                                                                                                             |
| 选择与筛选         | `OptionCombobox`、`BusinessObjectCombobox`、`TreeCombobox`、业务实体 Combobox（合同/销售单/客户/采购单/供应商/商品/品牌/商品分类/结算主体/仓库/负责人）、`SavedViewPicker`、`AdvancedFilterSheet`                                                                                               | `option-combobox.tsx`、`tree-combobox.tsx`、`entity-comboboxes.tsx`、`selectors.tsx` | 可搜索枚举/筛选、有效业务对象选择、层级字典树形选择、个人/团队视图、高级筛选；**禁止**在业务页继续使用 `Select` / `NativeSelect`；**禁止**用自由 `Input` 录入已有业务对象 ID/名称                                                                                                        |
| 值与状态           | `BusinessStatusBadge`、`StatusTrackSummary`、`BusinessObjectRef`、`MoneyValue`、`QuantityValue`、`RateValue`、`DocumentTotals`                                                                                                                                                  | `values.tsx`                                                    | 多维状态、稳定对象引用、精确十进制展示；`MoneyValue` 的 `taxBasis` 会叠「含税/不含税」Badge——**列头/标签已写明口径时不要传**（如销售建单「含税小计」）；`BusinessStatusBadge` 对 `PENDING` 等枚举原值 label 有兜底中文映射（业务页仍需自己映射领域枚举，兜底只防漏网） |
| 正式单据/对象详情  | `DocumentHeader`（M4 用 `density="compact"` + `meta`）、`DocumentSummary`、`DocumentSection`、`RevisionTimeline`、`RelatedDocumentList`、`ResponsibilityPanel`                                                                                                                  | `document.tsx`                                                  | 销售、采购、客户、票款、发票、结算等详情与版本追溯；唯一身份头                                                                                                                                                                                                         |
| 纸质单据预览       | `PaperDocument`                                                                                                                                                                                                                                                                 | `paper-document.tsx`                                            | 正式单据 A4 风格投影；`frame="framed"` 内嵌灰底，`frame="bare"` 透明浮层                                                                                                                                                                                               |
| 单据编辑           | `EditableLineItemTable`、`ApprovalDecisionPanel`、`AllocationWorkspace`                                                                                                                                                                                                         | `editor.tsx`                                                    | 行项目编辑、审批/确认、回款付款发票等多对多分配                                                                                                                                                                                                                        |
| 附件               | `DocumentAttachmentList`                                                                                                                                                                                                                                                        | `attachments.tsx`                                               | 受控上传、扫描状态、必需附件和失败重试                                                                                                                                                                                                                                 |
| 正式动作与协作     | `FormalActionConfirmDialog`、`SequentialProcessBar`、`BatchImpactPreview`、`ConflictResolutionDialog`、`EditorPresence`                                                                                                                                                         | `workflow.tsx`                                                  | 正式提交、连续处理、批量影响预览、ETag 冲突、编辑占用                                                                                                                                                                                                                  |
| 领域边界           | `CardVoucherLineItem`、`PrepaymentGate`、`InventoryBalanceSummary`、`AfterSalesTrackPanel`、`CostCoverageNotice`、`InterfaceErrorResolutionPanel`                                                                                                                               | `domain.tsx`                                                    | 卡券唯一明细、先款后货、库存四量、售后四段、成本覆盖、结果未知                                                                                                                                                                                                         |
| 任务、审计和导入   | `WorkTaskItem`、`BusinessDiffPanel`、`AuditTimeline`、`ImportStageIndicator`、`ImportIssueTable`、`BatchOperationResult`                                                                                                                                                        | `audit-import.tsx`                                              | 统一任务、字段差异、追加式审计、导入分阶段校验、批处理结果                                                                                                                                                                                                             |
| 状态反馈与敏感信息 | `GuardedBusinessAction`、`BusinessEmptyState`、`BusinessFailureState`（`onRetry`/`retryLabel` 快捷重试）、`AsyncSectionState`、`DraftSaveIndicator`、`ValidationSummary`、`FormalActionResult`（`referenceLabel`）、`BackgroundJobProgress`、`SensitiveValue`（揭示失败带重试） | `feedback.tsx`                                                  | 禁用原因、保留旧数据刷新、草稿、校验、正式结果、后台任务、短时明文                                                                                                                                                                                                     |

页面专属的字段组合留在 `features/<domain>`，不因只出现一次就进入本目录。基础按钮、输入框、
Tabs、Dialog、Popover、Tooltip 等仍直接使用 `components/ui`，不增加无业务语义的转发包装。

## 为业务模式补齐的 UI 原语

| UI 原语                                           | 扩展原因                                                                |
| ------------------------------------------------- | ----------------------------------------------------------------------- |
| `Alert`                                           | 增加 `success`、`warning`、`info` 语义变体，全部读取主题 token          |
| `Checkbox`                                        | 增加部分选择态，支持当前页与全范围选择表达                              |
| `Sheet`                                           | 增加主题化 `preview`（轻量 400px）与 `detail`（半屏读主事实 768px）尺寸 |
| `Table`                                           | 用 `--table-row-height` 消费紧凑/舒适密度 token                         |
| `DatePicker`、`DateRangePicker`、`DateTimePicker` | 明确 `YYYY-MM-DD`、秒级时间和业务时区的受控值                           |
| `DescriptionList`                                 | 详情页语义化 `dl/dt/dd` 摘要布局                                        |
| `FileUpload`                                      | 文件选择和拖放原语；上传请求仍由 TanStack Query mutation 执行           |
| `Timeline`                                        | 版本、审计和状态历史的有序语义结构                                      |

遮罩、状态、边框、表格行和应用壳尺寸均由 `app/globals.css` 的语义 token 提供。业务组件
不接受任意颜色或任意尺寸 props。

## DataTable 契约

`DataTable` 基于 `@tanstack/react-table` v8 当前稳定版实现。它是 headless table 的 ERP
适配层，不取数；页面把 TanStack Query 的结果和查询状态传入。

```tsx
<DataTable
    data={query.data.items}
    columns={columns}
    getRowId={(row) => row.id}
    rowCount={query.data.pageInfo.totalItems}
    sorting={sorting}
    onSortingChange={setSorting}
    pagination={pagination}
    onPaginationChange={setPagination}
    loading={query.isFetching}
/>
```

使用规则：

- `getRowId` 必须返回 ERP 不透明稳定 ID，不能使用行号、名称或外部单号；
- 默认开启服务端分页、排序和筛选，TanStack 的 `pageIndex` 从 `0` 开始，API 的 `page`
  从 `1` 开始，适配只在 feature 查询层完成；
- **排序契约**：`manualSorting`（默认 true）下只有页面接了 `onSortingChange` 排序入口才
  是真实交互；未接 `onSortingChange` 的页面列头**不再渲染排序按钮**（避免假箭头伪交互）。
  需要客户端本地排序时传 `manualSorting={false}`；
- **错误态契约**：查询失败必须由页面传 `errorState`（或 `errorSummary` + `onRetry`，
  内置 `BusinessFailureState` 错误块），否则表格会把系统故障误报成「当前筛选没有结果」；
- **空态契约**：无数据时默认显示「当前筛选没有结果」；首次进入无数据 / 无权限场景应传
  `emptyState`（自定义整块）或 `emptyTitle` + `emptyDescription` + `emptyAction`（引导 CTA）；
- 列通过 `meta.label`、`meta.align`、`meta.numeric` 和 `meta.width` 声明语义；
  `meta.width` 只能使用 `reference`、`status`、`amount`、`quantity`、`rate`、`tracks`
  等主题宽度档位，不传颜色、像素宽度或任意样式；
- `layout="inset"` 用于需要自带业务卡片内距与边界的列表，`layout="flush"` 用于边界已由
  外部框架（如 `BusinessTableFrame`）提供的场景；多行复合单元格使用 `density="comfortable"`；
- **flush 分页**：`DataTablePagination` 在 `layout="flush"` 时使用 `px-(--card-spacing) py-3`，
  与卡片头/工具条对齐；禁止分页贴左右边或贴卡片底边；
- 页面选择只保存稳定 ID。选择“当前筛选全部结果”时必须调用批量预览 API 冻结选择快照，
  不能把客户端当前页推断为正式批量范围；
- 正在刷新时保留已有行；轮询/自动刷新页可传 `showRefreshingBanner={false}` 关闭
  「正在刷新，当前内容会保留」提示条（或传 `refreshingLabel` 定制文案）；
- 筛选后总行数变少时页码自动钳回最后一个有效页，避免「共 N 条」与空表并存；
- 行 `aria-label` 优先用 `rowLabel`（业务名），未传时回退为「第 N 行」，不暴露内部 ID；
- 单击非交互区域 / `Enter` 的语义由**页面**注入（`onRowPreview` / `onRowOpen`）：
    - W05 销售单：二者均打开轻量 `PaperDocument` 浮层，**不用** `QuickPreviewSheet`；
    - 其它单据列表：可打开 `QuickPreviewSheet size="detail"`；
    - 进对象中心用行内「查看详情」等明确按钮，不要与行点读单混成两个预览入口。
      只接了 `onRowOpen` 的页面，单击行会回落到该入口（触屏用户没有键盘 Enter）。
      上下方向键在当前结果行间移动，行内交互控件不会冒泡触发行级动作。

## 可搜索 Combobox 约定

筛选条、表单枚举与业务对象选择统一使用 Combobox（基于 `components/ui/combobox`），**不要**再使用
`Select` / `NativeSelect`。

| 场景                                                                                           | 组件                                                                                                                                                                | 说明                                                                                                    |
| ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| 状态/环境/角色/付款条件/单位等枚举与筛选                                                       | `OptionCombobox`                                                                                                                                                    | `{ value, label }[]`，内置搜索；`allowClear={false}` 用于必选筛选；共享码表见 `lib/business-options.ts` |
| 有效合同 / 销售单 / 客户 / 采购单 / 供应商 / 商品 / 品牌 / 商品分类 / 结算主体 / 仓库 / 负责人 | `ContractCombobox`、`CustomerCombobox`、`SupplierCombobox`、`BrandCombobox`、`CategoryCombobox`、`SettlementPartyCombobox`、`WarehouseCombobox`、`OwnerCombobox` 等 | 展示编号、状态与摘要；分类为树形下拉（层级展开/收起，搜索命中平铺）；数据由 feature Query 注入，组件本身不请求                 |
| 通用层级字典（分类 / 组织等森林数据） | `TreeCombobox`                                                                                                                                                | 接收 `TreeComboboxNode` 森林；展开/收起、`ArrowLeft/Right` 折叠、搜索全树命中平铺、选中项打开时自动展开祖先；缺省全部展开                 |
| 通用带状态业务对象                                                                             | `BusinessObjectCombobox`                                                                                                                                            | 上述实体 Combobox 的底层                                                                                |
| TanStack Form 枚举字段                                                                         | `field.SelectField`                                                                                                                                                 | 已绑定 `OptionCombobox`                                                                                 |
| 列表全文搜索（单号关键词混搜）                                                                 | `Input` / `InputGroupInput`                                                                                                                                         | 非实体单选场景，可继续自由输入                                                                          |

**禁止**用自由文本 `Input`/`TextField` 录入应引用主数据或单据的字段（客户、供应商、合同、商品 SKU、结算主体、负责人等）。筛选条清空即「全部」时，实体 Combobox 用 `value={id || undefined}` + `onValueChange` 写 `null`/`undefined` 即可，不必再塞「全部」伪选项。

```tsx
// 筛选条
<OptionCombobox
  value={status}
  onValueChange={(v) => setStatus(v ?? "all")}
  options={[
    { value: "all", label: "全部状态" },
    { value: "EFFECTIVE", label: "生效" },
  ]}
  size="sm"
  className="w-[10rem]"
  allowClear={false}
  aria-label="状态"
/>

// 业务实体（Query 结果映射后传入）
<ContractCombobox
  contracts={rows.map((r) => ({
    contractId: r.contractId,
    contractNo: r.contractNo,
    customerName: r.customer.displayName,
    statusLabel: r.statusLabel,
    statusTone: r.statusTone,
    revisionNo: r.revisionNo,
    validTo: r.validTo,
  }))}
  value={contractId}
  onValueChange={setContractId}
  loading={query.isPending}
/>
```

## 组合准则

1. 列表页使用 `PageHeader`（`variant="page"`，默认）+ `ListToolbar` + `BusinessTableFrame` +
   `DataTable`；需要半屏业务核对时再组合 `QuickPreviewSheet`。**W05 销售单列表不要**再挂
   `QuickPreviewSheet` / 行内预览按钮。
    - **筛选 vs 视图切换分层**：`ListToolbar`（搜索框、字段筛选 Combobox、"高级筛选"）过滤的是
      当前这张表的行，**一律传给 `BusinessTableFrame` 的 `toolbar=`**，让筛选区嵌在卡片内、和
      结果强绑定；**不要**把 `ListToolbar` 单独摆在 `BusinessTableFrame` 外面。
    - 决定"看哪张表/哪个口径"的视图切换 `Tabs`（比如台账的"余额/流水/预留/调整"）级别高于表格
      筛选，**放在 `BusinessTableFrame` 外面**、作为页面级导航；**不要**塞进 `toolbar=` 的
      `filters=` 插槽，否则会和字段筛选混在一起。可参考 `inventory-ledger-page.tsx` /
      `customer-receivables-page.tsx` 的写法。
2. **M4 对象中心**使用：
    - `PageHeader variant="object-chrome"`：仅面包屑 + 轻动作（返回），**不要**再写工作面大标题；
    - `DocumentHeader density="compact"`：唯一身份头（名称 / 单号 / 版本 / 状态 / 主动作）；
      负责/协作等放 `meta`，不要塞成长段 `secondaryActions` 文案；
    - 可选 `MetricStrip density="compact"` + `MetricItem`：业务风险明细用 `detailMode="inline"`，
      口径旁白用 `tooltip` 或 `none`；
    - 可选 sticky 分区锚点 + `DocumentSection` 内容。
      **禁止** `PageHeader(title=…)` 与 `DocumentHeader` 双标题叠放。
3. 正式详情页的版本、关联单据和并行责任分别使用 `DocumentSummary`、`RelatedDocumentList`、
   `ResponsibilityPanel` 等专用组件，不合并成单一“状态”。
4. 需要纸张或账本版式投影时使用 `PaperDocument`：
    - 列表浮层：`frame="bare"` + **透明 Dialog 壳**（无标题栏/底栏/打印钮），见
      `features/sales-orders/sales-order-paper-dialog.tsx`；
    - 页面内嵌：默认 `frame="framed"`；
    - 页面必须传入服务端已确认的行金额、汇总、状态和签章内容，组件不代替后端计算。
5. 编辑页把 TanStack Form 字段节点传入 `EditableLineItemTable` 或 `AllocationWorkspace`；
   组件不复制字段状态和校验。
6. 正式命令先显示 `FormalActionConfirmDialog` 或 `BatchImpactPreview`，成功后固定展示
   `FormalActionResult`，不能只用瞬时 toast。
7. 业务异常、财务纠错和接口错误使用其真实任务或强类型事实投影，不创建通用 CRUD 卡片。

## 面向不同角色的文案与动作差异

共享组件被多个工作面复用。**某个页面需要不同措辞或更少动作时，加可选 prop 并保留原默认值，
不要直接改默认文案** —— 那会波及其它工作面。

| 组件                   | 扩展点                               | 默认                                                      | 谁在用非默认值                                               |
| ---------------------- | ------------------------------------ | --------------------------------------------------------- | ------------------------------------------------------------ |
| `PrepaymentGate`       | `copy?: Partial<PrepaymentGateCopy>` | 面向采购/财务的措辞（「先款后货门禁」「付款门禁已满足」） | W09 履约作业传一线口语（「先款条件」「货款已到，可以收货」） |
| `PrepaymentGate`       | `presentation?: "panel" \| "badge"`  | `panel` 完整卡片                                          | W09 传 `badge`：顶栏结果徽章，悬停展开详情，不打断读单       |
| `SequentialProcessBar` | `showProcess?: boolean`              | `true`                                                    | W09 只读角色传 `false`，同时隐藏主动作与「重新领取」         |
| `SequentialProcessBar` | `showProcessNext?: boolean`          | `true`                                                    | 主动作会离开当前页、或没有独立「并下一条」路径时传 `false`   |
| `SequentialProcessBar` | `statusExtras?: ReactNode`           | 无                                                        | W09 传入先款条件徽章，放在位置/租约状态之后                  |

只读角色不要用「渲染但禁用」表达 —— 禁用态不解释原因，且「重新领取」这类按钮
本就不该让只读用户看到。替换成一句说明加一个有用的出口。

文案本身的用词口径见仓库根目录的 `docs/ui-glossary.md`。
