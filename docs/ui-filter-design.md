# ERP 筛选区域统一设计与实现规范

> **状态**：统一标准（2026-08-12）
>
> **基准页面**：`/master-data/products`「商品与 SKU」
>
> **基准实现**：`erp-client/features/master-data/master-data-list-toolbar.tsx` 的商品分支
>
> **适用范围**：`erp-client` 新增或重构的列表页、队列页、树形列表、卡片列表及分析页明细区
>
> **目标**：所有页面使用同一套筛选区域结构、状态模型、URL 契约和视觉语言；业务页面只配置字段，不再自行设计筛选条

本文是筛选区域的权威实现规范。历史迁移记录、页面缺陷清单和临时兼容方案不再写入本文；它们应放在对应工作区文档或任务记录中。

---

## 1. 统一结论

所有新页面默认采用「商品与 SKU」页面已经落地的模式：

**显式提交 + 可折叠结构化筛选面板 + URL 作为已生效状态唯一事实源。**

### 1.1 标准交互

- 关键词和结构化条件先编辑本地草稿，不在每次输入或选择时请求。
- 用户点击「搜索」或在表单内按 Enter 后，一次性应用全部草稿条件。
- 有结构化筛选的页面，主行提供「高级筛选」按钮。
- 面板收起时，「搜索」按钮紧邻关键词输入框。
- 面板展开时，主行的「搜索」按钮隐藏，同一个提交动作移动到面板底部右侧。
- 已生效的结构化条件存在时，刷新、分享链接或浏览器前进/后退后，面板自动展开，并显示「已启用」。
- 「清除筛选」同时清理草稿、URL、校验错误和分页，并收起面板。

### 1.2 标准结构

```text
结果工作面
└─ BusinessTableFrame / surfacePanelClassName
   ├─ 标题与说明
   ├─ 筛选 form
   │  └─ ListToolbar
   │     ├─ primary
   │     │  ├─ search：关键词输入框
   │     │  ├─ filters：搜索按钮 + 高级筛选开关
   │     │  └─ actions：结果数 + 清除筛选
   │     └─ secondary
   │        ├─ 来源锁定 FilterChip（如有，收起时仍可见）
   │        └─ 可折叠筛选面板（展开时）
   ├─ SelectionScopeBar（如有批量选择）
   └─ 表格 / 列表 / 空态
```

### 1.3 不再作为新页面默认方案

以下模式只允许保留在已明确说明的特殊工作面中，新页面不得自行采用：

- 输入即请求的关键词防抖搜索。
- 每个下拉框变化后立即写 URL。
- 搜索、状态、来源、日期等全部挤在一行。
- 页面头部、指标条、表格卡片各有一套互不统一的筛选状态。
- Popover、Sheet 和常驻工具条同时承载同一组筛选条件。

若页面确实需要即时筛选，必须在对应页面设计文档中说明业务原因；其布局、URL、清除和可访问性仍须遵守本文。

---

## 2. 页面归属

筛选区域必须贴在它所过滤的结果工作面上，不能裸放在页面画布中。

| 页面类型 | 筛选区域位置 | 结果容器 |
| --- | --- | --- |
| 表格列表 | `BusinessTableFrame.toolbar` | `BusinessTableFrame` |
| 树形列表 / 卡片列表 | 结果主表面的顶部 | `surfacePanelClassName` |
| 队列工作区 | 队列 sticky 处理面的顶部 | `surfacePanelClassName` |
| 分析页明细 | 明细结果表面的顶部 | `BusinessTableFrame.toolbar` |
| 多 Tab 页面 | 每个 Tab 自己的结果表面内 | 每个 Tab 独立维护可见筛选 |

### 2.1 表格列表标准骨架

```tsx
<PageScaffold density="compact">
    <PageHeader title="…" actions={…} />

    {/* 可选；指标不是筛选表单的一部分 */}
    <MetricStrip>{/* 指标 */}</MetricStrip>

    <BusinessTableFrame
        title="…列表"
        description={filterDescription}
        toolbar={filterToolbar}
        table={tableOrEmptyState}
    />
</PageScaffold>
```

硬性要求：

- `BusinessTableFrame` 是表格与筛选的唯一主表面，外层不得再包一张 Card。
- 筛选表单必须传入 `toolbar`，不得在 frame 上方平行渲染另一条工具栏。
- 空结果只替换表格内容，筛选区域必须常驻。
- 表头说明在有筛选时应展示人可读摘要；无筛选时展示默认操作说明。

