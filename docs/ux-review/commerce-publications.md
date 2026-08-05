# UX 评审：/commerce/publications（商品发布列表）

> 评审日期：2026-08-05
> 评审范围：`erp-client/features/product-publications/product-publications-list-page.tsx` 及依赖的
> `components/business`（data-table / list / page / option-combobox / values / feedback）、
> `features/product-publications/{api,queries,types,safety-pause-panel}.ts`、`mock/product-publications.ts`
> 评审视角：产品经理 / UX
> 参考口径：`docs/ui-glossary.md`

---

## 1. 页面概述

「商品发布（W22）」列表页，管理一条 SKU 在某目标商城的发布（刊登）对象：从创建、修订、发送到商城、
商城确认生效、安全暂停的完整生命周期。页面结构：

- 页头：标题 + 面包屑 + 数据新鲜度 + 「刷新 / 新建发布（全局被阻断，禁用态 + 阻断 Alert）」
- 指标条（MetricStrip）：待发布 / 待商城确认 / 失败转人工 / 商城已生效 / 已暂停，五项均为可点击筛选
- 工具栏：搜索框（Enter 提交，`/` 聚焦）、目标商城、发布状态、发送状态三个下拉、条件性「清除筛选」
- 表格：11 列，SKU 左固定、操作右固定，compact 行距，行点击开预览、Enter 开详情、列设置/列宽调整齐全
- 分页：本地分页状态（20/页，可选 10/20/50），无 URL 同步

整体结构完整、组件复用度高、键盘可达性（`/` 聚焦、方向键移行、Enter 打开）明显优于一般 ERP 列表。
主要问题集中在**排序假交互**、**深链隐形筛选参数**、**内部 ID 上屏**三处，以及若干信息架构与文案瑕疵。

---

## 2. 易用性

**顺的部分：**

1. 三入口进详情：行点击（预览）→ 抽屉「查看详情」；每行「打开」按钮；行聚焦按 Enter 直达详情
   （product-publications-list-page.tsx:589-610，data-table.tsx:795-819）。路径最短且无冗余。
2. 搜索框支持 `/` 键全局聚焦（product-publications-list-page.tsx:95-114），placeholder 明示可搜字段
   （发布编号、SKU、商品名），与 `filterRowsBySearch` 实际字段一致（api.ts:138-145）。
3. 指标条五项即点即筛（URL 同步、自动回第一页，product-publications-list-page.tsx:143），
   是「找待办」的高频路径，入口层级优于先看表格再选状态。
4. 错误态、空态、过滤空态均区分处理，错误态带「重试」（product-publications-list-page.tsx:561-587）。

**堵的部分：**

1. **深链隐形过滤**：从供应商商品库进入会带 `skuId` / `supplierOfferingRevisionId` 两个参数
   （supplier-catalog-page.tsx:790），页面静默按此过滤（product-publications-list-page.tsx:68-75），
   但筛选摘要不含这两项（api.ts:88-125），「清除筛选」按钮的渲染条件也不含这两项
   （product-publications-list-page.tsx:529-533）——用户看到的是一张被过滤的列表，却没有任何
   「当前被筛选」的提示和解除入口，只能靠手动改 URL 或先设置别的筛选再清除。
2. **排序入口是假的**：所有列头都可点击并显示升降序图标（data-table.tsx:719-736），
   但 DataTable 默认 `manualSorting=true`（data-table.tsx:270），页面未传 `sorting` / `manualSorting={false}`，
   查询类型也没有任何排序字段（types.ts:166-180），服务端固定按 `updatedAt` 倒序（api.ts:192）。
   点击列头后箭头会变、行序纹丝不动，用户会以为是数据错误。
3. 表格「发布状态」下拉的默认「全部」实际不等于全部（默认排除已失效，见 §4），
   用户对「为什么我总数对不上」会产生困惑。

---

## 3. 信息密度

共 11 列：SKU/商品、目标商城、商城生效版、最新发布版、固定供给、含税销售价、发布状态、商城接收、
商城确认、负责人、操作（product-publications-list-page.tsx:152-331）。

- **好**：SKU 列三层结构（SKU/品名/规格·发布编号）、固定供给列双层（供应商名 + 供给可用性），
  关键状态（发布状态、商城接收）用 Badge + 红字错误摘要，视觉层级清晰；双列固定 + 列设置 + 可调列宽，
  高密度下有兜底。
- **偏多/可压缩**：
  - 「商城确认」列仅一个时间戳（product-publications-list-page.tsx:280-291），可并入「商城接收」列
    （tooltip 或第二行），为高频列让出空间。
  - 「负责人」（product-publications-list-page.tsx:292-299）对发布运维价值一般，可默认隐藏。
  - 表头无「更新时间」列，但行序按 `updatedAt` 倒序——用户无法从列表读出「最近更新」，信息与排序脱节。
- **可接受**：双版本列（商城生效版 / 最新发布版 + 待确认徽标）信息真实且高频，不建议合并。
- 表格框架 description 写的是 UI 机制（「SKU 与操作列固定；列表采用紧凑行距」，
  product-publications-list-page.tsx:557），占位但不提供业务信息，建议改为业务口径说明或删除。

