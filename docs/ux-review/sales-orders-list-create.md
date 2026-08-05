# UX 评审：销售单列表页（/sales/orders）与新建销售单页（?mode=create）

> 评审日期：2026-08-05 ｜ 评审视角：产品经理 / UX ｜ 仅分析，未改动任何代码
> 覆盖代码：
> - `erp-client/features/sales-orders/sales-orders-list-page.tsx`
> - `erp-client/features/sales-orders/sales-order-create-page.tsx`
> - 联动文件：`url-state.ts`、`filter-orders.ts`、`api.ts`、`queries.ts`、`types.ts`、`sales-order-paper-dialog.tsx`、`components/business/{list,data-table,editor,page,feedback,entity-comboboxes}.tsx`、`mock/sales-orders.ts`
> 文案基准：`docs/ui-glossary.md`

---

## 1. 页面概述

- **销售单列表**：页头（新建销售单 / 导出 / 数据新鲜度）→ 5 格指标快速筛选条（待处理/进行中/待收款/履约异常/商城协同）→ 表格框架（搜索框、业务性质 ToggleGroup、高级筛选 Popover（创建来源+主状态）、列设置、排序、分页）。筛选/排序/分页全部以 URL 参数承载，可深链可分享；支持 `/` 聚焦搜索、`j/k/↑↓` 行导航、`Enter` 预览、`Esc` 关闭的行键盘操作。
- **新建销售单**：单据头（有效合同选择器 + 合同 PDF 上传入口、业务性质、负责销售、福利场景、付款条件、履约期限、税率）→ 销售明细可编辑表格（实物/服务 与 卡券 两套列动态切换）→ 内部说明 → 底部吸底金额汇总条（含税/不含税/税额 + 取消/保存草稿/提交）→ 右侧本单摘要（xl 以上）。
- 总评：整体完成度高。列表的信息架构、URL 状态同步、键盘导航、金额实时汇总、合同驱动自动带出、取消/离开防误触保护都属于加分项；主要问题集中在「保存草稿语义失效」「查询失败态缺失」「切换业务性质静默清空明细」与「内部标识符泄漏」。

---

## 2. 易用性

### 列表
- 查找路径完整：搜索（单号/客户/合同/负责人，300ms 防抖 + Enter 即时提交）+ 指标条一键筛选 + 业务性质 + 高级筛选，全部回写 URL，支持浏览器前进/后退。`erp-client/features/sales-orders/sales-orders-list-page.tsx:129-167`
- 键盘行导航（j/k/Enter/Esc）体验好，但完全无提示，可发现性差（见问题 13）。
- 行点击即打开纸质预览，同时有「查看详情」链接、合同号即点即下载 PDF，交互通道多且个别语义隐晦（见问题 12）。
- 空态 / 错误态：查询失败时表格仍渲染「当前筛选没有结果」，无错误提示、无重试（见问题 2）；空态无新建 CTA（见问题 7）。

### 新建
- 步骤链路短：选合同 → 自动带出版本/客户/结算主体/负责销售/付款条件（`sales-order-create-page.tsx:388-429`）→ 加明细行 → 看实时汇总 → 提交。选合同这个「最大阻力点」被自动带出很好地消化了。
- 无合同可用时提供「上传合同 PDF」闭环（`sales-order-create-page.tsx:559-569`、`1176-1183`），上传成功自动回填合同，这是亮点。
- 明细行加/删有 GuardedBusinessAction 解释型禁用（卡券仅一条、至少保留一条），防误操作到位（`sales-order-create-page.tsx:966-989`）。
- 致命伤：**「保存草稿」与「提交」共用同一套全量校验**（`sales-order-create-page.tsx:326, 1066-1073`），未填完整（哪怕只差一个日期）的单子一张草稿都存不进去，草稿功能形同虚设（见问题 1）。
- 切换「业务性质」会静默清空全部已填明细行，无确认、无撤销（见问题 3）。

## 3. 信息密度

- 列表 8 列（销售单、业务性质、合同、进度、成交金额、负责人、提交时间、操作），但客户名内嵌在「销售单」列、三轨进度（履约/回款/开票）合并为一列，实际视觉列并不拥挤；首列/末列固定 + 列设置（显隐/顺序/固定/宽度），密度控制得当。
- 新建表单单据头 7 字段 + 明细行 4-5 个编辑列，对一个 ERP 建单页属正常范围，未过度。明细列随业务性质动态切换（卡券：面值/配赠率/卡形态；实物：交付日期），只显示与当前性质相关的字段，密度控制好。
- 瑕疵：右侧「本单摘要」仅 xl 屏可见（`sales-order-create-page.tsx:1095`），中屏以下用户看不到随填随变的摘要；顶部「含税金额/不含税/税额」三块汇总在 StickyTotalBar 中已有，未形成信息冗余问题，可接受。

## 4. 交互合理性