### 2.2 队列、树形和卡片列表

这些页面不强制使用 `BusinessTableFrame`，但必须使用同一套内部结构：

```tsx
<section className={surfacePanelClassName}>
    <form>{/* ListToolbar + 折叠面板 */}</form>
    <div>{/* 队列、树或卡片结果 */}</div>
</section>
```

不得因为结果不是表格，就手写另一套搜索和筛选布局。

### 2.3 分析维度与明细筛选

期间、口径、组织范围等会改变整张分析报表含义的控件属于**分析维度**，可以位于明细表面之外；关键词、状态、对象和明细字段属于**明细筛选**，必须进入明细结果表面。

「清除筛选」默认只清理明细筛选，不清期间和口径。若需要重置分析维度，应使用独立的「重置分析条件」动作。

---

## 3. 筛选区域视觉结构

### 3.1 主行

主行只放四类内容：

1. 关键词输入框。
2. 收起态的「搜索」按钮。
3. 「高级筛选」开关。
4. 结果数与「清除筛选」。

```text
[ 搜索图标 | 关键词输入框 ] [搜索] [筛选图标 高级筛选 已启用 ˅]   [共 N 条] [清除筛选]
```

使用组件：

- 外层：`ListToolbar`
- 搜索：`InputGroup` + `InputGroupAddon` + `InputGroupInput`
- 搜索按钮：`Button size="sm"`
- 高级筛选：`Button variant="outline" size="sm"`
- 状态提示：`Badge variant="info"`，文案固定为「已启用」
- 展开图标：`ChevronDownIcon`，展开时 `rotate-180`
- 计数：`text-xs text-muted-foreground`
- 清除：`Button variant="ghost" size="sm"`

`ListToolbar` 已定义主行响应式结构，业务页不得复制它内部的 flex 布局：

```tsx
<ListToolbar
    search={keywordInput}
    filters={submitButtonAndPanelToggle}
    secondary={chipsAndFilterPanel}
    actions={countAndClear}
/>
```

### 3.2 搜索输入框

标准结构：

```tsx
<InputGroup>
    <InputGroupAddon>
        <SearchIcon aria-hidden="true" />
    </InputGroupAddon>
    <InputGroupInput
        ref={searchInputRef}
        value={draft.q}
        onChange={(event) => updateDraft("q", event.target.value)}
        placeholder="编号、名称……"
        aria-label="搜索……"
    />
</InputGroup>
```

规则：

- URL 参数统一使用 `q`。
- 搜索框只承载输入，不把提交按钮嵌入 `InputGroup` 内。
- 占位文案必须说明可搜索的业务字段，不写泛化的「请输入关键词」。
- 列表页提供 `/` 聚焦快捷键；Dialog 或 Sheet 打开时不得聚焦背景搜索框。
- 搜索属于整个筛选表单，Enter 与点击「搜索」行为完全一致。

### 3.3 高级筛选按钮

```tsx
<Button
    type="button"
    variant="outline"
    size="sm"
    aria-expanded={panelOpen}
    aria-controls={panelId}
    onClick={() => setPanelOpen((open) => !open)}
>
    <FilterIcon data-icon="inline-start" aria-hidden="true" />
    高级筛选
    {hasAppliedStructuredFilters ? (
        <Badge variant="info">已启用</Badge>
    ) : null}
    <ChevronDownIcon
        data-icon="inline-end"
        aria-hidden="true"
        className={panelOpen ? "rotate-180 transition-transform" : "transition-transform"}
    />
</Button>
```

「已启用」只表示 URL 中存在**已生效**的结构化条件，不表示用户刚改但尚未提交的草稿。

### 3.4 展开面板

面板使用商品与 SKU 页的固定样式：

```tsx
<div
    id={panelId}
    className="flex w-full flex-col gap-3 rounded-lg border border-border/60 bg-muted/30 px-3 py-3"
    aria-label="筛选条件"
>
    {/* 固定枚举行 */}
    {/* 网格字段 */}
    {/* 校验错误 */}
    <div className="flex justify-end">
        <Button type="submit" size="sm">
            <SearchIcon data-icon="inline-start" aria-hidden="true" />
            搜索
        </Button>
    </div>
</div>
```

