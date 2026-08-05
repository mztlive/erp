# UX 评审报告：/workspace（今日工作台 / 角色落地页）

> 评审日期：2026-08-05 ｜ 评审视角：产品经理 / UX
> 覆盖代码：
> - `erp-client/features/workspace/workspace-home-page.tsx`（主页面，672 行）
> - `erp-client/features/workspace/destination.ts`、`freshness.ts`、`queries.ts`、`url-state.ts`
> - `erp-client/components/business/page.tsx`（PageHeader / PageActions / MetricStrip / MetricFilterItem / DataFreshness）
> - `erp-client/components/business/audit-import.tsx`（WorkTaskItem）、`feedback.tsx`（BusinessEmptyState / BusinessFailureState / AsyncSectionState）
> - `erp-client/mock/workspace.ts`、`erp-client/lib/ui-text.ts`
> - 文案口径：`docs/ui-glossary.md`（基线 v1.2）、`erp-client/AGENTS.md`（URL 参数与界面控件一一对应契约）

---

## 一、页面概述

今日工作台是登录后的角色落地页。布局为「页头（问候语 + 角色名 + 双数据新鲜度徽章 + 刷新）→ 预警条（可选）→ 4 格指标筛选条 → 左栏任务列表（按族折叠分组，每组预览最多 5 条）＋ 右栏（预警卡 + 最近打开）」。

核心任务闭环：指标卡或任务行「处理」→ 进入 W02 待办队列聚焦该任务 → 返回后 URL 记录 `focusWorkItemId` 并滚动/聚焦还原。加载骨架、错误重试、空态（无任务/筛选无结果/无数据范围/无权限）、刷新保留旧数据等状态覆盖完整；文案整体已过术语表（未发现投影/租约/幂等/W 编号等禁用词上屏）。

主要问题集中在**主操作按钮语义未业务化**、**scope 隐形 URL 状态与标题文案自相矛盾**，以及若干**严重度色值丢失、预警疲劳、内部 ID 泄漏到 URL/DOM** 等体验瑕疵。

---

## 二、易用性

**顺畅的部分：**

- 角色感知清楚：H1 问候语 + 描述「销售与采购协同工作台 · 先处理超期项，再完成今日到期任务。」（`workspace-home-page.tsx:430-432`），落地页第一屏就给出当日行动优先级。
- 待办入口顺畅：指标卡点击即筛选（`MetricFilterItem` 带 `aria-pressed` 选中态，`page.tsx:440-490`），任务行「处理/查看」直达，返回后焦点还原（`workspace-home-page.tsx:346-356`），主路径链路完整。
- 筛选/清除有闭环：「清除筛选」「查看全部待办」「查看该组全部 N 条」均带上下文参数（due/family）跳转，URL 可回退可分享。
- 分组默认展开与业务紧迫度挂钩（有超期的族才展开，`mock/workspace.ts:650-655`），未用硬编码展开。

**阻碍效率的问题：**

1. **主操作按钮「处理」语义过泛**：对所有任务类型统一渲染「处理」，而术语表 §3.1 明确要求按任务类型给动作词（采购二次确认→「去确认采购计划」、卡券票款复核→「去复核卡券票款」、回款核对→「去核对回款」、映射异常处理→「去处理映射异常」）；共享函数 `actionLabelForWorkItemType`（`ui-text.ts:185-191`）已存在却未接入，首页任务类型恰全部命中映射（`mock/workspace.ts:151/368/398/496`）。「处理」对确认类任务的真实动作（确认/复核/核对）表达不足（见 P1-1）。
2. **「团队待认领」数据在首页无入口**：`scope` 是唯一能改变列表语义的参数，但页面无任何控件切换我的/团队；用户只能靠拼 URL 看到团队待认领任务（见 P1-2）。
3. 被阻断任务行同时出现「处理」（禁用）+「查看」+ 阻断原因小字，三个元素挤在右列且原因重复两处（title 与文本），扫读负担（P2-5）。
4. 首页任务为零时，空态提示「可查看最近打开记录或进入其它业务模块」但无具体模块入口，快捷导航只剩最近打开与「查看全部待办」（P2-9）。

---

## 三、信息密度

**总体平衡较好：**

- 指标条 4 项（待我处理 / 今日到期 / 已超期 / 同步异常）带口径小字（「仅当前用户」「今天 18:00 前」「需要优先处理」「影响数据更新时间」），数字突出、语义自解释（`mock/workspace.ts:591-627`）。
- 任务条目走 WorkTaskItem 默认密度：类型 + 状态徽章 + 对象 + 往来方 + 进入时间/截止/责任方 + 原因/影响，信息分级清楚（`audit-import.tsx:138-173`）。
- 预览 5 条 + 折叠分组 + 「查看该组全部 N 条」控制首屏总量；预警/最近打开卡内容克制。

**过密 / 过疏的问题：**

