# UX 评审报告：卡券票款复核（/finance/card-funds-review）

- 评审视角：产品经理 / UX
- 涉及代码：
  - `erp-client/features/card-funds-review/card-funds-review-page.tsx`（主页面，1845 行）
  - `erp-client/features/card-funds-review/{types,queries,api}.ts`（契约 / 队列 / mock 语义）
  - `erp-client/mock/session-state.ts`（领取 / 版本 / 暂挂会话模拟）
  - `erp-client/components/business/{workflow,feedback,page,values,editor,document,audit-import}.tsx`（SequentialProcessBar / FormalActionConfirmDialog / FormalActionResult / MetricStrip / AllocationWorkspace / BusinessDiffPanel 等共享组件）
- 参考基线：`docs/ui-glossary.md`（v1.2，P0/P1/P2 禁用词表）；`erp-client/AGENTS.md`（按钮文案与行为一致、URL 参数与控件一一对应、枚举原值/内部 ID 不得上屏）
- 结论：页面是**高频连续复核作业页**，单条详情 + sticky 结论区 + 顶栏连续处理条的骨架设计合理，状态覆盖（加载/错误/空/结果）齐全，误操作防护意识强（强确认对话框、版本一致性校验）。但存在 1 个**阻断主流程的 P0**（登记回款/发票后复核必然被版本校验拒绝），以及一批**内部 ID / 枚举原值 / 开发者文案上屏**的术语违规（10 个 P1），演示控件与正式功能混排。

---

## 1. 页面概述

- 定位：W13 卡券票款复核，财务逐条核对「同步成交额 ↔ 应收 ↔ 净已收/净已开票」是否一致，选择通过 / 驳回 / 先跳过，并支持登记历史回款、历史发票形成凭证。
- 信息架构：页头（队列摘要 + 更新时点）→ 筛选条（类型/范围/自动下一项/演示开关）→ 结果反馈区 → 连续处理条（第 x/y 条 · 处理状态 · 返回队列/重新领取/复核通过/通过并打开下一条）→ 主卡片（单据摘要 → 5 指标 → 可靠性 Alert → 差异面板 → 回款与发票明细 + 登记入口）→ **sticky 结论区**（证据 / 备注 / 保存证据 / 从 0 起 / 复核通过 / 驳回 / 先跳过）→ 右栏（复核链只读 / 证据与导航）。
- 队列模型：URL 驱动的单条流式处理（`currentWorkItemId`），j/k 切换，自动领取，`autoNext` 决定完成后是否自动跳下一项。
- 双路核对信息：`BusinessDiffPanel`（SYNC_DELTA 时展示上一有效复核与当前记录差异）与 5 指标条（`MetricStrip`）。

## 2. 易用性

**顺畅的部分**
- 主路径"查看 → 核对 → 通过"三步清晰：顶部连续处理条同时提供「复核通过」「通过并打开下一条」两个语义明确的入口（`workflow.tsx:499-536`），与底部 sticky 结论区呼应；sticky 结论区保证长页面滚动时决策按钮始终可见（`card-funds-review-page.tsx:1424-1428`）。
- 「无历史票款，从 0 起」按上下文条件渲染（`canConfirmZero`，`:1474-1485`），避免在差额任务/非零余额下暴露无效动作；通过 / 驳回 / 先跳过均走强确认对话框（`FormalActionConfirmDialog`），锁定字段 + 影响清单 + 从-到状态条，防护意识强。
- 差异面板文案业务化：「上一有效复核与当前记录对比（系统最新数据）」（`:1104`）；「先跳过」对话框明示"不生成复核记录、任务保留在待处理列表"（`:1712`）。
- 任务完成后结果面板给出「下一项」「打开销售单」出口（`:874-896`），空态给「返回今日工作台」（`:928-936`）。

