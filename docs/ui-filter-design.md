# ERP 筛选区域统一设计与实现规范

> **状态**：统一标准（2026-08-21，公司商品池页面基准）
>
> **基准页面**：`/master-data/sellable-items`「公司商品池」
>
> **基准实现**：`erp-client/features/master-data/components/list/sellable-list-toolbar.tsx`、
> `erp-client/features/master-data/components/pages/sellable-items-list-page.tsx`
>
> **适用范围**：`erp-client` 新增或重构的列表页、队列页、树形列表、卡片列表及分析页明细区
>
> **目标**：所有页面使用同一套筛选区域结构、状态模型、URL 契约和视觉语言；业务页面只配置字段，不再自行设计筛选条

本文是筛选区域的权威实现规范。历史迁移记录、页面缺陷清单和临时兼容方案不再写入本文；它们应放在对应工作区文档或任务记录中。

---

## 1. 统一结论

所有新页面默认采用「公司商品池」已经落地的模式：

**显式提交 + 可折叠结构化筛选面板 + URL 作为已生效状态唯一事实源。**

### 1.1 标准交互

- 关键词和结构化条件先编辑本地草稿，不在每次输入或选择时请求。
- 整个筛选区域共用一个 `<form>`。用户在表单内按 Enter，或点击展开面板底部的「应用全部筛选」，一次性应用关键词与全部「更多筛选」草稿。
- 主行提供业务快捷筛选和「更多筛选」按钮；快捷筛选直接写入 Applied URL 状态。
- 面板收起时，关键词输入框不放提交按钮，也不在输入框外并列「搜索」按钮；收起态只靠 Enter 提交。
- 面板展开时，面板底部只保留一个主按钮「应用全部筛选」。
- 提交成功后面板收起；校验失败时面板保持展开，并用 `aria-invalid` / `aria-describedby` 关联错误字段。
- 已生效条件以 chip 显示；「更多筛选」内存在已生效条件时显示「已启用」。
- 初次通过带结构化条件的深链进入时展开面板。后续提交不得因 URL 回填再次强制展开。
- 「清空全部」同时清理草稿、URL、校验错误和分页，并收起面板；「重置更多条件」只清除结构化条件，保留关键词和快捷筛选。

### 1.2 标准结构

