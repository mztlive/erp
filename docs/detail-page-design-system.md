# ERP 详情页设计系统（Detail Workspace Design System）

> **设计基准标杆**：主数据编辑型 [`erp-client/features/master-data/pages/product-detail-page.tsx`](../erp-client/features/master-data/pages/product-detail-page.tsx)（商品详情）；单据双栏型 [`erp-client/features/sales-orders/pages/sales-order-detail-page.tsx`](../erp-client/features/sales-orders/pages/sales-order-detail-page.tsx)（销售单详情）  
> **主题体系**：飞书 Universe Design / 纯白通栏工作面（与 [`docs/list-table-design-system.md`](./list-table-design-system.md) 100% 同源）  
> **适用范围**：所有桌面端对象详情与详情编辑页（主数据编辑中心：商品、供应商、客户、仓库；以及正式单据详情中心：销售单、采购单、合同、供给资料等）。

---

## 1. 设计理念与五条黄金法则

列表工作面已经全面升级为「纯白通栏 + 水平细线 + 无框柔底轻控件 + 13px 紧凑节奏」。详情页必须与列表页保持完全一致的表面与排版语言，杜绝从列表页点进详情页时跳回「浅灰画布 + 多层白卡套娃」的割裂感。

### 五条黄金法则

1. **纯白通栏工作面，彻底消灭「套娃卡片」**
   - 详情页骨架（`DetailWorkspacePage` / `.erp-detail-workspace-page`）铺满纯白底色（`bg-card`），去掉浅灰画布、外层 `surfacePanelClassName` 边框、内层 `bg-muted/20` 灰底和分区白卡边框。
   - 所有左外沿——头部缩略图/标题、分区 Tabs、分区标题、表单首列、内嵌表格首列——沿同一条 `--scaffold-px` 严格垂直对齐（详见 `erp-client/app/globals.css` 与 `detail-workspace-styles.ts`）：
     - 窄窗 `< 768px`（`< 48rem` 视口）：`16px`（`--spacing-workspace-inline`）；
     - 中屏 `≥ 768px`（`≥ 48rem` 视口）且工作区容器 `< 70rem`：`20px`（`--spacing-page-inline`）；
     - 宽屏容器查询 `@container workspace (min-width: 70rem)`（容器宽度 `≥ 1120px`，扣除侧栏后约对应 `≥ 1360px` 视口，对应 `@[70rem]/workspace:px-page-inline-lg`）：`24px`（`--spacing-page-inline-lg`）；
     - 矮屏（`min-width: 48rem` 且 `max-height: 960px`）自动将 `--spacing-page-inline` / `--spacing-page-inline-lg` 分别收紧至 `16px` / `20px`。
   - 分区之间仅靠充裕留白（`py-6`）与极淡水平细线（`border-t border-border/60`）建立秩序；若需右侧摘要栏（如销售单/采购单），使用 `1px` 竖细线（`border-l border-border/60`）分栏，同样零套娃卡片。
2. **身份头 + 分区 Tabs 一体化吸顶**
   - `DetailWorkspaceChrome` 将对象缩略图、标题、主状态、元信息、操作按钮与底部分区 Tabs 整合为一块纯白通栏吸顶头部（`sticky top-0 z-20 bg-card border-b border-border/60`）。
   - 静止态零阴影，仅在页面向下滚动产生遮挡时（`data-scrolled="true"`）显现柔和底影（`0 2px 8px -2px rgb(31 35 41 / 0.06)`），确保长表单或宽 SKU 表滚动时「保存更新」等主操作始终触手可及。
3. **单栏表单限宽保舒适，双栏主列贴齐竖细线，表格全宽保密度**
   - `DetailWorkspaceSection` 提供两档语义宽度：
     - `width="form"`（单栏下 `max-w-4xl`，约 `896px`）：用于单栏工作面的基础资料表单、版本时间线等阅读/录入块，杜绝 1440px 宽屏下出现 1100px 超长空白输入框；**在带右侧栏的双栏工作面（`[data-slot="detail-workspace-split"]`）中，主列已由右侧栏与 `1px` 竖细线收束，`width="form"` 自动解除 `max-w-4xl` 截断并保持竖线两侧对称 `24px` 留白，杜绝主列与竖线间出现断层白带**；
     - `width="full"`（100% 铺满）：用于 SKU 价格矩阵、单据行项目表、图片画廊等宽表/多列内容。