**不顺的部分**
- **登记回款/发票后主流程被阻断**（见 P0-1）：登记成功提示"复核完成前指标仍可能不可靠"暗示用户可以继续完成，实际点「复核通过」必然报版本不一致，只能刷新页面恢复——用户会认为系统出故障。
- 「先跳过」确认后页面**无任何成功反馈**直接切到下一项（见 P1-2），用户无法确认跳过是否生效，可能重复操作。
- 结论区「返回队列」按钮实际跳转 `/workspace`（工作台）而非队列（见 P1-3），文案与行为不一致（违反 AGENTS.md 契约）。
- 证据必填的规则没有前置提示：用户按正常习惯（不填证据、不写备注）点「复核通过」→ 强确认 → 提交后才报"完成复核时证据不能为空"（见 P1-1）。
- 切换任务（j/k）时，**上一个任务的回款/发票登记表单金额原样保留**，换任务后直接复用，存在把 A 单金额登记到 B 单的风险（见 P1-8）。

## 3. 信息密度

- 首屏结构均衡：5 指标条（同步成交额 / 当前应收 / 净已收 / 净已开票 / 版本状态）把决策所需的金额一次性给出（`:1047-1077`），sticky 结论区不占首屏。
- 关键信息突出度整体合格：可靠性 Alert 区分「复核未完成」与「数据已变更」两种不可靠语义（`:1079-1099`）；「从 0 起」按钮出现在有对应场景时。
- 密度问题：
  - **差异面板缺少"差额合计"**：SYNC_DELTA 任务的核心问题是"差多少"，但 `BusinessDiffPanel` 只有字段级前后值表格 + 变更条数，没有汇总差额数字，需逐行扫读（P2-7）。
  - **结果面板两个近义字段并列**：「操作编号」与「操作号」展示两个内部 ID（`wa_*` / `op_*`，`:1829-1830`），对财务用户既无信息量又占空间（P1-4）。
  - 复核链卡片每行同时展示短哈希 + 前驱 ID + 「只读」徽章（`:1571-1581`），内部字段密度高而业务信息量低（P1-4 / P2-8）。
  - 主卡片纵向长度可观（摘要 4 项 + 5 指标 + Alert + 差异 + 明细），但 sticky 决策区缓解了操作可达性，可接受。

## 4. 交互合理性

- 加载态：骨架屏占位（`:759-770`）；错误态：整页「队列加载失败 + 重试」（`:772-781`）；空态：区分「已处理完」（`no-tasks`）与「筛选无结果」（filter），各自带出口。三态齐全。
- 反馈：`FormalActionResult` 结果面板带 focus 管理（`:291-297`）与 aria-live，键盘可达性良好；j/k、⌘↵ 快捷键有说明（`:1614`）。
- 误操作防护：通过/驳回/跳过均二次确认；驳回需选原因 + ≥5 字说明（zod 校验，`:118-126`）；确认对话框 pending 态禁点。**但结论区主按钮组未绑定 `formalPending` 禁用**（见 P1-10），且 `FormalActionConfirmDialog` 的 `onConfirm` 以 `void` 方式调用异步动作（`:1697-1703`），内部 pending 立即复位，提交期间主按钮仍可再次打开确认框。
- 数据一致性：mutation 成功统一 `invalidateQueries`（`queries.ts`），登记回款后金额/指纹回刷正确；**但客户端租约缓存不随版本升级**（P0-1）。
- 文案/术语（对照 `ui-glossary.md`）：
  - 主流程文案整体业务化，无「正式/租约/任务信封」等词；「先跳过」「从 0 起」符合 G1/G6 决议。
  - 违规集中在：内部 ID 上屏（`doc_*`、`rfr_*`、`wa_*`、`op_*`、账户 ID）、枚举原值上屏（`OPENING`/`SYNC_DELTA`/`PENDING`/`APPLY−REVERSE`/blockerCode）、开发者说明直出（AllocationWorkspace 描述）、架构词（「追加式链」「复核链」）——详见 P1-4/5/6/7。

---

## 5. 问题清单（按严重度）

### P0（阻断操作）

