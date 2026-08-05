# UX 评审：/supplier-api/settlements（供应商结算单列表）

> 评审日期：2026-08-05
> 评审对象：`erp-client/features/supplier-settlements/supplier-settlements-page.tsx`（`SettlementList` 列表分支 + 预览抽屉 + 新建草稿对话框 + 列表↔详情联动）、`queries.ts` / `api.ts` / `url-state.ts` / `lib/url-state.ts`，以及跨页入口（`mock/integration-errors.ts` / `features/supplier-payables/supplier-accounts-page.tsx` / `features/purchase-orders/purchase-order-detail-page.tsx`）
> 评审口径：`docs/ui-glossary.md`（术语表 v1.2）+ `erp-client/AGENTS.md`（URL 参数与控件一一对应、枚举/内部 ID 禁止上屏）
> 关联评审：详情中心（`SettlementCenter` / `DifferencesWorkspace`）的问题已单列于 `docs/ux-review/supplier-api-settlement-detail.md`，本报告不重复，仅覆盖列表页与列表↔详情联动。
> 结论：列表页整体完成度高——搜索/筛选/预览/空态/分页闭环齐全，金额与状态呈现突出。主要问题集中在「隐形筛选状态无控件」「筛选语义组合矛盾」与「筛选摘要泄漏枚举原值/跨页深链失效」。P0=0 / P1=4 / P2=11。

---

## 1. 页面概述

`/supplier-api/settlements` 由 `SupplierSettlementsPage` 按 URL 状态分发：无 `statementId` 时渲染 `SettlementList`（`supplier-settlements-page.tsx:227-236`）。列表页结构：

- 页头：标题「API 供应商结算」+ `DataFreshness`（结算数据更新时间）+ 刷新 / 新建结算草稿（权限受控）
- 跨页进入横幅（`returnTo` 预填供应商并回链来源页）
- `RoleDemoBar`（演示角色 + 权限/策略演示 flag）
- 期间策略未配置告警（`UNCONFIGURED`）
- 指标条（待对账 / 有差异 / 待复核 / 本期已确认金额，可点击过滤）
- 结算单列表框架：搜索框（Enter/「/」快捷键聚焦）、视图 Tab（待处理/我经办/我复核/已确认）、供应商 / 状态 / 差异类型筛选、共 N 条
- 9 列表格：结算单号（含期间小字）、供应商、期间、ERP 金额、账单金额、差异（含方向与未决徽章）、状态、经办/复核、操作（预览/打开）；首列与末列钉住
- 右侧预览抽屉（金额摘要 + 查看详情 / 打开差异处理）、新建草稿对话框
- 空态四分类（无模块权限 / 无数据范围 / 筛选无结果 / 无单据），错误态与加载骨架齐备

主流程：搜索/筛选 → 行点击预览或打开详情 → 差异处理 → 返回列表（URL 保留筛选）。列表↔详情以 query 形式（`?statementId=…`）跳转时筛选条件全程保留（`replaceUrl` 双分支设计，`supplier-settlements-page.tsx:182-200`），闭环顺畅。

---

## 2. 易用性

**做得好的：**

- **查找路径通畅**：搜索支持回车提交 + 「应用」按钮 + 「/」快捷键聚焦（`supplier-settlements-page.tsx:332-353, 758-769`），placeholder「结算单号、外部账单号、供应商」覆盖三种查法。
- **查看对账状态快**：指标条即筛选入口（693-742），「有差异/待复核」一键定位；行点击即开预览抽屉（`data-table.tsx:795-798`），预览内可直接「查看详情」「打开差异处理」（1035-1056），Enter 直达详情。
- **进入详情路径完整**：操作列「预览/打开」双按钮 + 行点击 + Enter，预览抽屉关闭时清 URL 参数（967-971），状态无残留。
- **返回列表保留筛选**：query 形式跳详情后返回，view/supplier/status/q/page 全部保留（174-200 的 `buildSettlementsSearchParams` 全量序列化）。
- **空态可自救**：筛选无结果时「清除筛选」一次性还原默认待处理视图并清掉全部参数含 `periodFrom/periodTo`（899-924）。
- **跨页进入有交代**：`CrossEntryBanner` 说明来源预填并提供「返回来源」（239-252, 643）。

**易用性缺口：**

1. **期间筛选是「隐形状态」**：`periodFrom/periodTo` 在 URL 状态与列表查询中生效（`url-state.ts:51-52`，`supplier-settlements-page.tsx:305-317`，`api.ts:545-550`），但列表页没有任何期间控件，用户无法主动设置，只能被动承受深链带入的期间过滤；且 `api.ts` 的 `filterSummary` 不包含期间部分（605-615），期间过滤生效时界面无任何体现。违反 AGENTS.md「URL 参数与界面控件一一对应」契约。
2. **视图 Tab 与指标卡清理行为不一致**：点击指标「有差异」（`status=HAS_DIFFERENCE`）后再点 Tab「已确认」（`view=confirmed`），Tab 不清 `status`（774-793），形成矛盾组合「视图=已确认 + 状态=有差异」→ 必空列表，用户会怀疑数据丢失。
3. **「应用」按钮语义含糊**：所有筛选器（供应商/状态/差异类型）都是即时生效，唯独「应用」按钮只提交搜索框草稿（862-874），与筛选器并列摆放极易被当作「提交全部筛选」。

