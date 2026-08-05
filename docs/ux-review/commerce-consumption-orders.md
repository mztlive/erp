# UX 评审报告 · 商城消费订单列表（/commerce/consumption-orders）

> 评审日期：2026-08-05 ｜ 页面：`erp-client/features/mall-consumption-orders/consumption-orders-list-page.tsx`
> 关联组件：`consumption-order-preview-panel.tsx`、`consumption-order-center-page.tsx`、`components/business/data-table.tsx`、`list.tsx`、`page.tsx`、`feedback.tsx`、`option-combobox.tsx`、`multi-option-combobox.tsx`、`values.tsx`、`api.ts`、`types.ts`
> 性质：纯只读追溯列表 + 预览抽屉 + 对象中心详情；无批量写操作。

---

## 1. 页面概述

W25 商城消费订单追溯列表。页面只读，以「事实发生期间」为门禁：未选择完整起止时间时禁止查询（不静默拉全量）。核心任务：**在大量历史支付/退款记录中定位目标订单 → 快速判断归集、履约、成本状态 → 进入预览或对象中心/供应商子订单钻取 → 按筛选结果导出**。

信息架构：页头（标题/数据新鲜度/刷新/导出）→ 只读边界 Alert（含演示状态）→ 5 个可点击指标（支付成功/待归集/记录差异/自动履约异常/成本未覆盖）→ 工具栏（搜索 + 9 组筛选）→ 筛选摘要 → 表格框架（11 列，首列与操作列固定）→ 预览抽屉。全部筛选状态经 URL 参数表达（`q/mall/fulfillmentChain/attributionStatus/paymentSource/costBasis/occurredFrom/occurredTo/factType/supplierStatus/dataSource/metric/demo/page/preview`）。

整体评价：**框架完整、URL 状态化做得很规范，但存在"看起来可用实际无效"的假交互（表头排序、未选期间时的指标条），以及若干术语表违规（W25 编号、成本枚举原值上屏）**，对"追溯/核对"这类高频扫视任务效率有实际影响。

---

## 2. 易用性

**做得好的：**
- 定位路径顺畅：行点击 / Enter / 「打开中心」三路进详情，行级预览抽屉 + 中心页两级递进，符合"先快速确认再深挖"的追溯心智（consumption-orders-list-page.tsx:1082-1083, 1101-1178）。
- `/` 快捷键聚焦搜索框，且已排除输入态误触（consumption-orders-list-page.tsx:232-251）。
- 搜索覆盖「商城单号、客户、ERP 编号」，与首列展示字段一致；供应商子订单列提供带 `returnTo` 的跨页深链（consumption-orders-list-page.tsx:474-480）。
- 筛选全部可经 URL 清除/分享，筛选摘要（filterSummary）实时回显，防呆到位（consumption-orders-list-page.tsx:994-998）。

**问题：**
- **期间门禁无视觉预示**：首次进入页面，5 个指标全部显示「—」且可点击，表格区空态文案解释需要选时间，但日期控件本身没有任何"必选"标记；用户会先点指标/翻筛选，发现无效后才读到空态说明（详见 §3 问题 P1-2）。
- **搜索提交与筛选即时生效的交互不一致**：下拉/日期筛选改动即写 URL 即刷新，而搜索需要回车或点「搜索」才提交（consumption-orders-list-page.tsx:816-817, 853-857, 988-990）。用户在输入框敲了半截关键词后去改筛选，草稿既不生效也无提示，容易误以为已按关键词查询。
- **Enter 的落点与文案不符**：表格说明写「Enter 查看详情」，但 `onRowOpen` 与 `onRowPreview` 同为打开预览抽屉，按 Enter 并不会到详情页（consumption-orders-list-page.tsx:1002, 1082-1083）。

---

## 3. 信息密度

共 11 列：商城订单 / 客户 / 支付时间 / 实付 / 支付构成 / 关键记录 / 履约链 / 供应商订单摘要 / 归集 / 成本口径 / 操作。首列、操作列固定，可经「列设置」调整显隐、顺序与固定位。

**偏多/冗余的点：**
- 状态类列达 4 列（履约链、供应商订单摘要、归集、成本口径）另加关键记录，接近全屏宽；多数行同时展开时状态噪声较高。列设置可救，但默认态即高频操作者的常态。
- 「支付构成」与「实付」信息重叠：组合支付时在每行重复列出两个金额（`组合 · 卡 ¥x / 微信 ¥y`），而实付列已含含税金额（consumption-orders-list-page.tsx:122-132）。金额细节更适合放在预览抽屉而非每行。
- 每行实付重复渲染「含税」徽标（`MoneyValue taxBasis="gross"`，values.tsx:385-388），而表格说明已声明「金额为人民币含税实付」，属于逐行噪声（consumption-orders-list-page.tsx:430-433, 1002）。

