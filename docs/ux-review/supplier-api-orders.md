# UX 评审报告 · 供应商订单列表（/supplier-api/orders）

> 评审日期：2026-08-05 ｜ 评审视角：产品经理 / UX
> 评审范围：`erp-client/features/supplier-orders/supplier-orders-list-page.tsx` 及关联组件（预览面板、DataTable、指标条、导出流、mock/url-state/types）
> 严重度定义：P0=阻断操作 ｜ P1=明显阻碍效率 ｜ P2=体验瑕疵

---

## 1. 页面概述

供应商订单列表（W26），供应商 API 域的核心工作台。职责：

- **分流**：5 个指标卡（待提交 / 结果未知 / 履约异常 / 售后待处理 / 全部订单）+ 视图切换（可操作/全部/最近完成）
- **筛选**：搜索框 + 供应商 + 履约状态（多选）+ 取消状态 + 退款状态 + 支付日期区间
- **浏览**：6 列表格（供应商订单、商城单号、三轨状态、外部单号、更新时间、操作），行点击/Enter 打开右侧预览抽屉
- **处理**：RESULT_UNKNOWN 订单行内「查询原结果」；导出当前筛选（后台任务 + 打码）
- **角色**：页头演示角色切换（采购/客服/运营/财务/管理员）驱动字段权限与可用动作

整体信息架构清晰，三轨状态（履约/取消/退款）正交展示是亮点；URL 状态全量代码化（`url-state.ts`），深链/分享/回源均有处理。主要问题集中在：指标与筛选结果的一致性、加载态误导、内部标识泄漏。

---

## 2. 易用性

**顺畅的部分**

- 查找路径完整：搜索（支持 Enter/失焦提交、`/` 快捷键聚焦）、指标卡一键筛选、供应商/状态/日期组合筛选，均可深链；搜索 placeholder 明确列出三种单号。
- 进入详情双通道：行点击/Enter → 预览抽屉，预览内「查看详情」→ 对象中心；行内另有「中心」直达。键盘导航（j/k、Escape 关闭预览并回焦行）实现完整。
- 回源上下文保留：`returnTo` 手动保留（supplier-orders-list-page.tsx:176-187），从商城订单钻入时有「返回来源」条。

**阻碍效率的部分**

- 初始加载期间表格显示「当前筛选没有结果」（见问题 1），用户会被误导以为筛选错了，实际是数据未到。
- 指标卡数字与点击后的筛选结果对不上（见问题 2），用户按数字判断工作量会反复试错。

---

## 3. 信息密度

6 列对 ERP 列表适中，身份列（单号+供应商名）与操作列左右固定（supplier-orders-list-page.tsx:1010-1013），三轨状态合并为一列（`StatusTrackSummary`）避免 3 列堆砌，设计合理。

**缺口**

- 列表完全没有金额/数量维度的列（行类型 `SupplierOrderListRow` 无任何金额/商品字段，types.ts:209-233；成本在详情里还按角色打码）。采购/财务无法在列表层按金额或货量分流，只能逐行开预览。信息密度偏「只有状态没有钱」。
- 供应商名只作为身份列次要行（supplier-orders-list-page.tsx:377-379），文字 11px，弱化了「按供应商分流」这一高频诉求——虽然供应商是独立筛选器，但行内一眼识别供应商的能力偏弱。
- 「共 X 条」在工具栏与分页条重复展示（见问题 11）。

---

## 4. 交互合理性