---

## 3. 信息密度

**平衡点较好：**

- 金额三列（ERP 金额 / 账单金额 / 差异）右对齐 + `MoneyValue` 千分位，差异列附方向小字与「未决 N」徽章（397-462）——对账页最高价值信息（金额、差异）得到了突出，是加分项。
- 状态列用 `BusinessStatusBadge` 中文徽章（463-475），颜色 + 文字双编码，不靠色盲依赖。
- 9 列 + compact 密度 + 首列/操作列钉住（295-298, 955-959），横向滚动可承载，密度可控。

**密度问题：**

1. **期间信息重复**：结算单号列下已渲染 `periodLabel` 小字（366-373），下一列又是完整「期间」`periodStart ~ periodEnd`（385-395），两列传达同一信息，可合并为单列或删除单号下的期间小字。
2. **指标口径与列表无法互相印证**：「待对账」指标只计 `DRAFT + PENDING_RECONCILE`（`api.ts:511-518`），而「待处理」Tab 还含 `HAS_DIFFERENCE/PENDING_REVIEW`（521-528），指标数之和与 Tab 条数对不上，用户核对数字时会产生困惑。
3. **「本期已确认金额」口径不实**：`api.ts:506-508` 对全部 `CONFIRMED` 种子累计，跨期间数据下并非「本期」金额，标签夸大口径（页 730-741）。
4. **行内时间缺失**：类型已含 `updatedAt`（`types.ts:132`）但列表未渲染；顶栏有全局 `DataFreshness`，尚可接受，但行级「最后更新时间」对多期间对账有帮助（P2 级）。

---

## 4. 交互合理性

**做得好的：**

- **加载/空/错误三态齐全**：整页骨架（578-586）、`BusinessFailureState` + 重试（588-604）、空态四分类（885-943）；NO_PERMISSION/NO_SCOPE 空态文案诚实说明演示语义（887-898）。
- **创建草稿链路防护完整**：按钮受 `canCreate`（角色 + 策略 + 权限）门控（521-525），对话框内策略版本过期/期间非法均被拦截并给结果面板（538-575, 1075-1082），成功即自动打开新单据详情。
- **分页与 URL 同步**：`page` 参数双向同步（320-330, 951-954），刷新/后退不丢页。
- **权限不足空态不伪装**：无权限/无范围分别渲染，不以 0 条伪饰（887-898）。

**交互缺口：**

1. **错误态文案与行为不符**：列表错误态描述「请重试。已有数据时保留旧列表。」（595）——TanStack Query 下 `isError` 只在无数据时成立，有旧数据时根本不会渲染该态，文案描述的「保留旧列表」场景不适用于此界面。
2. **刷新中新鲜度语义颠倒**：`DataFreshness` 的 `state` 在 `isFetching` 时传 `stale`（613-618），刷新进行中显示「数据可能过期」，刷新完成反而显示「数据已更新」——语义完全相反，应传 `syncing`。
3. **角色/演示 flag 切换整页骨架闪烁**：`role`/`demoFlag` 在列表 queryKey 中（`queries.ts:20-21`），切换即整页退回骨架（578-586），列表页高频的演示切换体验断层。
4. **新建按钮禁用无原因**：非经办角色时「新建结算草稿」disabled（634-637）无 tooltip 说明，只能靠 RoleDemoBar 的角色提示间接推断。
5. **预览行失效无引导**：URL `preview` 指向不在当前页数据中的行时只显示「未找到预览行」（1062-1064），无关闭提示或跳转详情入口。

---

## 5. 问题清单（按严重度）

### P0（阻断操作）：0 个

未发现阻断性操作缺陷。

### P1（明显阻碍效率 / 明显违反术语表与页面契约）：4 个

