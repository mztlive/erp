# UX 评审：/supplier-api/orders/[supplierOrderId]（供应商订单详情 / 处理中心）

> 评审日期：2026-08-05 ｜ 角色：产品经理 / UX 视角（纯分析，未改代码）
> 涉及文件：
> - `erp-client/app/(workspace)/supplier-api/orders/[supplierOrderId]/page.tsx`
> - `erp-client/features/supplier-orders/supplier-order-center-page.tsx`
> - `erp-client/features/supplier-orders/supplier-order-preview-panel.tsx`
> - `erp-client/features/supplier-orders/types.ts` / `queries.ts` / `api.ts`
> - `erp-client/components/business/{document,workflow,feedback,values,domain}.tsx`
> - 参照：`docs/ui-glossary.md`（术语表 v1.2）

## 一、页面概述

供应商单对象中心：从「供应商订单」列表或商城消费订单、统一任务队列进入，用于查看一笔供应商子订单的三轨状态（履约/取消/退款）、商品与成本快照、收货地址（短时揭示）、售后请求与动作审计，并在"结果未知"场景下执行「查询原结果 → 安全重发」、任务跳过、售后取消/退款等动作。页面同时服务两种语境：

1. **处理中心**（可操作：RESULT_UNKNOWN / 异常 / 售后待处理）——核心动作路径是「查询原结果 → 确认无结果 → 安全重发」；
2. **详情只读**（已完成/已发货等常态）——核对与追溯。

整体设计质量高：结果未知的风险告警、安全重发的确认层、地址短时揭示与自动清除、成本按字段权限打码、重复提交幂等兜底等硬功能力到位；文案主体业务化（"不得把结果未知直接改成成功""重复提交返回原结果"等表达专业）。主要短板集中在**术语泄漏（内部 ID / 枚举原值多处上屏）、售后动作缺少确认防护、核对所需金额汇总缺失、错误态误报**四个方面。

## 二、易用性

**好的方面**
- 主操作路径（结果未知 → 查询 → 重发）风险编排清晰：主按钮「查询原结果」，重放按钮在确认无结果前禁用并给出 title 说明（`supplier-order-center-page.tsx:538-563`），符合"不得在未查询前再次下单"的业务红线。
- 无查询能力的订单自动切换出口为「转接口错误中心」（`api.ts:136-145`、页面 `:574-584`），路径兜底完整。
- 返回路径双向打通：详情页可回「返回列表」「返回商城订单」，结果面板可跳「打开 API 结算」「返回商城消费订单」。
- 地址揭示的权限与时效感知好：默认打码 → 短时揭示 → 可立即隐藏，离开页面自动清除（`:217-221`、`:913-937`）。

**问题**
1. **售后动作是唯一没有确认层的对外动作**：安全重发走了 `FormalActionConfirmDialog`（`supplier-order-center-page.tsx:1216-1233`），而同样会向供应商发起的「提交取消 / 提交退款」点击即提交、无任何确认（`:1015-1043`）。虽然幂等与页脚说明兜底了重复提交，但误点一次即产生对外副作用，与同页"正式动作须确认"的防护标准不对称。
2. **切 Tab 整页重挂载**：路由 `key` 含 section（`page.tsx:33`），每次切换 Tab 都 remount 组件，`result` 结果横幅（如"已安全重发"）被清空、滚动位置回顶（`:213-215` 每次切 Tab focus 标题）；超过 staleTime 后切 Tab 还会闪加载骨架。处理完一单后想翻看别的分页核对，结果反馈就丢了。
3. **「已跳过」后可无限重复跳过**：跳过（DEFER）后任务仍待处理、按钮仍显示「先跳过」（`:565-572`），用户感知不到"本轮已处理过"，可能反复提交。

## 三、信息密度

**好的方面**
- 结果未知的告警信息价值密度高：明确告诉用户"能做什么、不能做什么、上次查询结论、重发是否已开放"（`:633-654`）。
- 售后分「商城退款 / 余额卡券恢复 / 供应商退款」三格对比展示，每格带状态与缺口提示（`:993-1013`），核对路径清晰。
- 成本区分含税/不含税并统一打码口径（`:1056-1118`）。

