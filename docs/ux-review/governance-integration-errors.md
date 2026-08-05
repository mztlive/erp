# 产品体验评审：接口错误与对账中心（/governance/integration-errors）

- 评审日期：2026-08-05
- 评审角色：产品经理 / UX
- 代码范围：`erp-client/features/integration-errors/integration-errors-page.tsx`（1874 行）、`url-state.ts`、`types.ts`、`queries.ts`、`api.ts`、`integration-error-detail-page.tsx`；共享组件 `components/business/{domain,workflow,feedback,audit-import,page,values,option-combobox}.tsx`；mock 种子 `mock/integration-errors.ts`
- 参考口径：`docs/ui-glossary.md`（基线 v1.2）

---

## 一、页面概述

「接口错误与对账中心」是 W29 工面的双栏工作台：左栏为任务/差异队列（38fr），右栏为当前处理项详情与动作区（62fr），顶部有 5 项业务指标条（结果未知/待人工/安全故障/未解决差异/最长滞留）、视图切换（我的任务/结果未知/安全故障/自动重试/对账差异/已解决）、模式/环境/错误类别/责任人筛选与搜索框。支持逐项处理：查询原结果 → 安全重发/转人工/补证/转交 → 解决/关闭，以及直接对账（无任务）的「确认无误/确认有效差异」终结路径。详情路由 `/errors/:id`、`/differences/:id` 复用同一页面（`integration-error-detail-page.tsx:6-21`）。

总体骨架清晰、状态模型严谨（fail-closed、禁止伪造成功、追加式证据），安全性与风控设计明显优于一般列表页。主要问题集中在：**文案层仍有大量术语表违规（枚举原值/W 编号/内部 ID 上屏）**、**高危动作无确认层**、**若干按钮「有样子没动作」或行为与文案不符**、**筛选/指标间存在口径冗余与失真**。

---

## 二、易用性

1. **定位错误路径基本顺畅**：队列行点击即切换右侧详情并同步 URL（`integration-errors-page.tsx:412-433`），行内另有「详情」链接进入独立详情路由（1151-1196），两种入口并存。搜索支持任务号/业务单号/事件摘要（1051），命中范围含单号、对象、事件摘要（`api.ts:340-353`）。
2. **终端动作前置条件交代清楚**：`actionBlockers` 逐动作列出阻断原因（1603-1614），「结果未知」等类别有红色 Alert 明确「禁止直接重新提交」并说明唯一出路（1312-1321）。
3. **但高危动作缺少确认层**：转交、关闭重复、标记已解决、确认有效差异均为不可逆/高风险动作，点击即直接提交（706-808、810-851、1701-1731），没有任何二次确认或影响预览。其他工面（如 W09 的 `FormalActionConfirmDialog`，`workflow.tsx:210-340`）已有标准确认组件，本页未复用——误点即造成处理结果落库。
4. **「关闭重复」需要业务用户手输内部任务 ID**：替代任务输入框默认值是 `wi_iet_orig_55102`（238、1714-1720）——这是 `wi_*` 内部 ID，业务用户根本无从获取/理解，真实系统中该操作无法由用户独立完成。
5. **连续处理条主按钮无实际功能**：「处理当前」按钮的 `onProcess` 仅执行 `headingRef.current?.focus()`（1234-1236），即点击后只把焦点移到标题上，不触发任何处理动作或滚动到动作区。按钮文案与行为严重不符。
6. **自动领取无感知**：进入/切换任务即静默自动领取（380-405），失败也被静默吞掉（398-400），用户完全不知道任务已被自己认领；多用户场景下存在「看一眼就抢了任务」的风险。
7. **详情加载期误导**：focusMode 下 `detailItemQuery` 未就绪时 `item` 为 undefined，右侧渲染「未选择处理项」空态（1845-1851），而用户实际已选中了一个具体任务。

## 三、信息密度