---

## 4. 交互合理性

### 4.1 URL 参数 ↔ 控件映射

| 参数 | 控件 | 清除方式 | 状态 |
| --- | --- | --- | --- |
| q | 搜索框 | 清除筛选 | ✅ |
| mall | 目标商城下拉 | 清除筛选 | ✅ |
| publicationStatus | 发布状态下拉 | 清除筛选 | ✅ |
| deliveryStatus | 发送状态下拉 | 清除筛选 | ✅ |
| metric | 指标条 | 再点一次 / 清除筛选 | ✅ |
| skuId / supplierOfferingRevisionId | 无 | 无（仅深链） | ❌ 见问题 P1-2 |

### 4.2 分页与加载

- 分页为纯本地 state（product-publications-list-page.tsx:79-82），**不写回 URL**：刷新、前进/后退均丢页码；
  筛选变更会正确重置回第一页（product-publications-list-page.tsx:143）。见问题 P2-4。
- 刷新中保留旧数据并提示「正在刷新，当前内容会保留」（data-table.tsx:655-663），体验好。
- 首屏加载用脉冲骨架（product-publications-list-page.tsx:559-560），无分页条闪烁问题。

### 4.3 状态筛选默认值

- `publicationStatus` / `deliveryStatus` / `metric` 默认 `all`（product-publications-list-page.tsx:73-75），
  但 API 在无显式发布状态筛选时**默认剔除 `INVALID`**（api.ts:185-190）——「全部」≠ 全部，且
  metric=`paused` 时又会把已失效的暂停记录放回来（api.ts:187 的排除条件不成立），口径不一致。见 P2-1。

### 4.4 排序

见问题 P1-1（列头排序无效）。

### 4.5 批量操作

未启用行选择（未传 `enableRowSelection`），列表无批量动作。发布对象的处理都是单对象级
（修订/暂停/重试），无批量场景，可接受。

### 4.6 空/错误状态

- 全空 → 「尚无商品发布」+ 新建阻断原因（product-publications-list-page.tsx:572-587）；
- 筛选空 → 「无符合条件的发布」+ 提示文案，但**空态内没有「清除筛选」动作按钮**（该按钮只活在工具栏且条件渲染），见 P2-6。

### 4.7 文案与术语

- 内部 ID / 枚举码 / 架构字段上屏（固定供给列 `sor_*`、阻断 Alert 的 code、
  安全暂停面板的 sourceObjectType/handlerKey/outbox 等），违反术语表 §2 P0 条目，见 P1-3。
- 「商城确认」列头与指标「待商城确认」措辞接近但含义不同（一个是确认时间、一个是发送状态），见 P2-5。
- 其余核心文案（待发布、商城已生效、失败/转人工、安全暂停、不可下单）均业务化，符合术语表。

---

## 5. 问题清单（按严重度）

### P0（阻断操作）：0 个

无完全阻断路径的操作问题。

### P1（明显阻碍效率）：3 个

1. **表格列头排序为假交互** — data-table.tsx:270/719-736（默认 `manualSorting=true`，列头可点）+ 
   product-publications-list-page.tsx:152-331（columns 无排序配置、未传 sorting 状态）+
   types.ts:166-180 / api.ts:192（查询无排序字段，服务端固定按更新时间倒序）。
   所有列头都能点击且箭头切换，但行序永不变化；用户无法按价格、商城、状态等排序，且会误判数据异常。
2. **深链参数 skuId / supplierOfferingRevisionId 为隐形状态，无控件、无提示、无解除入口** —
   product-publications-list-page.tsx:68-75（读取参数）+ 529-533（清除筛选渲染条件不含这两项）+
   api.ts:88-125（filterSummary 不含这两项）。从供应商商品库（supplier-catalog-page.tsx:790）进入后
   列表被静默过滤，用户不知情也无法在界面解除。违反 AGENTS.md「URL 参数与界面控件一一对应」契约。
3. **内部 ID / 枚举码 / 架构字段上屏，违反术语表** —
   - 固定供给列直接渲染 `offeringRevisionId`（如 `sor_ny_box_r12`）：product-publications-list-page.tsx:218-225
   - 阻断 Alert 用等宽字展示 `PUBLICATION_IDENTITY_POLICY_UNCONFIRMED`：product-publications-list-page.tsx:375-386
   - 预览抽屉内安全暂停面板展示 `SUPPLIER_EXTERNAL_PRODUCT`、`sep_*`、`sv-44`、`workItemId`、
     `BUSINESS_EXCEPTION`、`handlerKey`、`outboxMessageId`、`evidenceReference` 等原始值：
     safety-pause-panel.tsx:53-57, 110-119, 128-142, 150-169, 194-205
   - 对照术语表：内部 ID 不得进界面（§1.2 规则 4、§7）、枚举原值/架构词上屏为 P0 条目（§2 P0 表、
     第 5 轮 P0）；「来源版本」即术语表的「数据版本」替换口径。

### P2（体验瑕疵）：10 个

