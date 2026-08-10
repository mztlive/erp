# 筛选区域统一设计方案（Filter Area Design）

> **状态**：已评审通过；**布局归属与密度分层**为本轮补充（2026-08）  
> **实施进度**：  
> - **D1–D25**（交互/URL/隐形状态）：已全部落地（2026-08）  
> - **L1–L6**（布局归属与密度）：主路径已完成；**全站回归 + 遗漏扫尾**已落地（客户中心空态、chip/secondary、W07/W21 队列 ListToolbar、商城同步 sticky 合一、类目树 surface 等）  
> - **§3.6 显式提交折叠筛选面板**：新增模式（2026-08），与 §3.1 三层模型二选一，样板见 `/master-data/products`  
> **适用范围**：`erp-client` 全部含搜索/筛选/指标条的列表页、队列工作区、分析页明细  
> **关联**：`erp-ui-design.md`（页面布局 §2.5 / §4.3 / §4.4 / §4.9）、`ui-glossary.md`（文案）、`erp-client/AGENTS.md` §5（URL 契约）

## 1. 设计原则

| # | 原则 | 落地要求 |
| --- | --- | --- |
| P1 | URL 是唯一事实源 | 所有筛选、分页、排序写 URL；默认值省略（URL 最小化）；刷新/分享/后退一致 |
| P2 | 筛选变更恒 `replace` | 筛选/分页/搜索变更用 `router.replace`，不膨胀历史；打开/关闭详情等导航用 `push` |
| P3 | 搜索统一为「防抖即时 + Enter + `/`」 | 300ms 防抖自动生效；Enter 兜底；列表页提供 `/` 聚焦快捷键；变更后默认 `page` 回 1 |
| P4 | 清除筛选=清全部筛选参数 | 清：搜索词、全部筛选参数、分页回 1；**保留**：视图/scope、排序、期间/口径（分析页维度参数）、导航上下文参数（from/returnTo/sessionId 等） |
| P5 | 每个被查询消费的参数必须有控件或可移除徽标 | 禁止「隐形状态」；深链来源参数显性化为可单独移除的 chip |
| P6 | 分页统一写 URL | `page`（游标页 `cursor`）入 URL，且与筛选联动回第 1 页 |
| P7 | 指标条语义唯一 | 可点击即真筛选（`MetricFilterItem`，含「全部」可回退）；纯展示用只读卡且不加 hover/手型暗示；禁止「点了只滚动不筛数」的伪筛选 |
| P8 | 空态统一 | 使用 `BusinessEmptyState`，区分 `no-data`（无数据）/ `filter`（筛选无结果，带清除按钮）/ `no-scope`（无权限范围） |
| P9 | 控件形态统一 | 视图选择用 `Tabs`（独立）或 `ToggleGroup`/分段轨道（工具条内）；来源锁定统一 `FilterChip`；搜索统一 `InputGroup` 带图标 |
| **P10** | **筛选贴在被过滤的工作面上** | 筛选项不得裸飘在画布上；M2 进 `BusinessTableFrame.toolbar`，M3 进 sticky 处理面（surface 卡）。禁止「有的页在卡里、有的页在卡外」 |
| **P11** | **筛选分三层，禁止一锅炖一行** | 第 0 层视图/范围；第 1 层主工具条（搜索 + ≤3 主筛 + 计数/清除）；第 2 层次要条件与来源 chip。禁止把全部条件硬塞进同一 flex 行 |
| **P12** | **密度按枚举规模选型** | 选项 ≤4 可用分段/`ToggleGroup`；≥5 **必须** `OptionCombobox` 或收入高级筛选。禁止 6～7 项状态 Toggle 横排撑满工具条 |

---

## 2. 布局归属（按页面模式）

> 统一的是「**surface 卡 + 工具条槽位**」的账本语言，不是组件名必须相同。  
> 队列页**不要**硬套 `BusinessTableFrame`；列表页**不要**在 frame 外再挂一套平行工具条。

### 2.1 归属总表

