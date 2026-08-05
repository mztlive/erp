# /fulfillment（履约操作工作台/队列）产品体验评审

> 评审日期：2026-08-05 ｜ 视角：产品经理 / UX
> 范围：`erp-client/features/fulfillment-operations/` 全部 10 个组件 + `mock/fulfillment-operations.ts`、`api.ts`、`validation.ts`、`components/business/audit-import.tsx`(WorkTaskItem)、术语表 `docs/ui-glossary.md`
> 严重度：P0 阻断操作 / P1 明显阻碍效率（含误导与数据风险）/ P2 体验瑕疵

---

## 1. 页面概述

W09 履约操作工作台，一线仓储/采购经办按队列连续处理五类履约作业：入库、公司仓发、供应商直发、电子交付、线下服务。目标用户是高频、重复、赶时效的操作员，核心诉求是「选任务 → 填最少字段 → 提交 → 进下一条」的闭环尽量顺、尽量快、尽量不出错。

**页面结构**：
- 页头：岗位通道标题（收货与发货 / 交付与代发 / 中性「履约处理」）+ 数据新鲜度 + 筛选摘要
- 类型分段（ToggleGroup）+ 筛选工具栏（搜索、范围、仓库、到期、货款情况、身份、自动下一项）
- 左侧：待办队列列表（compact 任务条目 + 剩余量徽章）
- 右侧：处理进度条 + 先款条件徽章 + 来源上下文卡 + 五类分派表单 + 校验汇总 + 底部动作条（先跳过 / 放弃修改 / 保存草稿 / 确认并下一条）
- 提交确认弹窗（含影响预览与不可逆提示）、跳过原因弹窗、结果反馈面板

**总体判断**：整体完成度高、业务化文案落地扎实（G1「先跳过」/G5「确认入库」等均已落地），连续处理链路（自动领取、聚焦首字段并全选、J/K 切换、Ctrl+S/Ctrl+Enter）是亮点。主要问题集中在**错误态被伪装成空态、跳过路径上的反馈丢失与数据风险、内部 ID 残留**三处。

---

## 2. 易用性

### 顺畅的部分
- 任务切换后自动领取处理权并聚焦第一个要填的框且全选（`fulfillment-operations-page.tsx:305-324`，`fulfillment-draft-form.tsx:14-20`），入库数量默认带出剩余量，操作员可直接改写，省一次鼠标。
- 底部主按钮随自动跳转动态更名「确认入库并下一条 / 确认入库」（`fulfillment-operations-page.tsx:1312-1321`），按钮文案与行为一致（术语表契约）。
- 只读角色不摆一排禁用按钮，换成「你只能查看。这条由…处理，预计…前完成。」+「打开销售单」（`fulfillment-operations-page.tsx:1323-1345`），符合术语表 §7 只读口径。
- 有未保存修改时切换任务/类型/筛选/清筛选全部被拦截并给出统一提示（多处 `setActionError("有没保存的修改…")`），防误切做得很完整。

### 不顺畅的部分
- **跳过路径的反馈闭环断裂**（P1）：见问题 4——「先跳过」成功后的确认横幅实际上从不展示，操作员提交一个高风险动作后得不到任何结果反馈。
- **跳过前的兜底保存会静默失败**（P1）：见问题 5——编辑过草稿再点「先跳过」，若自动保存失败，跳过照常执行，改动丢失。
- 服务类表单每次要手动选开始/结束两个时间，无「现在」快捷填充（P2，见问题 16）。
- 直发/电子/服务三个表单的数量输入未声明 `inputMode="decimal"`，与入库/仓发不一致（P2，见问题 9）。

### 高频操作可及性
确认、保存、跳过、放弃四个动作常驻底部 sticky 栏（`fulfillment-operations-page.tsx:1284`），快捷键有明示入口（`?` 展开），自动下一项开关在工具栏可随时切。高频路径设计良好。

---

## 3. 信息密度

