# UX 评审报告 · 采购二次确认（/procurement/confirm）

> 评审对象：`erp-client/features/procurement-confirmation/procurement-confirmation-page.tsx`
> 关联组件：`components/business/workflow.tsx`（SequentialProcessBar / FormalActionConfirmDialog）、`components/business/feedback.tsx`（FormalActionResult / ValidationSummary / BusinessEmptyState）、`components/business/document.tsx`（DocumentSummary）、`features/procurement-confirmation/{api,queries,types}.ts`、`mock/procurement-confirmation.ts`
> 参考口径：`docs/ui-glossary.md`（v1.2 禁用词表与验收门禁）
> 评审日期：2026-08-05

## 一、页面概述

采购二次确认（W07）是「销售提交 → 采购确认 → 销售单生效」链路中的关键审批站。页面核心任务是：**看一条（核对明细与风险）→ 改确认分行（可选）→ 通过并生效 / 驳回 / 先跳过**，并支持连续处理队列（自动下一项、j/k 键盘切换、⌘S 保存、⌘↵ 通过）。

页面结构：页头（新鲜度 + 筛选摘要）→ 自动下一项开关条 → SequentialProcessBar（队列位置/处理权/返回/重新领取/双通过按钮）→ 双栏主体（左：销售提交卡 + 逐明细确认分行表格；右：sticky 决策摘要 + 销售单入口）→ sticky 底栏（保存/先跳过/驳回/确认通过）→ 确认对话框 / 驳回对话框。

总体评价：信息组织与连续处理体验是成型水准（覆盖校验、缺口阻断、确认影响清单、驳回三路说明都到位）；主要问题集中在**错误态缺失、未保存修改的终局操作保护、结果反馈被自动跳转吞掉**，以及**术语表硬门禁的一批内部 ID / 枚举原值上屏**。

## 二、易用性

### 路径顺畅度：良好，但有三个断点

1. **查看 → 核对 → 通过/退回** 主路径顺畅：进页即显示待确认项，风险 Alert + 决策摘要 + 逐明细表格齐全；通过走 FormalActionConfirmDialog（状态变化 + 锁定字段 + 影响清单 + 下一责任部门），驳回走独立表单对话框（原因 + 必填说明），误操作防护扎实。
2. **断点 A：队列加载失败 → 整页空白**。`queueQuery` 只有 `isPending` 分支（:735-749），没有 `isError` 分支；失败时 `view` 为 undefined，页面落到 `task ? ... : null`（:855），主体渲染为空，且无重试按钮。用户在错误状态下无法恢复（只能手动刷新）。
3. **断点 B：未保存修改不拦终局操作**。用户改了确认分行（dirty）后直接点「确认通过并使销售单生效」（:1473 的 disabled 条件只查 `formalPending || !allCovered`，不查 dirty）或「驳回」，对话框均无未保存提示，终局动作按**服务端旧草稿**执行，用户输入的行静默不被采纳。
4. **断点 C：终局结果反馈被自动跳转立即清空**。通过/驳回成功后先 `setLastResult`，随后 `advanceIfNeeded(true)` → `goToWorkItem` → `setLastResult(null)`（:524-526、:578-580 配合 :315-326）。autoNext 开（默认开）时，结果面板（含结果编号、事实、驳回后的「销售固定三条出路」指引卡）**完全不显示**，用户只能凭队列位置变化推断成功。
5. 顶部 SequentialProcessBar（:857-882）与底部 sticky 栏（:1436-1478）出现**两个「通过」入口**，且顶部「确认通过」（advanceAfterConfirm=false）在 autoNext 打开时行为与开关语义矛盾——用户开了「自动下一项」，点顶部「确认通过」却不前进。

### 键盘与效率

- j/k、⌘S、⌘↵ 快捷键设计好；但除 dirty 提示里的「⌘S 保存」外**无任何发现性提示**。
- j/k 切换被 dirty 拦截时提示「请先保存**或放弃**后再切换」（:677、:686），但页面**不存在「放弃修改」入口**——提示承诺了不存在的操作。

## 三、信息密度

整体密度合理，判断如下：

