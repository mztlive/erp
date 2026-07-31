# ERP 业务组件目录

本目录把 [108 个稳定页面](../../../docs/erp-page-map.md) 归并为可复用的页面模式，作为
`erp-client` 的业务展示层。组件实现以
[页面布局与交互规范](../../../docs/erp-interaction-spec.md) 为依据，不复制业务状态机、金额
计算、权限判断或网络请求。

运行项目后访问 `/business-components` 可验收全部 64 个公开组件；预览实现见
[业务组件预览页](../../app/business-components/page.tsx)。主题基础仍在 `/theme` 验收。

## 分层边界

| 层 | 负责 | 不负责 |
| --- | --- | --- |
| `components/ui` | Base UI / shadcn 原语、主题 token、可访问性和基础交互 | ERP 对象、单据状态和业务动作 |
| `components/business` | 跨领域复用的 ERP 页面模式、业务语义和受控动作插槽 | 请求、路由、表单业务状态、权限与正式计算 |
| `features/*` 与页面 | TanStack Query、TanStack Form、路由参数、API DTO 适配和 mutation | 重复实现通用页面结构 |
| ERP 后端 | `allowedActions`、`actionBlockers`、金额与数量口径、状态机、并发和幂等 | 让浏览器猜测正式结果 |

业务组件只接收已经授权、已经计算的展示数据。字段编辑器由页面通过 TanStack Form 注入，
读取和提交由 feature 通过 TanStack Query 完成。

## 页面模式与组件清单

| 页面模式 | 组件 | 文件 | 覆盖场景 |
| --- | --- | --- | --- |
| 应用壳 | `ErpAppShell`、`GlobalTopbar`、`TaskTabs`、`MaintenanceBanner` | `shell.tsx` | 全局导航、内部任务页签、维护与主责迁移冻结 |
| 页面公共区 | `PageHeader`、`PageActions`、`MetricStrip`、`MetricItem`、`DataFreshness` | `page.tsx` | 面包屑、标题、动作、工作台统计、数据水位 |
| 高密度列表 | `DataTable`、`DataTableViewOptions`、`DataTablePagination` | `data-table.tsx` | 服务端分页、排序、筛选、显隐、固定、调宽、跨页稳定选择和键盘行导航 |
| 列表编排 | `ListToolbar`、`SelectionScopeBar`、`StatusMatrix`、`BusinessTableFrame`、`QuickPreviewSheet` | `list.tsx` | 常驻筛选、选择范围、多轨状态、加载/空态/失败、右侧快速预览 |
| 选择与筛选 | `BusinessObjectCombobox`、`SavedViewPicker`、`AdvancedFilterSheet` | `selectors.tsx` | 有效业务对象选择、个人/团队视图、高级筛选 |
| 值与状态 | `BusinessStatusBadge`、`StatusTrackSummary`、`BusinessObjectRef`、`MoneyValue`、`QuantityValue`、`RateValue`、`DocumentTotals` | `values.tsx` | 多维状态、稳定对象引用、精确十进制展示、含税/不含税口径 |
| 正式单据详情 | `DocumentHeader`、`DocumentSummary`、`DocumentSection`、`RevisionTimeline`、`RelatedDocumentList`、`ResponsibilityPanel` | `document.tsx` | 销售、采购、库存、票款、发票、结算等详情与版本追溯 |
| 纸质单据预览 | `PaperDocument` | `paper-document.tsx` | 销售、采购、出入库、收付款和发票等正式单据的 A4 风格查看与打印投影 |
| 单据编辑 | `EditableLineItemTable`、`ApprovalDecisionPanel`、`AllocationWorkspace` | `editor.tsx` | 行项目编辑、审批/确认、回款付款发票等多对多分配 |
| 附件 | `DocumentAttachmentList` | `attachments.tsx` | 受控上传、扫描状态、必需附件和失败重试 |
| 正式动作与协作 | `FormalActionConfirmDialog`、`SequentialProcessBar`、`BatchImpactPreview`、`ConflictResolutionDialog`、`EditorPresence` | `workflow.tsx` | 正式提交、连续处理、批量影响预览、ETag 冲突、编辑占用 |
| 领域边界 | `CardVoucherLineItem`、`PrepaymentGate`、`InventoryBalanceSummary`、`AfterSalesTrackPanel`、`CostCoverageNotice`、`InterfaceErrorResolutionPanel` | `domain.tsx` | 卡券唯一明细、先款后货、库存四量、售后四段、成本覆盖、结果未知 |
| 任务、审计和导入 | `WorkTaskItem`、`BusinessDiffPanel`、`AuditTimeline`、`ImportStageIndicator`、`ImportIssueTable`、`BatchOperationResult` | `audit-import.tsx` | 统一任务、字段差异、追加式审计、导入分阶段校验、批处理结果 |
| 状态反馈与敏感信息 | `GuardedBusinessAction`、`BusinessEmptyState`、`BusinessFailureState`、`AsyncSectionState`、`DraftSaveIndicator`、`ValidationSummary`、`FormalActionResult`、`BackgroundJobProgress`、`SensitiveValue` | `feedback.tsx` | 禁用原因、保留旧数据刷新、草稿、校验、正式结果、后台任务、短时明文 |