| 页面模式 | 筛选所在表面 | 禁止 |
| --- | --- | --- |
| **M2 列表**（采购单、销售单、合同、台账…） | **一张** `BusinessTableFrame` 的 `toolbar`（及可选 `selectionBar`） | `ListToolbar` 画布裸放；frame 外再叠筛选卡 |
| **M3 队列**（交付与代发、待办队列、二次确认、票款复核…） | **一张 sticky 处理面**（`surfacePanelClassName` 卡）内：第 0 层范围/类型 + 第 1 层 `ListToolbar` + 第 2 层 chip | 类型 `ToggleGroup` 与 `ListToolbar` 分两处裸飘；筛选塞进不存在的表格 frame |
| **M5/M6 分析明细** | 分析维度（期间/口径）可贴画布或轻条；**明细表筛选项**仍进明细 `BusinessTableFrame.toolbar` | 维度条与明细筛视觉同级混排、无法区分「改口径」与「筛明细」 |
| **多 Tab 列表**（商城同步等） | 每个含表 Tab 各自 frame 内 toolbar；跨 Tab 残留参数在切 Tab 时清理 | 空态时整段拆掉筛选区 |

### 2.2 M2 列表页模板（自上而下）

样板对照：`/sales/orders`（销售单）、目标态 `/procurement/orders`（采购单）。

```
PageHeader（标题/动作/DataFreshness）          ← 贴画布，无底框
MetricStrip（可选：真筛选指标条，含「全部」） ← 轻表面，不另开厚卡
BusinessTableFrame                            ← 唯一主表面
  ├ title
  ├ description = 筛选摘要（有激活筛选时；否则默认说明）
  ├ ListToolbar                               ← 第 1 层（卡内）
  │   ├ search: InputGroup（防抖 + Enter + "/"）
  │   ├ filters: 主筛 ≤3 个（见 §3）
  │   └ actions: [共 N 条] [清除筛选（有激活时）]
  ├ 第 2 层（toolbar 内 secondary 或紧随 toolbar 的行）
  │   ├ 高级筛选入口（Popover/Sheet，可选）
  │   └ FilterChip…（深链锁定，可单独移除）
  ├ SelectionScopeBar（有多选时）
  └ DataTable / BusinessEmptyState（no-data | filter | no-scope）
```

**硬性要求**：

- 搜索、主筛、清除、计数**全部**经 `BusinessTableFrame` 的 `toolbar` 槽进入；禁止 frame 外平行 `ListToolbar`。
- `description` 有筛选时写人读摘要（与控件一致），无筛选时写默认操作说明。
- 空态嵌在 frame 表体内，**不**因空结果卸载 toolbar（筛选区常驻，便于改条件）。

### 2.3 M3 队列工作区模板

样板对照：目标态「交付与代发 / 收货与发货」（W09）、W02 待办队列、W07 二次确认。

```
PageHeader
┌─ sticky 处理面（surfacePanelClassName，一张卡）─────────────┐
│  第 0 层：范围/类型分段（scope、lane/type，可带计数）         │
│  第 1 层：ListToolbar                                       │
│     search | 主筛≤3（仓库/到期/门禁等） | actions             │
│     actions: [待处理 N] [自动下一项 Switch] [清除筛选]        │
│  第 2 层：FilterChip（salesOrderId / purchaseOrderId 等）   │
│  SequentialProcessBar（若本页承担连续处理位置指示）         │
└──────────────────────────────────────────────────────────────┘
主作业区（任务列表 + 当前项详情：1 张主卡或双栏 surface）
```

**硬性要求**：

- 第 0～2 层与连续处理偏好**同属一张** sticky 处理面，禁止类型分段在卡外、工具条又在别处。
- 队列**不是**表格列表：不套 `BusinessTableFrame` 装任务流；统一语言靠 surface + `ListToolbar` 槽位。
- `focus` / `currentWorkItemId`：按工作面约定（可 URL 或 sessionStorage）；**筛选参数**必须 URL 可分享。
- 完成/跳过/转交后自动前进；筛选变更清空或重算当前项焦点，避免「滤没了还停在旧项」。

### 2.4 分析页 / 多视图页补充

| 场景 | 规则 |
| --- | --- |
| 期间、口径、覆盖范围等分析维度 | 可在明细 frame **之上**单独成条；视觉权重低于主指标，且**不**与明细主筛挤在同一 `ListToolbar.filters` |
| 明细关键词、对象锁定、状态 | 进明细 `BusinessTableFrame.toolbar` |
| 清除筛选 | 清明细筛与搜索；**保留**期间/口径（P4）；若需「重置分析维度」须另入口，文案不得叫「清除筛选」 |
| 视图 Tabs | 贴画布或卡顶；切视图时清理对当前视图不可见的筛选参数（防 URL 幽灵状态） |