1. **指标条与队列数量口径不一致**：指标统计作用于全部开放项 `metricsFrom(projected)`（`api.ts:422`），而队列受 view/env/owner 过滤（398）。点击「结果未知/安全故障/未解决差异」指标只切 view/mode，保留环境=production、责任人=me 过滤（886-917），一旦某错误发生在「验证」环境，指标数字与切换后队列数量对不上，用户会产生「数据丢了」的错觉。
2. **「已解决」「自动重试」两个视图常驻但恒空**：`view=resolved` 直接返回空（`api.ts:334-336`），当前种子也没有 network-timeout/rate-limited 项，点击后永远看到「当前筛选项已处理完」（1125-1130）——一个永远为空的入口，纯误导。
3. **右栏详情堆叠过深**：同一屏内依次是 处理条 → 跳过按钮组 → 基本信息卡（8 枚徽章 + Fact 网格）→ 对账差异面板 → 修复链接 → 证据与尝试卡（尝试历史 + 证据时间线 + 处理审计 + 已关联证据 + 证据策略 Alert）→ 接口错误处理面板 → 处理动作卡，共 7 个区块，有效操作按钮分散在页面 2000px 高度内，处理效率受影响。
4. **同类按钮两处重复**：`InterfaceErrorResolutionPanel` 的动作槽（594-704 生成的「查询原结果/重新提交/转交/关联补偿/去修复」）与「处理动作」卡的按钮（1627-1731）内容重复，用户会疑惑哪个是生效的；且两处按钮状态可能不同步（一个受 lease 禁用、另一个只看 can()）。
5. **对账差异面板有无效行**：BusinessDiffPanel 中「边界」「更新时间」两行 before/after 完全相同（1396-1409），名为差异对比实则无差异，稀释了真正的差异信息。
6. **队列行「截止」标签语义错位**：`dueAt` 传入的是 `ageLabel`（1174），WorkTaskItem 在 compact 态将其渲染在「截止」下（`audit-import.tsx:127-130`），实际显示的是「已滞留 3h · 超 SLA」——滞留时长被标注为截止时间，语义误导。
7. **列表状态徽章颜色取自严重度而非状态**：`tone: severityTone(severity)`（1178-1181、1291-1295），「已跳过」「待处理」的徽章颜色取决于严重度，同一种状态可能红也可能灰，状态语义被弱化。

## 四、交互合理性

1. **加载/空/错误状态**：加载有骨架屏（562-573）、查询错误有「加载失败 + 重试」（575-583）、空队列有空态（1125-1130），基础完备；但 focusMode 下详情查询无独立骨架/错误态（见易用性 7），且「刷新当前任务」只 refetch 队列（557-560、1070-1073），在详情路由下 `item` 来自 `detailItemQuery`（191-219），刷新按钮对当前展示内容无效——按钮与行为不一致。
2. **筛选与 URL 参数**：view/mode/environment/errorClass/owner/q/autoNext/taskId/differenceId 全部落 URL 且可回退，符合「控件与参数一一对应」；但**筛选摘要直接拼接枚举原值**：`视图=mine · 模式=all · 环境=production · 类别=result-unknown`（`api.ts:412-418`，渲染于 1077-1080），英文原值上屏。
3. **视图与模式双重筛选重叠**：「对账差异」同时存在于视图切换（VIEW_LABEL）与模式下拉（MODE_LABEL）两个控件（`types.ts:399-412`），用户无法理解二者差异，且存在 view=reconciliation 与 mode=reconciliation 的冗余组合。
4. **恒禁用按钮**：「等待退避（禁止高频重试）」（696-698）是一个永远点不动的按钮，仅以文案告知，占位且用词违规（「退避」为禁用词，glossary §2 P2 应改「请稍后重试」）。
5. **导航被领取状态卡住**：`processDisabled={leaseStatus !== "active"}`（1224）把「下一项」也一并禁用（`workflow.tsx:431-432`），未领取时连只读的向后翻页都做不到，必须先点「重新领取」。
6. **演示/模拟机制泄露到界面**：「下次『查询原结果』模拟仍未知（结果仍未知 · 不自动下一项）」勾选框常驻（1082-1090、501），直接暴露结果是被模拟的，业务用户会看到与自己无关的测试开关。
7. **自动下一项默认开启**（`url-state.ts:76`）：终态成功后 400ms 即跳下一项（462-478），用户几乎看不到结果面板（1092-1116）；对「标记已解决」这类用户希望确认一下的操作，默认跳走容易造成不安。
8. **操作失败直接展示后端原文**：`setActionError(e.message)`（531、723、746、806、849），错误文案未经过业务化包装，可能泄漏内部词。
9. 死代码：`{sessionAutoNext !== autoNext ? null : null}`（1079）恒输出 null。
10. 搜索草稿只在初始化时读取 URL（237），浏览器后退改变 `q` 后输入框不回写，状态不同步。

## 五、问题清单（按严重度）

### P0（阻断操作）：0 个

无完全阻断级问题（演示 mock 下主流程可走通）。

### P1（明显阻碍效率 / 高危风险 / 术语表违规）：10 个