4. **标签克制退后，数据与操作成为主角，保存按需浮现**
   - 标题严格对齐列表页的 `18px`（`text-page-title`），分区标题 `15px`（`text-section-title`），输入值/正文 `13px`（`text-body-compact`），表单字段标签收紧为 `12px`（`text-xs font-medium text-foreground/75`），彻底纠正「标签比输入值还大还黑」的倒置问题。
   - 必填星号 `<RequiredMark />` 紧贴标签文字右侧 `2px`，不得漂离。
   - **编辑页按需保存（Dirty Save Dock）**：创建态在页头保留唯一实心主蓝「创建」按钮；编辑态未发生修改时页头保持极净（只放静默次要动作如「停用」），**一旦修改任意表单字段，页面底部居中平滑上浮 `DetailWorkspaceSaveDock`（`● 已修改 N 项 · 取消修改 / 保存更新`）**，跨 Tab 始终可见、同时与「供应商供给」等独立实时保存 Tab 的主蓝「添加供给」彻底解耦，并支持 `Cmd/Ctrl + S` 快捷提交。
5. **表单柔底轻描边，表格内联编辑静止无框**
   - 表单网格统一使用自包含组件 `<DetailFormGrid>`（输出 `data-slot="detail-form-grid"`，脱离 `.erp-detail-workspace-page` 置于弹窗/抽屉内同样生效）：内控件使用极浅柔底（`bg-muted/26`）+ 柔和描边，聚焦时切回纯白并亮起飞书蓝框与 `3px` 光环；不可修改字段在编辑态统一使用 [`<ReadOnlyField>`](../erp-client/components/form/read-only-field.tsx)（`@/components/form`，带锁图标轻量只读表面）替代沉重的禁用输入框/下拉框。
   - 内嵌可编辑表格统一使用自包含组件 `<DetailInlineTable>`（输出 `data-slot="detail-inline-table"`，脱离 `.erp-detail-workspace-page` 同样生效）：采用纯白表头、无左右外框；单元格内输入框静止态 **无硬边框柔底（`border-transparent bg-muted/52`）**，悬停微深、聚焦亮蓝框纯白底，与飞书多维表格编辑手感一致。

---

## 2. 页面架构与垂直解剖

