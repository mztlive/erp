# UX 评审：/supplier-api/settlements/[statementId]（供应商结算单详情）

> 评审日期：2026-08-05
> 评审对象：`erp-client/features/supplier-settlements/supplier-settlements-page.tsx`（`SettlementCenter` 详情分支 + `DifferencesWorkspace`）及其依赖的 `queries.ts` / `url-state.ts` / `api.ts` / `mock/supplier-settlements.ts` 与 `components/business` 共享组件（`document.tsx` / `feedback.tsx` / `workflow.tsx` / `values.tsx` / `audit-import.tsx` / `role-demo-bar.tsx` / `option-combobox.tsx`）
> 评审口径：`docs/ui-glossary.md`（术语表 v1.2）
> 结论：整体架构成熟（详情深链、跨页返回、动作门禁、不可逆确认层齐全），主要问题集中在「实现词/枚举原值/内部 ID 上屏」的文案合规与少量交互细节。P0=0 / P1=9 / P2=20。

---

## 1. 页面概述

该页为「API 供应商结算」详情中心，由列表页共用组件按 URL 路径 `[statementId]` 分支进入（`supplier-settlements-page.tsx:171-236`）。详情页结构：

- 页头：面包屑（供应商 API / API 结算 / 结算单号）+ 返回列表 + 跨页进入横幅（returnTo）
- `DocumentHeader`：供应商·期间标题、单号、状态徽章、版本、经办/复核/记录时间、动作区（刷新试算 / 提交复核 / 确认结算 / 驳回）
- 动作门禁 Alert + FormalActionResult 结果面板（成功后聚焦）
- 金额摘要卡（订单/运费/服务费/退款 → ERP / 供应商账单 / 差异 / 成本差额）
- 来源数据横幅（更新时间、外部账单、仅供参考）
- 六个 Tab：概览 / 结算明细 / 差异处理 / 复核记录 / 应付与票款 / 审计
- 差异处理工作台：左侧差异列表 + 右侧单条差异详情（ERP 侧 vs 供应商侧、字段级差异、证据、结论、登记结论/追加证据按钮）
- 四个操作对话框：登记结论、追加证据、提交复核（FormalActionConfirmDialog）、确认结算（不可逆确认）、驳回

流程闭环：经办刷新试算 → 处理差异 → 提交复核 → 复核人确认/驳回 → 应付与票款展示。差异处理正确聚焦在「受控结论 + 采购证据」的业务模型上，整体信息架构清晰、岗位职责文案化表达到位。

---

## 2. 易用性

**做得好的：**

- **深链可用且有反馈**：直接访问 `/supplier-api/settlements/[statementId]` 会以 URL 中的 statementId 加载详情（`supplier-settlements-page.tsx:171-180`）；`section` / `role` / `returnTo` 均可在 URL 中保持，刷新不丢上下文。
- **跨页进入有明确指引**：`CrossEntryBanner`（`supplier-settlements-page.tsx:239-252`）说明来源预填与「返回来源」链接，避免深链迷失。
- **返回列表保留筛选**：`onBack` 通过 `replaceUrl` 回到列表并保留 view/supplier/status 等过滤条件（`supplier-settlements-page.tsx:216-222`）。
- **差异处理入口多处直达**：概览「打开差异处理」、预览面板「打开差异处理」都直接切到 `section=differences`。
- **岗位职责一目了然**：概览「岗位与权限」卡（1757-1763）与 RoleDemoBar 提示（268-279）用业务语言说明谁可做什么。
- **结果反馈有焦点管理**：操作成功/未知后 `resultRef` 聚焦结果面板（1188-1192）。

**易用性缺口：**