- URL 参数与控件一一对应（search/nature/summary/origin/status/page/pageSize/sort/dir 均有控件），符合项目界面契约；但主状态筛选把**中文业务词当 URL 枚举值**（`status=待二次确认`），且状态列表在 `url-state.ts`、`filter-orders.ts`、页面 Popover 三处重复维护（见问题 5）。
- 分页（20/50/100）与排序切换后重置页码正确（`sales-orders-list-page.tsx:242-252`）。
- 金额汇总：明细行实时算小计，StickyTotalBar 实时汇总含税/不含税/税额，并明示「税率 X% 预估」「金额以提交后系统计算为准」，诚实标注估算口径，好（`sales-order-create-page.tsx:1009-1050`）。
- 校验：仅 onSubmit 触发（`sales-order-create-page.tsx:326`），长表单首次提交一次性爆出全部错误；且 lineItems 数组级错误（卡券仅一条、至少一条明细）挂在根路径，页面没有任何展示位（未接 `getRowErrors`，未用 `ValidationSummary`），错误不可见（见问题 8、9）。
- 误操作防护：取消（dirty 时弹 DiscardConfirmDialog）、beforeunload 守卫、提交中按钮自动禁用（`components/form/submit-button.tsx:34`）都到位。
- 导出：整个导出实际是客户端同步生成 CSV，却包装成「导出任务已完成 + 审计标签」的后台任务结果卡，机制性表述偏重，且泄漏内部标识（见问题 14、4）。
- 指标「待处理」含草稿口径存疑（见问题 15）。

---

## 5. 问题清单（按严重度）

### P0（阻断操作）：无

本轮未发现直接阻断主流程（列表查看 / 提交建单）的 P0 级问题。

### P1（明显阻碍效率 / 语义失效 / 数据损失 / 违规上屏）

| # | 严重度 | 问题 | 位置 |
| --- | --- | --- | --- |
| 1 | P1 | **「保存草稿」被全量校验阻断，功能语义失效**。「保存草稿」与「提交」共用 `onSubmit` 校验（schema 要求合同/客户/负责销售/福利场景/付款条件/履约期限/税率/明细价格全过），未填完整的单子无法保存任何草稿——草稿本应允许「先存一部分，稍后继续」。且保存草稿成功路径与提交完全一样（跳详情页），无草稿专属反馈与后续编辑入口。 | `sales-order-create-page.tsx:326, 1066-1073, 328-351` |
| 2 | P1 | **列表查询失败无错误态、无重试**。`ordersQuery.isError` 只体现在页头 DataFreshness 的「查询失败」小字（`list-page.tsx:496`），表格仍按空数据渲染成「当前筛选没有结果」（`components/business/data-table.tsx:858-871`），用户无法区分「真的没有」还是「查询挂了」，也没有重试按钮，只能手动刷新页面。 | `sales-orders-list-page.tsx:123-127, 777-794`；`components/business/data-table.tsx:858-871` |
| 3 | P1 | **切换「业务性质」静默清空全部已填明细行**。`onValueChange` 直接 `setFieldValue("lineItems", [createEmptyLine(nature)])`，此前输入的若干行明细、数量、单价全部丢失，无确认、无撤销；而业务性质本身是可来回切换的下拉框，误触/反复切换即造成数据损失。 | `sales-order-create-page.tsx:624-633` |
| 4 | P1 | **内部标识符泄漏上屏，违反术语表规则 7（必须清零）**。列表页头元数据直接渲染 `列表 · 权限 pv-w05-demo-1`（`list-page.tsx:511`，来自 `api.ts:90`）；导出结果卡 facts 展示「权限版本 pv-w05-demo-1」「审计标签 jobId」（`list-page.tsx:546-561`）；导出 CSV 首行注释含 `permissionVersion=…; source=client-filtered; audit=…`（`list-page.tsx:287`）。这些对业务用户（销售/财务）是不可理解的内部版本号，且 CSV 是交付给财务的文件。 | `sales-orders-list-page.tsx:511, 546-561, 287`；`features/sales-orders/api.ts:90` |

### P2（体验瑕疵）