- **筛选器 ↔ URL 参数**：`parseSupplierOrdersSearchParams`/`buildSupplierOrdersSearchParams`（url-state.ts:41-71）覆盖全部控件；`supplierOrderId`/`demoRole` 别名兼容旧链接；「清除筛选」覆盖所有筛选键。契约基本闭合，仅有少数残留（见问题 9、10）。
- **分页/排序**：页码与每页条数入 URL；排序只有 4 个可排字段（身份/商城/外部/更新时间），三轨状态与操作列禁排合理；排序变更重置页码（supplier-orders-list-page.tsx:209-219）。默认排序按 `priority` 倒序无 UI 提示，可接受。
- **空/错误态**：加载失败有「重试」；空结果有「打开商城消费订单」建议动作，文案「调整视图、供应商或支付时间」具体可执行。唯一缺陷是加载中误报空态（问题 1）。
- **按钮语义**：「查询原结果」「确认导出」「返回来源」均为动作化文案，符合术语表；「中心」按钮措辞成谜（问题 6）。
- **导出流**：BatchImpactPreview 展示筛选摘要、敏感字段清单、打码说明，确认前有完整影响预览，是正确范本；但结果卡片泄漏内部 ID（问题 4）。

---

## 5. 问题清单

### P1（5 个）

**1. 初始加载态误报「当前筛选没有结果」**
初始 pending 期间未传 `loading` prop，`!listQuery.isPending && rows.length === 0` 分支跳过，直接渲染空数组的 DataTable，显示默认空态文案「当前筛选没有结果」；骨架屏只在 `loading && data.length === 0` 时出现。
位置：supplier-orders-list-page.tsx:977, 994；components/business/data-table.tsx:775-784, 858-871

**2. 指标卡计数与点击后的筛选结果不一致（3 处）**
- 「待提交」计数口径 = RECEIVED + SUBMITTING，点击只筛 SUBMITTING
- 「履约异常」计数口径 = EXCEPTION + REJECTED，点击只筛 EXCEPTION
- 「售后待处理」计数口径 = 取消/退款各异常态（含 PENDING/MANUAL/PARTIAL），点击只筛 `refundStatus=REFUND_FAILED`
用户看到的数字是 5，筛出来 1 条，分流决策被误导。
位置：api.ts:323-375（buildMetrics）vs supplier-orders-list-page.tsx:627-650（onClick）、api.ts:285-304（matchesQuery）

**3. 指标条直接渲染内部参数名+枚举原值**
「结果未知」指标卡 inline detail 为 `fulfillmentStatus=RESULT_UNKNOWN`，随 `MetricFilterItem` 直接上屏。违反 AGENTS.md §5「禁止把枚举原值直接渲染」与 ui-glossary §2 第 5 轮 P0 条目。
位置：api.ts:341（detail 字段）；渲染路径 supplier-orders-list-page.tsx:613-652 → components/business/page.tsx:482-487

**4. 导出结果暴露内部标识**
- 事实表「权限版本」直接显示 `pv-w26-1`（supplier-orders-list-page.tsx:688-690，来源 api.ts:1112）
- 后台任务条 description 显示任务号 `exp-w26-xxx`（supplier-orders-list-page.tsx:705，来源 api.ts:1103）
用户不需要知道内部权限版本号与任务号，属内部 ID 泄漏（ui-glossary §7「内部 ID 不得进界面」）。

**5. 「查询原结果」禁用无原因，且唯一解释的 sr-only 文案条件挂错**
查询按钮 `disabled={!canQuery || isPending}`（supplier-orders-list-page.tsx:489-497）没有任何可见/悬停解释；而唯一存在的 sr-only 文案挂在 `!canReplay` 条件上、内容讲的是「不可重试」（supplier-orders-list-page.tsx:499-503）——当供应商无查询能力（blocker：NO_QUERY_CAPABILITY，api.ts:138-144）时屏幕阅读器听到的是错误原因。列表层也没有任何 REPLAY 按钮，该文案上下文错位。

### P2（8 个）

**6. 同一详情入口两种措辞：「中心」 vs 「查看详情」**
行操作按钮叫「中心」（supplier-orders-list-page.tsx:479-488），预览页脚叫「查看详情」（supplier-orders-list-page.tsx:1068-1078），指向同一 URL。应统一为业务可理解词（如「详情」），「中心」是内部对象中心话术。

**7. 列表缺金额/商品摘要列**
行类型无任何金额或商品字段（types.ts:209-233），列表无法按金额/货量分流。建议至少为有权限角色提供「成本（打码）」或「商品数/数量」列。

