# ERP 业务组件目录

本目录将四套前端权威设计规范落地为纯 UI 可复用业务组件层：

- [排版与字号设计系统](../../../docs/typography-design-system.md)
- [列表与表格设计系统](../../../docs/list-table-design-system.md)
- [详情页设计系统](../../../docs/detail-page-design-system.md)
- [任务页设计系统](../../../docs/task-page-design-system.md)

前端工程约束（TanStack Query / Form、文案口径、自动化 DOM `id`、双轨抽屉规范等）见 [erp-client 开发规范](../../AGENTS.md)。业务组件统一由本目录 [`index.ts`](./index.ts)（含 [`task/index.ts`](./task/index.ts)）收口导出，不复制业务状态机、金额计算、权限判断或网络请求；共享码表（付款条件、单位、承运方等）见 [`lib/business-options.ts`](../../lib/business-options.ts)。

部门树人员选择使用 `PersonPickerDialog`；单选、多选、数据范围及回填规则见 [人员选择弹窗接入合同](./person-picker-dialog.md)。

## 纯 UI 分层边界与单向依赖铁律

各目录严格单向依赖（由 `scripts/check-feature-cycles.mjs` 静态门禁强制校验）：

| 层                                  | 负责                                                                                                                                                                                                                                                                                                                        | 不负责 / 禁止事项                                                                             |
| ----------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| `components/ui`                     | Base UI / shadcn 原语、无业务耦合底座（含下沉的 `OptionCombobox` / `MultiOptionCombobox`）、主题 token 与可访问性                                                                                                                                                                                                           | ERP 对象、单据状态与业务动作；禁止反向依赖 `form` / `business` / `features`                   |
| `components/form`                   | 基于 TanStack Form 的统一表单字段原语（`TextField`、`TextareaField`、`SelectField`、`MultiSelectField`、`DateField`、`DateTimeField`、`DateRangeField`、`CheckboxField`、`SwitchField`、`RadioGroupField`、`MoneyField`、`SingleFileField`、`MultiFileField`、`PdfUploadField`、`EvidenceUploadField`、`ReadOnlyField` 等） | 业务 DTO 适配与请求；**禁止反向导入 `@/components/business` 或 `@/features/*`**               |
| `components/business`（含 `task/`） | 跨领域复用的 ERP 页面模式、纯 UI 语义展示与受控动作插槽                                                                                                                                                                                                                                                                     | **严禁反向导入 `@/features/*`**；不内置 TanStack Query 请求、路由状态、权限判断或正式金额计算 |
| `hooks/`                            | 跨领域共享客户端 Hook（如 `useListSelection`、`useWorkspaceHotkeys`）                                                                                                                                                                                                                                                       | 特定领域 API DTO 绑定                                                                         |
| `features/*` 与页面                 | TanStack Query、TanStack Form、路由参数、API DTO 适配、mutation 与领域薄适配壳                                                                                                                                                                                                                                              | 重复造通用工作面骨架与基础单元格                                                              |
| ERP 后端                            | `allowedActions`、`actionBlockers`、金额与数量口径、状态机、并发与幂等                                                                                                                                                                                                                                                      | 让浏览器猜测正式结果                                                                          |

> **纯 UI 原语 + 领域薄适配双层模式**：对于需绑定特定领域查询或变更的复合组件（例如 `UnifiedMaterialViewer` 的材料加载回调、`WorkItemResponsibilityDialog`、`DeliveryAllocationOverview`、`ApprovalActionBar`、`WorkspaceApprovalContextRail`、`WorkspaceTaskIdentityHeader`、`WorkspaceTaskHeaderActions`），纯 UI 展示与受控交互原语保留在 `components/business/`（含 `components/business/task/`），绑定 `useQuery` / `useMutation` / 领域权限的薄适配层留在对应 `features/*` 目录。

## 导出清册与页面模式索引