- **关键信息基本突出**：风险 Alert 有 tone 区分；「销售含税金额」在 DocumentSummary 用 `emphasized` 加粗；「预计采购含税」在决策摘要卡对比展示；供应商用 Combobox 输入。
- **冗余但有价值**：逐明细「覆盖 x/y」徽章（:1076-1085）与右栏「逐明细数量覆盖」（:1329-1348）信息重复，但因左右分栏距离远，可接受。
- **瑕疵**：
  - 参考成本直接渲染原始字符串（:1067-1069，「参考 华东优选供应链有限公司 / 430.00」），与全页其他金额 `money.format`（:988）格式不一致，无 ¥ 无千分位。
  - 「覆盖 100/120」徽章（:1080-1085）与决策摘要（:1343）无单位，需对照表头「承诺 120 件」才知道是件。
  - 数量/成本/税率三个 Input 宽度受限（w-20/w-24/w-16），长金额显示被截断无提示。
  - 表格 8 列（供应商/确认数量/含税成本/进项税率/预计交期/履约方式/资质/操作）在窄屏整体横向滚动（min-w-[40rem]，:1090），无列优先级降级策略。

## 四、交互合理性

| 维度 | 评价 |
| --- | --- |
| 加载态 | 骨架屏 + aria-hidden，OK |
| 空态 | ⚠️ `completed = Boolean(view) && tasks.length === 0`（:194）忽略 `emptyReason`（NO_TASKS / FILTER_NO_RESULT / NO_DATA_SCOPE，types.ts:139）；筛选无结果（如 orderNo 查无）也显示「本筛选项已处理完」，**误导为已完成** |
| 错误态 | ❌ 队列查询无错误分支（见 P0-1） |
| 批量/连续操作 | 自动下一项 + 会话内偏好 + 开关提示「该偏好仅在本次操作内生效」，清晰 |
| 确认对话框 | 影响清单、锁定字段、状态迁移、pending 防重入，扎实；但内容泄漏内部 ID/枚举（见 P1） |
| 驳回对话框 | 原因中文映射 + 说明 ≥5 字校验 + 三路说明，好；但**默认预选「无法履约」**（:590），用户未主动选择即可提交，可能误选 |
| 误操作防护 | 删除行限制 ≥1 行（:1266-1268）；终局操作有确认层；**但 dirty 未保存无拦截**（P0-2） |
| 反馈提示 | 保存成功/失败、动作错误均有呈现；**终局结果在 autoNext 下被清空**（P1-2）；「先跳过」在保存失败后仍继续执行（P1-4） |
| 自动跳转 | 位置徽章「第 x/N 条」实时反映；完成最后一项回空态，OK |
| 领取机制 | 从 W02 进入自动领取（:262-290），但领取失败 catch 后**静默**（:283-285），用户无感知；后续提交时才报错 |

## 五、问题清单

### P0 · 阻断操作 / 业务结果错误

1. **队列加载失败 → 空白页，无错误提示、无重试**
   `erp-client/features/procurement-confirmation/procurement-confirmation-page.tsx:735-854`
   只有 `queueQuery.isPending` 分支，无 `isError` 处理；失败时 `view` undefined → `task` undefined → 主体渲染为空（:855 `: null`）。用户面对只有页头的空白页，无法区分「加载中/无数据/失败」，且无重试入口，只能手动刷新。建议接 BusinessFailureState（`feedback.tsx` 已有 `kind="system"` 预设）+ 重试按钮。

2. **dirty 状态下通过/驳回不拦截、不提示，用户编辑的分行不生效**
   `procurement-confirmation-page.tsx:486-529`（通过）、`:539-586`（驳回）、`:1473`（主按钮 disabled 条件）、`:1480-1506`（确认对话框）
   终局动作提交的 decision 不含 lines（走服务端草稿），而「通过」会对销售单生效、形成应收——用户刚改的确认数量/供应商被静默丢弃却收到「已通过」成功反馈，业务结果与用户输入不一致。`handleDefer` 有「dirty 先保存」逻辑（:606-609）而通过/驳回没有，行为不一致。建议：打开终局对话框前若 dirty，提示「有未保存修改：先保存，或放弃后继续」，或自动带脏保存。

### P1 · 明显阻碍效率 / 影响正确认知