**P0-1 登记历史回款/发票后，复核通过/驳回必然被版本校验拒绝，必须整页刷新**
- 位置：`card-funds-review-page.tsx:605-607`（登记成功后 `leaseRef.current = { ...lease }` 保留旧版本）vs `card-funds-review-page.tsx:347-360`（`ensureLease` 命中同 workItemId 直接返回旧租约）vs `api.ts:501-508`（`expectedSubjectVersion !== item.workItem.subjectVersion` → 拒绝）vs `mock/session-state.ts:3249-3263`（`bumpW13SubjectHash` 更新会话版本但客户端 ref 不更新）
- 问题：登记回款/发票会 `bumpW13SubjectHash` 提升任务版本；客户端 `leaseRef` 仍是登记前的旧 `subjectVersion`。随后点「复核通过」（或驳回），`expectedSubjectVersion` 传旧值，`completeCardFundsReview` 在 `api.ts:501` 直接返回 failed「任务数据版本与当前记录不一致，已阻断。请刷新后重审。」。登记结果面板文案"复核完成前指标仍可能不可靠"（`:600`）暗示可继续复核，与真实行为矛盾。代码注释（`:604` "租约仍有效但 subject 已变：刷新 lease 展示"）表明开发者知道版本变了，但只刷新了展示、未刷新提交用的版本。用户必须整页刷新（或切换任务再切回）才能继续，**登记→通过是本期页面的核心业务路径**。

### P1（明显阻碍效率 / 契约违反）

**P1-1 证据必填校验无前置提示，通过路径按惯例操作必然踩空**
- 位置：`api.ts:399-408`（`EVIDENCE_REQUIRED`：证据文档与引用全空即拒绝）vs 确认对话框无任何证据要求提示（`card-funds-review-page.tsx:1645-1704`，`lockedFields`/`effects` 均未提及"须填写证据"）
- 问题：用户不填证据、不写备注直接「复核通过」，需经强确认后提交才收到"操作未生效"。应在打开确认框前校验并就地提示（结论区已有证据输入框，可加红标）。

**P1-2 「先跳过」成功无任何反馈，结果面板被立即清空**
- 位置：`card-funds-review-page.tsx:551-559`（`setLastResult(blocked)` 后紧跟 `goToWorkItem(nextId)`）vs `card-funds-review-page.tsx:316-320`（`goToWorkItem` 第一行 `setLastResult(null)`）
- 问题：React 批处理下两个 setState 合并，`lastResult` 最终为 null——跳过成功的结果面板从未渲染，页面无声切换到下一项。用户不知道跳过是否生效，可能重新点开该项（它仍在待处理列表，`:1712` 描述）造成困惑。

**P1-3 「返回队列」按钮实际跳转工作台，文案与行为不一致**
- 位置：`card-funds-review-page.tsx:950`（`onBack={() => router.push("/workspace")}`）vs `workflow.tsx:472-474`（按钮文案「返回队列」）
- 问题：违反 AGENTS.md「按钮文案必须与实际行为一致」。回到的是「今日工作台」而非任何队列页。

**P1-4 内部 ID 上屏（违反术语表规则 7）**
- 位置与示例：
  - `card-funds-review-page.tsx:1438-1443`：证据输入框标签「证据文档 ID」+ placeholder `doc_bank_slip_…`，强制用户填写内部 ID 前缀
  - `card-funds-review-page.tsx:1576-1581`：复核链「版本 … · 前驱 {predecessorReviewId}」，`rfr_*` 内部 ID 直出
  - `card-funds-review-page.tsx:1657, 1676`：确认对话框 description / lockedFields 中「应收 {task.account.id}」
  - `card-funds-review-page.tsx:1829-1830`：结果面板「操作编号 {workflowActionId}」「操作号 {operationId}」（`wa_w13_*` / `op_w13_*`）
  - `card-funds-review-page.tsx:899-915, 1840-1842`：驳回后继 Alert 标题与结果事实中直接展示 `REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED` 原值
- 问题：财务用户看不到业务含义；其中「操作编号」「操作号」两个近义字段并列属于冗余展示。

**P1-5 枚举原值 / 字段语义上屏**
- 位置：
  - `api.ts:192-194`：队列摘要「期初 OPENING」「差额 SYNC_DELTA」——原始枚举直出
  - `card-funds-review-page.tsx:1814`：HELD 结果事实 `outcome.workItemStatus === "IN_PROGRESS" ? "处理中" : outcome.workItemStatus`——非 IN_PROGRESS 时把 `PENDING` 原值漏给用户
  - `card-funds-review-page.tsx:1061`：指标 detail「APPLY−REVERSE」——内部字段语义缩写上屏