下表覆盖 `@/components/business` 公共桶的现役规范导出（Phase 5 已删除零引用文件 `filter-select.tsx`、`detail-presentation.tsx` 及 7 个死导出：`FilterSelect`、`DetailSummary`、`DetailSummaryItem`、`DetailRecordSection`、`detailApprovalSummaryClassName`、`surfaceFlatClassName`、`ConflictResolutionDialog`，并将仍在用的 `DetailHint` 并入 `detail-workspace.tsx` 统一导出）：

| 页面模式                  | 规范导出组件 / 工具                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | 源码文件                                                                                                                                                                                                          | 覆盖场景                                                                                                                                                                                                                                                                                             |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 应用壳                    | `ErpAppShell`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | `shell.tsx`                                                                                                                                                                                                       | 全局导航与主内容容器（声明 `@container/workspace`）                                                                                                                                                                                                                                                  |
| 页面公共区                | `PageScaffold`、`PageActions`、`MetricStrip`、`MetricItem`、`DataFreshness`、`surfaceClassName`、`surfaceInsetClassName`、`workspaceTaskSurfaceClassName`、`workspaceEmbeddedScaffoldClassName`、`workspaceTaskSurfacePadClassName`                                                                                                                                                                                                                                                                                                                                                                                                      | `page.tsx`                                                                                                                                                                                                        | 内容区脚手架与扁平工作面；工作台作业面用水平细线分区，禁止套娃卡片                                                                                                                                                                                                                                   |
| 高密度列表与单元格        | `DataTable`、`DataTableViewOptions`、`DataTablePagination`、`ListDataTable`、`TableToolbar`、`TableIdentityCell`、`TableTextCell`、`TableEmptyCell`、`TableAsyncCell`、`TableStatusActionCell`、`TableSwitchCell`、`TableRowActions`、`IconActionButton`                                                                                                                                                                                                                                                                                                                                                                                 | `data-table.tsx`、`list-data-table.tsx`、`table-toolbar.tsx`、`table-cells.tsx`、`table-row-actions.tsx`、`icon-action-button.tsx`                                                                                | 服务端分页、排序、显隐、固定、调宽、跨页稳定选择、键盘行导航、内置三态空态/失败重试及标准单元格原语；`meta.width` 内置 `actions` 档位                                                                                                                                                                |
| 列表工作面与编排          | `ListWorkspacePage`、`ListWorkSurface`、`ListWorkspaceHeader`、`ListWorkspaceViews`、`ListSelectionBar`、`ListSelectionBarActionButton`、`useFrozenSelectionSnapshot`、`ListToolbar`、`QuickPreviewSheet`、`TaskActionSheet`、`WorkspaceSplitQueue`                                                                                                                                                                                                                                                                                                                                                                                      | `list-workspace.tsx`、`list-selection-bar.tsx`、`list.tsx`、`task-action-sheet.tsx`、`workspace-split-queue.tsx`                                                                                                  | 纯白通栏列表页壳（默认底部悬浮勾选胶囊 `list-selection-dock` 与自动视图排版）、Master-Detail 分栏队列、双轨抽屉（只读 `QuickPreviewSheet` 与 768px 有界轻作业 `TaskActionSheet`）；规范见 [列表与表格设计系统](../../../docs/list-table-design-system.md) 与 [AGENTS.md §7](../../AGENTS.md)         |
| 列表工作面筛选            | `ListWorkspaceFilterBar`（默认通栏托盘 `morePresentation="popover"` / `"tray"`，支持 `foldedChipKeys` 与 `idConvention`）、`ListSearchField`、`ListWorkspaceFilterField`、`ListFilterSections`、`ListFilterSection`、`ListFilterRangeField`、`FilterChip`                                                                                                                                                                                                                                                                                                                                                                                | `list-workspace-filter-bar.tsx`、`list-search-field.tsx`、`filter-chip.tsx`                                                                                                                                       | 查询按钮、常驻常用筛、通栏分组高级筛托盘、双输入区间胶囊、折叠已生效 chip、待查询提示                                                                                                                                                                                                                |
| 详情工作面与编排          | `DetailWorkspacePage`、`DetailWorkspaceChrome`、`DetailWorkspaceActionButton`、`DetailWorkspaceMeta`、`DetailWorkspaceDirtyBadge`、`DetailWorkspaceTabs`、`DetailWorkspaceSubTabs`、`DetailWorkspaceBody`、`DetailWorkspaceMetrics`、`DetailWorkspaceMetricItem`、`DetailWorkspaceFacts`、`DetailWorkspaceFact`、`DetailWorkspaceSection`、`DetailFormGrid`、`DetailInlineTable`、`DetailHint`、`DetailWorkspaceSaveDock`、`DetailWorkspaceSkeleton`、`detailWorkspaceStyles`、`WorkspaceFactGrid`、`WorkspaceFactItem`、`WorkspaceFactGroup`                                                                                            | `detail-workspace.tsx`、`detail-workspace-styles.ts`                                                                                                                                                              | 纯白通栏详情工作面：一体化吸顶身份头 + 通栏分区 Tabs + 非吸顶 KPI 摘要带 + 容器查询只读栅格 + 自包含柔底表单网格 `DetailFormGrid` 与无框内联编辑表 `DetailInlineTable` + 业务口径浮层提示 `DetailHint` + 脏状态底部悬浮保存胶囊；规范见 [详情页设计系统](../../../docs/detail-page-design-system.md) |
| 任务与审批工作台骨架      | `UnifiedTaskWorkbench`（含 `autoNext` 开关槽位）、`UnifiedTaskWorkbenchRailToggle`、`useUnifiedTaskWorkbenchRail`、`WorkspaceTaskPane`、`WorkspaceTaskFooter`、`WorkspaceTaskPageProvider`、`WorkspaceInspectorModeProvider`、`UnifiedMaterialViewer`、`PdfDocumentCanvas`、`WorkItemResponsibilityDialog`、`ReasonTextareaField`、`PersonPickerDialog`                                                                                                                                                                                                                                                                                  | `unified-task-workbench.tsx`、`workspace-task-pane.tsx`、`unified-material-viewer.tsx`、`pdf-document-canvas.tsx`、`work-item-responsibility-dialog.tsx`、`reason-textarea-field.tsx`、`person-picker-dialog.tsx` | 4 原型容器查询工作台、外部注入式材料对照/灯箱附件预览、单条/批量责任流转纯 UI 弹窗、UTF-8 字节/字符原因计数、部门树人员选择弹窗                                                                                                                                                                      |
| 任务页沉淀原语（`task/`） | `WorkspaceProcessingPane`、`WorkspaceCompactTaskHeader`、`WorkspaceTaskIdentityHeader`、`WorkspaceTaskHeaderActions`、`WorkspaceDocumentBadge`、`WorkspaceTaskContextHelp`、`WorkspaceHotkeyCheatSheetButton`、`DecisionKpiBar`、`DecisionKpiCell`、`TaskRiskSummaryBar`、`ApprovalActionBar`、`ApprovalRouteStepper`、`WorkspaceApprovalContextRail`、`WorkspaceApprovalResubmissionBanner`、`ApprovalLineFilterBar`、`ApprovalLinePagination`、`WorkspaceReworkCommandCard`、`ProfitValue`（别名 `PurchaseProfitValue`）、`FulfillmentRouteBadge`、`sourceRouteCategory` / `sourceAccentForIndex`、`CoverageRatioBar`、`CostMarginBar` | `task/*`（经 `task/index.ts` 再导出）；配套全局快捷键见 [`hooks/use-workspace-hotkeys.ts`](../../hooks/use-workspace-hotkeys.ts)                                                                                  | 审批决策、财务核验、驳回待处理行动指挥卡、多源履约色标与纯 CSS 微可视化条；规范见 [任务页设计系统](../../../docs/task-page-design-system.md)                                                                                                                                                         |
| 选择与筛选                | `OptionCombobox`、`MultiOptionCombobox`、`BusinessObjectCombobox`、`TreeCombobox`、`MultiTreeCombobox`、业务实体 Combobox（合同/销售单/客户/采购单/供应商/商品/品牌/商品分类/结算主体/仓库/负责人）                                                                                                                                                                                                                                                                                                                                                                                                                                      | `option-combobox.tsx`、`multi-option-combobox.tsx`、`tree-combobox.tsx`、`entity-comboboxes.tsx`、`selectors.tsx`                                                                                                 | 可搜索枚举/多选筛选（底座下沉于 `components/ui/` 并在此再导出）、有效业务对象选择、层级字典树形选择；**禁止**在业务页使用 `ui/select` / `NativeSelect` 或用自由 `Input` 录入已有业务对象                                                                                                             |
| 值、金额与范围横幅        | `BusinessStatusBadge`、`StatusTrackSummary`、`MoneyValue`（支持 `size="compact"` 与 `inheritColor`）、`QuantityValue`、`RateValue`、`DocumentTotals`、`taxAmountToneClass`、`MoneyChangePhrase`、`FundsScopeBanner`、`wholeUnallocatedCell`                                                                                                                                                                                                                                                                                                                                                                                              | `values.tsx`、`money-change-phrase.tsx`、`funds-scope-banner.tsx`、`funds-scope-columns.tsx`                                                                                                                      | `MoneyValue` 默认 `display="full"`；列表和概览选 `compact`，单价和纸质单据选 `exact`；万元起简写、亿元换单位；舍入标 `≈`，量级颜色只用于简写且 `inheritColor` 时不覆盖父色。复制单个金额输出精确元值。`taxBasis` 叠加税口径 Badge，列头或标签已说明时不得重复传入                                    |
| 正式单据与纸质投影        | `RevisionTimeline`、`RelatedDocumentList`、`PaperDocument`、`PaperDocumentViewport`、`DeliveryAllocationOverview`、`ChangeOrderBlockers`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | `document.tsx`、`paper-document.tsx`、`paper-document-viewport.tsx`、`delivery-allocation-overview.tsx`、`change-order-blockers.tsx`                                                                              | 版本追溯、容器查询关联单据列表、A4 正式单据投影（`frame="framed"` 内嵌 / `frame="bare"` 透明浮层）与右轨纸面缩放视口、多地址履约分配总览与变更单阻塞项                                                                                                                                               |
| 单据编辑与附件            | `EditableLineItemTable`、`AllocationWorkspace`、`DocumentAttachmentList`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | `editor.tsx`、`attachments.tsx`                                                                                                                                                                                   | 行项目编辑、回款/付款/发票等多对多分配、受控附件上传与扫描状态展示                                                                                                                                                                                                                                   |
| 正式动作与领域边界        | `FormalActionConfirmDialog`、`BatchImpactPreview`、`PrepaymentGate`、`CostCoverageNotice`、`InterfaceErrorResolutionPanel`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | `workflow.tsx`、`domain.tsx`                                                                                                                                                                                      | 正式提交与高危审批单层确认、批量影响预览、先款后货门禁、成本覆盖提示与接口结果未知恢复面板                                                                                                                                                                                                           |
| 任务、审计与导入          | `WorkTaskItem`、`BusinessDiffPanel`、`AuditTimeline`、`ImportStageIndicator`、`ImportIssueTable`、`BatchOperationResult`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | `audit-import.tsx`                                                                                                                                                                                                | 统一任务项、字段差异、追加式审计轨迹、导入分阶段校验与批处理结果                                                                                                                                                                                                                                     |
| 状态反馈与敏感信息        | `GuardedBusinessAction`、`BusinessEmptyState`、`BusinessFailureState`（8 类语义预设 + `onRetry`）、`AsyncSectionState`、`DraftSaveIndicator`、`ValidationSummary`、`FormalActionResult`、`BackgroundJobProgress`、`SensitiveValue`（短时明文 + 权限内默认一键复制 `copyable` + 失败重试）、`DiscardConfirmDialog`                                                                                                                                                                                                                                                                                                                        | `feedback.tsx`、`feedback-states.tsx`                                                                                                                                                                             | 禁用原因提示、三态空态、失败预设与重试、草稿状态、校验摘要、正式结果、后台任务进度、短时敏感明文与一键复制、未保存离开确认                                                                                                                                                                           |