1. **文案泄漏族（术语表 v1.2 硬门禁「必须清零」类）**
   - 结果面板事实表直接用字段名做标签：`{ label: "submissionId" }`、`{ label: "subjectHash" }`（`procurement-confirmation-page.tsx:1610/1615`）；驳回结果「驳回提交」显示 `sosub_*` 内部 ID（:1647-1649）
   - 确认对话框 effects 泄漏工作流类型枚举原值：「完成当前 **PROCUREMENT_CONFIRMATION** 任务」（:1500）；lockedFields 泄漏 `submissionId sosub_*`、`subjectHash a1b2…`（:1493-1494）
   - 结果 fallback 分支直接渲染枚举原值：「任务状态 **PENDING / IN_PROGRESS**」（:1673）
   - 「采购创建依据」值 `pcb_*`、「销售版本」值 `rev_*`（:1624-1635）
   - 重提 Alert：新提交编号 `sosub_*`（:897-899）、上一驳回提交 `sosub_*`（:906-908）、上级证据原值引用（:914-916）
   - DocumentSummary「提交编号」`sosub_*` 与「数据版本」原始指纹短码（:953-973）
   按 glossary 规则 7 / §2 第 5 轮 P0 表，字段名与枚举原值不得上屏；内部 ID（sosub_*/pcb_*/rev_*）按「品名 + 数量 + 业务单号」口径替换。建议统一收敛为业务单号 + 中文标签（如「提交编号」改为「第 N 次提交 · 时间 · 提交人」）。

2. **终局结果反馈被自动下一项立即清空**
   `procurement-confirmation-page.tsx:524-526`（通过）、`:578-580`（驳回）→ `goToWorkItem`（:315-326）`setLastResult(null)`
   autoNext 默认开，通过/驳回成功瞬间跳转下一条，结果面板（结果编号 PC-OK-*/PC-REJ-*、facts、「销售固定三条出路」指引卡 :827-834）完全不显示。驳回场景尤其严重——销售下一步的出路指引直接丢失。建议：成功跳转前把结果信息以「轻量确认条 + 跳转」或「结果面板 + 下一条同屏」方式保留数秒，或提供「查看上一条结果」入口。

3. **空态误报：「筛选无结果」显示成「本筛选项已处理完」**
   `procurement-confirmation-page.tsx:194`（completed 判定忽略 emptyReason）、`:846-854`（统一文案）
   `types.ts:139` 已区分 `FILTER_NO_RESULT / NO_DATA_SCOPE / NO_TASKS`，但页面只用 `tasks.length === 0` 判断。orderNo 查无单号时用户会误以为队列已清空而离开。建议按 emptyReason 分发三套文案（「当前筛选无结果」/「当前角色无数据范围」/「本筛选项已处理完」）。

4. **「先跳过」在保存失败后仍继续执行，未保存修改静默丢失**
   `procurement-confirmation-page.tsx:606-618` 配合 `:450-467`
   `handleSave` 捕获异常只 setActionError 不 rethrow；`handleDefer` 的 `if (dirty) await handleSave()` 后无条件继续 defer 并跳转，草稿修改随任务切换被丢弃。建议：保存失败时中止跳过，把错误与「重试保存」作为唯一出路。

5. **j/k 拦截提示承诺「放弃」操作但无此入口**
   `procurement-confirmation-page.tsx:677`、`:686`
   「有未保存修改，请先保存或放弃后再切换」——页面没有「放弃修改」按钮，用户只能被锁在当前任务。建议：加「放弃修改」按钮（清空草稿重载服务器版本）或改文案为「请先保存后再切换」。

6. **「行为/任务 · 无确认单号」徽章与技术词「不可变提交」**
   `procurement-confirmation-page.tsx:940`、`:943`
   「行为」泄漏内部动作概念（work item action），业务用户无法理解；「不可变提交」是实现词（glossary §2 散词族）。建议改「二次提交 · 无确认单号（首二次确认）」类业务表达。

### P2 · 体验瑕疵