- 问题：全部违反术语表「枚举原值不得上屏」，需中文映射（如「已收减冲正」或删除）。

**P1-6 AllocationWorkspace 开发者说明文案直出**
- 位置：`card-funds-review-page.tsx:1329`（description="分配对象与金额由本页受控；差额由调用方展示，组件不重算业务。"）
- 问题：这是组件实现说明，直接暴露给财务用户；应改为业务说明（如「分配合计须等于单据含税金额」）。

**P1-7 「追加式链」「复核链」等架构词上屏**
- 位置：`card-funds-review-page.tsx:1536`（复核链卡片描述「追加式链 · 旧记录不可编辑删除」）、`:1432`（结论区说明「提交时将核对账户、复核链与数据版本」）、`:1692`（确认对话框 effects「追加复核链尾并完成任务」）
- 问题：术语表 §1.3 要求实现词翻译成业务语言，建议改为「历史复核记录只读，不可修改或删除」「提交时将核对账户、历史复核记录与数据版本」。

**P1-8 切换任务不重置回款/发票登记表单，金额可跨任务误带**
- 位置：`card-funds-review-page.tsx:226-233`（任务切换 effect 只重置 `evidenceRef`/`evidenceDocId`/`comment`/`allocationMode`，未重置 `receiptForm`/`invoiceForm`/`allocLines`）
- 问题：用户在任务 A 输入回款金额后按 j 切到任务 B，再开「登记历史回款」，金额仍是 A 的。误提交即把 A 的金额记到 B 头上，属数据一致性风险。

**P1-9 URL 参数 `q`/`due` 无界面控件，形成用户改不动的隐形筛选状态**
- 位置：`card-funds-review-page.tsx:141-144`（`due`、`q` 被消费进 filters）——页面无搜索框、无时限控件，`filterSummary`（`api.ts:197`）却会显示「搜索 xxx」；类型/范围有 ToggleGroup 可清除，`q`/`due` 一旦带参进入页面无法清除。
- 问题：违反 AGENTS.md「URL 参数与界面控件一一对应」；且 `scope=role_pool` 同样无切换控件（`card-funds-review-page.tsx:133-134`），从深链进入后无法回到「仅我的」。

**P1-10 结论区主按钮组未绑定 `formalPending`，提交期间可再次触发**
- 位置：`card-funds-review-page.tsx:1465-1526`（「复核通过/驳回/先跳过」无 `disabled` 绑定）vs 顶栏处理条正确传了 `processDisabled={formalPending}`（`:946-949`）
- 问题：`FormalActionConfirmDialog` 的 `onConfirm` 以 `void runApprove(...)` 调用（`:1697-1703`），`handleConfirm` 的 `await onConfirm()` 立即完成、内部 pending 失效；提交进行中用户再点结论区按钮会重新打开确认框（此时仅 dialog 内部按钮禁用）。同一任务并发提交的窗口未关死，建议与处理条一致绑定 `formalPending`。

### P2（体验瑕疵）

**P2-1 演示控件与正式功能混排**
- 位置：`card-funds-review-page.tsx:854-862`（筛选条「演示：完成前模拟数据变更阻断」checkbox）、`:1514-1525`（结论区内「演示：外部数据版本变更（仅演示）」按钮）
- 问题：演示/测试控件出现在正式决策区，与「复核通过」「驳回」同级，易误触；即使按 G6 保留演示标记，也应下沉到页面角落或折叠区。

**P2-2 autoNext 自动跳转 600/800ms 过快，结果面板几乎不可读**
- 位置：`card-funds-review-page.tsx:453-455`（通过后 600ms 前进）、`:522-524`（驳回后 800ms 前进）
- 问题：用户刚看到「复核通过 · 复核号 xxx」即被切换；结果面板与复核号是财务留痕信息，建议改为用户点击「下一项」或延时至可感知时长。