```text
┌────────────────────────────────────────────────────────────────────────────────────────────┐
│ [Sticky Chrome] 纯白通栏吸顶头部 (bg-card, border-b border-border/60, px=--scaffold-px)     │
│  ┌────┐ 商品主标题 18px Semibold  [✓ 当前启用]  (● 有未保存修改)    [停用]  [取消修改] [保存更新]│
│  │44px│ 商品编号：SPU... · 品牌 · 分类 · 类型 · 单位：台           (ghost)   (ghost)  (主蓝 32px)│
│  └────┘                                                                                    │
│  商品资料      供应商供给      图片与详情      变更记录与引用                                │
│  ━━━━━━━━ (2px 蓝条压盖通栏底线，左沿对齐 --scaffold-px)                                    │
├────────────────────────────────────────────────────────────────────────────────────────────┤
│ [Alerts] 条件告警 / 资料同步差异横幅 (empty:hidden，无告警零占位)                           │
├────────────────────────────────────────────────────────────────────────────────────────────┤
│ [Section 1 · width="form" (max-w-4xl)] 基础信息 (无卡片边框，12px 沉稳标签 + 柔底输入)      │
│  基础信息                                                                                  │
│  商品名称* (8 col)                                          商品类型 (4 col · 只读轻表面)   │
│  基础单位* (4 col)              分类* (4 col)               品牌* (4 col)                  │
│  商品描述 (12 col · 舒适宽度)                                                              │
├────────────────────────────────────────────────────────────────────────────────────────────┤
│ [Section 2 · width="full"] 规格与 SKU (1px 细线自然分隔，全宽无套娃外框)                     │
│  规格与 SKU  [1 个 SKU]  默认规格                              [修改规格 ∨]  [查看商品库存]  │
│  SKU 名称与规格*(31%)   SKU 编码/条码(16%)   代发价   集采价   起订量   市场价    状态(13%)  │
│  ────────────────────────────────────────────────────────────────────────────────────────  │
│  [48px图] [柔底无框名称输入]  SKU...-01       [¥ 277]  [¥ 246]  [  30]  [¥ 499]  [✓ 已上架] │
│           默认规格           [柔底条码输入]                                      [●━]已启用 │
└────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. 字号、色彩与控件规范

所有值均消费 `erp-client/app/globals.css` 的语义令牌，与列表页共用[排版与字号设计系统](./typography-design-system.md)。`Label` 原语统一提供 12px / 18px，详情表单网格沿用同一标签样式。

| 视觉角色 | Token / 类 | 字号 / 行高 | 规范要求 |
| --- | --- | --- | --- |
| 对象主标题 | `text-page-title` | `18px / 26px` | `font-semibold tracking-tight text-foreground`（禁止使用 `text-2xl`） |
| 分区标题 | `text-section-title` | `15px / 22px` | `font-semibold tracking-tight text-foreground`，不再在标题下画冗余 `border-b` |
| 表单输入值 / 按钮 / Tab | `text-body-compact` | `13px / 20px` | 核心业务数据字号；金额/数量/编号一律加 `.num` |
| 字段标签 / 表头 / 元信息 / 徽章 | `text-xs` | `12px / 18px` | 表单标签 `font-medium text-foreground/75`；元信息 `text-muted-foreground` 配 `·` 分隔 |
| 未保存修改微胶囊 | `text-tiny` | `11px / 16px` | `DetailWorkspaceDirtyBadge`（琥珀柔底 `bg-warning-soft` + `size-1.5 bg-warning` 圆点） |

### 控件分级矩阵

| 位置 | 视觉层级 | 标准实现 |
| --- | --- | --- |
| 页头唯一主动作（保存更新 / 创建 / 提交） | 实心飞书蓝 | `DetailWorkspaceActionButton tier="primary"` 或 `LoadingButton size="sm" variant="default" className="px-3.5"`（容器统一定高 `h-control` 32px） |
| 页头次要动作（取消修改 / 导出 / 打印） | 无框轻按钮 | `DetailWorkspaceActionButton tier="quiet"` 或 `Button size="sm" variant="ghost" className={detailWorkspaceStyles.quietButton}` |
| 页头破坏性动作（停用 / 作废） | 静默危险轻按钮 | `DetailWorkspaceActionButton tier="quiet-danger"` 或 `Button size="sm" variant="ghost" className={detailWorkspaceStyles.quietDangerButton}`（静止灰字，悬停显柔和红底红字） |
| 分区工具按钮（修改规格 / 批量改价 / 查看库存） | 无框浅灰柔底 | `Button size="sm" variant="ghost" className={detailWorkspaceStyles.subtleButton}`（展开态自动切冰蓝，与列表「更多筛选」同源） |
| 状态表达 | 带图标语义徽章 | 统一使用 `StatusBadge` / `BusinessStatusBadge`，禁止裸 `<Badge>` 无图标表达状态 |

---

## 4. 复用组件与实施蓝图

全部详情工作面原语位于 [`@/components/business/detail-workspace`](../erp-client/components/business/detail-workspace.tsx)（已由 `@/components/business` 统一再导出），表单只读字段位于 [`@/components/form/read-only-field`](../erp-client/components/form/read-only-field.tsx)（已由 `@/components/form` 统一再导出并注册至 `useAppForm`）：

| 组件 / 样式 | 职责 |
| --- | --- |
| `DetailWorkspacePage` | 纯白通栏页壳（`PageScaffold` + `.erp-detail-workspace-page`） |
| `DetailWorkspaceChrome` | 一体化吸顶头部（返回行 + 媒体 + 18px 标题 + 状态 + 元信息 + 动作 + 底部通栏 Tabs + 滚动按需显影；`summary` 槽已 `@deprecated`，通栏摘要请用 `DetailWorkspaceBody.summary`） |
| `DetailWorkspaceActionButton` | 统一动作分级按钮（`tier="primary" \| "quiet" \| "quiet-danger"`，默认 `size="sm"`，基于 `LoadingButton` 封装） |
| `DetailWorkspaceMeta` | 12px 元信息行，自动插入 `·` 分隔符并过滤空项 |
| `DetailWorkspaceDirtyBadge` | 琥珀色 `● 有未保存修改` 微胶囊 |
| `DetailWorkspaceTabs` | 贴底通栏 `line` Tabs，对齐 `--scaffold-px`，`<2` 项自动隐藏 |
| `DetailWorkspaceSubTabs` | 柔底二级分段胶囊切换条（用于合并主分区内的多子视图，区别于一级线型 Tab 与业务操作按钮） |
| `DetailWorkspaceBody` | 承载非吸顶通栏 `summary`、告警槽、纵向细线分隔分区，支持 `sidebar` + `sidebarLabel` 宽屏单竖线双栏 |
| `DetailWorkspaceMetrics` / `DetailWorkspaceMetricItem` | 基于 `@container/detail-metrics` 的无框纯白 KPI 指标条（`variant="bar"` 通栏金额摘要带 / `variant="inline"` 分区汇总） |
| `DetailWorkspaceFacts` / `DetailWorkspaceFact` | 基于 `@container/detail-facts` 的纯白通栏只读信息栅格（12px 柔灰标签 + 13px 正文值，支持 `.num`、跨列与 `—` 兜底） |
| `DetailWorkspaceSection` | 无边框纯白分区，支持 `width="form"`（单栏 `max-w-4xl`）与 `width="full"` |
| `DetailFormGrid` | 自包含柔底表单网格（`layout="grid" \| "stack"`），内聚柔底轻描边与只读输入样式，脱离详情页壳置于弹窗/抽屉同样生效 |
| `DetailInlineTable` | 自包含无框内联编辑表容器（`frame="inline" \| "scroll" \| "none"`、`insetCells`），内聚纯白表头与静止无框柔底输入样式 |
| `DetailHint` | 业务口径按需查看 Popover 按钮（支持鼠标、键盘与触屏，已由 `detail-presentation.tsx` 并入 `detail-workspace.tsx` 统一导出） |
| `ReadOnlyField`（`@/components/form`） | 带锁只读轻表面字段，支持 `form.AppField` 内自动读值（`field.ReadOnlyField`）或独立使用 |
| `DetailWorkspaceSaveDock` | 脏状态底部居中悬浮保存胶囊（与列表页 `list-selection-dock` 同源，退场冻结计数防闪 `0`） |
| `DetailWorkspaceSkeleton` | 纯白骨架屏，防止加载期间闪出浅灰底 |
| `detailWorkspaceStyles` | 共享动作按钮、表单网格与内联表格样式表 |

### 最小页面模板

```tsx
<DetailWorkspacePage>
    <form id="entity-detail-form" className="flex min-w-0 flex-1 flex-col" onSubmit={onSubmit}>
        <DetailWorkspaceChrome
            title={title}
            back={{ id: "entity-detail-back", label: "返回列表", onClick: onBack }}
            primaryStatus={{ label: "当前启用", tone: "success" }}
            titleExtra={dirty ? <DetailWorkspaceDirtyBadge /> : null}
            meta={
                <DetailWorkspaceMeta
                    items={[
                        <span key="no" className="num">编号：{stableNo}</span>,
                        categoryLabel,
                        ownerName,
                    ]}
                />
            }
            secondaryActions={
                <DetailWorkspaceActionButton tier="quiet-danger" onClick={onDisable}>
                    停用
                </DetailWorkspaceActionButton>
            }
            primaryAction={
                <DetailWorkspaceActionButton tier="primary" type="submit" loading={pending}>
                    保存更新
                </DetailWorkspaceActionButton>
            }
            tabs={
                <DetailWorkspaceTabs
                    value={activeSection}
                    onValueChange={setActiveSection}
                    items={SECTIONS}
                    tabIdPrefix="entity-detail-tab"
                />
            }
        />

        <DetailWorkspaceBody alerts={alerts}>
            <DetailWorkspaceSection title="基础信息" width="form">
                <DetailFormGrid>
                    <div className="sm:col-span-8">
                        {/* 可编辑表单字段：自动获得 12px 标签 + 柔底轻描边控件 */}
                    </div>
                    <ReadOnlyField
                        id="entity-detail-type"
                        className="sm:col-span-4"
                        label="对象类型"
                        value={entityTypeLabel}
                        title="创建后不可更改对象类型"
                    />
                </DetailFormGrid>
            </DetailWorkspaceSection>

            <DetailWorkspaceSection title="明细表" width="full" actions={sectionActions}>
                <DetailInlineTable frame="scroll">
                    {/* 无外框纯白表头内联编辑表：金额/数量列 data-align="end" + .num */}
                </DetailInlineTable>
            </DetailWorkspaceSection>
        </DetailWorkspaceBody>
    </form>