1. **statementId 未命中时反馈失真**：`fetchSettlementDetail` 对不存在的 ID 返回 `null`，UI 一律渲染「结算单加载失败」+「重试」+「返回列表」（1226-1244）。用户从分享链接进入一个已被作废/删除的单据时，看到的是"系统失败"而非"单据不存在"，且「重试」永远无效，形成死胡同。
2. **加载期无单号反馈**：详情加载骨架（1216-1224）不显示正在加载哪张结算单，深链落地瞬间用户无从核对。
3. **复核任务「待领取」无领取入口**：复核记录 Tab 显示「待领取」（1896-1906），`api.ts` 也下发了 `CLAIM_REVIEW` 动作（api.ts:281），但页面没有任何领取按钮，文案与可操作性脱节。
4. **无全局的 statementId 展示**：statementId（`st_*`）不出现在界面上（正确），但详情页也没有任何「由深链进入」的提示性文案，首屏只有面包屑中的结算单号，反馈偏弱（P2 级）。

---

## 3. 信息密度

**平衡点较好：**

- 金额摘要卡 8 行（订单/运费/服务费/退款 + ERP/账单/差异/成本差额）用纵向分行的 `DocumentTotals`，每行带「含税」徽章与方向提示，结构清晰不挤。
- 结算明细 11 列表格（1778-1792）信息完整（单号可跳采购单/订单、记录、四类金额、ERP、账单行），横向滚动承载高密度数据是合理选择。
- 差异工作台左右分栏（2232-2391）：左侧 16rem 列表做导航，右侧单条详情含证据与结论，避免一屏堆叠。

**密度问题：**

1. **差异金额不突出（评审重点）**：整页没有任何金额被视觉强调——差异金额行（1642-1655）与普通行同字号同字重，方向仅以小号 warning 文字提示；差异侧栏列表项（2238-2254）甚至不含金额。对于"对账结算"页面，差异金额是全页最高价值信息，应该用强调色/加粗/更大字号突出，目前需要用户逐行阅读才能定位。
2. **差异列表缺金额**：侧栏仅显示类型+状态+待举证/阻断标记，用户在列表与详情间来回跳转才能看到每条差异的金额（P2）。
3. **结算明细缺「数量」列**：类型定义中有 `quantity`（types.ts:180）但表格未渲染（1778-1792），对账时无法按「品名+数量+金额」核对，术语表 §7 也明确内部 ID 要换成"品名+数量+单号"的组合。
4. **概览卡偏疏**：概览 Tab 两张卡（1710-1766）信息量较少（状态、未决差异数、角色说明），其中「岗位与权限」卡内容在 RoleDemoBar 已重复表达一次，可考虑合并或放入更多可操作摘要（如待处理差异清单）。

---

## 4. 交互合理性

**做得好的：**

- **不可逆操作防护完善**：确认结算使用 `FormalActionConfirmDialog`（2133-2159），含状态变化、锁定字段、影响清单、「无法自动撤回的影响」红色清单与下一责任部门；按钮在 `irreversibleEffects` 存在时自动变 destructive（workflow.tsx:318-321）。
- **驳回原因必填**：无原因码时「确认驳回」禁用（2190-2196）。
- **动作门禁双保险**：阻断时按钮变为「提交复核（已阻断）」disabled + 门禁 Alert 说明原因（1468-1491, 1506-1532）。
- **岗位错配静默防护**：经办人视角看不到「确认结算」按钮（allowed 集合驱动），采购仅看到「追加采购证据」。
- **加载/空/错误三态齐全**：骨架、BusinessFailureState、差异空态（BusinessEmptyState）均有。

**交互缺口：**