---

## 3. 密度与分层（禁止全挤一行）

### 3.1 三层模型

| 层 | 名称 | 放什么 | 控件形态 | 布局 |
| --- | --- | --- | --- | --- |
| **0** | 视图 / 范围 | 改变「看哪一类工作」：我的/全组、lane、业务大类、列表视图名 | `Tabs` 或分段轨道 / `ToggleGroup`（项数 ≤4） | Header 旁、Metric 旁，或队列 sticky 卡顶；**不进** `filters` 主槽与主筛抢位 |
| **1** | 主工具条 | 搜索 + 日常最高频 1～3 个维度 + 计数 + 清除 | 搜索 `InputGroup`；主筛见 §3.2；清除在 `actions` | `ListToolbar` 主行；宽屏一行，窄屏允许 search 与 filters 折行 |
| **2** | 次要 / 锁定 | 低频组合条件、高级筛选、深链对象锁定 | 「高级筛选」按钮 + Popover/Sheet；`FilterChip` | **独立第二行**（`basis-full` 或 `ListToolbar` secondary 槽），不与主筛抢同一视觉行 |

### 3.2 主筛配额与控件选型

| 规则 | 要求 |
| --- | --- |
| 主工具条常驻筛选项（不含搜索、清除、计数、Switch） | **≤ 3 个** |
| 枚举选项数 **≤ 4**（含「全部」） | 可用分段轨道或 `ToggleGroup` |
| 枚举选项数 **≥ 5** | **必须** `OptionCombobox`，或收入「高级筛选」；**禁止** 6～7 段 Toggle 横排 |
| 多维低频条件（来源 + 状态 + 性质 + …） | 1～2 个最高频留主筛，其余进高级筛选（参考销售单 Popover 模式） |
| 深链 `customerId` / `salesOrderId` / `purchaseOrderId` / `skuId` 等 | **只**以 `FilterChip` 出现在第 2 层，不进主筛下拉冒充「可选手动选」 |
| 与指标条重叠的维度 | 指标可承担粗筛；工具条勿再放同义 7 项 Toggle（二选一或指标重置重叠维，见 §4.3） |

### 3.3 行布局约定

```
主行（第 1 层）:
  [🔍 搜索  min 12rem / max 20–24rem]  [主筛1] [主筛2] [主筛3]   |  [共 N 条] [清除筛选]

次行（第 2 层，有内容才渲染）:
  [高级筛选（已启用徽标）]  [Chip: 销售单 SO-…]  [Chip: 客户 …]
```

实现：优先使用 `ListToolbar` 的 **`secondary` 槽**（已落地，`data-slot="list-toolbar-secondary"`）。第 2 层有内容才传 `secondary`；无则省略。

```tsx
<ListToolbar
  search={/* InputGroup */}
  filters={/* ≤3 主筛 */}
  secondary={/* 高级筛选入口 + FilterChip；无则 undefined */}
  actions={/* 共 N 条 + 清除筛选 */}
/>
```

### 3.4 窄屏（小于 `lg` 断点）

| 优先级 | 行为 |
| --- | --- |
| 必显 | 搜索；清除（有激活时）；计数可缩为短文案 |
| 可折叠 | 第 0 层分段允许换行；主筛超过 2 个时可收入「筛选」面板，但 **URL 状态与清除语义不变** |
| 禁止 | 为窄屏再写一套不写 URL 的本地-only 筛选 |

### 3.5 反模式（明确否决）

1. 全部筛选条件（含 7 项状态）与搜索挤在同一行，宽屏横向溢出、窄屏无序折行。  
2. M2 列表：`ListToolbar` 在 `BusinessTableFrame` **外**，表格另起一张卡。  
3. M3 队列：类型 `ToggleGroup` 画布裸放，下方再裸放 `ListToolbar`，两者都不进 surface。  
4. 空结果时卸载整个筛选区，用户无法改条件。  
5. 同页「指标真筛选 + 工具条同义 Toggle + 高级筛选第三套」三重控件不同步。  
6. 为统一而把队列硬塞进 `BusinessTableFrame`，或把列表筛选拆成页头下第三张便签卡。

