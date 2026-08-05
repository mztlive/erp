# UX 评审：客户往来 /finance/customer-accounts（W11）

> 评审日期：2026-08-05 ｜ 评审视角：产品经理 / UX
> 代码范围：
> - `erp-client/features/customer-receivables/customer-receivables-page.tsx`（主页面，1731 行）
> - `erp-client/features/customer-receivables/allocation-session-panel.tsx`（核销工作区，743 行）
> - 关联：`features/customer-receivables/{api,session,types,queries}.ts`、`components/business/{list,page,feedback,editor}.tsx`
> 文案依据：`docs/ui-glossary.md`（术语表 v1.2）

---

## 1. 页面概述

「客户往来」是客户应收的核心工作台，业务模型：**应收台账（应收子账）** 与 **回款 / 销项发票** 两条独立流水轨道，通过「核销（分配）」动作将回款/发票分配到应收目标上。页面由三部分组成：

1. **列表页**（`customer-receivables-page.tsx`）：4 个视图 Tab（应收台账 / 回款 / 销项发票 / 待核销）+ 4 张可点击过滤的指标卡 + 搜索与筛选 + 分页表格 + 右侧详情预览抽屉（768px）。
2. **核销工作区**（`allocation-session-panel.tsx`）：全屏会话（`sessionId` URL 深链），左侧记录表单（回款/发票信息）+ 右侧同主体待核销池，下方分配明细表与提交区。
3. **纠错动作**：回款冲正 / 退款 / 红票，通过预览抽屉底部按钮触发，带原因说明的确认对话框。

整体架构清晰、状态与数据分离良好（TanStack Query + 受控会话），已按术语表做了大量业务化改写（如「登记回款」「核销」「待核销」）。主要问题集中在**核销工作区的目标选择效率、纠错动作的防误操作、以及少量实现术语/内部 ID 泄漏**。

---

## 2. 易用性

**顺畅的路径（做得好的）：**

- 查看应收：指标卡点击即切换视图 + 到期筛选（`page.tsx:440` MetricFilterItem 带按钮语义与选中态），比隐藏筛选更直接。
- 行内「预览」开抽屉、行内「核销」一键进入会话（`customer-receivables-page.tsx:472-495`），两步内完成主任务。
- W05 销售单深链自动创建会话并预选应收目标（`session.ts:261-301`），回链上下文处理完整（返回 Alert + 自动回跳）。
- `/` 快捷键聚焦搜索框（`customer-receivables-page.tsx:221-240`）。
- 键盘导航 / aria 标签齐全（`/` 快捷键、`aria-label` 覆盖搜索、筛选、分页、分配金额输入）。
- 空态区分「无数据 / 无匹配筛选 / 无数据范围 / 权限收回」四种，且有对应动作（清除筛选、申请数据范围提示），没有用 0 元假装无应收。

**不顺畅的路径（核心问题）：**

- **行内「核销」丢失目标上下文（P1）**：用户在应收台账点某一行「核销」，`startSession` 只传了往来主体（`customer-receivables-page.tsx:488-491`），池是**该主体全部**开放目标，用户刚点的那一行并不被预选/置顶。同主体有多个开放应收时，用户必须在池里再翻找一遍刚才点的那张单。对比 W05 深链是带 `receivableAccountId/salesOrderId` 预选的（`session.ts:261-301`），行内入口却没有复用这套能力。
- **「登记回款」双入口行为不一致（P1）**：顶部「登记回款」走主体选择对话框（`customer-receivables-page.tsx:1408-1460`），行内「核销」直开；同一个动作两个入口，新用户会疑惑区别。且行内入口没有「选择主体」确认环节，误点即进会话。
- **核销工作区中「记录表单」与「核销池」左右分栏**：发票模式下表单有 7 个字段（代码/号码/日期/含税/不含税/税额），加上右侧池列表，1280px 宽度下仍偏挤；`lg:grid-cols-2`（`allocation-session-panel.tsx:445`）在常见笔记本上两栏都很窄。