页面专属的字段组合留在 `features/<domain>`，不因只出现一次就进入本目录。基础按钮、输入框、Tabs、Dialog、Popover、Tooltip 等仍直接使用 `components/ui`，不增加无业务语义的转发包装。

### 过渡期 `@deprecated` 原语与规范替代

下列旧原语仅为兼容少量存量页面而暂时保留，已标注 `@deprecated`，**新代码禁止继续使用**：

| 已废弃原语 / 属性                                             | 所在文件                           | 规范替代方案                                                                                                  |
| ------------------------------------------------------------- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `FixedOptionCheckboxFilter`                                   | `fixed-option-checkbox-filter.tsx` | 使用 `MultiOptionCombobox`（筛选条统一胶囊规范）                                                              |
| `ListWorkspaceInlineFilter`                                   | `list-workspace-filter-bar.tsx`    | 使用 `ListWorkspaceFilterBar` 的 `primaryFilters` + `ListWorkspaceFilterField`                                |
| `listWorkspaceFilterStatusText`                               | `list-workspace-filter-bar.tsx`    | 使用 `DataTable` / `ListDataTable` 工具栏内置摘要与底部分页条（空闲态不重复展示总数）                         |
| `listWorkspaceStyles.exportButton`                            | `list-workspace-styles.ts`         | 页头次要导出按钮统一使用 `ListWorkspacePage` `actions`（`tier: "quiet"`）或 `listWorkspaceStyles.quietButton` |
| `BusinessTableFrame`                                          | `list.tsx`                         | 迁至 `ListWorkSurface` / `ListWorkspacePage`                                                                  |
| `DocumentSummary`                                             | `document.tsx`                     | 迁至 `DetailWorkspaceFacts` + `DetailWorkspaceFact` / `DetailWorkspaceMetrics`                                |
| `DocumentSection`                                             | `document.tsx`                     | 迁至 `DetailWorkspaceSection`                                                                                 |
| `DetailWorkspaceChromeProps.summary`                          | `detail-workspace.tsx`             | 迁至 `DetailWorkspaceBody` 的 `summary` 槽（非吸顶通栏 KPI 摘要带）                                           |
| `ResultState` 遗留扩展字段（`idempotencyKey` 等页面私有字段） | `feedback.tsx`                     | 统一使用 `pendingIdempotencyKey` 等标准 `ResultStateBase` 契约                                                |