```text
PageHeader：标题、说明、只读/口径提示、页级动作
BusinessTableFrame
├─ 筛选 form
│  └─ ListToolbar（独立轻卡片）
│     ├─ primary
│     │  ├─ search：关键词
│     │  └─ filters：快捷筛选 +「更多筛选」
│     └─ secondary
│        ├─ 已生效 FilterChip +「清空全部」
│        └─ 可折叠筛选面板 + 单一主提交
└─ 结果卡片
   ├─ 可见标题、结果数、操作说明、列设置
   ├─ DataTable / 空态
   └─ 分页
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

主行只放三类内容：

1. 关键词输入框。
2. 最多 3 个可直接判断的业务快捷筛选。
3. 「更多筛选」开关。

结果数不放在筛选主行，而放在结果卡可见标题里。

```text
[ 搜索图标 | 关键词输入框 ] [全部 N | 单一供应商 N | 全国可供 N] [更多筛选 已启用 ˅]
```

使用组件：

- 外层：`ListToolbar`
- 搜索：`ListSearchField`；内部使用 `InputGroup`、`InputGroupAddon`、`InputGroupInput`
- 快捷筛选：`role="group"` 分段控件 + 受控 `Button aria-pressed`；组内横向滚动，不拆成三处按钮
- 更多筛选：`Button variant="outline"`
- 状态提示：`Badge variant="info"`，文案固定为「已启用」
- 展开图标：`ChevronDownIcon`，展开时 `rotate-180`
- 全部清除：位于 chip 行的 `Button variant="ghost" size="xs"`，文案「清空全部」
- 表级动作（视图 / 列设置）：`Button variant="outline"`，由 `BusinessTableFrame` 结果卡标题栏承载，
  **始终留在结果卡标题行**，不随面板展开被垂直居中拽进面板

#### 3.1.1 控件高度：主行与面板输入统一 `h-control`

筛选主行搜索框、「更多筛选」和面板内输入类控件统一为 **`h-control`（36px / `--spacing-control`）**。
`ListToolbar` 已在自身子树把 `InputGroup` 和 `filters` 槽里的按钮压到这个高度，业务页不必逐个传尺寸。

| 控件 | 达成方式 |
| --- | --- |
| `Button`（主行、面板主提交） | 用**默认档**（`h-control`）。不要为了对齐再叠一层高度 class |
| chip 行「清空全部」 | 公司商品池使用 `size="xs"` |
| `FixedOptionRadioFilter` / `FixedOptionCheckboxFilter` | 共享组件当前内置 `h-8`，业务页直接使用 |
| `Input` / `InputGroup` / `OptionCombobox` / `CategoryCombobox` | 表单默认就是 `h-control`；在 `ListToolbar` 内无需再改 |

三条硬性要求：

- **主行不要混入第二种主控件高度。** 不要在搜索框旁再放一个更矮或更高的独立「搜索」按钮。
- **不要为了对齐去改 `components/ui` 的默认尺寸。** 筛选区的收敛由 `ListToolbar` 的作用域规则负责。
- 下拉面板经 portal 渲染在 `ListToolbar` 子树之外，不受该规则影响，也不需要受影响。

`ListToolbar` 已定义主行响应式结构，业务页不得复制它内部的 flex 布局：

```tsx
<ListToolbar
    search={keywordInput}
    filters={shortcutFiltersAndPanelToggle}
    secondary={chipsAndFilterPanel}
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
- 搜索框内部不放提交按钮；也不得在输入框外并列独立「搜索」按钮。
- 占位文案必须说明可搜索的业务字段，不写泛化的「请输入关键词」。
- 列表页提供 `/` 聚焦快捷键；Dialog 或 Sheet 打开时不得聚焦背景搜索框。
- 搜索属于整个筛选表单。收起态按 Enter、展开态点「应用全部筛选」，都必须调用同一个 `applyFilters`。

### 3.3 更多筛选按钮

```tsx
<Button
    type="button"
    variant="outline"
    aria-expanded={panelOpen}
    aria-controls={panelId}
    onClick={() => setPanelOpen((open) => !open)}
>
    <FilterIcon data-icon="inline-start" aria-hidden="true" />
    更多筛选
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

面板使用公司商品池页的固定样式：

```tsx
<div
    id={panelId}
    className="flex w-full flex-col gap-3 border-t pt-3"
    aria-label="筛选条件"
>
    {/* 固定枚举行 */}
    {/* 网格字段 */}
    {/* 校验错误 */}
    <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
        <p className="text-xs text-muted-foreground">
            将同时应用上方关键词和以下筛选条件；结果也用于导出。
        </p>
        <div className="flex flex-wrap items-center gap-2 sm:justify-end">
            <Button type="button" variant="ghost">重置更多条件</Button>
            <Button type="submit">
                <SearchIcon data-icon="inline-start" aria-hidden="true" />
                应用全部筛选
            </Button>
        </div>
    </div>
</div>
```

视觉要求：

- 面板是筛选轻卡片内部的第 2 层，不再嵌套 Card、阴影或另一圈圆角边框。
- 与主行通过 `border-t pt-3` 分隔，字段组间距固定 `gap-3`。
- 面板标题通常由「更多筛选」按钮和各字段标签共同表达，不再增加「筛选条件」大标题。
- 最后一行左侧必须解释提交范围，右侧依次为「重置更多条件」和唯一主按钮「应用全部筛选」。

### 3.5 提交按钮位置

同一个 `<form>` 里只有一条提交路径：`applyFilters`。可见提交按钮只在面板展开时出现。

| 状态 | 提交入口 |
| --- | --- |
| 面板收起 | 表单内 Enter；搜索框不渲染提交按钮 |
| 面板展开 | 面板最后一行右侧的主按钮「应用全部筛选」；Enter 走同一 `onSubmit` |

```tsx
filters={<ShortcutFiltersAndMoreFilterToggle />}
secondary={hasChips || panelOpen ? <ChipsAndFilterPanel /> : undefined}
```

禁止：

- 搜索框尾部再放提交箭头，或在输入框外再放独立「搜索」按钮。
- 展开后主行和面板底部同时出现两个主提交。
- 搜索输入框属于一个 form，筛选面板属于另一个 form。
- 在 form 内嵌套 form。
- 收起态 Enter 与展开态主按钮调用不同的应用逻辑。

### 3.6 结果数与清除

结果数放在结果卡可见标题，不放在 `ListToolbar.actions`：

```tsx
<BusinessTableFrame
    showHeader
    title={
        <span className="inline-flex items-baseline gap-2">
            可售商品
            <span className="font-normal text-muted-foreground">
                {rowCount} 条
            </span>
        </span>
    }