**问题**
1. **缺少订单级金额汇总与数量差异**：明细表只有单价与数量（`:793-840`），全页没有订单金额合计、也没有下单数量与商城明细数量的差异列。本页核心任务是"核对"，但核对所需的总数要用户逐行心算；「成本差额」也仅在成本 Tab 以文本裸值呈现（`:1092-1098`），无对比参照。这与"关键信息（金额、数量差异）是否突出"的评审目标差距最大。
2. **状态三轨重复占屏**：履约/取消/退款状态在 `DocumentHeader` statuses（`:503-518`）与 `StatusTrackSummary`（`:601-622`）各渲染一遍，主徽章又是履约状态第三遍。首屏信息冗余，挤占了后续告警/任务卡的注意力。
3. **概览 Tab 机话密度高**：「连接 JD-PROD-01 / PRODUCTION」「支付记录键 pay_fact_m90881」「固定供给版本 SV-12」「发布版本 PV-8」「版本 3」连排（`:765-780`），对采购/客服用户基本无语义，属于内部标识堆砌。

## 四、交互合理性

**加载 / 空 / 错误状态**
- 加载有骨架屏、未找到有返回列表出口（`:407-436`）——基础状态齐全。
- **错误态误报（P1）**：`query.isError`（网络/加载失败）时 `detail` 同样为 undefined，落入 `!detail` 分支展示「未找到供应商订单 · 不存在或无权访问」（`:417-436`）。加载失败被定性为"权限/不存在"，用户会误判并可能去找权限审批，实际是系统故障。系统已有 `BusinessFailureState(kind="system")` 与 `AsyncSectionState` 可用而未用。
- **mutation 异常无反馈（P2）**：`handleQueryResult/handleReplay/handleAfterSales/handleReveal` 均为 `void handleX()`，`mutateAsync` 一旦 reject（真实网络错误），无任何提示、控制台 unhandled rejection；重发弹层 `FormalActionConfirmDialog` 的 `onConfirmError` 未接（`:1216-1233`），弹层会卡在 pending 状态无声失败。

**操作按钮语义与禁用逻辑**
- 「查询原结果」禁用时无解释（title 只给了重放按钮），依赖上方告警说明，可接受但略弱；此处更适合 `GuardedBusinessAction`（`feedback.tsx:88-148` 已存在未复用）。
- 售后按钮禁用无原因说明：`disabled={!as.allowedActions...}`（`:1019-1022`、`:1033-1036`）静默禁用，`as.actionBlockers` 未在卡片上展示（只在预览面板出现），用户不知道为什么不能点。
- **跳过失败态语义错位（P2）**：`status: res.status === "succeeded" ? "succeeded" : "blocked"`（`:180`）——API 返回 `failed`（如无关联任务）也被定性为"已阻断"，结果面板徽章与实际不符。

**反馈提示**
- 各动作结果统一走 `FormalActionResult`，成功/阻断/未知四态都有对应文案与事实列表，反馈闭环完整。
- 但结果事实里多处输出**枚举原值**（详见问题清单 P1-3），把 PENDING / ACCEPTED / CANCEL_PENDING 这类实现值直接给用户。

**误操作防护与回退**
- 重发：确认弹层完整（状态迁移、影响、不可撤回影响全列出），幂等键稳定，防误点到位。
- 售后取消/退款：无确认层（P1-4）。
- 地址揭示：有审计与自动隐藏，但揭示前无确认（demo 可接受）。
- 回退：返回列表固定 `?view=actionable`（`:480`），从「全部」或「最近完成」视图进入的用户回退后视图被换掉（P2）。

**无障碍（P2）**
- 跳过弹层是手写 `fixed` div（`:1235-1303`）：无 `role="dialog"` / `aria-modal` / 焦点陷阱 / Esc 关闭，键盘用户会被困在模态里；与同页 `FormalActionConfirmDialog`（AlertDialog 语义完整）不一致。

## 五、问题清单（按严重度）

### P0（阻断操作）— 0 个