## 为业务模式补齐的 UI 原语

| UI 原语                                                                  | 扩展原因                                                                                                    |
| ------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------- |
| `Alert`                                                                  | 增加 `success`、`warning`、`info` 语义变体，全部读取主题 token                                              |
| `Checkbox`                                                               | 增加部分选择态，支持当前页与全范围选择表达                                                                  |
| `Sheet`                                                                  | 增加主题化 `preview`（轻量 400px）与 `detail`（半屏读主事实 768px）尺寸                                     |
| `Table`                                                                  | 用 `--table-row-height` 消费紧凑/舒适密度 token                                                             |
| `Input`                                                                  | 提供 `size="sm" \| "default" \| "lg"` 三档语义高度（28/32/36px），保护 iOS `text-base md:text-body-compact` |
| `DatePicker`、`DateRangePicker`、`DateTimePicker`、`DateTimeRangePicker` | 明确 `YYYY-MM-DD`、秒级时间与业务时区受控值，默认 `size="default"`（32px）且内置 `.num` 与筛选冰蓝态        |
| `OptionCombobox`、`MultiOptionCombobox`                                  | 下沉为无业务耦合的基础可搜索单/多选原语，供 `components/form` 与 `components/business` 共同复用             |
| `DescriptionList`                                                        | 基于 `@container` 容器查询的详情页语义化 `dl/dt/dd` 摘要布局                                                |
| `FileUpload`                                                             | 文件选择和拖放原语；上传请求仍由 TanStack Query mutation 执行                                               |
| `Timeline`                                                               | 版本、审计和状态历史的有序语义结构                                                                          |