页面专属的字段组合留在 `features/<domain>`，不因只出现一次就进入本目录。基础按钮、输入框、
Tabs、Dialog、Popover、Tooltip 等仍直接使用 `components/ui`，不增加无业务语义的转发包装。

## 为业务模式补齐的 UI 原语

| UI 原语 | 扩展原因 |
| --- | --- |
| `Alert` | 增加 `success`、`warning`、`info` 语义变体，全部读取主题 token |
| `Checkbox` | 增加部分选择态，支持当前页与全范围选择表达 |
| `Sheet` | 增加主题化 `preview` 尺寸，用于快速预览和高级筛选 |
| `Table` | 用 `--table-row-height` 消费紧凑/舒适密度 token |
| `DatePicker`、`DateRangePicker`、`DateTimePicker` | 明确 `YYYY-MM-DD`、秒级时间和业务时区的受控值 |
| `DescriptionList` | 详情页语义化 `dl/dt/dd` 摘要布局 |
| `FileUpload` | 文件选择和拖放原语；上传请求仍由 TanStack Query mutation 执行 |
| `Timeline` | 版本、审计和状态历史的有序语义结构 |

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
- 列通过 `meta.label`、`meta.align`、`meta.numeric` 和 `meta.width` 声明语义；
  `meta.width` 只能使用 `reference`、`status`、`amount`、`quantity`、`rate`、`tracks`
  等主题宽度档位，不传颜色、像素宽度或任意样式；
- `layout="inset"` 用于需要自带业务卡片内距与边界的列表，`layout="flush"` 用于边界已由
  外部框架提供的场景；多行复合单元格使用 `density="comfortable"`；
- 页面选择只保存稳定 ID。选择“当前筛选全部结果”时必须调用批量预览 API 冻结选择快照，
  不能把客户端当前页推断为正式批量范围；
- 正在刷新时保留已有行；初次加载、空态和失败态由业务框架明确区分；
- 单击非交互区域用于快速预览；`Enter` 优先打开详情，未配置详情动作时回退到快速预览；
  业务列中的明确按钮/链接提供鼠标详情入口。上下方向键在当前结果行间移动，行内交互控件
  不会冒泡触发行级动作。

## 组合准则

1. 列表页使用 `PageHeader` + `ListToolbar` + `BusinessTableFrame` + `DataTable`，需要时组合
   `SelectionScopeBar` 和 `QuickPreviewSheet`。
2. 正式详情页使用 `DocumentHeader` + `DocumentSummary` + `DocumentSection`，版本、关联单据和
   并行责任分别使用专用组件，不合并成单一“状态”。
3. 需要纸张或打印投影时使用 `PaperDocument`；页面必须传入服务端已经确认的行金额、汇总、
   状态和签章内容，组件不代替后端计算正式结果。
4. 编辑页把 TanStack Form 字段节点传入 `EditableLineItemTable` 或 `AllocationWorkspace`；
   组件不复制字段状态和校验。
5. 正式命令先显示 `FormalActionConfirmDialog` 或 `BatchImpactPreview`，成功后固定展示
   `FormalActionResult`，不能只用瞬时 toast。
6. 业务异常、财务纠错和接口错误使用其真实任务或强类型事实投影，不创建通用 CRUD 卡片。