未发现完全阻断主流程的问题；主路径（查询→重发）可闭环。但 P1-1/2/3 违反术语表"必须清零"级契约，P1-4 属真实系统中高危的误操作面，建议按 P0 节奏修复。

### P1（明显阻碍效率 / 明显风险）— 6 个

| # | 问题 | 位置 |
| --- | --- | --- |
| P1-1 | **关联任务卡泄漏内部 ID 与类型码**：`workItemType · workItemId` 直接渲染，界面出现 `INTEGRATION_RESULT_UNKNOWN · wi-sfo-unknown-01`（mock 证实 `wi_*`）。术语表 §2 P0「work_item / work_item_type 用户可见一律禁止」、§7「内部 ID 不得进界面」 | supplier-order-center-page.tsx:671 |
| P1-2 | **工作项状态徽章渲染枚举原值**：`label={detail.workItem.workItemStatus}`，`PENDING / IN_PROGRESS / COMPLETED / TRANSFERRED` 原样上屏 | supplier-order-center-page.tsx:677-686 |
| P1-3 | **四个动作结果 facts 均泄漏枚举原值**：查询结果 `workItemStatus`（:301）；重发结果 `fulfillmentStatus`（"ACCEPTED"）、`workItemStatus`（:340、:344）；售后结果 `cancelStatus` / `refundStatus`（"CANCEL_PENDING"/"REFUND_PENDING"，:385-386）；跳过结果非 PENDING / 非 RELEASED 分支直接输出原值（:189-199）。同一根因，建议统一补中文映射 | supplier-order-center-page.tsx:189-199, 301, 340, 344, 385-386 |
| P1-4 | **售后「提交取消 / 提交退款」无确认层直接向供应商发起**：与同页安全重发的确认防护不对称；误点即产生对外副作用 | supplier-order-center-page.tsx:1015-1043 |
| P1-5 | **加载/网络错误被误报为「订单不存在或无权访问」**：`query.isError` 与 404 共用 `!detail` 分支，错误定性错误，引导用户去查权限而非重试 | supplier-order-center-page.tsx:417-436 |
| P1-6 | **核对信息缺失：无订单金额合计、无下单数量 vs 商城明细差异、成本差额无参照**：核心任务"核对"需要人工心算 | supplier-order-center-page.tsx:793-840, 1092-1098 |

### P2（体验瑕疵）— 16 个

| # | 问题 | 位置 |
| --- | --- | --- |
| P2-1 | 概览「连接」泄漏连接码与环境英文原值（`JD-PROD-01 / PRODUCTION`） | :765-767 |
| P2-2 | 「支付记录键」/「支付凭证」展示内部事实键 `pay_fact_*`，建议改业务口径（如「支付凭证号」）或仅审计层展示 | :628、:777-779 |
| P2-3 | 「固定供给版本 SV-12」「发布版本 PV-8」及明细表「发布/供给」列：`SV-*`/`PV-*` 前缀机话（与 commerce-publications 评审中 `sv-44` 同族） | :769-775、:824-825 |
| P2-4 | `DocumentHeader`「版本 {lockVersion}」展示乐观锁版本，业务用户无含义 | :497 |
| P2-5 | 结果面板「结果编号」展示 `evq_*/evr_*/act-*` 证据 ID 与 operationId 原值 | :288、:334、:383、:404、feedback.tsx:747-751 |
| P2-6 | 「任务号尾号」（审计表）与「任务号尾部」（预览面板）用词不一致 | :1152 vs supplier-order-preview-panel.tsx:217 |
| P2-7 | mutation 异常未捕获：4 个 handler 均 `void` 调用、无错误反馈，重发弹层 `onConfirmError` 未接 | :234-405、:1216-1233 |
| P2-8 | 切换 Tab 整页重挂载：丢结果横幅、滚回顶部，超 staleTime 后闪骨架 | page.tsx:33、:213-215 |
| P2-9 | 无查询能力的订单（no-query seed）告警文案误导：「尚未查询。主按钮仅『查询原结果』…」但此时主按钮禁用、实际路径是「转接口错误中心」 | :633-653 |
| P2-10 | 跳过弹层为手写 div 模态：无 dialog 语义、无焦点陷阱、无 Esc 关闭；「先跳过 / 本轮跳过 / 已跳过本轮」三套说法并存 | :565-572、:1239、:181、:1235-1303 |
| P2-11 | 「安全重放」按钮 vs 弹层与结果「安全重发」用词不一致 | :562 vs :1219、:332 |
| P2-12 | 已跳过（held）后「先跳过」按钮仍可用且无感知，可无限重复跳过 | :565-572 |
| P2-13 | 履约/取消/退款三轨状态在 Header 与 StatusTrackSummary 重复渲染，首屏冗余 | :503-518 vs :601-622 |
| P2-14 | 状态历史为空时无空态文案；动作审计表空时亦无空态 | :943-964、:1145-1185 |
| P2-15 | 跳过失败语义错位：API `failed` 被映射为「已阻断」 | :180 |
| P2-16 | 返回列表固定跳 `?view=actionable`，从「全部/最近完成」进入的用户回退丢失原视图 | :480 |