1. **「发布状态 - 全部」不等于全部** — api.ts:185-190 默认排除 `INVALID`，下拉却提供「全部」语义；
   且 metric=`paused` 时排除失效，metric 为空时排除失效，口径不一致。建议下拉改「有效发布」或提供显式
   「含已失效」选项，并在空态解释。
2. **指标与发送/发布状态筛选维度重叠且 AND 叠加** — 指标「待商城确认」与发送状态「待商城确认」为同一
   集合（api.ts:59-63 vs 163-171）但文案重复；仅点「待商城确认」指标会清发送状态
   （product-publications-list-page.tsx:405-411），其余四个指标不清（如 deliveryStatus=已确认 + 指标=商城已生效
   → 必然空结果且无提示）；指标与发布状态也可同时生效产生空结果。建议指标切换时清除另一维度，或改为
   单选互斥。
3. **刷新中显示「数据可能过期」** — product-publications-list-page.tsx:344-350 把 `isFetching` 映射为
   `stale`，点「刷新」时徽标变成「数据可能过期」，语义相反（应是「正在刷新」，
   组件已有 syncing 态：page.tsx:542）。刷新前数据本身就是旧的，显示「数据已更新」也误导。
4. **分页不写回 URL** — product-publications-list-page.tsx:79-82 + 125-127；刷新/前进后退丢失页码，
   无法分享带页码的列表状态。
5. **列头「商城确认」歧义** — product-publications-list-page.tsx:280-291 实为「商城确认时间」时间戳，
   与指标「待商城确认」（发送状态）近义异义，建议改「商城确认时间」。
6. **筛选空态无内联解除动作** — product-publications-list-page.tsx:572-587 空态只给文字
   「可清除筛选或调整条件后重试」，无按钮；工具栏「清除筛选」为条件渲染，深链场景（P1-2）下不可见。
7. **表格框架说明为 UI 机制描述** — product-publications-list-page.tsx:557「SKU 与操作列固定；列表采用
   紧凑行距」是实现信息不是业务信息，与术语表「按钮说动作/状态说结果」精神相悖，建议删除或换业务口径。
8. **未提交的搜索文本会被静默丢弃** — product-publications-list-page.tsx:89-93 的 effect 在 URL q 变化时
   用旧值覆盖输入框；用户输入一半点筛选下拉，提交 q 不变但输入框文字被回滚成旧值。
9. **列设置/列宽/固定调整不持久** — data-table.tsx:292-331（状态在内存）+ 455-537（尺寸同步仅运行期）；
   刷新页面全部还原，多列工作区每次重配。建议按用户偏好持久化（localStorage 或服务端设置）。
10. **预览面板在翻页后失效** — product-publications-list-page.tsx:132 `previewRow` 只在当前页
    `items` 中查找；预览打开期间翻页/刷新，抽屉变成「发布预览」空壳（615-626）。

---

## 6. 改进建议

**优先级高（P1 修复方案）：**

1. 排序（P1-1）：
   - 方案 A（推荐，投入小）：页面显式关掉列头排序能力（columns 逐列 `enableSorting: false` 或
     DataTable 传 `sorting={[]}` + `onSortingChange={() => {}}`），去掉假入口；
   - 方案 B：为 `ProductPublicationListQuery` 增加 `sortBy` / `sortOrder`，服务端实现
     （api.ts:192 处排序改为按参数），列表页把排序状态写回 URL 并与 `replaceParams` 联动重置页码。
2. 隐形筛选（P1-2）：
   - 为 `skuId` / `supplierOfferingRevisionId` 增加只读筛选徽标（如「已按 SKU：xxx」「已按固定供给：xxx」），
     带 × 可移除，并入 filterSummary（api.ts:88-125）；
   - 「清除筛选」按钮渲染条件补上这两项（product-publications-list-page.tsx:529-533）。
3. 内部 ID 上屏（P1-3）：
   - 固定供给列第二行只显示「供给可用性」，或把 `offeringRevisionId` 换成业务编号（如供应商名 + 版本号）；
   - Alert 去掉 code 的 mono 展示，仅保留业务 message（术语表 §2 第 5 轮 P0 口径）；
   - 安全暂停面板（safety-pause-panel.tsx）把 sourceObjectType/sourceObjectId/sourceVersion 换业务口径
     （如「来源商品：茶 · SKU-09」），workItemType/handlerKey/outboxMessageId/evidenceReference 对用户隐藏
     或换中文业务名（对照术语表 §4 内部词保留清单）。

**优先级中（P2 快速修复）：**

- 指标切换时互斥清除发送/发布状态维度，消除空结果陷阱（P2-2）；
- 刷新态用 syncing 而非 stale（P2-3）；分页写回 URL（P2-4）；
- 空态加内联「清除筛选」按钮（P2-6）；「商城确认」改「商城确认时间」（P2-5）；
- 删除表格框架机制描述（P2-7）。

**长期：**

- 列偏好持久化（P2-9）；预览面板兜底空态（P2-10）；「全部」状态口径与 INVALID 排除逻辑对齐（P2-1）。