### 3.6 显式提交折叠筛选面板（Explicit-Submit Collapsible Filter Panel，2026-08 新增）

> 与 §3.1 三层模型**二选一**，不是叠加。适用场景：结构化筛选字段数量较多（5 个以上）、都是常用维度、不想再区分「主筛 / 高级筛选」优先级的列表页。样板：`/master-data/products`（商品列表）。

**触发场景**：字段太多时，硬塞进「主筛 ≤3」会漏掉高频维度；塞进 §4.5 的 Popover 高级筛选又要多点一次才能看到已选值，字段之间来回切换成本高；但字段全部铺开常驻又会把表格挤到折叠线以下。商品列表（类型/分类/品牌/启停/版本/上架/供给覆盖/供应商/销售价区间，9 个维度）属于这种情况。

**默认态（收起）**：

- 只有一个关键词搜索框（`InputGroup`，纯输入框，**不嵌按钮**）+ 紧挨着的一个独立「搜索」按钮 + 一个「高级筛选」折叠按钮（`FilterIcon` + 文案 + 展开态 `ChevronDownIcon` 旋转 + 有已生效结构化筛选时的「已启用」`Badge`）。三者都在 `ListToolbar` 主行，视觉上「一框一钮一开关」，不做多余修饰。
- 面板默认收起；但如果 URL 上已带有任意结构化筛选参数（深链进入、刷新页面），默认**自动展开**，不能让用户带着生效筛选却看不到面板在哪。

**展开态**：

- 点击「高级筛选」在 `ListToolbar` 的 `secondary` 槽展开一个带边框的面板（`rounded-lg border`），与搜索框同属**一个 `<form>`**（不是 Popover，不是独立弹层）。
- 面板内：每个固定枚举字段（`FixedOptionRadioFilter`）独占一行；其余字段（`OptionCombobox`、区间输入）按可用宽度分栏横排（如 4 列，窄屏降级为 2 列/1 列）。
- 展开后，主行旁边那个独立「搜索」按钮**消失**，同一个「搜索」按钮改为出现在面板**最后一行、右对齐**——跟着用户视线走：编辑筛选在面板里，提交按钮也在面板里，不用为了点一下提交再把视线移回最上面。收起面板时按钮回到搜索框旁边。两处按钮本质是同一个 `type="submit"`，只是渲染位置随展开态切换，行为完全一致。

**提交与状态**：

- 全部控件（关键词 + 结构化字段）只更新本地草稿 state，`onChange` 不写 URL；在任意输入框按 Enter，或点击「搜索」按钮（不论它此刻在哪个位置），才把草稿一次性 `patchUrl`、`page` 回 1。
- 「清除筛选」同时重置草稿 state、收起面板、清空 URL 参数，三者不能只做一部分。
- 浏览器前进/后退、清除筛选等外部 URL 变化时，用一个 effect 把草稿和面板展开态一起重新同步回 URL 派生值（参考 `master-data-page.tsx` 里 `productKindDraft` 等草稿的 resync effect，以及 `hasStructuredProductFilters` → `productFilterPanelOpen` 的联动）。

**与既有原则的关系（本模式内的显式例外，不是全站默认改规则）**：

- **不套用 P3**「防抖即时 + Enter」——本模式所有字段都是「编辑草稿，点搜索才生效」，即时防抖反而会在用户还没选完时就发起多次请求。
- **不套用 P11**「主筛 ≤3 + 二层 Popover 收纳高级条件」——本模式不做优先级裁剪，字段全部收在一个可展开面板里，靠「先编辑、后一次性提交」而不是「限制数量」或「弹层」来控制认知负担。
- **P12**「≥5 选项必须 `OptionCombobox`」仍然适用——枚举选项多时依旧用 `OptionCombobox`，不要拿 `FixedOptionRadioFilter` 摆一堆选项。

**参考实现**：`erp-client/features/master-data/master-data-page.tsx`，`resource === "products"` 分支的 `toolbar`；统一提交入口是 `applyProductFilters`，展开态状态是 `productFilterPanelOpen`。