### 预览面板（supplier-order-preview-panel.tsx）附注

- 半屏预览的信息组织良好（支付告警 → 三轨 → 身份来源 → 异常/下一步 → 商品摘要 → 最近动作）。
- 「最近动作」区块展示「任务号尾部 · 尝试 N」可接受，但与中心页"任务号尾号"措辞不一致（P2-6）；「结果未知」提示与中心页一致，路径引导正确。
- 无独立发现的问题条目。

## 六、改进建议

1. **术语清零（对应 P1-1/2/3、P2-1~6）**：① 关联任务卡改为业务表达——「任务类型（如：接口结果待确认）+ 关联订单号」，彻底移除 `workItemId`/`workItemType`；② `workItemStatus`/`fulfillmentStatus`/`cancelStatus`/`refundStatus`/`leaseDisposition` 全部走中文映射（`types.ts` 已有 `*_LABEL` 表，`workItemStatus` 需补 `PENDING→待处理、IN_PROGRESS→处理中、COMPLETED→已完成、TRANSFERRED→已转交`）；③ 概览区砍掉连接码/环境/支付记录键/锁版本，保留业务口径；④ SV-/PV- 版本可保留但加「数据版本」前缀或去掉代号。
2. **售后动作补确认层（P1-4）**：复用 `FormalActionConfirmDialog`，确认文案写明「将向供应商提交取消/退款，引用售后请求 MALL-AS-xxx，重复提交返回原结果」；禁用态改用 `GuardedBusinessAction` 展示原因（含 `actionBlockers` 信息）。
3. **错误态区分（P1-5）**：query error 走 `BusinessFailureState(kind="system")` + 重试按钮；仅 404 走「未找到订单」；各 mutation 增加 catch，失败时在弹层内展示错误。
4. **补核对信息（P1-6）**：商品明细表尾加「合计金额（含税）/ 合计数量」行；有数量差异时以对比列或 warning 徽章突出；成本 Tab 的「成本差额」给参照口径（如 vs 下单成本合计）。
5. **Tab 切换去掉整页重挂载（P2-8）**：`key` 不含 section，改为组件内部维护 activeSection（URL 同步仍保留），避免丢横幅、回顶与骨架闪烁。
6. **跳过弹层 a11y 与词统一（P2-10/11/12）**：换 AlertDialog 语义；「安全重发」「先跳过/本轮跳过/已跳过本轮」统一口径；held 后按钮改为「本轮已跳过（可查看记录）」禁用态或给出明确提示。
7. **去重状态展示（P2-13）**：`StatusTrackSummary` 与 `DocumentHeader.statuses` 二选一，首屏让位给风险告警与动作区。

---

**结论**：页面骨架与业务编排成熟，主流程（结果未知 → 查询 → 安全重发）是整期最佳实践样板；短板集中在术语契约执行（6 处 ID/枚举泄漏）、售后动作防护缺失、核对金额缺失与错误态误报。建议以 P1-1~3（术语清零）、P1-4（售后确认层）、P1-5（错误态）为第一优先级。