遮罩、状态、边框、表格行和应用壳尺寸均由 `app/globals.css` 的语义 token 提供。业务组件不接受任意颜色或任意尺寸 props。

## DataTable 契约

`DataTable` 基于 `@tanstack/react-table` v8 当前稳定版实现。它是 headless table 的 ERP 适配层，不取数；页面把 TanStack Query 的结果和查询状态传入（标准列表页优先直接使用内置三态空态与失败重试的 `ListDataTable`）。

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
- 默认开启服务端分页、排序和筛选，TanStack 的 `pageIndex` 从 `0` 开始，API 的 `page` 从 `1` 开始，适配只在 feature 查询层完成；
- **排序契约**：`manualSorting`（默认 `true`）下只有页面接了 `onSortingChange` 排序入口才是真实交互；未接 `onSortingChange` 的页面列头**不再渲染排序按钮**（避免假箭头伪交互）。需要客户端本地排序时传 `manualSorting={false}`；
- **错误态契约**：查询失败必须由页面传 `errorState`（或传 `error` / `errorSummary` + `onRetry`，内置 `BusinessFailureState` 错误块），否则表格会把系统故障误报成「当前筛选没有结果」；
- **空态契约**：无数据时默认使用 `BusinessEmptyState`；首次进入无数据 / 无权限场景应使用 `ListDataTable` 的三态参数或传 `emptyState` / `emptyTitle` + `emptyDescription` + `emptyAction`；
- 列通过 `meta.label`、`meta.align`、`meta.numeric` 和 `meta.width` 声明语义；`meta.width` 只能使用 `flex`、`reference`、`tracks`、`amount`、`default`、`status`、`quantity`、`rate`、`actions` 等主题宽度档位，不传颜色、像素宽度或任意样式；
- `layout="inset"` 用于需要自带业务卡片内距与边界的列表，`layout="flush"`（默认）用于边界已由外部框架（如 `ListWorkSurface`）提供的场景；**默认 `density="comfortable"`**（行高与单元格留白对齐公司商品池 `/master-data/sellable-items`），高密度账表再显式传 `density="compact"`；`showToolbarSummary` 默认 `false`（总数统一由底部分页条承载）；
- **flush 分页**：`DataTablePagination` 使用 `px-table-cell-inline py-table-pagination-block`（在 `ListWorkSurface` 中左右边距由 `--scaffold-px` 接管），与结果头和单元格对齐。只有一页时「每页条数 / 共 N 条 / 已全部显示」收在左侧一组，不把状态文案甩到最右侧；多页时左侧元信息、右侧翻页；
- 页面选择只保存稳定 ID。选择“当前筛选全部结果”时必须调用批量预览 API 冻结选择快照，不能把客户端当前页推断为正式批量范围；
- 正在刷新时保留已有行；轮询/自动刷新页可传 `showRefreshingBanner={false}` 关闭「正在刷新，当前内容会保留」提示条（或传 `refreshingLabel` 定制文案）；
- 筛选后总行数变少时页码自动钳回最后一个有效页，避免「共 N 条」与空表并存；
- 行 `aria-label` 优先用 `rowLabel`（业务名），未传时回退为「第 N 行」，不暴露内部 ID；
- 单击非交互区域 / `Enter` 的语义由**页面**注入（`onRowPreview` / `onRowOpen`）：
    - W05 销售单：二者均打开轻量 `PaperDocument` 浮层，**不用** `QuickPreviewSheet`；
    - 其它单据列表：可打开 `QuickPreviewSheet size="detail"`；
    - 进对象中心用行内「查看详情」等明确按钮，不要与行点读单混成两个预览入口。只接了 `onRowOpen` 的页面，单击行会回落到该入口（触屏用户没有键盘 Enter）。上下方向键在当前结果行间移动，行内交互控件不会冒泡触发行级动作。