| # | 严重度 | 问题 | 位置 |
| --- | --- | --- | --- |
| 5 | P2 | 主状态枚举**三处重复维护**（`url-state.ts:38-49`、`filter-orders.ts:12-22`、页面 Popover `list-page.tsx:725-736`），且 URL 直接使用中文业务词做参数值（`status=待二次确认`），一旦文案调整，旧链接全部失效；建议统一用稳定枚举码 + 集中映射。 | `features/sales-orders/url-state.ts:38-49`；`filter-orders.ts:12-22`；`sales-orders-list-page.tsx:725-736` |
| 6 | P2 | 表格描述与真实筛选状态不一致：仅搜索或仅业务性质筛选时（summary=all 且 origin/status=all），description 仍显示默认文案「按提交时间查看…」，不反映当前筛选。 | `sales-orders-list-page.tsx:619-627` |
| 7 | P2 | 空态无引导动作：未传 `emptyState`，统一显示「当前筛选没有结果」，首次进入无数据时也没有「新建销售单」CTA 或「清除筛选」提示。 | `sales-orders-list-page.tsx:777-794`；`components/business/data-table.tsx:864-868` |
| 8 | P2 | 校验时机过晚：表单仅 `onSubmit` 校验，长表单首次点提交才一次性爆出全部错误；建议关键必填字段失焦即校验。 | `sales-order-create-page.tsx:326` |
| 9 | P2 | lineItems 数组级校验错误无展示位：「至少需要一条销售明细」「卡券销售单必须恰好只有一条明细」挂在 `lineItems` 根路径，明细表未接 `getRowErrors`、页面未用 `ValidationSummary`，这类错误即使触发也不可见。 | `sales-order-create-page.tsx:118, 149-155, 959-990` |
| 10 | P2 | 加载时序文案泄漏内部状态：「合同版本尚未加载完成」「客户尚未加载完成」（`create-page.tsx:128-141`）让用户看到的是系统内部加载过程；选合同后立即点提交会弹这类错误，建议改为「正在同步合同信息，请稍后再提交」。 | `sales-order-create-page.tsx:128-141` |
| 11 | P2 | 「保存草稿」的 pendingLabel 是「正在创建…」，与按钮语义不一致（对照「提交」→「正在提交…」）。 | `sales-order-create-page.tsx:1066-1069` |
| 12 | P2 | 列表「合同」列点击即触发 PDF 下载（仅 title 弱提示，`list-page.tsx:387-399`），而同一行点击又打开预览，两个无差异的「点击」行为叠加，易误触下载；建议改为明确的下载图标按钮或先预览再下载。 | `sales-orders-list-page.tsx:387-399, 790-791` |
| 13 | P2 | 键盘快捷键（`/` 聚焦搜索、`j/k` 行导航）无任何可见提示，可发现性差，普通用户完全不知道存在。 | `sales-orders-list-page.tsx:169-217` |
| 14 | P2 | 导出本是客户端同步生成 CSV，却包装成「导出任务已完成」后台任务结果卡（`list-page.tsx:539-567`），配合「审计标签」等表述，机制感强于业务感；CSV 文件名含 jobId（`list-page.tsx:296`）。 | `sales-orders-list-page.tsx:539-567, 296` |
| 15 | P2 | 指标口径：「待处理」包含「草稿」（`filter-orders.ts:71, 144-152`），草稿并非待办事项，销售看到「待处理 N」会误以为有 N 件需处理；建议草稿单独立项或排除。 | `features/sales-orders/filter-orders.ts:64-76, 141-152` |

---

## 6. 改进建议（按优先级）

1. **草稿语义修复（P1-1）**：`SAVE_DRAFT` 路径使用宽松校验（仅要求合同已选 + 至少一行明细），`SUBMIT` 才走全量 schema；保存草稿后留在新建页并给出「草稿已保存」反馈，或跳详情页并提供「继续编辑」入口；列表页为草稿单提供续编入口。
2. **列表错误态（P1-2）**：`ordersQuery.isError` 时用 `BusinessFailureState` 渲染整表失败态（含重试按钮，重试即 `queryClient.refetch` 或重新触发 query）；空态区分「无数据」与「筛选无结果」，前者给「新建销售单」CTA。
3. **明细防丢失（P1-3）**：切换业务性质前弹确认（说明将重置明细），或将原明细暂存、切回时恢复。
4. **术语清理（P1-4）**：删除 `列表 · 权限 ${PERMISSION_VERSION}`、导出结果卡的「权限版本/审计标签」facts；CSV 头注释改为业务说明或删除。
5. **枚举集中化（P2-5）**：状态值改用稳定枚举码（如 `status=awaiting_confirm`）并集中一处中文映射（参考 `NATURE_LABEL` 模式），三处列表合并。
6. **校验体验（P2-8/9）**：给明细表接 `getRowErrors` 展示行级错误；必填字段 onBlur 校验；提交后若仍有错误，滚动到第一个错误字段并高亮。
7. **交互语义（P2-12）**：合同号列改为「预览合同/下载 PDF」图标按钮二选一，去掉「点击即下载」的隐晦行为。
8. **快捷键提示（P2-13）**：搜索框 placeholder 或工具栏加「/ 快速搜索 · ↑↓ 选择行 · Enter 预览」轻提示。
9. **指标口径（P2-15）**：待处理指标剔除草稿，或指标条加「草稿」独立计数。

---

### 附：术语表对照结论

页面整体业务化表达合格：未出现「投影/租约/幂等/正式」等禁用词；状态、进度、金额文案均为业务语言；按钮说动作。唯一显著违规是 P1-4 的内部标识符泄漏（`pv-w05-demo-1`、审计标签、CSV 头注释），以及 P2-10 的「加载完成」类时序内部状态文案，均需按 `docs/ui-glossary.md` 第 5 轮口径清理。