/>
```

- 结果数显示当前已生效筛选对应的结果，不显示草稿预估。
- 已生效条件统一进入独立 chip 行，关键词和快捷筛选也不例外。
- chip 行文案固定为「已筛选」；末尾动作固定为「清空全部」。
- 空态使用同一个 `clearAllFilters`，但空态按钮可写「清除筛选」。
- 「重置更多条件」只允许出现在展开面板底部，且不得清除关键词或快捷筛选。

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
<div className="flex min-w-0 flex-col gap-1.5 text-sm">
    <span className="text-muted-foreground">品牌</span>
    <OptionCombobox className="w-full" {...props} />
</div>
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
- 「清空全部」会一并清除全部来源锁定条件。

当 chip 和展开面板同时存在时：

```tsx
secondary={
    hasChips || panelOpen ? (
        <div className="w-full space-y-3">
            {hasChips ? (
                <div className="flex flex-wrap items-center gap-2 border-t pt-3">
                    <span className="text-xs text-muted-foreground">已筛选</span>
                    {chips}
                    <Button type="button" variant="ghost" size="xs">
                        清空全部
                    </Button>
                </div>
            ) : null}
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
    setPanelOpen(false)
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
- 点击快捷筛选。
- 点击「清空全部」。

```tsx
React.useEffect(() => {
    setDraft(toFilterDraft(applied))
    setFilterError(null)
}, [applied])
```

面板展开态不得在回填 effect 中无条件重置。否则用户刚点击「应用全部筛选」收起面板后，URL 更新会立即把面板重新打开。

`applied` 必须由稳定的 URL 参数值通过 `useMemo` 派生，或使用稳定的序列化签名作为 effect 依赖；不要在每次 render 创建新对象后直接作为依赖，否则会造成重复回填。

若关键词输入框正处于编辑状态，页面可以做焦点保护，避免其它 URL 条件变化覆盖尚未提交的关键词；但 `clearAllFilters` 必须直接清空关键词草稿，不能依赖 effect 猜测。

### 5.5 面板展开态

- 初始值由 URL 中是否存在结构化条件决定。
- 用户可以手动展开或收起，展开态本身不写 URL。
- 用户手动收起已生效面板时，「已启用」仍显示。
- 展开或收起不得提交草稿。
- 校验通过并成功写 URL 后必须收起；校验失败保持展开。
- 首次深链进入带结构化条件时展开；已挂载页面的 URL 回填只同步 Draft，不得抢夺用户当前展开态。

### 5.6 清除与局部重置

标准清除函数必须完成全部动作：

```tsx
const clearAllFilters = React.useCallback(() => {
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

全部清除范围：

| 清除 | 保留 |
| --- | --- |
| `q`、全部结构化筛选、来源锁定参数、分页 | 排序、视图、scope、分析期间/口径、返回路径等导航上下文 |

公司商品池的 `resetMoreFilters` 必须同时清除商品类型、分类、品牌、供应商、可供区域和销售价区间的 Draft 与 Applied URL；必须保留 `q` 和 `supplyPreset`，保持面板展开，并回第 1 页。

页面必须集中声明筛选参数清单，`hasAppliedFilters`、`applyFilters`、`clearAllFilters`、查询参数和导出参数应使用同一清单，避免漏清或隐形状态。

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

「清空全部」只能清 `FILTER_PARAM_KEYS` 和分页，不得用遍历全部 search params 的方式误删视图或导航上下文。

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

`MetricStrip` 位于筛选表单之外。主行快捷筛选位于 `ListToolbar.filters`。两者都只能表达明确、可回退的 Applied 条件。

例外：W01 我的工作台的待办口径（待我处理 / 已超期 / 受阻 / 我发起的）使用主面板工具条分段控件，**不**使用独立 `MetricStrip` 轻卡。数量仍来自服务端统计，点击写入与列表相同的 URL 筛选。

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
- 快捷筛选必须显示每个选项在当前服务端结果集内的数量；切换选项时这些数量不得随当前选中项一起归零。
- 公司商品池固定使用 `supplyPreset=single-supplier|nationwide`；该参数只派生显示视图，不改变服务端销售资格规则。
- `单一供应商` 固定解释为 `supplierCount === 1`；`全国可供` 固定解释为 `supplyRegions` 包含「全国」。
- 导出必须消费同一个 `supplyPreset`，保证导出范围与表格一致。

禁止把只会滚动、跳转或改变视觉高亮的指标伪装成筛选项。

---

## 8. 公司商品池完整实现合同

本节是 `/master-data/sellable-items` 的页面级权威合同，也是后续同类页面的视觉与交互样板。实现者必须先满足本节，再复用第 8.2 节的通用状态代码。

### 8.1 页面构成与固定文案

| 区域 | 必须实现 | 禁止 |
| --- | --- | --- |
| 页头 | `PageHeader density="default"`；标题「公司商品池」；资格说明；`PageHeaderMeta` 三项口径「只读查询 / 销售可见口径 / 采购成本受保护」；动作「导出当前结果」 | 新建、编辑、采购成本、内部 API 或修订术语上屏 |
| 筛选卡 | 搜索框、`全部 / 单一供应商 / 全国可供` 快捷筛选、「更多筛选」 | 输入框内嵌提交箭头、输入框外再放独立「搜索」按钮、主行再放结果数 |
| 已筛选行 | 所有 Applied 条件均显示 chip，包含 `q` 与 `supplyPreset`；末尾「清空全部」 | 隐形查询参数、只清输入框不清 URL |
| 更多筛选 | 商品类型、分类、品牌、供应商、可供区域、销售价；底部范围说明、「重置更多条件」、「应用全部筛选」 | 第二个主提交、嵌套 Card、输入即请求 |
| 结果标题 | 「可售商品」、当前条数、行点击说明、列设置 | 使用读屏器专用标题代替可见结果标题 |
| 表格 | 商品名称·规格、SPU 编号、销售价（含税）、市场参考价、可供区域、供应保障 | 操作列、采购成本列、商品池独立 ID/版本 |

页面结构固定如下：

```text
PageHeader
├─ 公司商品池 + 资格说明 + 三项口径提示
└─ 导出当前结果

ListToolbar 筛选卡
├─ 收起态：[搜索] [全部 N | 单一供应商 N | 全国可供 N] [更多筛选]
├─ 已生效：已筛选 [chip…] [清空全部]
└─ 展开态：结构化字段
   └─ 范围说明 [重置更多条件] [应用全部筛选]

BusinessTableFrame 结果卡
├─ 可售商品 N 条 + 操作说明 + 列设置
├─ DataTable
└─ 分页
```

交互合同：

1. 收起态 Enter 与展开态「应用全部筛选」必须是同一 `<form>`、同一 `applySellableFilters`。
2. 点击「更多筛选」只改变 UI 展开态；不得请求、不得写 URL。
3. 「应用全部筛选」先校验销售价区间，再一次性写 `q` 与全部结构化参数；成功后收起面板，失败时保持展开。
4. Enter 在搜索框或面板输入控件内提交同一批条件；Combobox 自身选项键盘操作按组件契约处理。
5. 快捷筛选直接写 `supplyPreset` 并回第 1 页；不得覆盖尚未提交的关键词或更多筛选草稿。
6. 任一 chip 只移除自己的 Applied 条件；销售价上下界作为一个 chip 一起移除。
7. 单击行或聚焦行后按 Enter 均打开 `QuickPreviewSheet size="preview"`；完整商品资料只从预览底部进入。
8. 表格不设固定最小高度。少量结果时卡片随内容收缩，空白留在页面画布，不在表格卡片内制造大块假工作区。

供应保障显示合同：

| 条件 | 显示 | 语义 |
| --- | --- | --- |
| `supplierCount <= 1` | warning 徽标「单一供应商」+ 警示图标 | 存在断供后无替换来源的风险 |
| `supplierCount > 1` | success 徽标「N 家可供」 | 当前存在多个有效来源 |
| 可供区域 1–2 项 | 每项一个 `secondary` badge | 便于快速扫描 |
| 可供区域超过 2 项 | 前 2 项 + `neutral` badge「+N」；完整内容写入 `title` | 不撑高行 |

响应式合同：

- `lg` 及以上：搜索、快捷筛选和「更多筛选」同一主行。
- `sm` 至 `lg`：主行允许换行；搜索占首行可用宽度，快捷筛选保持成组。
- 小于 `sm`：搜索独占一行；快捷筛选横向可滚动；更多筛选字段单列；底部两个动作同组可换行。
- 表格保持横向滚动，不删除身份列、销售价或供应保障；身份列固定在左侧。
- 结果数始终在结果卡标题，不随断点挪到筛选主行。

样式边界：

- 页面样式优先使用现有 token、`ListToolbar` 和组件局部 class；不得为单页向 `globals.css` 增加选择器。
- 只有跨页面的语义 token 或共享组件默认行为需要统一时才修改 `globals.css`；修改后必须验证其它列表页。
- `--table-header` 是表头语义 token；禁止在页面内写独立灰色值。
- `DataTable` 默认不得有 `min-h-*`；需要固定作业高度的队列页面必须通过显式可选 prop 自行声明。

权威代码映射：

| 合同 | 文件 |
| --- | --- |
| 页面组合、页头提示、结果卡 | `erp-client/features/master-data/components/pages/sellable-items-list-page.tsx` |
| 筛选结构与固定文案 | `erp-client/features/master-data/components/list/sellable-list-toolbar.tsx` |
| 搜索输入框 | `erp-client/features/master-data/components/list/list-search-field.tsx` |
| Applied / Draft / UI 状态 | `erp-client/features/master-data/hooks/use-sellable-list-filters.ts` |
| 快捷筛选计数、chip、导出快照 | `erp-client/features/master-data/hooks/use-sellable-list-state.ts` |
| 快捷筛选派生规则 | `erp-client/features/master-data/lib/sellable-supply-preset.ts` |
| 表格列与供应保障表达 | `erp-client/features/master-data/hooks/use-sellable-list-columns.tsx` |
| 结果卡与表格表面 | `erp-client/components/business/list.tsx`、`erp-client/components/business/data-table.tsx` |

### 8.2 通用状态实现模板

以下模板用于其它同类列表的 URL、Draft 与提交状态。公司商品池的结构与文案必须直接以第 8.1 节和权威代码映射为准，不得用模板中的占位文案覆盖。

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
        setPanelOpen(false)
    }, [draft, patchUrl])

    const clearAllFilters = React.useCallback(() => {
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
                    <Button
                        type="button"
                        variant="outline"
                        aria-expanded={panelOpen}
                        aria-controls={panelId}
                        onClick={() => setPanelOpen((open) => !open)}
                    >
                        <FilterIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        更多筛选
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
                }
                secondary={
                    panelOpen ? (
                        <div
                            id={panelId}
                            className="flex w-full flex-col gap-3 border-t pt-3"
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

                            <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                <p className="text-xs text-muted-foreground">
                                    将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                </p>
                                <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                    <Button type="button" variant="ghost">
                                        重置更多条件
                                    </Button>
                                    <Button type="submit">
                                        <SearchIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                        应用全部筛选
                                    </Button>
                                </div>
                            </div>
                        </div>
                    ) : undefined
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
- `lg`：查询工具与 `actions` 分置左右。公司商品池不使用 `actions` 槽。
- 搜索容器：默认整行；`sm` 起最小宽度为 `--spacing-search-min`（`16rem`），并 `flex-1` 吃掉主行剩余空间。业务页不得再覆盖成固定 `28rem` / `34rem`。
- 筛选按钮组：允许 `flex-wrap`。
- 快捷筛选保持为一个 `role="group"` 分段控件；空间不足时组内横向滚动，不拆散为三处按钮。

### 9.2 面板

- 固定枚举行在小屏上下排列，`sm` 起使用标签列 + 选项列。
- 网格字段：默认 1 列，`sm` 2 列，`lg` 4 列。
- 区间输入内部保持同一行；空间不足时输入自身收缩，不让面板横向溢出。
- 提交按钮始终位于面板末尾右侧；小屏可与「重置更多条件」换行但不得分离出面板。

### 9.3 禁止

- 为移动端复制一套 local-only 筛选。
- 使用固定像素总宽度导致横向滚动。
- 在业务页覆盖 `ListToolbar` 内部布局以追求单页特例。
- 小屏直接隐藏已生效条件而不给查看和清除入口。

---

## 10. 可访问性与键盘操作

- 整个筛选区域使用一个语义 `<form>`。
- Enter 提交当前全部草稿。
- 「更多筛选」按钮必须是 `type="button"`。
- 「更多筛选」按钮提供 `aria-expanded` 和 `aria-controls`。
- 面板提供可识别的 `aria-label`。
- 输入框、Combobox、日期和区间端点都有可读标签。
- 结果卡条数变化应可被读屏器感知；不要把结果数做成打断式播报。
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

- 提交后可以禁用当前可见的提交按钮或显示加载状态，避免重复提交。
- 筛选草稿在请求期间不得被清空。
- 远程 Combobox 的加载状态由组件的 `loading` 表达。

---

## 12. 反模式

以下实现一律不通过评审：

1. `ListToolbar` 放在 `BusinessTableFrame` 外。
2. 搜索框内嵌提交箭头，或在输入框外再放独立「搜索」按钮。
3. 展开面板后主行和面板底部同时保留主提交。
4. 搜索和每个结构化筛选分别提交，产生多次 URL 更新和请求。
5. Draft 直接进入 query key，用户每改一项就请求。
6. `hasActiveFilters` 漏掉某些接口实际消费的参数。
7. 「清空全部」只清关键词，或误删排序、视图、期间和返回路径。
8. URL 中写入 `all`、空字符串或默认页码。
9. 查询消费了没有控件、没有 chip、也无法清除的隐形参数。
10. 6 个以上选项仍横向铺成固定单选或复选按钮。
11. 业务页面手写平行的 flex 工具栏、色板、圆角或阴影。
12. 空结果时卸载整个筛选区。
13. 指标条和筛选面板分别维护同一维度的两套状态。
14. 用户尚未提交筛选，导出却读取了未提交草稿。
15. 为移动端创建不写 URL 的第二套筛选逻辑。
16. 筛选主行混入第二种主控件高度。
17. 表级动作（视图 / 列设置）随筛选面板展开被垂直居中，落进面板中间。
18. 把结果数放回 `ListToolbar.actions`，与结果卡标题形成两套计数。

---

## 13. 新页面开发步骤

1. **列参数清单**：区分 filter、view、analysis、navigation 参数。
2. **定义解析器**：从 URL 解析 Applied 状态，非法值回默认。
3. **定义 Draft 类型与默认值**：所有字段受控。
4. **定义 `hasStructuredFilters` 和 `hasAppliedFilters`**。
5. **实现单一 `applyFilters`**：全量校验、一次 patch、分页归一、成功后收起面板。
6. **实现单一 `clearAllFilters`**：草稿、错误、面板、URL 同时重置。
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
- [ ] 主行只包含搜索、快捷筛选和面板开关。
- [ ] 结果数在结果卡可见标题，不在筛选主行。
- [ ] 结构化字段全部位于折叠面板。
- [ ] 面板使用统一类名和响应式网格。

### 14.2 状态与 URL

- [ ] URL 是 Applied 状态唯一事实源。
- [ ] Draft 变化不会请求。
- [ ] Enter 和展开态「应用全部筛选」走同一提交函数。
- [ ] 提交一次性更新全部筛选并回第 1 页。
- [ ] 默认值从 URL 省略。
- [ ] 浏览器前进、后退和刷新可恢复筛选。
- [ ] 有结构化条件的初始深链会展开面板；提交成功后 URL 回填不会再次强制展开。
- [ ] 每个查询参数都有控件或可移除 chip。

### 14.3 清除与结果

- [ ] `hasAppliedFilters` 覆盖全部筛选参数。
- [ ] 「清空全部」同时重置 Draft、错误、面板、URL 和分页。
- [ ] 「重置更多条件」清除结构化条件，但保留关键词和快捷筛选。
- [ ] 清除保留排序、视图、期间与导航上下文。
- [ ] 结果卡条数、表头摘要、导出和空态读取 Applied 状态。
- [ ] 筛选无结果时工具栏仍存在，空态可清除筛选。

### 14.4 样式与可访问性

- [ ] 使用 `ListToolbar`，没有平行手写工具条。
- [ ] 筛选主行搜索框和按钮同为 `h-control`（36px）。
- [ ] 面板展开后页面只剩「应用全部筛选」一个主提交；收起态没有独立「搜索」按钮或内嵌提交箭头。
- [ ] 表级动作位于可见结果标题栏，没有被拽到筛选面板中间。
- [ ] 固定枚举使用共享 Radio / Checkbox 筛选组件。
- [ ] 动态选项使用 Combobox，字段标签清晰。
- [ ] 小屏无横向溢出，没有第二套筛选状态。
- [ ] 「更多筛选」按钮有 `aria-expanded` / `aria-controls`。
- [ ] 错误提示与字段正确关联。
- [ ] `/` 可聚焦搜索，弹层打开时不会误触发。

---

## 15. 权威参考实现

| 目的 | 文件 |
| --- | --- |
| 公司商品池完整页面组合 | `erp-client/features/master-data/components/pages/sellable-items-list-page.tsx` |
| 搜索、快捷筛选、chip 与更多筛选 | `erp-client/features/master-data/components/list/sellable-list-toolbar.tsx` |
| 搜索输入框 | `erp-client/features/master-data/components/list/list-search-field.tsx` |
| URL、Draft、提交、清除与回填 | `erp-client/features/master-data/hooks/use-sellable-list-filters.ts` |
| 快捷筛选计数与导出一致性 | `erp-client/features/master-data/hooks/use-sellable-list-state.ts` |
| 表格列与供应保障表达 | `erp-client/features/master-data/hooks/use-sellable-list-columns.tsx` |
| `ListToolbar` 与 `BusinessTableFrame` | `erp-client/components/business/list.tsx` |
| 筛选区 `h-control` 收敛规则 | `erp-client/components/business/list.tsx`（`ListToolbar` 根类名） |
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

---

## 16. LLM 执行入口

后续需要让 LLM 继续制作同一视觉语言的查询列表页时，**第一份且必须提供的文档就是本文：`docs/ui-filter-design.md`**。

执行输入必须至少包含：

```text
请严格执行 docs/ui-filter-design.md，使用其中“公司商品池完整实现合同”作为页面基准。
先读取该文档列出的权威代码映射；用户可见文案同时遵守 docs/ui-glossary.md；
若页面属于具体工作面，再读取对应 docs/ui-workspaces/w*.md。
不得自行新增第二套筛选交互、页面私有色板或独立全局 CSS。
```

LLM 必须按以下顺序执行：

1. 识别页面业务对象、只读/可写边界和禁止展示字段。
2. 复用 `PageHeader`、`ListToolbar`、`BusinessTableFrame`、`DataTable`，不得先手写平行组件。
3. 先实现收起态、展开态、Applied chip、快捷筛选和结果卡五个视觉状态。
4. 再接 URL、TanStack Query、导出和空态，确保它们消费同一 Applied 状态。
5. 补单元测试，至少覆盖单一主提交（收起态 Enter / 展开态「应用全部筛选」）、收起态没有独立搜索按钮、URL 回填、局部重置、全部清除、快捷筛选和导出一致性。
6. 在 1440、1024、768、375 宽度下做浏览器截图与交互验收；记录因登录、权限或数据阻塞而未验证的边界。

公司商品池的业务资格、价格和敏感字段边界仍以 `docs/ui-workspaces/w14-basic-data.md` 为准；本文负责页面结构、视觉、交互与实现门禁。两者冲突时，业务边界不得被视觉规范覆盖。