视觉要求：

- 面板是主工作面内部的轻层级，不使用额外 Card、阴影或厚重标题栏。
- 圆角固定 `rounded-lg`。
- 边框固定 `border border-border/60`。
- 背景固定 `bg-muted/30`。
- 内边距固定 `px-3 py-3`，字段组间距固定 `gap-3`。
- 面板标题通常由高级筛选按钮和各字段标签共同表达，不再增加「筛选条件」大标题。
- 提交动作固定在最后一行右侧。

### 3.5 搜索按钮位置

搜索按钮在同一个 `<form>` 中按面板状态切换位置：

| 状态 | 位置 |
| --- | --- |
| 面板收起 | 主行，紧邻关键词输入框 |
| 面板展开 | 面板最后一行右侧 |

```tsx
filters={
    <>
        {!panelOpen ? <SubmitSearchButton /> : null}
        <AdvancedFilterToggle />
    </>
}
secondary={panelOpen ? <FilterPanel /> : undefined}
```

禁止：

- 展开后主行和面板底部同时出现两个搜索按钮。
- 搜索输入框属于一个 form，筛选面板属于另一个 form。
- 在 form 内嵌套 form。
- 两个按钮调用不同的应用逻辑。

### 3.6 结果数与清除

标准结构：

```tsx
<>
    <span className="text-xs text-muted-foreground" aria-live="polite">
        共 {rowCount} 条
    </span>
    {hasAppliedFilters ? (
        <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={clearFilters}
        >
            清除筛选
        </Button>
    ) : null}
</>
```

- 结果数显示当前已生效筛选对应的结果，不显示草稿预估。
- 只有存在已生效筛选时才显示「清除筛选」。
- 空态中也必须提供同一个 `clearFilters` 动作。
- 不得出现「重置」「清空」「恢复默认」等平行文案。

---

## 4. 字段布局与组件选型

筛选字段只分为两类：**固定枚举行**和**网格字段**。

### 4.1 固定单选枚举

适用于选项固定、标签较短、需要直接比较的单选条件，例如商品类型、启停、版本、上架和供给覆盖。

统一使用 `FixedOptionRadioFilter`：

```tsx
<FixedOptionRadioFilter
    label="启停"
    value={draft.lifecycleStatus}
    onValueChange={(value) => updateDraft("lifecycleStatus", value)}
    options={LIFECYCLE_OPTIONS}
/>
```

组件已内置结构：

```text
桌面：[4.5rem 标签] [可换行的选项组]
移动：[标签]
      [可换行的选项组]
```

选项视觉：

- 高度 `h-8`。
- 未选中：虚线边框、弱文字。
- 选中：实线主色边框、正常文字、`font-medium`。
- hover：轻背景和更清晰边框。
- 必须有文字，不得只用色点或图标。

选型边界：

- 单选建议包含「全部」后不超过 5 项。
- 标签很长、选项超过 5 项或需要搜索时，改用 `OptionCombobox`。
- 「全部」只存在于草稿 UI，写 URL 时必须转换为参数缺省。

### 4.2 固定多选枚举

适用于最多约 5 个、可以组合命中的固定选项，统一使用 `FixedOptionCheckboxFilter`。

```tsx
<FixedOptionCheckboxFilter
    label="供应能力"
    value={draft.capabilityCodes}
    onValueChange={(value) => updateDraft("capabilityCodes", value)}
    options={CAPABILITY_OPTIONS}
/>
```

超过 5 项、选项会动态增长或需要搜索时，使用 `MultiOptionCombobox`，不得无限横向铺开复选项。

### 4.3 网格字段

动态字典、远程对象、日期、数值和区间字段进入网格：

```tsx
<div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
    {/* 字段 */}
</div>
```

标准字段外壳：

```tsx
<label className="flex min-w-0 flex-col gap-1.5 text-sm">
    <span className="text-muted-foreground">品牌</span>
    <OptionCombobox className="w-full" {...props} />
</label>
```

| 字段类型 | 组件 | 说明 |
| --- | --- | --- |
| 可搜索单选 | `OptionCombobox` | 分类、品牌、供应商、仓库等 |
| 可搜索多选 | `MultiOptionCombobox` | 多状态、多标签、多组织等 |
| 日期 | `DatePicker` | 使用业务日期文案，默认值可省略 URL |
| 日期区间 | 两个日期控件 | 从、至必须有明确标签 |
| 数值 | `Input` | 设置 `inputMode`，提交时校验 |
| 数值区间 | 两个 `Input` | 使用「最低值 / 最高值」与中间「至」 |
| 布尔条件 | 固定单选 | 使用「全部 / 是 / 否」业务文案，不直接使用 Switch |