## 3. 信息密度

**合适的部分：**

- 指标卡 4 宫格：开放应收 / 已逾期应收 / 待分配回款 / 待分配销项发票，大号数字（`page.tsx:474-481`）+ 说明行（"需催收""已到账""卡券待复核 N"），层次分明。
- 表格金额列右对齐 + `num` 字体 + 行内 `MoneyValue` 千分位；列头明确标注「（含税）」口径（`customer-receivables-page.tsx:400-424`）。
- 待核销视图把回款与发票分区展示，并用 Alert 明确「两类未分配余额不得相加」（`api.ts:451`），防止财务误读单一指标。

**过密 / 有歧义的部分：**

- **「净已开票 / 可开票」列口径混排（P2）**（`customer-receivables-page.tsx:416-424`）：`invoicedTotal` 是「净」（不含税），`openInvoiceableTotal` 与「开放应收」同源（含税口径），同一列两个数字一净一税，且列头未标口径。财务读表极易拿两个不同基数的数字直接比。
- **「销售单 / 子账」列**（`customer-receivables-page.tsx:387-396`）：`子账 #1 · 卡券` 的「子账」是内部账户概念，全文无解释；且「净已开票/可开票」「开放应收（含税）」等 6 个数字列 + 到期 + 状态 + 操作共 8 列，横向信息量大，移动端虽有冻结列但数字列间视觉区分弱（无斑马纹、无分隔线）。
- **待核销分区标题的未分配金额未格式化（P2）**（`customer-receivables-page.tsx:1098-1099、1127-1128`）：`未分配 {metrics?.unallocatedReceiptTotal}` 直接插裸字符串，无千分位、无货币符号，与表格里 `MoneyValue` 的格式不一致。
- **指标卡「开放应收」的 detail 是空标签（P2）**（`customer-receivables-page.tsx:912-913`）：detail 固定写「系统更新时间」但没有时间值（真实时间在页头 DataFreshness），相当于一个悬空标签。
- 核销工作区的摘要（待分配总量 / 已分配 / 拟未分配，`allocation-session-panel.tsx:589-613`）与行内金额、池中开放余额口径一致（均含税），这一块密度控制得好。

## 4. 交互合理性

**加载 / 空 / 错误状态：**

- 指标卡骨架屏、表格骨架屏、会话加载骨架屏齐全（`customer-receivables-page.tsx:954-963、1087-1088、714-720`）。
- 列表错误态给出「重试」；会话失效给出「返回列表」（`customer-receivables-page.tsx:722-739`）；权限/数据范围态不伪装空数据。
- 瑕疵：**预览抽屉错误态误报「未找到对象」（P2）**（`customer-receivables-page.tsx:1402-1403`）：`detailQuery.isError` 与 `data === null` 未区分，网络错误会显示成"记录不存在"。
- 列表错误文案「未展示 0 元结清结论」（`customer-receivables-page.tsx:766`）是内部口径，业务用户读不懂。

**核销交互（核心区）：**

- 多选：池中逐行「加入」→ 进入分配表，重复加同目标会被忽略（`allocation-session-panel.tsx:182`），有基本防护；但**加入后该行无「已加入」状态**，只是按钮禁用（P2）。
- 金额分配：每行一个 `inputMode="decimal"` 输入框，实时校验（负数、超开放余额、超记录金额）在下方 ValidationSummary 汇总（`allocation-session-panel.tsx:151-173`），提交按钮在错误时禁用——校验链路完整。
- **无「一键分配全部余额」（P1）**：财务最常见操作是"到账 5 万全部分完"，现在必须对每个目标手输金额，多行时是纯体力活；「分配金额」输入框也无占位/无快速填满。
- **池中开放余额显示误导（P1）**（`session.ts:123-137`）：同一销售单的 N 条主增分录**每行都显示整单 openTotal**（"简化：按账户开放余额均摊"），用户看到两个目标各自"开放 12,345.67"，实际分配完第一个后第二个余额已被消耗，提交时才被 OVER_ALLOCATE 拒绝。展示的"可分配上限"与真实上限不符。
- **移除分配行无确认、无撤销（P2）**（`allocation-session-panel.tsx:208-210`）：点删除图标即丢失该行金额，无二次确认。
- 提交防重复：幂等键 + 提交中禁用（`allocation-session-panel.tsx:246-248、685`），BALANCE_CONFLICT 后自动刷新目标余额（`applyPostResult` 339-347），做得专业。
- **提交成功后工作区仍可编辑（P2）**（`allocation-session-panel.tsx:289-309`）：成功后只置结果横幅，分配表与表单要等会话 refetch 完成才变禁用（`disabled={session.status === "posted"}`），存在一个"已成功但按钮还能点"的窗口；幂等兜底了数据层，但体验上应立刻锁定。