</DetailWorkspacePage>
```

### 核心自包含原语用法模板

#### 1. `DetailFormGrid`（`@/components/business`）

替代手写 `<div data-slot="detail-form-grid" ...>`，内置 `detailWorkspaceStyles.formGridSurface`，在详情页、抽屉与弹窗内均可独立呈现柔底轻描边与只读轻表面：

- `layout?: "grid" | "stack"`（默认 `"grid"`）：
  - `"grid"`：自动附加标准 12 列栅格 `grid gap-x-5 gap-y-4 sm:grid-cols-12`；
  - `"stack"`：不强制 12 列网格，仅注入柔底轻控件与只读表面样式，适合纵向堆叠分组或自定义列布局。

```tsx
import { DetailFormGrid } from "@/components/business"

// 12 列详情表单（默认 layout="grid"）
<DetailFormGrid>
    <div className="sm:col-span-8">
        <form.AppField name="name">
            {(field) => <field.TextField label="商品名称" required />}
        </form.AppField>
    </div>
    <div className="sm:col-span-4">
        <form.AppField name="unit">
            {(field) => <field.SelectField label="基础单位" options={unitOptions} required />}
        </form.AppField>
    </div>
</DetailFormGrid>

// 弹窗/抽屉内纵向堆叠（脱离 .erp-detail-workspace-page 依然生效）
<DetailFormGrid layout="stack" className="space-y-4">
    {fields}