### 4.4 区间输入

商品销售价区间是标准参考：

```tsx
<div className="flex min-w-0 flex-col gap-1.5 text-sm">
    <span className="text-muted-foreground">销售价</span>
    <div className="flex items-center gap-1.5">
        <Input className="w-0 min-w-0 flex-1" placeholder="最低价" />
        <span className="text-muted-foreground">至</span>
        <Input className="w-0 min-w-0 flex-1" placeholder="最高价" />
    </div>
    {error ? (
        <span className="text-xs text-destructive" role="alert">
            {error}
        </span>
    ) : null}
</div>
```

规则：

- 校验发生在提交时；修改任一输入后可清除旧错误。
- 下界不得大于上界。
- 金额比较使用项目统一的精确数值方式，不用浮点数直接比较。
- 错误字段设置 `aria-invalid` 和 `aria-describedby`。
- 校验失败时不得写 URL、不得请求、不得重置分页。

### 4.5 来源锁定条件

从其它页面带入的 `customerId`、`salesOrderId`、`purchaseOrderId`、`skuId` 等条件必须显性展示为 `FilterChip`，不能成为用户看不见的 URL 状态。

- chip 位于 `ListToolbar.secondary`。
- 面板收起时仍保持可见。
- 展示业务编号或名称，不展示内部 ID。
- chip 的关闭按钮只移除该条件。
- 「清除筛选」会一并清除全部来源锁定条件。

当 chip 和展开面板同时存在时：

```tsx
secondary={
    hasChips || panelOpen ? (
        <div className="w-full space-y-2">
            {hasChips ? <div className="flex flex-wrap gap-2">{chips}</div> : null}
            {panelOpen ? <FilterPanel /> : null}
        </div>
    ) : undefined
}
```

---

## 5. 状态模型

每个筛选页面必须明确区分三层状态。

| 状态 | 含义 | 存放位置 | 是否触发查询 |
| --- | --- | --- | --- |
| Applied | 已生效筛选 | URL | 是 |
| Draft | 用户正在编辑、尚未提交的条件 | React 本地受控 state | 否 |
| UI | 面板展开、校验提示等界面状态 | React 本地 state | 否 |

### 5.1 URL 是已生效状态唯一事实源

- 查询参数、结果数、筛选摘要、导出范围和空态都读取 Applied 状态。
- TanStack Query 的 query key 和 query 参数只使用 Applied 状态。
- Draft 变化不得直接触发服务端查询。
- 不得同时维护另一份「已生效筛选 state」。

### 5.2 草稿初始化

```tsx
const applied = parseFilters(searchParams)
const [draft, setDraft] = React.useState(() => toFilterDraft(applied))
const [panelOpen, setPanelOpen] = React.useState(
    hasStructuredFilters(applied),
)
```

草稿默认值约定：

- 关键词：`""`
- 单选全部：`"all"`
- 可清除对象：`null`
- 多选：`[]`
- 区间端点：`""`

### 5.3 提交

所有字段通过同一个 `applyFilters` 提交：

```tsx
const applyFilters = React.useCallback(() => {
    const error = validateFilterDraft(draft)
    setFilterError(error)
    if (error) return

    patchUrl({
        q: draft.q.trim() || null,
        lifecycleStatus:
            draft.lifecycleStatus === "all" ? null : draft.lifecycleStatus,
        categoryId: draft.categoryId,
        minPrice: draft.minPrice.trim() || null,
        maxPrice: draft.maxPrice.trim() || null,
        page: null,
    })
}, [draft, patchUrl])
```

提交契约：

1. 先规范化和校验全部草稿。
2. 一次性 patch 全部筛选参数，禁止逐字段连续更新 URL。
3. 默认值转为删除参数，不把 `all`、空字符串写进 URL。
4. 删除 `page` 或写为第 1 页。
5. 使用 `router.replace(..., { scroll: false })`，不膨胀浏览历史。
6. URL 更新后由 Applied 状态驱动查询。