## 可搜索 Combobox 约定

筛选条、表单枚举与业务对象选择统一使用 Combobox（基于 `components/ui/combobox`），**禁止**在业务代码使用 `components/ui/select` 或 `NativeSelect`（静态门禁 `check-ui-theme.mjs` 已零容忍拦截）。

| 场景                                                                                           | 组件                                                                                                                                                                | 说明                                                                                                                             |
| ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| 状态/环境/角色/付款条件/单位等单选枚举与筛选                                                   | `OptionCombobox`                                                                                                                                                    | `{ value, label }[]`，内置搜索；`allowClear={false}` 用于必选筛选；共享码表见 `lib/business-options.ts`                          |
| 枚举多选筛选                                                                                   | `MultiOptionCombobox`                                                                                                                                               | 多选胶囊 Combobox，底座位于 `components/ui/multi-option-combobox.tsx`                                                            |
| 有效合同 / 销售单 / 客户 / 采购单 / 供应商 / 商品 / 品牌 / 商品分类 / 结算主体 / 仓库 / 负责人 | `ContractCombobox`、`CustomerCombobox`、`SupplierCombobox`、`BrandCombobox`、`CategoryCombobox`、`SettlementPartyCombobox`、`WarehouseCombobox`、`OwnerCombobox` 等 | 展示编号、状态与摘要；有值时自动点亮冰蓝态（`data-has-value="true"`）；分类为树形下拉；数据由 feature Query 注入，组件本身不请求 |
| 通用层级字典（分类 / 组织等森林数据）                                                          | `TreeCombobox` / `MultiTreeCombobox`                                                                                                                                | 接收 `TreeComboboxNode` 森林；支持展开/收起、`ArrowLeft/Right` 折叠、搜索平铺、远程检索与冰蓝筛选态                              |
| 通用带状态业务对象                                                                             | `BusinessObjectCombobox`                                                                                                                                            | 上述实体 Combobox 的底层                                                                                                         |
| TanStack Form 枚举单选 / 多选字段                                                              | `field.SelectField` / `field.MultiSelectField`                                                                                                                      | 直接消费 `components/ui` 底座，无反向依赖                                                                                        |
| 列表全文搜索（单号关键词混搜）                                                                 | `ListSearchField` / `Input` / `InputGroupInput`                                                                                                                     | 非实体单选场景，可继续自由输入                                                                                                   |

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