**关键信息突出度：**
- 金额、客户、两个核心状态（履约链/归集）均以徽标/等宽数字呈现，视觉层级正确；外单号加粗、ERP 编号与商城名弱化居次行，主次合理。
- 「成本口径」列把主口径徽标 + 明细文本双行展示，信息最重的列给了最合理的密度（consumption-orders-list-page.tsx:509-529）。

---

## 4. 交互合理性

- **筛选器 ↔ URL 一一对应**：除 `pageSize` 外全部参数有控件、可清除、可回放（consumption-orders-list-page.tsx:315-330）；非法值经白名单过滤（103-114）。
- **分页**：`manualPagination` + 服务端总数；改筛选自动回第 1 页（resetPage）。但 `pageSize` 不落 URL，刷新后页长重置为 8 而 `page` 参数保留（见 P1-4）。
- **空/错误状态**：四种空态（无权限/无范围/需选期间/筛选无结果）与错误态（查询失败+重试）齐备，`FILTER_EMPTY` 带「清除筛选」兜底。
- **批量操作**：只读页无行选择，批量诉求由「导出」承载（含 BatchImpactPreview 敏感字段说明 + 后台作业回执），闭环完整。
- **排序**：表头全部渲染可排序按钮与升降序图标，但数据并未接排序（见 P1-1）。
- **指标筛选默认值**：指标条无默认选中、筛选器默认 all，符合"无预设口径"的追溯诉求。

---

## 5. 问题清单（按严重度）

### P0（阻断操作）

无。页面无不可达路径与不可恢复操作。

### P1（明显阻碍效率）

1. **表头排序是"假交互"**：所有 11 列表头都以可点击按钮 + 升降序图标呈现（data-table.tsx:719-742，`getCanSort()` 默认开启），但查询排序硬编码为 `sort: "occurredAt.desc"`（consumption-orders-list-page.tsx:283），且 `manualSorting=true`（data-table.tsx:427）下本地不重排、无 `onSortingChange` 回写 URL。用户点击任意列头只会翻转排序箭头图标，行序纹丝不动——排序能力看似存在实则无效，对"按金额/按时间找单"的用户是明显的误导。`consumption-orders-list-page.tsx:384-568`
2. **指标条在未选期间时是"死按钮"，且指标口径与筛选不一致**：
   - 未选期间时查询被禁用（consumption-orders-list-page.tsx:304-306），五个指标全显示「—」，但按钮仍可点击（749-804），点击只写 URL 无任何数据反馈，用户得不到"需先选期间"的指引。
   - 选中期间后，指标计数取自**未过滤的全量** `computeMetrics(allRows)`（api.ts:419-421），不随商城、履约链、归集等筛选联动。例如筛选"商城 A + 待归集"后表格只剩 3 条，指标条「待归集」仍显示全局数字（api.ts:94-95），指标条与列表上下文脱节，无法作为筛选结果的校验依据。
3. **成本口径枚举原值上屏，违反术语表规则 7**：`COST_BASIS_LABEL` 将 `ACTUAL/STANDARD/NONE` 自映射回英文原值（types.ts:86-90），列表列直接渲染这些原值，明细文本更是混排 `NONE×1（空）`、`ACTUAL×2`（consumption-orders-list-page.tsx:143-152, 509-529），筛选下拉同样展示「ACTUAL/STANDARD/NONE」（966-984）。按术语表应译为「实际成本/标准成本/无成本」等业务词，当前形态让非财务用户无法一眼读懂。
4. **W25 编号与实现词进入用户可见文案**：`BOUNDARY_NOTICE` 原文「W25 是由不可变关键记录形成的追溯视图，不是商城可变员工订单的实时副本……」直接渲染进页面 Alert（api.ts:38-39，consumption-orders-list-page.tsx:628-631）。术语表 §3.6 明确 W 编号禁止出现在面向业务用户的提示中，应改为页面中文名；「追溯视图」等实现词也应换成业务说法。

### P2（体验瑕疵）