筛选区是查询控制表单，不是业务资料编辑表单；可以使用受控 `<form>` 和本地草稿 state，但不得使用非受控 `FormData` 拼装隐式状态。涉及业务数据保存的表单仍必须遵守 TanStack Form 规范。

### 5.4 URL 回填

以下情况必须把 URL 重新同步到草稿：

- 首次进入。
- 刷新页面。
- 浏览器前进或后退。
- 从外部深链进入。
- 点击指标快速筛选。
- 点击「清除筛选」。

```tsx
React.useEffect(() => {
    setDraft(toFilterDraft(applied))
    setFilterError(null)
    setPanelOpen(hasStructuredFilters(applied))
}, [applied])
```

`applied` 必须由稳定的 URL 参数值通过 `useMemo` 派生，或使用稳定的序列化签名作为 effect 依赖；不要在每次 render 创建新对象后直接作为依赖，否则会造成重复回填。

若关键词输入框正处于编辑状态，页面可以做焦点保护，避免其它 URL 条件变化覆盖尚未提交的关键词；但 `clearFilters` 必须直接清空关键词草稿，不能依赖 effect 猜测。

### 5.5 面板展开态

- 初始值由 URL 中是否存在结构化条件决定。
- 用户可以手动展开或收起，展开态本身不写 URL。
- URL 中存在结构化条件时，外部导航后自动展开。
- 用户手动收起已生效面板时，「已启用」仍显示。
- 展开或收起不得提交草稿。

### 5.6 清除筛选

标准清除函数必须完成全部动作：

```tsx
const clearFilters = React.useCallback(() => {
    setDraft(DEFAULT_FILTER_DRAFT)
    setFilterError(null)
    setPanelOpen(false)
    patchUrl({
        q: null,
        lifecycleStatus: null,
        categoryId: null,
        minPrice: null,
        maxPrice: null,
        sourceId: null,
        page: null,
    })
}, [patchUrl])
```

清除范围：

| 清除 | 保留 |
| --- | --- |
| `q`、全部结构化筛选、来源锁定参数、分页 | 排序、视图、scope、分析期间/口径、返回路径等导航上下文 |

页面必须集中声明筛选参数清单，`hasAppliedFilters`、`applyFilters`、`clearFilters`、查询参数和导出参数应使用同一清单，避免漏清或隐形状态。

---

## 6. URL 与查询契约

### 6.1 参数规则

- 关键词统一为 `q`。
- 默认值不写 URL。
- 页码统一为 `page`，第 1 页省略。
- 多选参数使用项目已有 codec；没有 codec 时使用去重、排序后的逗号分隔值。
- URL 中的非法枚举值在解析时降级为默认值，不能继续传给接口。
- 每个被查询消费的参数必须有可见控件或可移除 `FilterChip`。

### 6.2 参数分类

```ts
const FILTER_PARAM_KEYS = [
    "q",
    "lifecycleStatus",
    "categoryId",
    "supplierId",
    "minPrice",
    "maxPrice",
] as const

const VIEW_PARAM_KEYS = ["view", "scope", "sort"] as const
const NAVIGATION_PARAM_KEYS = ["from", "returnTo", "sessionId"] as const
```

「清除筛选」只能清 `FILTER_PARAM_KEYS` 和分页，不得用遍历全部 search params 的方式误删视图或导航上下文。

### 6.3 TanStack Query

```tsx
const appliedFilters = parseFilters(searchParams)

const listQuery = useDomainListQuery({
    q: appliedFilters.q || undefined,
    lifecycleStatus: appliedFilters.lifecycleStatus,
    categoryId: appliedFilters.categoryId,
    page: appliedFilters.page,
})
```

- query key 必须包含全部已生效筛选和分页参数。
- 不允许在组件 `useEffect` 中自行 `fetch`。
- 导出使用同一份 Applied 筛选快照，不得读取未提交 Draft。
- 结果计数、空态和筛选摘要以查询结果和 Applied 状态为准。

---

## 7. 指标条与快速筛选

`MetricStrip` 位于筛选表单之外。指标有两种且只能选一种语义：

| 类型 | 组件 | 行为 |
| --- | --- | --- |
| 只读指标 | `MetricItem` | 只展示，不可点击 |
| 快速筛选指标 | `MetricFilterItem` | 点击直接更新一个明确的 Applied 条件 |

快速筛选规则：