1. 列表页统一使用 `ListWorkspacePage` + `ListWorkspaceHeader` + `ListWorkspaceFilterBar`（默认通栏托盘模式）+ `ListWorkSurface` + `ListDataTable`；需要轻预览或侧边轻作业时分别组合 `QuickPreviewSheet` 或 `TaskActionSheet`（见 [AGENTS.md §7](../../AGENTS.md)）。**W05 销售单列表不要**再挂 `QuickPreviewSheet` / 行内预览按钮。
    - **筛选 vs 视图切换分层**：`ListWorkspaceFilterBar`（搜索框、常用筛、「更多筛选」）过滤的是当前这张表的行，**一律传给 `ListWorkSurface` 的 `toolbar=`**；
    - 决定「看哪张表/哪个口径」的视图切换 `ListWorkspaceViews` / `Tabs` 级别高于表格筛选，放在页头或主工作面上方，**不要**塞进筛选条件插槽。
2. **M4 对象中心与二级详情页**统一使用 `DetailWorkspacePage` + `DetailWorkspaceChrome` + `DetailWorkspaceBody`（规范见 [详情页设计系统](../../../docs/detail-page-design-system.md)）：
    - `DetailWorkspaceChrome` 作为唯一吸顶身份头（返回导航、标题、单号/版本、状态、元信息与动作）；负责/协作等放 `meta`，不要塞成长段 `secondaryActions` 文案；性质等身份标签放 `titleExtra`；通栏分区切换统一用 `DetailWorkspaceTabs`；
    - 金额/KPI 摘要使用 `DetailWorkspaceMetrics` + `DetailWorkspaceMetricItem` 挂在 `DetailWorkspaceBody` 的 `summary` 槽（置于正文顶部通栏，不吸顶）；统计项只展示数据，不绑定点击、选中态或跳转；筛选放入页签或筛选控件；
    - 正文分区统一用 `DetailWorkspaceSection`（水平细线分隔，禁止外框阴影卡片套娃）；表单网格统一用自包含 `<DetailFormGrid>`，不可修改字段使用 `@/components/form` 的 `<ReadOnlyField>`，内嵌可编辑明细表统一用 `<DetailInlineTable>`；
    - **页头动作按钮分级（对齐 `docs/detail-page-design-system.md` §3）**：次要操作（取消修改、导出、打印等）使用无框轻按钮 `Button size="sm" variant="ghost" className={detailWorkspaceStyles.quietButton}`（或 `<DetailWorkspaceActionButton tier="quiet" />`）；破坏性操作（停用、作废、删除、退役等）使用静默危险无框轻按钮 `Button size="sm" variant="ghost" className={detailWorkspaceStyles.quietDangerButton}`（静止灰字，悬停显柔和红底红字，或 `<DetailWorkspaceActionButton tier="quiet-danger" />`），均放在 `secondaryActions`；唯一实心飞书蓝主按钮放 `primaryAction`（`LoadingButton size="sm" variant="default" className="px-3.5"` 或 `<DetailWorkspaceActionButton tier="primary" />`）、排在最右；按钮统一 `size="sm"`（容器统一定高 `h-control` 32px）。