**选型建议**：常规列表页（筛选字段 ≤4、以浏览为主、偶尔筛一下）优先 §3.1 三层模型 + 即时生效；筛选字段本身就多、且用户习惯「调好几个条件再一起查」的重查询主数据列表，可以用本模式。

---

## 4. 组件与实现规范

### 4.1 搜索

- 组件：`InputGroup` + `SearchIcon` + `InputGroupInput`
- 交互：`useEffect` 300ms 防抖写 URL（`replace`），Enter 立即提交；列表/队列提供 `/` 聚焦（参考 `sales-orders-list-page.tsx`）
- 草稿：本地 state `searchDraft`，URL 回填时保留焦点保护（输入中不被 URL 覆盖）
- URL 参数统一 `q`（旧 `search` 通过 codec 别名兼容）
- 槽位：必须走 `ListToolbar` 的 `search`，禁止塞进 `filters` 槽

### 4.2 清除筛选

- 位置：`ListToolbar` **actions** 槽（有激活筛选时条件渲染）+ 空态 `BusinessEmptyState filter` 内
- 范围：清搜索词 + 全部筛选参数 + 分页回 1；保留 view/scope/sort/期间/导航上下文
- 文案：统一「清除筛选」
- 激活判定：须包含主筛、搜索、第 2 层 chip、高级筛选条件；**是否含 scope** 按工作面约定写清（W01/队列若 scope 算「看谁的」而非临时筛，清除时可保留 scope，但须在页面说明与 P4 表一致）

### 4.3 指标条

- 真筛选：`MetricStrip` + `MetricFilterItem`，第一项「全部」允许回退；点击写指标参数并回 page 1
- 纯展示：`MetricStrip` + `MetricItem`（无 active/onClick）
- 与其它筛选：指标与普通筛选 AND 共存；指标点击**默认不**清其它筛选；若与工具条维度语义重叠（如 summary×status），指标点击时**一并重置重叠维**，避免矛盾空结果
- 与密度：指标已承担的粗维度，工具条不再用长 Toggle 重复同一枚举

### 4.4 来源锁定 chip（深链参数显性化）

- 语义：`customerId` / `salesOrderId` / `purchaseOrderId` / `skuId` / `orderNo` 等来源页带入的参数
- 组件：共享 `FilterChip`（`components/business/filter-chip.tsx`）
- 位置：**第 2 层**（secondary 行），不进主筛 Combobox
- 行为：× 只移除该参数；「清除筛选」一并清除
- 展示：优先业务单号/名称，避免只显示内部 ID
- 禁止各页自造平行 chip

### 4.5 高级筛选

- 入口：第 1 层末或第 2 层按钮「高级筛选」；有生效条件时 `Badge`「已启用」
- 容器：`Popover`（条件少）或 `Sheet size="preview"`（条件多）；应用后写 URL `replace`，回 page 1
- 与主筛关系：主筛改动不自动清空高级条件（除非文档声明互斥）；清除筛选两者都清
- 样板：`sales-orders-list-page.tsx` 的高级筛选 Popover

### 4.6 分页

- `page` / `pageSize`（或 `cursor`）写 URL；筛选变更自动回 page 1
- 每页默认 20（现状有特殊默认值的页面可保留，如消费订单 8）

### 4.7 状态管理

- 优先 `createUrlStateCodec`（`lib/url-state.ts`）；已有 codec 的页面保持
- 搜索词 URL 参数统一 `q`；多值参数用 array 类型（逗号分隔）
- 导航/上下文参数（from/returnTo/sessionId/previewId/queueContextId 等）与筛选参数分列，清除筛选不清除它们

### 4.8 `ListToolbar` 槽位契约

| 槽 | 用途 | 不要放 |
| --- | --- | --- |
| `search` | 唯一关键词入口 | 状态 Toggle、仓库 Combobox |
| `filters` | 主筛 ≤3；过渡期可用 `basis-full` 次行 | 页级「新建/导出」（应在 `PageHeader` / `actions`） |
| `savedView` | 已保存视图 | 临时筛选 |
| `actions` | 计数、清除筛选、队列「自动下一项」等 | 主筛控件 |
| `secondary` | 第 2 层 chip + 高级筛选入口（有内容才传） | 搜索 / 主筛 |