</DetailFormGrid>
```

#### 2. `DetailInlineTable`（`@/components/business`）

替代手写 `<div data-slot="detail-inline-table" ...>`，内置 `detailWorkspaceStyles.inlineTableSurface`（纯白表头、无左右外框、静止态无框柔底输入与聚焦蓝框光环）：

- `frame?: "inline" | "scroll" | "none"`（默认 `"inline"`）：
  - `"inline"`：套用 `detailWorkspaceStyles.inlineTableWrapper`（`w-full max-w-full border-y border-border/70`）；
  - `"scroll"`：套用 `detailWorkspaceStyles.dataTableWrapper`（`w-full max-w-full overflow-x-auto border-y border-border/70`），推荐宽明细表使用；
  - `"none"`：不加外层上下细线，适配已有边界的嵌套子表。
- `insetCells?: boolean`（默认 `false`）：默认首尾列外沿贴齐容器（首列 `ps-0` / 末列 `pe-0` 去除外沿内边距）；传 `true` 时输出 `data-inset-cells` 保留首尾列左右内边距。

```tsx
import { DetailInlineTable } from "@/components/business"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"

<DetailInlineTable frame="scroll">
    <Table>
        <TableHeader>
            <TableRow>
                <TableHead>SKU 名称</TableHead>
                <TableHead data-align="end">含税单价</TableHead>
            </TableRow>
        </TableHeader>
        <TableBody>
            <TableRow>
                <TableCell><Input size="sm" value={row.name} onChange={...} /></TableCell>
                <TableCell data-align="end"><MoneyInput value={row.price} onChange={...} /></TableCell>
            </TableRow>
        </TableBody>
    </Table>