- 点击是明确的一次性动作，可以直接写 URL。
- 必须写入筛选面板使用的同一个 canonical 参数，不能维护第二套状态。
- URL 更新后必须回填 Draft，使指标和面板选中值一致。
- 如果额外保留 `metricKey` 仅用于高亮，它不得作为查询条件。
- 必须提供「全部」或其它可回退入口。
- 指标与面板条件冲突时，应重置同一维度的旧值，不能制造用户无法理解的空结果。

禁止把只会滚动、跳转或改变视觉高亮的指标伪装成筛选项。

---

## 8. 完整实现模板

以下模板是新列表页的推荐起点。字段可以替换，结构和行为不得自行改写。

```tsx
function DomainListFilterToolbar() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    const appliedQuery = searchParams.toString()
    const applied = React.useMemo(
        () => parseDomainFilters(new URLSearchParams(appliedQuery)),
        [appliedQuery],
    )

    const [draft, setDraft] = React.useState(() => toDraft(applied))
    const [panelOpen, setPanelOpen] = React.useState(
        hasStructuredDomainFilters(applied),
    )
    const [error, setError] = React.useState<string | null>(null)
    const panelId = React.useId()

    const patchUrl = React.useCallback(
        (patch: Record<string, string | null>) => {
            const next = new URLSearchParams(searchParams.toString())
            for (const [key, value] of Object.entries(patch)) {
                if (value == null || value === "") next.delete(key)
                else next.set(key, value)
            }
            const query = next.toString()
            router.replace(query ? `${pathname}?${query}` : pathname, {
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    const applyFilters = React.useCallback(() => {
        const nextError = validateDraft(draft)
        setError(nextError)
        if (nextError) return

        patchUrl({
            q: draft.q.trim() || null,
            status: draft.status === "all" ? null : draft.status,
            categoryId: draft.categoryId,
            minAmount: draft.minAmount.trim() || null,
            maxAmount: draft.maxAmount.trim() || null,
            page: null,
        })
    }, [draft, patchUrl])

    const clearFilters = React.useCallback(() => {
        setDraft(DEFAULT_DRAFT)
        setError(null)
        setPanelOpen(false)
        patchUrl({
            q: null,
            status: null,
            categoryId: null,
            minAmount: null,
            maxAmount: null,
            page: null,
        })
    }, [patchUrl])

    React.useEffect(() => {
        setDraft(toDraft(applied))
        setError(null)
        setPanelOpen(hasStructuredDomainFilters(applied))
    }, [applied])

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                applyFilters()
            }}
        >
            <ListToolbar
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            ref={searchInputRef}
                            value={draft.q}
                            onChange={(event) =>
                                setDraft((current) => ({
                                    ...current,
                                    q: event.target.value,
                                }))
                            }
                            placeholder="编号、名称"
                            aria-label="搜索记录"
                        />
                    </InputGroup>
                }
                filters={
                    <>
                        {!panelOpen ? (
                            <Button type="submit" size="sm">
                                <SearchIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                搜索
                            </Button>
                        ) : null}
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            aria-expanded={panelOpen}
                            aria-controls={panelId}
                            onClick={() => setPanelOpen((open) => !open)}
                        >
                            <FilterIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            高级筛选
                            {hasStructuredDomainFilters(applied) ? (
                                <Badge variant="info">已启用</Badge>
                            ) : null}
                            <ChevronDownIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                                className={
                                    panelOpen
                                        ? "rotate-180 transition-transform"
                                        : "transition-transform"
                                }
                            />
                        </Button>
                    </>
                }
                secondary={
                    panelOpen ? (
                        <div
                            id={panelId}
                            className="flex w-full flex-col gap-3 rounded-lg border border-border/60 bg-muted/30 px-3 py-3"
                            aria-label="列表筛选条件"
                        >
                            <FixedOptionRadioFilter
                                label="状态"
                                value={draft.status}
                                onValueChange={(status) =>
                                    setDraft((current) => ({
                                        ...current,
                                        status,
                                    }))
                                }
                                options={STATUS_OPTIONS}
                            />

                            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                {/* OptionCombobox / DatePicker / Input */}
                            </div>

                            {error ? (
                                <span
                                    className="text-xs text-destructive"
                                    role="alert"
                                >
                                    {error}
                                </span>
                            ) : null}

                            <div className="flex justify-end">
                                <Button type="submit" size="sm">
                                    <SearchIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    搜索
                                </Button>
                            </div>
                        </div>
                    ) : undefined
                }
                actions={
                    <>
                        <span
                            className="text-xs text-muted-foreground"
                            aria-live="polite"
                        >
                            共 {rowCount} 条
                        </span>
                        {hasAppliedDomainFilters(applied) ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                onClick={clearFilters}
                            >
                                清除筛选
                            </Button>
                        ) : null}
                    </>
                }
            />
        </form>
    )
}
```