1. **登记结论无不可逆提示**：差异结论一经登记即改变差异状态、成本差额预览并写入审计（api.ts:1049-1094），页面没有任何"结论不可撤回"的提示；与「确认结算」的严谨度形成落差（登记结论对话框 2003-2063 仅为普通 Dialog）。
2. **「d」快捷键全局拦截且无提示**：window 级 keydown 拦截（1195-1214）注释说"when center focused"，实现却是全窗口生效；页面上没有任何快捷键提示（对比列表页在预览面板中有键盘提示 1058-1060）。用户在操作组合框时按 d 可能意外切走 Tab。
3. **demoFlag 在详情页是无效控件**：详情页渲染了权限演示下拉（1402-1412），但 `useSettlementDetailQuery` 只消费 `statementId + role`（queries.ts:33-43），切换到「无模块权限/无数据范围/期间策略缺失」时页面毫无变化，控件形同虚设，违反「URL 参数与界面控件一一对应」契约。
4. **角色切换整页骨架闪烁**：role 在 queryKey 中（queries.ts:22-23），切换角色触发新查询，整页退回骨架（1216-1224），体验断层；可改为保留旧内容 + 顶部刷新指示（`AsyncSectionState` 模式）。
5. **mutation 异常无兜底**：`onResolve`/`onEvidence`/`onRefresh` 为裸 `void mutateAsync`（1259-1305），拒绝后成为未处理 Promise；`FormalActionConfirmDialog` 的 `onConfirmError` 未传入（2166-2159），异常被静默吞掉且对话框保持打开（workflow.tsx:245-255）。mock 不抛错掩盖了该问题，接真实后端后将成为静默失败。
6. **阻断按钮 title 提示不可靠**：disabled 按钮的 `title`（1469, 1486）在部分浏览器/辅助技术上不展示，项目已有 `GuardedBusinessAction`（feedback.tsx:88-148）专门解决此问题，此处未复用。
7. **选中差异无 URL 锚定**：`activeDiffId` 是纯本地 state（1171），刷新或分享差异页 URL 后回落到第一条差异（1251-1254），无法深链到具体差异。
8. **确认对话框 fromStatus 硬编码**：fromStatus 固定「待复核」（2140），若状态已流转（如被并发驳回）则对话框状态图失真，应使用实际 `st.statusLabel`。
9. **提交对话框 lockedFields 内容重复**：`["来源数据版本已锁定", "数据版本已锁定"]` 语义重复（2110-2114），列表项含糊。

---

## 5. 问题清单（按严重度）

### P0（阻断操作）：0 个

未发现阻断性操作缺陷。

### P1（明显阻碍效率 / 明显违反术语表）：9 个

| # | 位置 | 问题 |
|---|---|---|
| P1-1 | `supplier-settlements-page.tsx:2121-2131` | 「提交复核」对话框中的「说明（可选）」Textarea 被包裹在 `sr-only` 中：用户看不见、无从填写，却仍随提交上报 `reviewComment`。隐藏的不可见可提交字段，属于明显功能缺陷。 |
| P1-2 | `supplier-settlements-page.tsx:1525` | 动作门禁 Alert 以 `font-mono` 直接渲染错误码原值（`SOD_VIOLATION` / `BLOCKING_DIFFERENCES`），违反术语表「枚举原值禁止上屏」（规则 7 / §2 第 5 轮 P0）。 |
| P1-3 | `supplier-settlements-page.tsx:1986-1994` + `api.ts:855-857, 874-877` | 「审计」Tab 直接渲染原始动作枚举（`CREATE_DRAFT` / `REFRESH_TRIAL` / `RESOLVE_DIFFERENCE` 等）与内部哈希值（`sourceSnapshotHash=ssh_jd_202607_a4f2`）及 `AUD-W27-*` 工作面编号（术语表 §3.6 禁止 W 编号）。 |
| P1-4 | `supplier-settlements-page.tsx:1921` | 「复核记录」直接渲染 `r.reasonCode` 原值（如 `NEEDS_MORE_EVIDENCE`），未用中文映射（mock 的 `reasonLabel` 已存在但未使用）。 |
| P1-5 | `api.ts:776-780, 872-877, 1109, 1205-1208` + `supplier-settlements-page.tsx:114-153` | 操作结果面板 `facts`/`reference` 泄漏内部 ID 与字段名：`sourceSnapshotHash`（label 原样）、`ssh_*` / `sh_*` / `wi_*` / `req_*` / `op_*` 前缀值作为「结果编号」「请求编号」「数据版本」展示（`outcomeToResult` 的 `reference` 直接上屏）。违反术语表内部 ID 禁止规则（§2 / §7）。 |
| P1-6 | `api.ts:1200` | 提交复核成功消息「已冻结提交版本并创建 SUPPLIER_SETTLEMENT_REVIEW 待办。」直接出现 `workItemType` 枚举原值，应改为「已创建复核待办」。 |
| P1-7 | `supplier-settlements-page.tsx:1723` | 概览渲染「锁版本 {st.lockVersion}」，`lockVersion` 实现词泄漏（术语表 §2 P1：锁版本 → 数据版本/已更新文案）；页头已有「版本 4」展示，此处冗余且违规。 |
| P1-8 | `supplier-settlements-page.tsx:2226` | 差异空态「可直接进入复核（在明细守恒满足时）」，「守恒」为系统不变量实现词，业务用户无法理解（应改为「明细金额核对一致时」或删除）。 |
| P1-9 | `supplier-settlements-page.tsx:1177-1179` | 「登记差异处理结论」对话框默认值自相矛盾：resolution 默认 `ERP_ACCEPTED`（ERP 认可=接受供应商账单），reasonCode 默认 `BILL_ALIGNED`（账单已对齐=账单与 ERP 一致）。两者语义互斥，经办直接点「提交结论」会登记出前后矛盾的结论，构成误操作风险。 |