3. 正式详情页的只读信息栅格、版本与关联单据分别使用 `DetailWorkspaceFacts` / `DetailWorkspaceFact`（或任务页 `WorkspaceFactGroup` / `WorkspaceFactGrid`）、`RevisionTimeline`、`RelatedDocumentList`。履约、付款等并行状态必须分别展示，不合并成单一“状态”；关联采购单等需要履约/付款并行进度时，给 `RelatedDocumentList` 传 `tracks`，不要把进度塞进 `status.label` 长文案。
4. 需要纸张或账本版式投影时使用 `PaperDocument`：
    - 列表浮层：`frame="bare"` + **透明 Dialog 壳**（无标题栏/底栏/打印钮），见 `features/sales-orders/components/sales-order-paper-dialog.tsx`；
    - 页面内嵌：默认 `frame="framed"`；右轨缩放使用 `PaperDocumentViewport`；
    - 页面必须传入服务端已确认的行金额、汇总、状态和签章内容，组件不代替后端计算。
5. 编辑页把 TanStack Form 字段节点传入 `EditableLineItemTable` 或 `AllocationWorkspace`；组件不复制字段状态和校验。
6. 正式命令先显示 `FormalActionConfirmDialog` 或 `BatchImpactPreview`，成功后固定展示 `FormalActionResult`，不能只用瞬时 toast。
7. 业务异常、财务纠错和接口错误使用其真实任务或强类型事实投影，不创建通用 CRUD 卡片。

## 面向不同角色的文案与动作差异

共享组件被多个工作面复用。**某个页面需要不同措辞或更少动作时，加可选 prop 并保留原默认值，不要直接改默认文案** —— 那会波及其它工作面。

| 组件             | 扩展点                               | 默认                                                      | 谁在用非默认值                                               |
| ---------------- | ------------------------------------ | --------------------------------------------------------- | ------------------------------------------------------------ |
| `PrepaymentGate` | `copy?: Partial<PrepaymentGateCopy>` | 面向采购/财务的措辞（「先款后货门禁」「付款门禁已满足」） | W09 履约作业传一线口语（「先款条件」「货款已到，可以收货」） |
| `PrepaymentGate` | `presentation?: "panel" \| "badge"`  | `panel` 完整卡片                                          | W09 传 `badge`：顶栏结果徽章，悬停展开详情，不打断读单       |

只读角色不要用“渲染但禁用”表达 —— 禁用态不解释原因，且业务主动作本就不该让只读用户看到。替换成一句说明加一个有用的出口。

文案本身的用词口径见 [AGENTS.md §5](../../AGENTS.md) 与 [`erp-client/lib/ui-text.ts`](../../lib/ui-text.ts)。