**P2-3 保存证据无成功反馈、按钮无 pending 禁用**
- 位置：`card-funds-review-page.tsx:663-684`（成功后不设置任何反馈）、`:1466-1472`（按钮无 disabled 绑定）
- 问题：保存是否成功全凭指标区静默刷新；连点可重复提交。

**P2-4 任务切换直接丢弃未保存的证据/备注输入，无脏值提示**
- 位置：`card-funds-review-page.tsx:226-233`（effect 重置输入）
- 问题：用户在证据框输入未点「保存证据」就按 j 切任务，内容静默丢失；共享组件 `DiscardConfirmDialog`（`feedback.tsx:1039`）可复用，或至少给出脏值提示。

**P2-5 快捷键提示弱、⌘↵ 在未领取时静默无效**
- 位置：`card-funds-review-page.tsx:1613-1615`（提示仅右栏小字）、`:698-717`（`activeLease` 为空时 ⌘↵ 无任何反馈）
- 问题：未领取（自动领取失败）时按 ⌘↵ 无响应也无引导；提示文案可提到结论区附近。

**P2-6 数据新鲜度「刚刚」硬编码**
- 位置：`card-funds-review-page.tsx:797-800`（`updatedAt="刚刚"`）
- 问题：与 `queueContextUpdatedAt` 并存却恒显「刚刚」，失去信息意义且误导（队列更新时间本就该用它）。

**P2-7 差异面板无差额汇总数字**
- 位置：`card-funds-review-page.tsx:1101-1115`（`BusinessDiffPanel` 仅字段级 before/after 表格，`audit-import.tsx:202-263`）
- 问题：SYNC_DELTA 任务的核心决策量"差额"需逐行扫读；可在面板头部补一行「合计差额」或把差异金额标红。

**P2-8 复核链短哈希/内部版本信息对财务用户信息量低**
- 位置：`card-funds-review-page.tsx:1576-1581`（每行「版本 {shortHash}」+ 前驱 ID）
- 问题：短哈希不具业务可读性，前驱 ID 又违反 P1-4；可仅保留「复核号 + 复核人 + 时间 + 结果」，版本校验细节留给审计。

---

## 6. 改进建议（按优先级）

1. **修复 P0-1（登记后复核被阻断）**：登记回款/发票成功后，用返回的 `fundsFactVersion`/新 subject 刷新本地租约（或直接置空 `leaseRef` 触发重新领取、或在 `ensureLease` 内比较 `task.workItem.subjectVersion` 与租约版本决定是否重领）。验收：登记 → 直接「复核通过」应一次成功。
2. **证据必填前置化（P1-1）**：打开确认框前校验证据输入；为空时在结论区就地标红并禁用「复核通过/从 0 起」，而不是提交后报错。
3. **跳过流程反馈（P1-2）**：`goToWorkItem` 增加"保留 lastResult"选项（或先跳转再渲染结果横幅），保证「先跳过」成功有可见结果；同时把对话框「可手动浏览下一项」改为与自动跳转一致（P2 文案顺带修正）。
4. **术语清零（P1-4/5/6/7）**：按术语表替换——`doc_*` 输入改为「回单号/发票号」业务引用；`前驱/操作编号/操作号/blockerCode/OPENING/SYNC_DELTA/PENDING/APPLY−REVERSE` 一律中文化或删除；AllocationWorkspace 描述改业务口径。
5. **表单状态隔离（P1-8）**：任务切换时重置 `receiptForm`/`invoiceForm`/`allocLines`，与证据字段同一 effect 处理。
6. **按钮契约对齐（P1-3/P1-10）**：「返回队列」改为「返回工作台」或真正跳队列；结论区主按钮绑定 `formalPending`，`onConfirm` 改为返回 Promise 使 dialog 内部 pending 生效。
7. **URL 控件补齐（P1-9）**：加 `q` 搜索框与 `due` 时限控件（或在参数带值时显示可清除的筛选 pill）；`scope` 加切换入口。
8. **低优先级**：演示控件移出决策区（P2-1）；autoNext 延时放宽或改为手动（P2-2）；保存证据加反馈（P2-3）；差异面板加合计差额（P2-7）；数据新鲜度用真实时间（P2-6）。