### 队列列表（左栏）
compact 条目展示：任务类型 + 状态徽章、单号（销售/采购）、客户、截止、责任方，外加「待处理 N 单位」「另 N 行明细」两个徽章（`fulfillment-queue-list.tsx:38-88`）。信息量与操作台定位匹配，密度合理。
- 瑕疵：`待处理 N` 徽章只取**第一行**明细的剩余量（`fulfillment-queue-list.tsx:78`），多行任务时数字易被误读为整单剩余（P2，问题 15）。

### 详情卡（右栏）
来源上下文 6 字段网格（销售单/采购单/仓库/还剩多少/供应商/客户，`fulfillment-operations-page.tsx:1197-1256`）+ 表单。对一线操作员，「还剩多少」用品名拼接（`X；Y` 挤在一个单元格）在明细多时会折行变高，但可接受。
- 表单字段量：入库 = 时间 + 每行 4 字段；仓发/直发 = 时间/承运方/物流单号 + 每行 1 数量；电子 = 交付对象(只读)/时间/结果 + 每行 1 数量；服务 = 地点/起止/结果/说明 + 每行 1 数量。**总体克制，没有超出任务需要的字段**，每行带「剩余可收/留货/现有库存」上下文，密度判断为合格。

---

## 4. 交互合理性

### 加载 / 空 / 错误状态
- 加载：骨架屏（`fulfillment-operations-page.tsx:767-778`）✓
- 空态：按类型定制（「今天的入库都干完了」）、筛空空态、权限空态，出口齐全 ✓
- **错误态：缺失，且被伪装成空态**（P1，问题 1）——查询失败时页面落入「没有符合条件的任务」分支，操作员无法区分「没活干」和「系统坏了」。

### 表单校验反馈
- 客户端校验集中 `clientValidation`（`validation.ts:35-164`），错误经 `ValidationSummary`（「还差这些没填好」）聚合展示并带字段锚点，交互正确。
- 缺口：电子交付表单**完全无客户端校验**，服务表单不校验结果/地点，入库不校验质量结果（P2，问题 7/8）——当前被 mock 预填值（`mock/fulfillment-operations.ts:35/93/114` result 恒为 SUCCESS）掩盖。

### 自动跳转与批量
- 提交后按 `outcome.nextWorkItemId` 优先跳转、兜底 `neighborId(1)`（`fulfillment-operations-page.tsx:423-436`），有重复任务保护逻辑。✓
- 无批量操作需求（每次处理天然独立），合理。

### 误操作防护
- 重复提交：主按钮与 Ctrl+Enter 均以 `formalPending`（post/defer/claim 任一 pending）拦截（`fulfillment-operations-page.tsx:373-377, 614-633`）✓
- 数值错误：数量上限按「留货/剩余」校验（`validation.ts:110-117`），合格+不合格 ≤ 到货（`validation.ts:70-77`）✓
- 不可逆风险：确认弹窗带影响预览 + 不可逆提示（`CORRECTION_NOTICE`，`fulfillment-operations-page.tsx:1377-1397`）✓
- 唯一缺口是问题 5 的跳过前保存失败不阻断。

### 键盘 / 效率
J/K 与 ↑/↓ 切任务、Ctrl+S 保存、Ctrl+Enter 确认，`?` 展开帮助，均有 dirty 防护。瑕疵：↑/↓ 被全局劫持，焦点在左侧队列按钮上时方向键不会滚动列表（P2，问题 14）；自动下一项开启时成功反馈横幅整体被吞（P2，问题 2）。

### 其他
- 数据新鲜度、来源返回条、deep-link 定位销售单（`page:813-837`）细节到位。
- 队列过长时列表内滚动但当前项不自动滚入视野（P2，问题 10）。

---

## 5. 问题清单（按严重度）

### P0（阻断操作）
无。

### P1（明显阻碍效率 / 误导 / 数据风险）—— 5 个

1. **队列查询失败被渲染成「没有符合条件的任务」空态**
   `fulfillment-operations-page.tsx:1351-1375` 只处理 `isPending` 与 `emptyReason === "NO_PERMISSION"`，无 `isError` 分支；`queueQuery.isError` 时 `view` 为 undefined，落入末段 `BusinessEmptyState kind="filter"`。操作员会把「系统坏了」当成「没活干」，清筛选后依然无解，且可能误以为当天工作已完成。建议：加 `queueQuery.isError` 分支，渲染错误态 + 「重新加载」（refetch）按钮，避免与空态混用。