**8. 列头与列设置菜单命名不一致**
列头显示「更新时间」（supplier-orders-list-page.tsx:452），`meta.label` 却是「最近业务变化」（supplier-orders-list-page.tsx:453），列设置弹窗与排序 aria-label 会用后者。

**9. 「清除筛选」附带重置视图，且日期筛选单独生效时不出现该按钮**
清除时强制 `view: "actionable"`（supplier-orders-list-page.tsx:950），用户在「全部」视图点清除会被悄悄拉回「可操作」；显示条件（supplier-orders-list-page.tsx:931-935）未包含 `paidFrom/paidTo`，只设日期时按钮不出现。

**10. 所有筛选变更走 `router.replace`，筛选历史不可回退**
supplier-orders-list-page.tsx:184。用户无法用浏览器后退逐步撤销筛选步骤；且从商城订单钻取带筛选进入时（W25 钻取 effect，supplier-orders-list-page.tsx:154-174）会 replace 掉全部筛选参数。

**11. 「共 X 条」重复展示**
工具栏 actions（supplier-orders-list-page.tsx:929）与分页条（data-table.tsx:1065）同时显示同一总数。

**12. 指标条数值不随当前筛选变化**
指标基于全量种子计算（api.ts:452-454），供应商/状态筛选生效后指标数字仍是全局值，点击后又保持当前筛选，出现「数字与点击结果」双重偏差。

**13. 预览面板泄漏内部版本编码**
「固定供给 / 发布：SV-12 / PV-8」（supplier-order-preview-panel.tsx:126-128）与商品行「供给 SV-12」（supplier-order-preview-panel.tsx:186）直接把内部 `supplyVersion/publicationVersion` 编码上屏，建议翻译为业务口径（如「供给版本 12」或干脆隐藏）。

---

## 6. 改进建议

1. **加载态（P1-1）**：给 DataTable 传 `loading={listQuery.isPending}`，或在 pending 时直接渲染骨架态，避免「没有结果」误报；也可用 `keepPreviousData` 让刷新期间保留旧行（DataTable 已有「正在刷新，当前内容会保留」条，data-table.tsx:655-663）。
2. **指标一致性（P1-2、P2-12）**：指标卡要么数字口径与点击筛选完全一致（修 `buildMetrics` 的筛选键，如「待提交」筛 `[RECEIVED, SUBMITTING]`、多值筛选取消/退款），要么点击时同步切换为等价视图；「售后待处理」建议筛多值 `cancelStatus/refundStatus` 而非单一 REFUND_FAILED。
3. **清除枚举泄漏（P1-3）**：删掉 `buildMetrics` 里的 `detail: "fulfillmentStatus=RESULT_UNKNOWN"`，改为业务口径（如「需先查询原结果」）或直接不展示 detail。
4. **导出结果脱敏（P1-4）**：facts 中「权限版本」改为不展示或用「权限规则快照已固定」代替原值；任务条 description 去掉 jobId，仅保留行数与打码说明。
5. **禁用按钮可解释（P1-5）**：复用 actionBlockers 的 message：`!canQuery` 时用 Tooltip 展示 blocker 文案（如「供应商无查询能力，请进入接口错误与对账中心」），并修正 sr-only 文案条件（按 QUERY_RESULT blocker 渲染，而非 `!canReplay`）。
6. **入口统一（P2-6）**：行操作「中心」改「详情」，与预览页脚一致。
7. **信息密度（P2-7）**：为成本可见角色（采购/财务/管理员）增加「成本」列（无权限显示 `•••`），为所有角色增加「商品数/数量」列；供应商名字号提升到 12px。
8. **清除语义（P2-9）**：「清除筛选」只清筛选键、不动 view；显示条件补上 `paidFrom/paidTo`。
9. **预览面板（P2-13）**：`SV-12/PV-8` 改为业务化展示或删除该行。