### P2（体验瑕疵）：20 个

| # | 位置 | 问题 |
|---|---|---|
| P2-1 | `supplier-settlements-page.tsx:1226-1244` | statementId 不存在的深链被渲染成「结算单加载失败」且「重试」无效，应区分「单据不存在」并提供回列表/换链接指引。 |
| P2-2 | `supplier-settlements-page.tsx:1216-1224` | 详情加载骨架不含单号/供应商信息，深链落地无核对锚点。 |
| P2-3 | `queries.ts:22-23` + `supplier-settlements-page.tsx:1216-1224` | 切换演示角色即整页骨架闪烁，应保留旧内容做局部刷新。 |
| P2-4 | `supplier-settlements-page.tsx:1642-1655` | 差异金额（及方向）无任何视觉强调（颜色/字重/字号），与普通行同质化，对账页最高价值信息不突出。 |
| P2-5 | `supplier-settlements-page.tsx:2237-2254` | 差异列表侧栏项不含金额，需要往返主区域才能对照。 |
| P2-6 | `supplier-settlements-page.tsx:2268-2271` | 差异详情副标题「差异额 2000.00（含税）」直接拼原始字符串，未走 `MoneyValue` 格式化（应为 ¥2,000.00），与全页金额风格不一致。 |
| P2-7 | `supplier-settlements-page.tsx:1778-1792` | 结算明细表缺「数量」列（类型已含 `quantity`），无法按「品名+数量+金额」对账。 |
| P2-8 | `supplier-settlements-page.tsx:1195-1214` | 「d」快捷键无任何界面提示、全局生效，可误触切换 Tab。 |
| P2-9 | `supplier-settlements-page.tsx:1402-1412` + `queries.ts:33-43` | 详情页权限演示下拉（demoFlag）不参与详情查询，切换无任何效果，是无效控件。 |
| P2-10 | `supplier-settlements-page.tsx:1896-1906` + `api.ts:281` | 「复核任务 · 待领取」无领取按钮，`CLAIM_REVIEW` 动作无 UI 承载。 |
| P2-11 | `supplier-settlements-page.tsx:2110-2114` | 提交复核确认框 lockedFields 两条「…版本已锁定」语义重复。 |
| P2-12 | `supplier-settlements-page.tsx:1566` | 金额摘要卡描述「订单/运费/服务费/退款 + ERP vs 供应商 + 差异方向」类图表符号语法，非业务口语，且出现英文 vs。 |
| P2-13 | `supplier-settlements-page.tsx:1638` + `api.ts:843-848` | 「账单未同步（不可用 ERP 代填）」与 mock 实际行为矛盾——点击「刷新试算」后系统会自动以 ERP 金额代填账单（`o.supplierAmountGross = seed.erpAmountGross`）。 |
| P2-14 | `supplier-settlements-page.tsx:1685-1688` | 「账单 JD-BILL-202607@v2」的 `@版本` 拼写偏技术化；「不参与正式结算」中「正式」前缀按术语表应删（改「不进入结算结果」）。 |
| P2-15 | `supplier-settlements-page.tsx:1383` | 面包屑「API 结算」与页头「API 供应商结算」命名不一致。 |
| P2-16 | `supplier-settlements-page.tsx:1469, 1486` | 「提交复核（已阻断）」「确认结算（已阻断）」用 disabled + title 提示原因，应复用 `GuardedBusinessAction`（Tooltip + 可聚焦说明）。 |
| P2-17 | `supplier-settlements-page.tsx:1259-1305, 2054-2096` + `workflow.tsx:245-255` | 刷新试算/登记结论/保存证据的 mutation 异常无兜底：裸 `void mutateAsync`、`onConfirmError` 未传入，真实失败会静默无提示。 |
| P2-18 | `supplier-settlements-page.tsx:1171, 1251-1254` | 选中差异无 URL 锚定，刷新/分享后丢失上下文（回到第一条）。 |
| P2-19 | `supplier-settlements-page.tsx:2140` | 确认结算对话框 fromStatus 硬编码「待复核」，未使用实际状态。 |
| P2-20 | `supplier-settlements-page.tsx:2003-2063` | 「登记差异处理结论」为不可逆追加操作，但对话框无影响/不可撤回清单，防护层级低于「确认结算」。 |