1. **严重度色值被丢弃**：mock 为「已超期」指标配置 `tone: destructive`、为「同步异常」配置 `warning`（`mock/workspace.ts:616/624`），但 `MetricFilterItem` 不接收/渲染 tone（`page.tsx:440-490`），首页对「最该红」的超期数没有任何强调色，与任务行内「已超期」红色徽章信息不对齐（P2-1）。
2. **优先级不可见**：任务卡描述宣称「按超期、优先级与截止时间展示」（`workspace-home-page.tsx:508`），但优先级既无徽章也无字段，用户只能接受不明就里的排序（P2-7）。
3. 页头同时挂两组 DataFreshness 徽章（待办 / 工作台汇总）+ 刷新按钮（`workspace-home-page.tsx:433-464`），compact 密度下与问候语同排，窄屏换行后略显拥挤；且「工作台汇总」与「待办」的差异用户难以直观理解（可接受，但属潜在困惑）。

---

## 四、交互合理性

**做得好的：**

- 刷新不闪屏：`placeholderData` 保留旧数据 + `AsyncSectionState` 刷新 pill「正在刷新，仍显示上次结果」（`queries.ts:40`；`feedback.tsx:414-468`）。
- 错误不丢内容：刷新失败保留上次成功内容并给出「重试」（`workspace-home-page.tsx:537-546`）。
- 焦点还原链路完整：`onOpenTask` 用 `history.replaceState` 写 `focusWorkItemId`，返回后 `scrollIntoView` + `focus`（`:329-356`），且切换指标时主动清除（`url-state.ts:59-83`）。
- 状态覆盖齐全：骨架屏（`:97-118`）、整体失败（`:362-376`）、无权限（`:382-392`）、无数据范围（`:394-408`）、无任务（`:548-554`）、筛选无结果（`:556-567`）。
- 空态语义区分到位：「无数据范围」明确「系统不会展示虚假的 0 指标」（`:404`），避免误导。

**问题：**

1. **scope 为隐形 URL 状态**：`scope` 被 queryFn 消费（`url-state.ts:87-98`）、被任务队列链接携带（`:101-108`），但首页无控件设置/清除，违反 AGENTS「URL 参数与界面控件一一对应」契约（P1-2）。
2. **预警状态重复传达**：投影过期时，页头 `DataFreshness` 徽章「数据可能不是最新（>1 分钟）」与黄色 `Alert`「工作台数据可能不是最新」同时出现（`:435-448` vs `:467-486`）；且 mock 让投影恒滞后 90 秒（`mock/workspace.ts:741-744`），每次进入必现警告，「狼来了」效应使该信号逐渐失效（P2-8）。
3. **无自动刷新**：过期只提示、不补救，需手动点页头刷新（P2-11）。
4. 代码健壮性小瑕疵：`if (!view) return <WorkspaceHomeSkeleton />`（`:378-380`）在 pending/error 分支之后不可达，属死代码。
5. 内部 ID 进入 URL/DOM：`/sales/orders/so_1001`（`destination.ts:56`）、`focusWorkItemId=wi_pc_01`、`data-work-item-id="wi_pc_01"`（`workspace-home-page.tsx:179-180/334`）——地址栏与 DOM 对用户/爬虫可见（P2-6）。

---

## 五、问题清单

### P0（阻断操作）：0 项

未发现阻断主流程的功能性缺陷。

### P1（明显阻碍效率 / 误导 / 契约违例）：2 项

| # | 严重度 | 问题 | 位置 |
|---|--------|------|------|
| P1-1 | P1 | **主操作按钮「处理」未按任务类型业务化**：全部任务行统一渲染「处理」，而术语表 §3.1 要求按类型给动作词（去确认采购计划 / 去复核卡券票款 / 去核对回款 / 去处理映射异常），共享函数 `actionLabelForWorkItemType` 已存在（`ui-text.ts:185-191`）且首页任务类型全部命中映射（`mock/workspace.ts:151/368/398/496`）却未接入；对确认/复核类任务，「处理」掩盖了真实动作语义，且与「按钮文案必须与实际行为一致」契约的粒度不符 | `workspace-home-page.tsx:220/234`；`ui-text.ts:185-191`；`docs/ui-glossary.md:193-210` |
| P1-2 | P1 | **scope 为隐形 URL 状态且文案自相矛盾**：`scope`（mine/role_pool）被 queryFn 消费（`toTodayWorkspaceQuery`）并被队列链接携带，但首页无任何控件设置/清除它（违反 AGENTS「URL 参数与界面控件一一对应」）；进入 `role_pool` 后指标显示「团队待认领」而任务卡标题仍按 `FILTER_SUMMARY.mine` 渲染「待我处理」（`:418/504`），列表与标题语义冲突 | `workspace-home-page.tsx:418/504`；`url-state.ts:31/87-98/117-122`；`mock/workspace.ts:591-601` |

### P2（体验瑕疵）：12 项