2. **「先跳过」前草稿兜底保存失败仍继续跳过，编辑内容静默丢失**
   `fulfillment-operations-page.tsx:505` `if (dirty) await handleSave()`，但 `handleSave`（`page:406-421`）内部 try/catch 只 `setActionError` 不抛出，保存失败后跳过照常执行并离开当前任务，操作员刚填的数量/说明丢失且无感知。建议：保存失败时中止跳过流程并提示「先保存成功再跳过」，或跳过弹窗内改为手动选择。

3. **先款徽章文案与任务类型/实际阻断范围矛盾（仓发 + BLOCKED）**
   `fulfillment-operations-page.tsx:1066` blockedBadge「暂时不能收货」、`page:1077` blockedBody「差额补齐之前，入库、直发、电子交付和服务都确认不了」；而仓发任务与入库共享采购单的门禁覆盖（`api.ts:67-98` 按 `poId` 覆盖 gate），仓发任务会以「确认发货」主按钮挂「暂时不能收货」徽章，且正文枚举里漏掉仓发。措辞应随作业类型区分「收货/发货」，阻断范围与实际一致（术语表契约：状态说结果、文案不撒谎）。

4. **「先跳过」成功反馈被立即吞掉；残留路径泄漏枚举原值**
   `fulfillment-operations-page.tsx:525-536` `setLastResult({status:"blocked",…})` 后紧接 `goToWorkItem(nextId)`，而 `goToWorkItem`（`page:339-348`）先 `setLastResult(null)`，同一批状态更新中横幅被清掉，「已跳过这一条 · 原因：…」从不展示；只有无下一条的边角路径会残留横幅，而该横幅 facts 直接输出 `lastResult.outcome.workItemStatus`（`page:931-932`），渲染出「PENDING」这类枚举原值，违反术语表「枚举原值不得上屏」。建议：跳过成功反馈改为在跳转后仍可见的 toast/轻提示，并给 workItemStatus 补中文映射。

5. **内部 ID（so_*/po_*/wh_*）泄漏进用户可见界面**
   `fulfillment-queue-toolbar.tsx:159` `销售单 ${salesOrderId}`、`:167` `采购单 ${purchaseOrderId}`（值为 `so_1002`/`po_2001` 这类内部 ID，见 `mock/fulfillment-operations.ts:145/198`）；`api.ts:166` `仓 ${filters.warehouseId}`、`:169` `销售 ${filters.salesOrderId}` 拼进 `filterSummary`，并被页头直接展示（`fulfillment-operations-page.tsx:806`）。违反术语表 §2/§7「内部 ID 不得进界面」口径。建议：chip 与摘要都显示业务单号（salesOrderNo / purchaseNo / 仓库名），ID 只进 URL 不进界面。

### P2（体验瑕疵）—— 12 个

6. **自动下一项开启时，提交成功横幅从不展示**
   `fulfillment-operations-page.tsx:483-485` 成功态 `setLastResult` 后被 `advanceIfNeeded → goToWorkItem` 的 `setLastResult(null)`（`page:341`）覆盖，记录编号、库存变化事实、验收提醒（`NOT_ACCEPTANCE_NOTICE`）操作员全部看不到。设计上有意为之（防打断），但关键事实零反馈；建议连续模式给个紧凑 toast 摘要（含记录编号），完整事实保留在停留模式下。
7. **电子交付表单无任何客户端校验**
   `validation.ts:127-162` 只校验服务表单；ELECTRONIC 分支为空，结果/数量为空或为负可直接过客户端校验，依赖服务端兜底。当前被 mock 预填 SUCCESS 掩盖，接真实数据即暴露。
8. **服务表单不校验履约结果与服务地点；入库不校验质量结果**
   `validation.ts:145-162` 仅校验时间与完成说明；`validation.ts:57-79` 未校验 `qualityResult` 是否已选。均存在空值提交路径。