| # | 位置 | 问题 |
| --- | --- | --- |
| P1-1 | `integration-errors-page.tsx:706-808, 1701-1731` | 转交/关闭重复/标记已解决/确认有效差异等不可逆终态动作**无二次确认层**，一键生效。同项目已有 `FormalActionConfirmDialog`（`workflow.tsx:210`）标准组件未复用。误操作直接落库。 |
| P1-2 | `integration-errors-page.tsx:238, 1710-1731` | 「关闭重复」要求用户输入替代任务，默认值即内部 ID `wi_iet_orig_55102`。业务用户无法获取/理解 `wi_*` ID，该操作在实际系统中无法独立完成（glossary §7：内部 ID 不得进界面）。 |
| P1-3 | `integration-errors-page.tsx:1077-1080` + `api.ts:412-418` | 筛选摘要泄漏枚举原值：「视图=mine · 模式=all · 环境=production · 类别=result-unknown」英文上屏，违反 glossary §2 第 5 轮 P0「枚举原值清零」。 |
| P1-4 | `integration-errors-page.tsx:1424-1436` + `mock/integration-errors.ts:125/247/351/687` | 修复跳转按钮文案含工作面编号：「W26 供应商订单」「W17 商城同步映射」「W20 API 连接」「W27 API 结算」，违反 glossary §2 P1「Wxx 面向业务用户的提示必须用中文页名」。 |
| P1-5 | `api.ts:334-336` + `integration-errors-page.tsx:939-944` | 「已解决」「自动重试」两个视图恒空（resolved 直接返回空、种子无 time-out/rate-limited 项），常驻切换栏形成永远为空的入口，空态文案「当前筛选项已处理完」进一步误导。 |
| P1-6 | `integration-errors-page.tsx:1234-1236` | 连续处理条主按钮「处理当前」仅 `headingRef.current?.focus()`，无任何实际处理行为，文案与行为不符（glossary：按钮说动作不说机制）。 |
| P1-7 | `integration-errors-page.tsx:1539-1540` | 证据策略 Alert 直接展示内部策略 ID「pol_result_unknown_potential@v1」，内部 ID 上屏（glossary §4）。 |
| P1-8 | `api.ts:717-721`（渲染于 `feedback.tsx:752-761`） | 操作结果 facts 的 label 使用枚举原值（`COMPENSATION_RESULT` 等 raw kind）、value 为内部 recordId，结果面板泄漏实现层标识。 |
| P1-9 | `integration-errors-page.tsx:1156-1196` | 队列行整体为 `<button>`，内部又嵌套 `<Link>`「详情」——交互元素嵌套（button 内嵌 a），HTML 无效、键盘焦点与事件冲突，a11y 缺陷。 |
| P1-10 | `integration-errors-page.tsx:557-560, 1070-1073` | 详情路由下「刷新当前任务」只 refetch 队列，而当前展示项来自 `detailItemQuery`，点击后界面无任何变化，按钮行为与文案不符。 |

### P2（体验瑕疵 / 口径问题）：20 个

| # | 位置 | 问题 |
| --- | --- | --- |
| P2-1 | `integration-errors-page.tsx:886-917` + `api.ts:398, 422` | 指标统计口径（全部开放项）与队列口径（筛选后）不一致，点击指标保留 env/owner 过滤，数量可能对不上。 |
| P2-2 | `integration-errors-page.tsx:1174` + `audit-import.tsx:127-130` | 队列行「截止」标签显示的是滞留时长（已滞留 3h · 超 SLA），语义错位。 |
| P2-3 | `integration-errors-page.tsx:1224` + `workflow.tsx:431-432` | 未领取时「下一项」等只读导航一并禁用，必须先领取才能翻页。 |
| P2-4 | `integration-errors-page.tsx:594-704` vs `1627-1731` | 解析面板与「处理动作」卡重复渲染同一组按钮，冗余且状态可能不同步。 |
| P2-5 | `integration-errors-page.tsx:1078` | 详情（focusMode）路由下仍渲染「筛选：视图=mine · 模式=all」摘要，与详情上下文无关的噪音。 |
| P2-6 | `integration-errors-page.tsx:1082-1090, 501` | 「模拟仍未知」演示开关常驻页面，泄露结果是被模拟的机制（G6 演示标记仅允许明示数据来源，不允许暴露测试开关）。 |
| P2-7 | `integration-errors-page.tsx:1079` | 死代码 `{sessionAutoNext !== autoNext ? null : null}`。 |
| P2-8 | `integration-errors-page.tsx:696-698` | 恒禁用按钮「等待退避（禁止高频重试）」占位无操作；「退避」为禁用词（glossary P2 → 「请稍后重试」）。 |
| P2-9 | `integration-errors-page.tsx:1521, 1454-1456, 874, 1318` | 实现词残留：「已关联强类型证据」「追加式」「集成更新时间」「重复请求标识」（glossary §2：证据→记录/凭证、水位→数据更新时间、幂等键→原任务号）。 |
| P2-10 | `types.ts:387` vs `domain.tsx:1231-1235` | 错误类别筛选项「限流」与详情面板「调用次数受限」用词不一致。 |
| P2-11 | `api.ts:1015, 1089`（渲染于 `integration-errors-page.tsx:1492, 1508`） | 直接对账的补证/终结记录在证据、审计时间线中泄漏枚举原值（`ADD_EVIDENCE`、`CONFIRM_NO_ERROR`）。 |
| P2-12 | `integration-errors-page.tsx:1526` | 「已关联强类型证据」列表直接展示内部 recordId（ver_mof_55102 等）。 |
| P2-13 | `url-state.ts:76` + `integration-errors-page.tsx:462-478` | 自动下一项默认开启，终态成功后结果面板一闪而过，用户来不及确认。 |
| P2-14 | `integration-errors-page.tsx:1396-1409` | BusinessDiffPanel「边界」「更新时间」行 before=after 完全相同，无效差异行稀释信息。 |
| P2-15 | `integration-errors-page.tsx:1845-1851`（配合 191-219） | 详情路由下 `detailItemQuery` 加载中/失败时显示「未选择处理项」空态，误导用户。 |
| P2-16 | `integration-errors-page.tsx:1178-1181, 1291-1295` | 状态徽章 tone 取自严重度而非状态语义，「已跳过」等状态可能显示成红色。 |
| P2-17 | `types.ts:399-412` | 视图与模式两套筛选器都含「对账差异」，职责重叠，组合无意义。 |
| P2-18 | `integration-errors-page.tsx:237` | 搜索草稿仅初始化时读 URL，浏览器前进/后退后输入框与 `q` 不同步。 |
| P2-19 | `integration-errors-page.tsx:380-405` | 自动领取静默执行且失败静默吞掉，无任何感知与提示，多用户下可能抢任务。 |
| P2-20 | `integration-errors-page.tsx:531, 723, 746, 806, 849` | 操作失败 Alert 直接展示后端错误原文，未业务化包装。 |