跨页**禁止**第三套手写 `flex` 工具条平行实现。

---

## 5. 已知缺陷清单

### 5.1 交互 / URL（D1–D25，已落地）

下列项在 2026-08 轮次已按本表修复；新页开发仍须遵守，避免回归。

| # | 页面 | 缺陷 | 修复动作 |
| --- | --- | --- | --- |
| D1 | 采购确认 W07 | `scope/due/sort/orderNo` 被查询消费但无控件 | 补工具条：scope 分段 + due 分段 + 单号搜索 + 清除筛选（参照 W13） |
| D2 | 实际盈亏 | `customerId/salesOrderId` 无控件 | 加来源锁定 chip + 移除 |
| D3 | 客户质量 | `customerId` 隐形；`focusCustomerId` 滞留；`focusMetric` 伪筛选 | customerId 加 chip；focus 参数清理；指标点击只滚动→改为真筛选或只读 |
| D4 | 卡券分析 | `coverage` 被消费无控件 | 加「覆盖口径」控件（或移除参数）；筛选变更重置分页 |
| D5 | 审计 | `org` 无控件；搜索 replace 与高级筛选 push 不一致 | org 加控件或 chip；统一 replace |
| D6 | 往来 | `focusId` 无控件；视图隐藏但 URL 残留；空态清除漏 focusId | focusId 清理；切视图时清除不可见筛选；清除范围补全 |
| D7 | 连接 | 空态清除漏 capability/supplierId；空态隐藏整个筛选区；ListToolbar 在 frame 外 | 补全清除范围；空态保留筛选区；移入 frame |
| D8 | 商品库 | 空态清除只清 q 漏 sourceType；list 模式残留 queue 参数；SKU 锁定不可清除 | 补全清除范围；残留参数清理；SKU chip 可移除 |
| D9 | 商城同步 | 「清除筛选」只清 q，对象 id 跨视图残留 | 清除范围补全对象参数；切视图清理 |
| D10 | 错误中心 | 无清除按钮但空态文案指引「清除筛选」；resolved/auto_retry 视图无 UI 入口 | 补清除按钮；视图入口补全 |
| D11 | 回填 | 无清除入口；搜索在 filters 槽 | 补清除；搜索移 search 槽 |
| D12 | 导入期初 | 列表无清除/详情有清除不一致 | 列表补清除 |
| D13 | 基础资料 | 无 URL 同步；指标与 ToggleGroup 双控件不同步 | 全量 URL 同步；指标与 ToggleGroup 统一为同一状态源 |
| D14 | 合同 | back/forward 不同步（state-first）；isFiltered 不含 customerId；清除分裂 | 改 URL-first；isFiltered 含锁定；清除统一 |
| D15 | 客户中心 | 清除连排序一起清；非空态无清除入口；分页不回读 | 清除保留排序；补常驻清除；分页回读 |
| D16 | 销售单 | 指标无「全部」不可取消；summary×status 矛盾组合 | 补「全部」；指标点击重置重叠维度 |
| D17 | 工作台 W01 | 用 push；scope 不算激活筛选；无搜索 | 改 replace；scope 计入激活；补搜索 |
| D18 | 票款复核 W13 | 清除残留 type=all；completed 空态无清除 | 清除不写默认值；completed 空态补清除 |
| D19 | 采购单 | 指标 `pending_create` URL 值无高亮控件；清除只在空态 | 清除入口补工具栏（有筛选时） |
| D20 | 库存 | 搜索防抖+push 独特；指标=view 组合三重语义 | 改 replace；指标语义简化 |
| D21 | 供应商订单 | 空态无清除；互斥规则过于复杂 | 空态补清除 |
| D22 | 结算 | 清除只在空态；分页双写漂移；无排序 | 补常驻清除；分页单源 |
| D23 | 供应商往来 | 分页/排序本地；清除保留 view 名不副实 | 分页排序入 URL；清除语义对齐 |
| D24 | 执行信息 | ListToolbar 在 frame 外；无筛选也显示筛选空态；无 `/` 快捷键 | 移入 frame；空态区分；补 `/` |

### 5.2 布局归属与密度（L 系列，规范已定 / 实现待推进）