**确认对话框与误操作防护：**

- 提交确认对话框（FormalActionConfirmDialog）列出状态迁移、锁定字段、影响清单，防护到位。
- **冲正 / 退款 / 红票风险控制不足（P1）**（`customer-receivables-page.tsx:1322-1386` + `session.ts:788-877`）：
  1. 对话框只有「原因说明」，**没有金额输入**，而 mock 行为是**全额**冲正/退款/红冲（`session.ts:890` 红票可取 amount 但 UI 从不传；回款类直接全量反冲）——UI 没有任何「将全额冲正」的提示；
  2. 「冲正」与「退款」两个按钮行为完全一致（都全量反冲 + 追加反向单，仅单号前缀 TK/CZ 不同），产品语义差异（内部纠错 vs 真实退款）没有向用户说明。
- **离开工作区无草稿确认（P1）**（`allocation-session-panel.tsx:377-380`）：「取消并返回」直接丢弃未保存输入（头部有「未保存草稿」字样但无任何离开确认，也无 beforeunload 拦截）。财务人员误关页面丢失半天输入。
- 主体选择对话框**默认预选第一个主体**（`customer-receivables-page.tsx:324`），用户以为没选也能点「打开核销工作区」，有选错主体风险（会话创建后不可换主体）。

**反馈提示：**

- 成功/未知/失败三类结果用 FormalActionResult 分色呈现，未知结果给「查询最终结果」按钮（`allocation-session-panel.tsx:411-419`），符合术语表"处理结果待确认，请勿重复提交"口径。
- **保存草稿成功反馈过弱（P2）**：仅头部小字从「未保存草稿」变「草稿已保存」（`allocation-session-panel.tsx:373`），无时间戳、无 toast，用户可能误以为没保存。
- 导出是**假成功**（P2）：「导出任务已创建 … 7 天内可下载（演示）」（`customer-receivables-page.tsx:809-816`）——永远不会有文件可下载，且伪造成成功结果横幅，容易误导。

**文案合规（对照 ui-glossary.md）：**

- ❌ **P0：内部字段名 `counterparty_party_id` 上屏**：
  - 会话头部 note「核销严格按 counterparty_party_id 锁定；池中仅同主体开放对象。拟分配合计仅作输入提示，不冒充核销。」渲染于 `allocation-session-panel.tsx:375`（来源 `session.ts:335-337`）——命中术语表 §2 第 5 轮 P0「字段名/枚举原值上屏」；
  - 跨主体拒绝错误「跨 counterparty_party_id 分配被拒绝…」（`session.ts:435-437`）展示于工作区错误 Alert（`allocation-session-panel.tsx:440-441`）——同类问题。
- ❌ **P0：内部 ID 上屏**：W05 入口 Alert「销售单 {salesOrderId}」（`customer-receivables-page.tsx:845`）直接渲染 `so_1001` 这类内部 ID（mock 数据证实），命中术语表 §7「内部 ID 不得进界面（rsv_*/pla_*/sv_*/wi_*…换成业务单号）」。
- ✅ 其余主路径文案（登记回款、核销、待核销、冲正、红票、未分配余额、允许保留未分配余额（系统统一判定））符合术语表口径；「会话」类内部词已按 G2/术语表 §2 P2 改写为「本次核销」「本次草稿」。