## 六、改进建议

1. **（P1-1）终态动作加确认层**：复用 `FormalActionConfirmDialog`，对 转交/关闭重复/标记已解决/确认有效差异 统一弹「状态变化 + 影响 + 不可撤回提示」确认框；转交还应展示目标角色与「转交不是解决」说明。
2. **（P1-2）替代任务改为业务选择器**：用 OptionCombobox 从当前队列/已处理任务中选择「业务单号 + 对象名」的候选项，替换手输 `wi_*` ID；默认值禁止内部 ID。
3. **（P1-3/P1-4/P1-7/P1-8）文案术语清零**：`filterSummary` 改用 VIEW_LABEL/MODE_LABEL/ENV_LABEL/ERROR_CLASS_LABEL 生成（建议把摘要生成移入 `types.ts` 或服务端，避免英文原值下传）；修复跳转标签去掉 W 编号（用 `workspaceLabel` 或「供应商订单」等中文名，`ui-text.ts` 已有现成映射）；证据策略展示改「证据要求：xxx · 岗位分离：独立复核」；facts 的 label 用 `EVIDENCE_KIND_LABEL` 映射。
4. **（P1-5）清理恒空视图**：「已解决」要么接入已完成队列数据（`getCompletedQueueTaskIds` 已有），要么在指标/切换栏隐藏；「自动重试」当前无数据源时同样降级隐藏。
5. **（P1-6）「处理当前」改为有效动作**：滚动到首个可操作按钮并聚焦，或直接改为触发该错误类别的主动作；同时把「下一项」从领取状态中解耦（只读导航不应被禁用）。
6. **（P1-9）拆解嵌套交互**：行按钮内移除「详情」链接，整行点击即进入详情（保持 URL 参数），或在行尾用独立按钮替代嵌套 Link。
7. **（P1-10）「刷新当前任务」按来源刷新**：focusMode 下同时 refetch `detailItemQuery` 与队列，并给刷新中的视觉反馈。
8. **指标口径统一**：指标数字改为「当前筛选范围」统计，或点击指标时清空 env/owner 只保留 view 维度，并在指标旁注明口径。
9. **演示开关收敛**：`forceUnknownOnce` 移入开发者模式或从界面移除；「自动下一项」默认关闭，终态后停留展示结果面板，用户确认后再前进。
10. **术语长尾**（P2-8/9/10/11/12）：「退避→稍后重试」「强类型→受控/核验」「追加式→不可覆盖/历史保留」「集成更新时间→数据更新于」「限流→调用次数受限」「重复请求标识→原任务号」；审计/证据时间线统一走 `ACTION_LABEL`/中文映射；recordId 不展示或仅显示摘要。