---

## 6. 改进建议

**文案合规（优先，P1-2 ~ P1-8 同类问题一并修）：**

1. 建立本页「枚举/ID 上屏映射表」：动作门禁 code（`SOD_VIOLATION`→岗位冲突、`BLOCKING_DIFFERENCES`→未决阻断差异…）、审计动作、复核 reasonCode 全部走中文映射；`reference` 字段改用业务可读编号（结算单号/应付编号），不再透出 `wi_*`/`op_*`/`req_*`。
2. `api.ts` 审计 summary/facts 中删除 `ssh_*`/`sh_*` 哈希值，统一为「数据版本 v{n}」；`AUD-W27-*` 改为业务流水号或删除 W 编号。
3. 「锁版本」→「数据版本」；「明细守恒」→「明细金额核对一致」；「SUPPLIER_SETTLEMENT_REVIEW 待办」→「复核待办」；「不参与正式结算」→「不进入结算结果」。

**交互修复（P1-1 / P1-9 优先）：**

4. 提交复核的「说明（可选）」移出 `sr-only`，作为确认框内可见字段（FormalActionConfirmDialog 增加可编辑区）或直接删除。
5. 登记结论对话框：默认值改为语义一致的组合（如 `ERP_ACCEPTED` + `ACCEPT_BILL`），或默认置空强制选择；并补「结论不可撤回」影响提示（对齐确认结算的防护层级）。
6. 详情加载失败态区分「单据不存在（404）」与「系统错误」，404 时隐藏无效「重试」。
7. 详情页隐藏对数据无影响的 demoFlag 控件，或将 demoFlag 纳入详情查询语义。
8. 「d」快捷键加界面提示或改为显式按钮/键盘说明；复核任务增加「领取任务」按钮（服务端已预留 `CLAIM_REVIEW`）。

**信息密度：**

9. 差异金额统一视觉强调（首选项：差异行红色/琥珀色加粗 + 方向箭头），差异侧栏追加金额；结算明细补「数量」列。
10. 选中差异支持 URL 参数锚定（如 `?diff=df_xxx` 配合映射展示），刷新不丢选中项。