---

## 9. 响应式规则

响应式由共享组件和固定网格断点承担，不为移动端维护第二套筛选状态。

### 9.1 主行

`ListToolbar` 当前契约：

- 默认：纵向排列。
- `sm`：搜索与按钮可横排并自动换行。
- `lg`：查询工具与 actions 分置左右。
- 搜索容器：`min-w-0 flex-1 sm:max-w-sm`。
- 筛选按钮组：允许 `flex-wrap`。

### 9.2 面板

- 固定枚举行在小屏上下排列，`sm` 起使用标签列 + 选项列。
- 网格字段：默认 1 列，`sm` 2 列，`lg` 4 列。
- 区间输入内部保持同一行；空间不足时输入自身收缩，不让面板横向溢出。
- 提交按钮始终位于面板末尾右侧。

### 9.3 禁止

- 为移动端复制一套 local-only 筛选。
- 使用固定像素总宽度导致横向滚动。
- 在业务页覆盖 `ListToolbar` 内部布局以追求单页特例。
- 小屏直接隐藏已生效条件而不给查看和清除入口。

---

## 10. 可访问性与键盘操作

- 整个筛选区域使用一个语义 `<form>`。
- Enter 提交当前全部草稿。
- 高级筛选按钮必须是 `type="button"`。
- 高级筛选按钮提供 `aria-expanded` 和 `aria-controls`。
- 面板提供可识别的 `aria-label`。
- 输入框、Combobox、日期和区间端点都有可读标签。
- 计数使用 `aria-live="polite"`，避免打断式播报。
- 校验错误使用 `role="alert"`，字段关联 `aria-invalid` / `aria-describedby`。
- `/` 聚焦搜索时忽略输入框、文本域、Dialog 和 Sheet 场景。
- 不使用颜色作为唯一选中或错误表达。

---

## 11. 空态、错误态和加载态

### 11.1 空态

| 场景 | 组件 | 动作 |
| --- | --- | --- |
| 系统尚无数据 | `BusinessEmptyState kind="no-data"` | 新建或业务引导 |
| 当前筛选无结果 | `BusinessEmptyState kind="filter"` | 「清除筛选」 |
| 无数据范围 | `BusinessEmptyState kind="no-scope"` | 权限或范围说明 |

筛选无结果时：

- 筛选工具栏和展开面板保持原状。
- 空态使用与工具栏同一个 `clearFilters`。
- 不得把「无数据」误写成「筛选无结果」。

### 11.2 错误态

- 查询失败不得卸载筛选区域。
- 用户可以修改条件后重新搜索。
- 重试使用 Query 的 `refetch`，不另写请求。
- 已有上次成功数据时按页面约定保留，但必须明确提示数据未刷新。

### 11.3 加载态

- 提交后可以禁用搜索按钮或显示加载状态，避免重复提交。
- 筛选草稿在请求期间不得被清空。
- 远程 Combobox 的加载状态由组件的 `loading` 表达。

---

## 12. 反模式

以下实现一律不通过评审：

1. `ListToolbar` 放在 `BusinessTableFrame` 外。
2. 搜索输入框内嵌搜索按钮，另一侧又出现独立搜索按钮。
3. 展开面板后主行和面板底部同时保留提交按钮。
4. 搜索和每个结构化筛选分别提交，产生多次 URL 更新和请求。
5. Draft 直接进入 query key，用户每改一项就请求。
6. `hasActiveFilters` 漏掉某些接口实际消费的参数。
7. 清除筛选只清关键词，或误删排序、视图、期间和返回路径。
8. URL 中写入 `all`、空字符串或默认页码。
9. 查询消费了没有控件、没有 chip、也无法清除的隐形参数。
10. 6 个以上选项仍横向铺成固定单选或复选按钮。
11. 业务页面手写平行的 flex 工具栏、色板、圆角或阴影。
12. 空结果时卸载整个筛选区。
13. 指标条和筛选面板分别维护同一维度的两套状态。
14. 用户尚未点击搜索，导出却读取了未提交草稿。
15. 为移动端创建不写 URL 的第二套筛选逻辑。