9. **直发/电子/服务数量输入缺 `inputMode="decimal"`**
   `fulfillment-direct-form.tsx:77`、`fulfillment-electronic-form.tsx:80`、`fulfillment-service-form.tsx:105`；入库（`fulfillment-receipt-form.tsx:61`）与仓发（`fulfillment-ship-form.tsx:83`）已带，三处不一致，移动端/触屏数字录入体验差。
10. **队列当前项不自动滚动到可见区**
    `fulfillment-queue-list.tsx:37-38` 列表容器 `max-h-[min(36rem,70vh)] overflow-y-auto`，切换任务后若当前项在容器外，操作员看不到自己在队列中的位置。建议滚动到选中项。
11. **长队列无分页/虚拟化**
    `fulfillment-queue-list.tsx:37` 全量渲染；演示数据量小无感，量级上来会卡。
12. **两处「清除筛选」口径不一致**
    工具栏「清除筛选」保留类型分段（`fulfillment-queue-toolbar.tsx:174-191`），空态「清除全部筛选」连类型一起清（`fulfillment-operations-page.tsx:1023/1370`），同名不同义易误导。
13. **只读角色下 scope 参数无控件可改**
    `fulfillment-queue-toolbar.tsx:117-127` `showScope=false` 隐藏选择器，但 URL `scope` 仍参与查询且无法清除，违反「URL 参数与界面控件一一对应」契约（可触发场景：先以可执行角色切到「全组」，再换只读角色）。
14. **方向键全局劫持影响列表滚动**
    `fulfillment-operations-page.tsx:640-657` 将 ArrowDown/Up 绑定任务切换，而队列条目是 `<button>`（`fulfillment-queue-list.tsx:39`）不属 `inField`，焦点在列表上时按方向键不能滚动列表，只能靠鼠标。
15. **「待处理 N」徽章只取首行剩余量**
    `fulfillment-queue-list.tsx:78` 多行明细任务（如 2+ 行）时数字单指第一行，需配合「另 N 行明细」才能猜出全貌，易误读为整单剩余。
16. **服务表单开始/结束时间无「现在」快捷填充**
    `fulfillment-service-form.tsx:43-58` 每次需手动选两个日期时间；一线作业常见「刚做完」场景，应默认带出当前时间（或提供「填当前时间」小按钮）。
17. **确认弹窗锁定字段「来源单据版本」略技术化**
    `fulfillment-operations-page.tsx:1389` 术语表口径为「版本」；建议改为「来源单据、版本和留货」这类业务组合。

---

## 6. 改进建议

**优先级一（数据正确性 & 反馈闭环）**
- 补 `isError` 分支（问题 1），错误态必须与空态分离，附「重新加载」。
- 「先跳过」改为「保存成功才允许跳过」，或弹窗内提供「保存并跳过」并显式展示保存结果（问题 2）。
- 连续模式（autoNext 开）下用 toast 保留成功摘要（记录编号 + 一句话库存影响），停留模式保持现有横幅（问题 6、4）。

**优先级二（术语合规，低成本高价值）**
- 筛选 chip 与 filterSummary 全部改用业务单号/仓库名，ID 不进界面（问题 5）。
- 补 `workItemStatus` 中文映射，横幅 facts 不再输出枚举原值（问题 4）。
- 先款徽章文案随作业类型区分收/发货，阻断范围与实际一致（问题 3）。

**优先级三（效率打磨）**
- 服务表单默认时间 = 当前时间（问题 16）；三处数量输入补 `inputMode`（问题 9）。
- 队列列表选中项自动滚入视野，长列表考虑虚拟化或分页（问题 10、11）。
- 方向键劫持仅当焦点不在列表/按钮上时生效，或改为仅 J/K（问题 14）。
- 补齐电子/服务/入库的 result、qualityResult 客户端校验（问题 7、8）。
- 统一两处清除筛选的口径（问题 12），只读角色 scope 参数随角色切换清掉（问题 13）。