</DetailInlineTable>
```

#### 3. `ReadOnlyField`（`@/components/form`）

落地法则 5「带锁只读轻表面」，替代编辑态沉重的禁用输入/下拉框；既可在 `form.AppField` 内作为 `field.ReadOnlyField` 自动读取 `field.state.value`，也可作为独立字段组件使用：

- 核心 Props（`ReadOnlyFieldProps`）：
  - `label: ReactNode`、`hideLabel?: boolean`（`sr-only`）；
  - `value?: ReactNode`（在 `form.AppField` 内省略时自动读取当前字段值）、`emptyText?: string`（默认 `"—"`）；
  - `locked?: boolean`（默认 `true`，右侧展示锁图标）、`lockedLabel?: string`（默认 `"不可修改"`，传 `""` 仅显示锁图标）、`title?: string`（悬停解释不可修改原因）；
  - `numeric?: boolean`（金额/编号/数量启用 `.num`）、`size?: "sm" | "default" | "lg"`（默认 `"default"` 对应 `h-control` 32px）、`multiline?: boolean`（多行自动换行）；
  - `description?: ReactNode`、`id?: string`、`className?: string`、`surfaceClassName?: string`。

```tsx
import { ReadOnlyField } from "@/components/form"

// 方式 A：独立使用（常用于编辑态不可修改的主数据属性）
<ReadOnlyField
    id="product-detail-code"
    className="sm:col-span-4"
    label="商品编号"
    value={product.productCode}
    numeric
    title="系统生成的稳定编号，创建后不可修改"
/>

// 方式 B：在 TanStack Form 的 form.AppField 内自动读值
<form.AppField name="productTypeLabel">
    {(field) => (
        <field.ReadOnlyField
            label="商品类型"
            lockedLabel=""
            title="创建后不可变更商品类型"
        />
    )}
</form.AppField>
```

---

## 5. 全站详情页迁移路线与验收清单

### 分批迁移参考
1. **标杆页（已完成）**：主数据编辑型——商品详情页（`product-detail-page.tsx`）；单据双栏型——销售单详情页（`sales-order-detail-page.tsx`）、采购单详情页（`purchase-order-detail-page.tsx`）。
2. **编辑型主数据详情页**：供应商详情（`supplier-detail-page.tsx`）、客户详情（`customer-detail-page.tsx`）、仓库/分类对象中心（`object-center-view.tsx`）——直接迁入 `DetailWorkspacePage` + `DetailWorkspaceChrome` + `DetailWorkspaceBody`。
3. **其余单据型详情页**：合同详情（`contract-detail-page.tsx`，顺带消除外层 `surfacePanelClassName` 与 `ObjectSectionTabs` 双层边框）、供应商供给详情（`supplier-offering-detail-page.tsx`）——对齐销售单/采购单详情，采用 `DetailWorkspaceBody summary={...} sidebar={...}` 通栏摘要 + 单竖线双栏布局。

### 视觉与交互验收清单（QA Checklist）
- [ ] **零套娃与单基准对齐**：整页纯白通栏，无外层圆角白卡套内层灰底再套分区白卡；头部媒体/标题、Tabs、分区标题、表单首列、内嵌表首列严丝合缝对齐 `--scaffold-px`。
- [ ] **一体化吸顶头部**：滚动长页面时标题、状态与主保存按钮保持吸顶；静止态无阴影，滚动后浮现微影。
- [ ] **字号与星号精准**：标题 `18px`、分区标题 `15px`、输入值/正文 `13px`、表单标签 `12px`、元信息 `12px` 配 `·` 分隔；必填星号 `*` 紧贴标签右侧 `2px`。
- [ ] **动作克制与状态统一**：页头仅 1 个实心飞书蓝主按钮，次要与停用按钮为无框轻按钮；状态统一用带图标的 `StatusBadge`；未保存修改用琥珀色微胶囊提示。
- [ ] **表单与表格比例**：表单分区使用 `<DetailFormGrid>`（受 `max-w-4xl` 约束无超长空白条）；不可修改字段在编辑态使用 `<ReadOnlyField>` 轻只读表面；内嵌可编辑表格使用 `<DetailInlineTable>`（无框柔底输入、金额/数量右对齐 `.num`、缩略图悬浮显露操作按钮），禁止再手写原始 `data-slot="detail-form-grid"` / `data-slot="detail-inline-table"` 容器。
- [ ] **自动化契约完好**：所有原有 DOM `id`、表单 `aria-label`、分节 `<h2>` 标题文案及权限显隐逻辑零破坏。