---

## 13. 新页面开发步骤

1. **列参数清单**：区分 filter、view、analysis、navigation 参数。
2. **定义解析器**：从 URL 解析 Applied 状态，非法值回默认。
3. **定义 Draft 类型与默认值**：所有字段受控。
4. **定义 `hasStructuredFilters` 和 `hasAppliedFilters`**。
5. **实现单一 `applyFilters`**：全量校验、一次 patch、分页归一。
6. **实现单一 `clearFilters`**：草稿、错误、面板、URL 同时重置。
7. **用 `BusinessTableFrame.toolbar` 或结果 surface 承载 form**。
8. **用 `ListToolbar` 组合主行与 secondary**。
9. **按字段类型选择固定枚举行或网格字段**。
10. **让 Query、导出、计数、摘要和空态只读取 Applied 状态**。
11. **补 URL 回填、`/` 快捷键和可访问性属性**。
12. **按第 14 节验收**。

---

## 14. 验收清单

### 14.1 结构

- [ ] 筛选区位于被过滤结果的同一主表面。
- [ ] 表格页使用 `BusinessTableFrame.toolbar`。
- [ ] 页面只存在一个筛选 form。
- [ ] 主行只包含搜索、提交、面板开关、计数和清除。
- [ ] 结构化字段全部位于折叠面板。
- [ ] 面板使用统一类名和响应式网格。

### 14.2 状态与 URL

- [ ] URL 是 Applied 状态唯一事实源。
- [ ] Draft 变化不会请求。
- [ ] Enter 和两个位置的「搜索」走同一提交函数。
- [ ] 提交一次性更新全部筛选并回第 1 页。
- [ ] 默认值从 URL 省略。
- [ ] 浏览器前进、后退和刷新可恢复筛选。
- [ ] 有结构化条件的深链会自动展开面板。
- [ ] 每个查询参数都有控件或可移除 chip。

### 14.3 清除与结果

- [ ] `hasAppliedFilters` 覆盖全部筛选参数。
- [ ] 清除同时重置 Draft、错误、面板、URL 和分页。
- [ ] 清除保留排序、视图、期间与导航上下文。
- [ ] 工具栏计数、表头摘要、导出和空态读取 Applied 状态。
- [ ] 筛选无结果时工具栏仍存在，空态可清除筛选。

### 14.4 样式与可访问性

- [ ] 使用 `ListToolbar`，没有平行手写工具条。
- [ ] 固定枚举使用共享 Radio / Checkbox 筛选组件。
- [ ] 动态选项使用 Combobox，字段标签清晰。
- [ ] 小屏无横向溢出，没有第二套筛选状态。
- [ ] 高级筛选按钮有 `aria-expanded` / `aria-controls`。
- [ ] 错误提示与字段正确关联。
- [ ] `/` 可聚焦搜索，弹层打开时不会误触发。

---

## 15. 权威参考实现

| 目的 | 文件 |
| --- | --- |
| 商品与 SKU 筛选区结构 | `erp-client/features/master-data/master-data-list-toolbar.tsx` |
| URL、Draft、提交、清除与回填 | `erp-client/features/master-data/master-data-page.tsx` |
| `ListToolbar` 与 `BusinessTableFrame` | `erp-client/components/business/list.tsx` |
| 固定单选筛选 | `erp-client/components/business/fixed-option-radio-filter.tsx` |
| 固定多选筛选 | `erp-client/components/business/fixed-option-checkbox-filter.tsx` |
| 可搜索单选 | `erp-client/components/business/option-combobox.tsx` |
| 来源锁定条件 | `erp-client/components/business/filter-chip.tsx` |
| 主工作面样式 | `erp-client/components/business/page.tsx` |
| 页面视觉总规范 | `docs/erp-ui-design.md` |
| 用户可见术语 | `docs/ui-glossary.md` |

当本文与某个历史页面实现冲突时：

- 新页面以本文为准。
- 重构页面应向本文收敛。
- 共享组件行为以 `components/business` 当前契约为准；若确需扩展，优先增加兼容的可选 prop，不为单页复制组件。