| # | 问题 | 位置 |
|---|------|------|
| P2-1 | 「已超期」「同步异常」指标的严重度色值（mock `tone: destructive/warning`）在渲染时被丢弃，超期数无红色强调，与任务行内红色「已超期」徽章不对齐 | `page.tsx:440-490`（MetricFilterItem 无 tone 支持）；`mock/workspace.ts:616/624` |
| P2-2 | WorkTaskItem 默认密度字段标签「进入队列」为内部队列机制词，用户语境应为「进入时间/接收时间」 | `audit-import.tsx:142` |
| P2-3 | 右栏卡片标题「预警与数据新鲜度」名不副实：新鲜度信息实际在页头 DataFreshness 徽章，卡片内只有预警 | `workspace-home-page.tsx:590` |
| P2-4 | 预警按钮「打开处理面」含机制词「处理面」，术语表 §3.1 模板为「前往/去＋业务动作」（如超期预警→「前往待办队列」） | `workspace-home-page.tsx:628` |
| P2-5 | 被阻断任务「处理」禁用按钮的 `title` 与下方小字重复展示同一阻断原因，且禁用按钮 +「查看」+ 原因三元素并排，扫读负担 | `workspace-home-page.tsx:227-255` |
| P2-6 | 内部 ID 泄漏到 URL/DOM：`/sales/orders/so_1001`、`focusWorkItemId=wi_pc_01`、`data-work-item-id="wi_pc_01"`，地址栏与 DOM 可见，违反「内部 ID 不进界面」精神 | `destination.ts:56`；`workspace-home-page.tsx:179-180/334` |
| P2-7 | 描述宣称「按超期、优先级与截止时间展示」但优先级无任何视觉呈现，用户无法理解排序依据 | `workspace-home-page.tsx:508` |
| P2-8 | 投影过期时页头徽章与黄色 Alert 重复提示同一状态；且 mock 恒使投影滞后 90 秒，每次进入必现警告，预警逐渐失效 | `workspace-home-page.tsx:435-448/467-486`；`mock/workspace.ts:741-744` |
| P2-9 | 首页无业务模块快捷入口：空态只提示「进入其它业务模块」却无具体入口，角色落地页的快捷导航仅剩最近打开 + 查看全部待办 | `workspace-home-page.tsx:552` |
| P2-10 | 刷新失败时页头 destructive Alert（数据更新失败）与任务卡内 AsyncSectionState 错误卡双重报错，同一故障两处噪音 | `workspace-home-page.tsx:467-486/537-546` |
| P2-11 | 无自动刷新/轮询：数据过期只提示「可能不是最新」，需用户手动点刷新补救 | `freshness.ts:54-66`；`workspace-home-page.tsx:341-343` |
| P2-12 | `focusWorkItemId` 无独立清除控件，只能靠切换指标顺带清除；URL 带该参数刷新时自动滚动聚焦，分享链接场景行为不可预期 | `workspace-home-page.tsx:322-339/346-356` |

---

## 六、改进建议

**P1 优先修复：**

1. **P1-1 按钮业务化**：任务行主按钮改用 `actionLabelForWorkItemType(item.workItemTypeLabel)`（未登记类型自然回退「前往处理」）；同时确认 W02 落点聚焦行为与该文案的一致性——若按钮文案承诺「去确认采购计划」，落点至少应直达该任务的确认上下文，避免「说一套、落点另一步」。
2. **P1-2 scope 出口**：补「我的 / 团队待认领」切换控件（可放在指标条旁，联动指标 label 与 `FILTER_SUMMARY` 标题），或从首页查询中摘除 `scope`；`FILTER_SUMMARY` 按 scope 出标题，杜绝「团队待认领」指标配「待我处理」标题。

**P2 建议：**

- 指标条支持 tone 渲染（MetricFilterItem 增加 status/tone），「已超期」红色高亮、与其在任务行内的红色徽章对齐（P2-1）。
- 预警提示收敛：过期时只保留一处（Alert 或徽章），且只在「上次成功数据 + 本次失败」时出现，正常 stale 不每次刷屏；考虑 60s 自动轮询替代手动刷新（P2-8/P2-11）。
- 文案微调：「进入队列」→「进入时间」（共享组件加 prop，不改默认值，遵循术语表 §5）；「打开处理面」→「前往待办队列/按目的地出动作词」；卡片标题改「需要关注的预警」（P2-2/P2-4/P2-3）。
- URL 清理：`focusWorkItemId` 与 `currentWorkItemId` 不落地址栏（改用 sessionStorage 或 hash），或提供清除控件；`data-work-item-id` 换业务单号（P2-6/P2-12）。
- 空态补模块入口：任务为零时给出 2-3 个常用模块快捷链接（如销售单、采购确认、库存），兑现「进入其它业务模块」的指引（P2-9）。
- 排序说明落地：要么给任务行加优先级徽章，要么把描述改为「按超期与截止时间展示」（P2-7）。

---

*备注：以上为纯分析结论，未修改任何代码。*