| # | 范围 | 问题 | 目标态 |
| --- | --- | --- | --- |
| **L1** | M3 交付与代发 / 收货与发货（W09） | 类型 `ToggleGroup` 与 `ListToolbar` 裸在画布 | ✅ sticky 处理面：第 0 scope + 类型 + 第 1 ListToolbar + 第 2 chip |
| **L2** | M2 采购单等 | 状态 6～7 项 `ToggleGroup` 横排 | ✅ 状态 `OptionCombobox`；指标保留粗筛 |
| **L3** | 导入期初、历史回填等 | `ListToolbar` 在 frame 外 | ✅ 已迁入 frame；错误态不卸载筛选 |
| **L4** | 多页主筛过载 / 长 Toggle | ≥5 枚举横排 Toggle | ✅ W02 任务族、商品库变化类型、错误中心视图等改 Combobox；回填主筛≤3 + secondary |
| **L5** | 队列页（W02 / W29） | 筛选裸飘或与处理条分裂 | ✅ W02、接口错误中心 sticky 处理面 |
| **L6** | `ListToolbar` 组件 | 无 `secondary` 槽 | ✅ 已增加 `secondary` |

**落地状态**：L1–L6 主路径 + 全站回归 + 遗漏扫尾已完成：

| 轮次 | 页 / 项 | 改动 |
| --- | --- | --- |
| 样板 | W09 / W08 / W02 / 回填 / 导入 | sticky / frame + density |
| 回归 | 供应商订单 / 连接 / 结算 / 消费订单 / 执行信息 / 销售单 | 主筛 ≤3 + `secondary` |
| 回归 | 权限审计 / 商城同步 / 票款复核 | secondary / sticky 分层 |
| 扫尾 | **客户中心** | 空态常驻 frame + toolbar |
| 扫尾 | **商品发布 / 库存** | 清除→actions；chip→secondary |
| 扫尾 | **合同 / 应收 / 供给 / 盈亏** | chip→secondary；应收复核下沉 |
| 扫尾 | **W07 二次确认 / W21 商品库队列** | sticky 内 `ListToolbar` |
| 扫尾 | **商城同步** | Tabs + 搜索 **一张** sticky 卡 |
| 扫尾 | **类目树** | surface + `ListToolbar` |

仍为分析页自有布局、未强制 `ListToolbar` 的：客户质量、卡券分析等 M6（期间/口径贴画布合法）。  
新页开发仍以本表与 §3 密度规则验收。

---

## 6. 验收标准

### 6.1 交互 / URL（D 系列回归）

- [x] 布局改造相关页 `pnpm typecheck` 通过（全仓 lint 按 CI 门禁）
- [x] D1–D25 交互/URL 项已按 §5.1 落地（新页仍须遵守）
- [x] 空态统一 `BusinessEmptyState`；列表空态不卸载筛选区（含客户中心）
- [x] 跨页控件形态一致（Tabs / ToggleGroup / `FilterChip` / `InputGroup` / `ListToolbar.secondary`）
- [ ] 全页 300ms 搜索防抖统一（W02/W09 等队列仍为 Enter/blur，允许队列例外）
- [ ] AGENTS.md §5 全量 param 无隐形状态：持续门禁，非本轮布局范围

### 6.2 布局归属与密度（L 系列）

- [x] **归属**：M2 筛选在 `BusinessTableFrame`；M3 在 sticky 处理面；主列表无画布裸工具条
- [x] **分层**：`secondary` 承载 chip / 高级筛选；主筛 ≤3
- [x] **配额**：无 ≥5 项横排 `ToggleGroup`（层 0 短分段除外）
- [x] **空态**：无结果时筛选区仍在（客户中心已修）
- [x] **扫尾**：W07/W21 ListToolbar、商城同步 sticky 合一、类目树 surface

### 6.3 样板页（改造后优先对拍）

| 模式 | 路由 | 对拍要点 |
| --- | --- | --- |
| M2 | `/sales/orders` | frame 内 toolbar；高级筛选 Popover；主筛不过载 |
| M2 | `/procurement/orders` | 与销售单同构；状态密度符合 §3.2 |
| M3 | 交付与代发 / 收货与发货 | sticky 单卡处理面；类型在卡内第 0 层 |
| M3 | `/tasks` 或 W02 等价路由 | 与 W09 同语言的 sticky 筛选 + 连续处理 |