---

## 5. 问题清单（按严重度）

### P0（阻断 / 必须清零）

| # | 问题 | 位置 |
| --- | --- | --- |
| P0-1 | 内部字段名 `counterparty_party_id` 泄漏到两处用户可见文案（会话头部 note、跨主体拒绝错误消息），违反术语表 §2 第 5 轮 P0「字段名/枚举原值上屏必须清零」 | `allocation-session-panel.tsx:375`；`session.ts:335-337`、`session.ts:435-437` → `allocation-session-panel.tsx:440-441` |
| P0-2 | W05 入口 Alert 将内部销售单 ID（`so_1001`）原样展示，违反「内部 ID 不得进界面」，应改为业务单号（如 XS2026…）或中文标签 | `customer-receivables-page.tsx:840-856`（`销售单 {salesOrderId}` 于 :845） |

### P1（明显阻碍效率）

| # | 问题 | 位置 |
| --- | --- | --- |
| P1-1 | 行内「核销」/详情「登记回款并核销」不携带目标上下文，池不预选、不置顶用户刚选中的应收；同主体多单时需二次翻找 | `customer-receivables-page.tsx:487-495`、`1293-1304`；对比 `session.ts:261-301` |
| P1-2 | 冲正/退款/红票：无金额输入、无「全额执行」提示；「冲正」与「退款」行为与文案差异不明，财务误操作风险高 | `customer-receivables-page.tsx:1322-1386`；`session.ts:788-877` |
| P1-3 | 离开核销工作区（取消并返回/关标签页）无未保存草稿确认，输入可被静默丢弃 | `allocation-session-panel.tsx:377-380` |
| P1-4 | 池中同单多个分配目标均显示整单开放余额，展示上限与真实可分配上限不符，误导分配导致提交被拒 | `session.ts:123-137`（池构建）→ `allocation-session-panel.tsx:562-565` 展示 |
| P1-5 | 无「一键分配全部/填入开放余额」快捷操作，多行分配需逐行手输金额 | `allocation-session-panel.tsx:650-658`、`202-206` |
| P1-6 | 发票核销的「不含税/税额」需财务手算，gross 变更无自动推算（税率固定 13% 场景），易输入错误 | `allocation-session-panel.tsx:515-528`；`session.ts:663-664` |
| P1-7 | 禁用按钮不解释原因：顶部「登记回款/登记销项发票」、行内「核销」禁用时无 tooltip/说明（术语表 §7：禁用态不解释原因） | `customer-receivables-page.tsx:819-834`、`482-495` |
| P1-8 | `status`、`reviewStatus`、`salesOrderId`、`receivableAccountId` 等 URL 参数被查询消费但无界面控件（仅「清除筛选」能清部分），违反 AGENTS.md「URL 参数与界面控件一一对应」 | `customer-receivables-page.tsx:120-121`、`158-172`、`1163-1175` |

### P2（体验瑕疵）