| # | 位置 | 问题 |
|---|---|---|
| P1-1 | `features/supplier-settlements/url-state.ts:51-52` + `supplier-settlements-page.tsx:305-317` + `api.ts:545-550` | `periodFrom/periodTo` 是「隐形筛选状态」：被查询消费、可被深链带入（含别名 `period`），但列表页无任何控件可设置、无摘要提示（`filterSummary` 不含期间），用户无法感知和解除（仅空态清除按钮兜底）。违反 AGENTS.md「URL 参数与界面控件一一对应」。 |
| P1-2 | `supplier-settlements-page.tsx:774-793` vs `705-741` | 视图 Tab 切换不清除 `status`/`differenceType`，与指标卡（切视图同时清 status）行为不一致。典型路径：点指标「有差异」→ 点 Tab「已确认」→ 空列表 + 矛盾筛选摘要「视图=confirmed · 状态=HAS_DIFFERENCE」，用户误以为数据丢失。 |
| P1-3 | `api.ts:605-613` → `supplier-settlements-page.tsx:747, 903` | 筛选摘要直接拼原始枚举：`视图=prepared_by_me`、`状态=HAS_DIFFERENCE` 上屏，违反术语表「枚举原值禁止上屏」（规则 7）。应映射为「我经办」「有差异」。 |
| P1-4 | `mock/integration-errors.ts:686-689` + `lib/url-state.ts:177-184` | 跨页深链 `href: "/supplier-api/settlements?jobId=st_2026_07_jd"` 的 `jobId` 未在结算 codec 声明，解析时被静默忽略，用户点击直达链接落到**默认列表**而非目标结算单；且链接 label「W27 API 结算」含工作面编号（术语表 §3.6 禁止）。应改为 `?statementId=st_2026_07_jd` 或 `?q=<结算单号>`。 |

### P2（体验瑕疵）：11 个

| # | 位置 | 问题 |
|---|---|---|
| P2-1 | `supplier-settlements-page.tsx:862-874` | 「应用」按钮只提交搜索草稿，与即时生效的筛选器并列，语义含糊、易误导（建议改为仅搜索区按钮或删除）。 |
| P2-2 | `supplier-settlements-page.tsx:634-637` | 「新建结算草稿」disabled 无原因提示；应复用 `GuardedBusinessAction` 式 tooltip 说明（如「仅财务经办可新建」）。 |
| P2-3 | `supplier-settlements-page.tsx:366-394` | 期间信息重复：单号列下 `periodLabel` 与独立「期间」列重复传达。 |
| P2-4 | `api.ts:506-508` + `supplier-settlements-page.tsx:730-741` | 「本期已确认金额」按全部 CONFIRMED 累计，「本期」口径不实，跨期数据下夸大/误导。 |
| P2-5 | `api.ts:511-518` vs `521-528` | 「待对账」指标（DRAFT+PENDING_RECONCILE）与「待处理」Tab（含 HAS_DIFFERENCE/PENDING_REVIEW）口径不一致，指标与列表数字无法印证。 |
| P2-6 | `supplier-settlements-page.tsx:595` | 错误态描述「已有数据时保留旧列表」与实际行为不符（isError 仅在无数据时渲染），文案应改为「请重试」。 |
| P2-7 | `supplier-settlements-page.tsx:613-618` | `DataFreshness` 在刷新中（isFetching）传 `stale` 显示「数据可能过期」，语义相反；应传 `syncing`（「正在同步」）。 |
| P2-8 | `queries.ts:20-21` + `supplier-settlements-page.tsx:578-586` | 切换演示角色/flag 整页退回骨架闪烁，应保留旧列表做局部刷新指示。 |
| P2-9 | `supplier-settlements-page.tsx:1124` | 新建草稿对话框泄漏内部策略 ID：「策略 `spp_api_default@3` · Asia/Shanghai」，应改为「结算期间策略：2026-07 等自然月」类业务描述。 |
| P2-10 | `supplier-settlements-page.tsx:1062-1064` | 预览行失效（URL preview 指向当前页外数据）仅显示「未找到预览行」，无关闭/跳转引导。 |
| P2-11 | `supplier-settlements-page.tsx:692` | `MetricStrip aria-label="结算数据更新"` 与内容（四个筛选按钮 + 金额）不符，应改为「结算快捷筛选」。 |

---

## 6. 改进建议

**P1 优先：**

1. **期间筛选补控件**：列表筛选区增加期间区间选择（或复用 `periodFrom/periodTo` 现有 URL 状态），使隐形状态可见可清；`filterSummary` 同步加入期间口径。
2. **统一视图切换语义**：Tab 切换时清 `status`/`differenceType`（对齐指标卡行为），或反向——保留组合但将空态摘要改为业务中文并提示「可清除筛选」。
3. **筛选摘要全部走中文映射**：建立 `viewLabel(status)` 映射（我经办/我复核/已确认 + 状态中文），杜绝 `视图=`/`状态=` 原始枚举。
4. **修复跨页深链**：`mock/integration-errors.ts` 的 repairLink 改传 `statementId`（或 `q` 传结算单号），label 去掉 W 编号；建议在 codec 层对未声明参数做告警或透传，避免静默丢弃。

**P2 顺手修：**

5. 「应用」按钮并入搜索区或删除；新建按钮禁用加 tooltip；单号列下的期间小字与「期间」列二选一。
6. `DataFreshness` 刷新中传 `syncing`；错误态文案改「请重试」；指标口径与 Tab 口径对齐（统一为同一状态分组）或在指标下注明口径。
7. 新建对话框策略信息改业务文案（期间列表本身已足够），删除 `policyId@version · timezone`。
8. 角色切换时保留旧列表（`placeholderData` 或骨架仅限表格区），避免整页闪烁。