1. **`pageSize` 不落 URL，刷新后状态漂移**：初始分页只从 URL 取 `page`（consumption-orders-list-page.tsx:200-203），`handlePaginationChange` 也只写 `page`（332-342）。用户改页长为 20 并翻到第 3 页后刷新，页长回 8 而 `page=3` 保留：要么展示与刷新前不同的行集，要么越界后误显示「当前范围没有消费订单」空态（1062-1067），误导为"无数据"而非"页码失效"。
2. **术语不一致：「关键事实」vs「关键记录」**：列表列头已用「关键记录」（consumption-orders-list-page.tsx:445-447），预览抽屉的同名区块却用「关键事实」（consumption-order-preview-panel.tsx:193-194，术语表 §2 P2 行禁用词），同一功能两套叫法。
3. **「Enter 查看详情」文案与行为不符**：Enter 实际打开的是预览抽屉而非对象中心页（consumption-orders-list-page.tsx:1002 vs 1082-1083）。
4. **支付构成文案金额格式不一致**：组合支付时输出 `组合 · 卡 100.00 / 微信 200.00`（无货币符号），单来源时却有 `卡券 ¥…`（consumption-orders-list-page.tsx:122-132）。
5. **内部标识上屏**：页头 DataFreshness 标签渲染原始 `permissionVersion` 串（`记录更新 · pv-…`，consumption-orders-list-page.tsx:589），导出结果卡也展示「权限版本」原始串（664-678）；预览面板标签「ERP 稳定 ID」（consumption-order-preview-panel.tsx:106-110）。
6. **技术措辞残留**：底部「页长 8」（consumption-orders-list-page.tsx:1095）应为「每页 8 条」；预览面板「行列守恒：有效/差异」（consumption-order-preview-panel.tsx:233-237）对业务用户偏实现化。
7. **搜索草稿静默失效**：输入未提交的关键词后改动任一即时筛选，草稿不生效也无提示，用户容易带着错误预期看结果（consumption-orders-list-page.tsx:198, 355-357, 816-817）。
8. **`FILTER_EMPTY` 空态指引不全**：描述只提「商城、履约链或归集状态」三种，实际可筛维度有 11 种（consumption-orders-list-page.tsx:1031-1039）。
9. **刷新时状态条措辞反向**：`isFetching` 时 DataFreshness 显示「数据可能过期」（warning 语气），实际是"正在刷新、数据可用"（consumption-orders-list-page.tsx:588），每次刷新都会短暂闪烁警示。
10. **行内供应商摘要文案弱**：无子订单时仅「无子订单」三字，未区分"履约链未到下单阶段"与"已下单未生成"，与预览面板的分层提示（原人工/未形成子订单）不对齐（consumption-orders-list-page.tsx:154-164）。

---

## 6. 改进建议（按优先级）

1. **（P1-1）排序要么实现要么撤掉**：在查询层放开 `sort` 参数并映射到表头点击（写 URL 参数、服务端排序），或在业务列 meta 上关闭 `getCanSort` 并隐藏排序图标——不要保留一个点了没反应的排序控件。
2. **（P1-2）指标条与门禁、筛选口径对齐**：未选期间时禁用指标按钮并附「选择期间后可筛选」提示；指标计算改为基于 `filtered`（当前筛选上下文），或至少在指标项上标注口径「全部记录口径」。
3. **（P1-3）成本枚举补齐中文映射**：`COST_BASIS_LABEL` 改为业务词（如「实际成本/标准成本/无成本」），列表明细与筛选选项同步替换，删除 `NONE×1（空）` 这类混排。
4. **（P1-4）边界文案业务化**：重写 `BOUNDARY_NOTICE`，去掉 W25 编号与「追溯视图」等实现词，改用「本页为商城消费记录的只读快照，仅反映支付、退款、完成等五类结果」。
5. **（P2-1）页长入 URL**：将 `pageSize` 与 `page` 一并持久化并在越界时回写最后有效页，刷新不再漂移。
6. **（P2 其余）** 统一「关键记录」措辞；修正 Enter 文案为「Enter 预览」；组合支付金额统一货币符号；页头隐藏 `permissionVersion` 原始串；「页长」改「每页」；搜索未提交时切换筛选给出「搜索框内容尚未应用」的提示；空态指引列出全部可筛维度；刷新态改「正在刷新」。

---

## 附：评审依据文件

- `erp-client/features/mall-consumption-orders/consumption-orders-list-page.tsx`
- `erp-client/features/mall-consumption-orders/consumption-order-preview-panel.tsx`
- `erp-client/features/mall-consumption-orders/api.ts` / `queries.ts` / `types.ts`
- `erp-client/components/business/data-table.tsx` / `list.tsx` / `page.tsx` / `feedback.tsx` / `option-combobox.tsx` / `multi-option-combobox.tsx` / `values.tsx`
- `docs/ui-glossary.md`（§2 禁用词表、§3.6 W 编号规则、规则 7 枚举原值）