| # | 问题 | 位置 |
| --- | --- | --- |
| P2-1 | 待核销分区标题「未分配」金额为裸字符串，无千分位/货币格式，与表格 MoneyValue 不一致 | `customer-receivables-page.tsx:1098-1099`、`1127-1128` |
| P2-2 | 指标卡「开放应收」detail 写死「系统更新时间」但无时间值，为悬空标签 | `customer-receivables-page.tsx:912-913` |
| P2-3 | 导出为伪成功结果（「7 天内可下载（演示）」实际无文件），易误导 | `customer-receivables-page.tsx:809-816` |
| P2-4 | 演示开关（模拟结果不确定 / 模拟跨主体提交拒绝）常驻业务工作区底部，误勾选会得到诡异结果 | `allocation-session-panel.tsx:696-718` |
| P2-5 | 预览详情错误态与「未找到对象」未区分，网络错误误报为记录不存在 | `customer-receivables-page.tsx:1402-1403` |
| P2-6 | 「净已开票 / 可开票」列净/含税口径混排且列头未标口径 | `customer-receivables-page.tsx:416-424` |
| P2-7 | 列表错误文案「未展示 0 元结清结论」为内部口径 | `customer-receivables-page.tsx:765-766` |
| P2-8 | 主体选择对话框默认预选第一个主体，可能无感选错主体（会话锁定不可换） | `customer-receivables-page.tsx:322-326`、`1408-1460` |
| P2-9 | 移除分配行无确认/无撤销，误删即丢金额 | `allocation-session-panel.tsx:208-210` |
| P2-10 | 保存草稿成功反馈过弱（仅头部小字变化，无时间无 toast） | `allocation-session-panel.tsx:237-238`、`373` |
| P2-11 | 提交成功后到会话 refetch 完成间，分配表仍可编辑、按钮仍可点 | `allocation-session-panel.tsx:289-309`、`685-691` |
| P2-12 | 池行「加入」后仅按钮禁用，无「已加入」视觉状态 | `allocation-session-panel.tsx:568-577` |
| P2-13 | 4 个视图 Tab 共用同一分页状态，切换视图可能落空页 | `customer-receivables-page.tsx:134-137`、`965-986` |

---

## 6. 改进建议（按优先级）

**P0 文案清零（1 小时内可完成）**
1. 删除 `session.ts:336` note 中的 `counterparty_party_id`，改为业务表述：「本次核销已锁定往来主体，池中仅同主体的开放应收；拟分配合计仅作输入提示，以提交后系统结果为准。」跨主体错误改为「仅可分配当前往来主体的开放应收，已拒绝提交。」（`session.ts:435-437`）
2. W05 Alert 的 `销售单 {salesOrderId}` 改为 `销售单 {salesOrderNo}`（随会话数据传入）或删除编号、仅保留「返回销售单」按钮（`customer-receivables-page.tsx:845`）。

**P1 主路径效率（核销工作区）**
3. 行内「核销」与详情「登记回款并核销」把 `salesOrderId / receivableAccountId` 传入 `startSession`，复用 W05 的预选逻辑（`session.ts:261-301`），点击的行自动进入分配表且池中置顶。
4. 分配表每行加「填满」快捷按钮（填入 min(记录余额, 目标开放余额)）；输入框加千分位展示。
5. 池目标金额按真实开放余额逐分录返回（修 `session.ts:123-137` 的均摊简化），展示与提交校验口径一致。
6. 冲正/退款对话框增加金额输入（默认全额并明示「将按全额追加反向记录」），并给出「冲正=撤销本次回款记录」「退款=向客户退回资金」的一行说明；红票同理（已支持 amount 入参，UI 未用）。
7. 离开工作区时若 `draftSavedAt` 早于最近编辑或从未保存，弹确认：「本次核销尚未保存草稿，确定离开？」。
8. 禁用按钮挂 `GuardedBusinessAction` reason 或 tooltip 说明（登记权限、已结清无余额等）。
9. 发票表单 gross 变化时按 13% 预填 不含税/税额（可编辑覆盖）。

**P2 打磨**
10. 待核销分区标题用 `MoneyValue` 渲染；指标卡 detail 接真实时间或删标签。
11. 导出改为明确的演示占位说明（或做成真实 CSV 导出，演示数据量小，成本极低）。
12. 演示开关移入独立的「演示面板」（如固定角落小图标展开），避免与正式操作同框。
13. 预览错误态区分 isError 与 not-found；「净已开票 / 可开票」列头标注（净/含税）。
14. 提交成功后立即以 `succeeded` 状态锁定表单与按钮，不等 refetch。
15. 池行加入后显示「已加入 ✓」徽章；移除分配行加轻量确认（或 5 秒撤销条）。
16. 「登记回款」顶部入口与行内「核销」合并文案或统一行为，避免双入口歧义。