1. **参考成本未按货币格式化**：`procurement-confirmation-page.tsx:1067-1069` 直接渲染 `referenceCost` 原始字符串，与全页 `money.format` 不一致（无 ¥/千分位）。
2. **覆盖徽章无单位**：`procurement-confirmation-page.tsx:1080-1085`、`:1343`「覆盖 100/120」需对照承诺行才知单位，建议带单位（如「120 件」）。
3. **顶部与底部双「通过」入口语义打架**：`procurement-confirmation-page.tsx:867-874` 的「确认通过」（不前进）与 autoNext 开关语义冲突；且两处按钮 gating 不一致——顶部需先领取（canProcess 要求 active lease，workflow.tsx:431-434），底部「确认通过并使销售单生效」（:1467-1477）不要求领取（提交时静默 ensureLease）。建议底部按钮同样受领取状态约束或明确提示。
4. **快捷键无发现性**：j/k/⌘↵ 无任何界面提示（:646-703），仅 dirty 时出现「⌘S 保存」。建议在自动下一项开关条附近给一行快捷键说明。
5. **驳回原因默认预选「无法履约」**：`procurement-confirmation-page.tsx:588-600`，用户不主动选择即可提交；建议默认空、未选则不可提交。
6. **文案不一致「固定三路 / 三条出路」**：驳回对话框 :573「固定三路」vs 结果卡 :1745「三条出路」vs :1558「固定出路」，统一为「三条固定出路」。
7. **保存成功提示泄漏内部词**：`procurement-confirmation-page.tsx:462`「已保存 · 编辑版本 {n}」——「编辑版本」为对象版本族词（glossary §2 第 5 轮），建议「已保存 · 第 n 次修改」或去掉。
8. **重提 Alert 中「版本 {subjectHashSummary}」展示原始指纹短码**：`procurement-confirmation-page.tsx:901-904`，属校验码/指纹族，用户无需可见，建议删除或改「数据版本 v2」。
9. **进入即自动领取无告知**：`procurement-confirmation-page.tsx:262-290` 从 W02 进入即 claim，若用户只查看不处理，任务被会话持有；且领取失败 catch 静默（:283-285），建议失败时给出提示。
10. **DataFreshness 硬编码「刚刚」**：`procurement-confirmation-page.tsx:766` `updatedAt="刚刚"`，与 `context.queueContextUpdatedAt` 脱节，任何时刻都显示「刚刚」，mock 期间请标注或接真实时间。
11. **空态后无「调整筛选」动作**：`procurement-confirmation-page.tsx:846-854` 只有「返回今日工作台」，筛选/单号过滤（orderNo 参数可来自 URL）无清除入口；URL 参数与控件对应契约（AGENTS.md §5）要求可见可清除。
12. **窄屏表格无降级**：`procurement-confirmation-page.tsx:1089-1090` 整体横向滚动，无关键列置顶/隐藏策略。

## 六、改进建议（按优先级）

1. **P0-2 未保存拦截 + 自动带脏保存**（改动最小、收益最大）：通过/驳回对话框打开时若 dirty，追加一行警示「当前修改未保存，将按已保存数据提交」，并提供「先保存」入口；或复用 handleDefer 的先保存逻辑。
2. **P0-1 错误态**：补 `queueQuery.isError` 分支，渲染 BusinessFailureState（kind="system"）+ 重试按钮。
3. **P1-2 结果反馈保留**：终局成功且自动跳转时，将结果压缩为「顶部结果条（含结果编号 + 查看上一条结果/驳回出路）」而非完全清空。
4. **P1-1 文案清理**：全页回收 submissionId/subjectHash/procurementCreationBasisId/salesOrderRevisionId/workItemStatus 等上屏值，统一走 ui-text 常量；`buildResultFacts` 英文标签改中文。
5. **P1-3 空态三分**：按 `emptyReason` 分发 NO_TASKS / FILTER_NO_RESULT / NO_DATA_SCOPE 文案，与 `BusinessEmptyState` 预设对齐。
6. **P1-4 先跳过失败熔断**：`handleSave` 失败时中止 defer 流程。
7. 其余 P2 按「术语表扫描验收 + 文案一致性」批次处理，其中快捷键提示、驳回原因默认空、货币格式化成本极低，建议随 P1 一起做。

## 七、结论

页面主流程（连续确认、覆盖校验、影响确认、驳回三路）设计成熟，误操作防护意识强。必须优先解决的三件事：**未保存修改的终局保护（P0）**、**队列错误态空白（P0）**、**术语表硬门禁的 ID/枚举泄漏（P1 高）**——尤其结果面板与确认对话框中直接上屏的英文字段名与枚举原值，属验收红线。
