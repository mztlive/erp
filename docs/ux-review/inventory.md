# UX 评审报告：/inventory（库存台账）

> 评审对象：`erp-client/features/inventory/inventory-ledger-page.tsx`（2141 行）及同目录
> `api.ts` / `queries.ts` / `cursor.ts` / `types.ts`、`mock/inventory.ts`、`mock/session-state.ts`、
> 共享组件 `components/business/{option-combobox,entity-comboboxes,selectors,data-table,feedback,page,list}.tsx`、
> `lib/patch-search-params.ts`、`lib/ui-text.ts`。
> 评审视角：产品经理 / UX；纯分析，未改动任何代码。
> 术语基准：`docs/ui-glossary.md`（禁用词表：枚举原值不上屏、内部 ID 不上屏、W 编号不进用户提示）。

---

## 1. 页面概述

- 路由：`/inventory`（W10），页头"库存台账"，面包屑「采购与履约 › 库存台账」。
- 页面结构（自上而下）：页头（数据新鲜度 + 刷新/导出）→ 操作结果条 / 导出任务进度 / 错误 Alert →
  库存边界说明 Alert → 4 项指标筛选条（库存组合/有效预占组合/零可用组合/待处理调整）→
  4 个视图 Tab（余额/流水/销售预占/调整记录）→ 工具条（搜索 + 仓库 + 可用状态 + 流水类型 + 日期 + 排序 + 清除筛选）→
  数据表格（分页）→ 余额详情抽屉（最近流水/来源单据/有效预占/进行中的调整）→ 库存调整对话框 → 确认提交对话框。
- 数据链：TanStack Query + mock API，全部查询条件走 URL 参数（view/q/warehouseId/skuId/availability/
  movementType/occurredFrom/occurredTo/sort/cursor/pageSize/balanceId/adjustmentId/salesOrderLineId），
  列表 `fetchInventoryList` 按视图切页返回。
- 核心任务：查库存（按品/仓库）、看结存与可用、追溯出入库流水、查看销售预占、发起库存调整。

## 2. 易用性

**顺畅的部分**
- 查库存路径短：进页即见指标条 + 余额表；搜索框支持 SKU 编码、名称、规格、仓库（`inventory-ledger-page.tsx:1386`），
  仓库下拉可搜索，指标条一键切"零可用/有预占"。
- 查流水路径完整：余额行「查看」→ 详情抽屉 →「查看全部流水」直达流水视图；流水来源单据可链回履约/销售页。
- 键盘细节用心：`/` 聚焦搜索（`:294-312`）、关闭抽屉后焦点回落触发行（`:409-418`）、
  表行 Enter 打开详情（`onRowOpen`，`:1611-1612`）。
- 调整流程的岗位分离、版本冲突、结果未定三态都给了面向业务的文案，流程闭环（草稿 → 提交待复核 → 结果可查询）。
- 移动端只读降级明确（`:453-456`、页头说明）。

**阻碍效率的问题**
- **流水视图永远看不全**：进入流水 Tab 默认被"2026-07-03 ~ 2026-08-02"硬编码窗口过滤（`:118-119`、`:222-227`），
  且「清除筛选」把日期参数删掉后回落到同一默认窗口（`:1521-1543`），手动清空日期输入框同样弹回默认值——
  **没有任何路径能看到全量流水**；窗口日期写死，随真实时间推移会越来越错（详见 P1-1）。
- **批次维度缺失**：需求口径"按品/仓库/批次查询"中批次无任何入口（搜索与筛选均无批次字段），
  批号只能靠来源单据号间接找。属功能缺口，非 bug。
- 从余额详情「查看全部流水」跳转存在 URL 竞态，可能弹回余额视图并留下隐形筛选（详见 P1-2）。

## 3. 信息密度

**密度基本合理**
- 余额表 7 列、compact 密度、`identity`/`actions` 左右固定；数字等宽 + 单位小字（`:165-174`）；
  零可用行带红色徽章（`:642-646`），关键数字扫读无障碍。
- 结存（账面现存）/ 有效预占 / 可用数量在"指标条 → 表列 → 详情卡"三层重复出现，突出度足够；
  详情卡可用数量用 primary 色强调并注明"系统计算"（`:1753-1763`）。无"在途"概念（领域内不适用，不扣分）。
- 详情抽屉按"最近流水/来源单据/有效预占/进行中的调整"分节，业务对象齐全。

**失衡之处**
- 常驻「自有实物库存边界」Alert（`:1273-1279`）内容偏技术且较长（期初说明 + 排除口径两段），
  与页脚"页面不提供编辑库存…纠错须走调整单"（`:1927`）语义重复，窄屏下挤压首屏，多数用户不会读。→ P2-9
- 预占表"建立/剩余"一列内塞了建立/剩余/已消耗/已释放 4 个数（`:856-869`），信息量略超单格可扫读范围，
  但数字等宽 + 分行处理，可接受。

## 4. 交互合理性

**加载 / 空 / 错误态（齐全）**
- 初载骨架屏（`:996-1009`）、加载失败重试（`:1011-1025`）、权限收回专态（`:1027-1046`）、
  无数据范围专态（`:1048-1062`）、筛选无结果空态（`:1554-1579`）、无数据空态带「前往导入与期初」引导（`:1580-1596`）——
  四态五态区分完整，是加分项。
- 细节缺口：**refetch 失败时旧数据静默展示**（`isError && !data` 才渲染失败态，`:1011`），
  DataTable 的"正在刷新，当前内容会保留"横幅未接入，刷新失败无感知。→ P2-10

**筛选器与 URL**
- 筛选状态全部入 URL，刷新/分享可还原，清 cursor 逻辑正确（`patch-search-params.ts:48-50`）。
- 但有 3 个隐形参数无控件、清除按钮的条件与清单都不覆盖（详见 P1-3），违反 AGENTS.md「URL 参数与界面控件一一对应」契约。
- 排序参数跨视图残留（详见 P2-6）。

**分页 / 导出 / 按钮语义**
- 分页 cursor 编码（`cursor.ts`）+ pageSize 同步 URL，翻页 replace 不堆历史，`/` 快捷键、`aria-label` 齐全。
- 导出走后台任务进度组件，范围说明来自 `filterSummary`，语义完整；但任务号与文件名泄漏内部编号（P2-4）。
- 按钮语义基本"说动作"（查看/库存调整/提交待复核/查看全部流水/前往导入与期初），
  与行为一致；「完成并确认结果（仅演示）」是唯一与业务语义相悖的按钮（P2-8）。

## 5. 问题清单（按严重度）

### P0（阻断操作）· 0 项

### P1（明显阻碍效率）· 7 项

1. **流水日期窗口硬编码且无法清除，永远看不到全量流水**
   `inventory-ledger-page.tsx:118-119`（`MOVEMENT_FROM_DEFAULT = "2026-07-03"` / `MOVEMENT_TO_DEFAULT = "2026-08-02"`）、
   `:222-227`（参数缺失即回落到默认窗口）、`:1521-1543`（清除筛选置 null 后仍回落到默认窗口）。
   后果：流水 Tab 首屏即被限 30 天窗口；「清除筛选」与手动清空日期都无法恢复全量；
   日期写死为 2026 年常量，时间推移后默认窗口将显示过期区间，用户查历史批次必漏数据且无感知。
2. **「查看全部流水」URL 竞态，可能弹回余额视图并遗留隐形筛选**
   `inventory-ledger-page.tsx:1697-1705`：先 `patchUrl({view:"movement", balanceId, warehouseId, skuId})`（push），
   紧接着 `closeDetail()` 再 `patchUrl({balanceId:null})`（replace）。第二次 replace 基于渲染快照的旧
   searchParams（view=balance）重建 URL（`patch-search-params.ts:43-57`），会把 view 弹回 balance；
   即便留在流水视图，`skuId`/`warehouseId` 也是无控件的隐形筛选（skuId 无任何控件）。用户会落在
   "余额视图只剩 1 行"或"流水被锁死在单品"且看不到原因。
3. **隐形筛选参数 `salesOrderLineId` / `adjustmentId`：无控件、清除按钮不渲染、清除也不清**
   `inventory-ledger-page.tsx:213`、`:216`（读参）、`:1513-1520`（清除按钮出现条件不含这两个参数）、
   `:1527-1536`（清除列表不含这两个参数）；api.ts:376-380、401-404 确实消费它们。
   从销售单/调整单深链进入后（`mock/inventory.ts:754` 生成 `/inventory?view=adjustment&adjustmentId=…`），
   页面被过滤到 1 条且**界面上没有任何提示与清除入口**（清除按钮都不出现），只能手改 URL，属于隐形状态陷阱。
4. **来源单据按钮直接显示 W 编号（W09/W10/W18/W05）**
   `inventory-ledger-page.tsx:1841`（`{doc.workspaceId ?? "打开"}` 作按钮文字），
   `mock/inventory.ts:746-773` 的 `workspaceId` 即 "W09"/"W10"/"W18"/"W05"。
   术语表 §3.6「禁止 W 编号进提示」；`lib/ui-text.ts:115-129` 已有 `workspaceLabel` 中文映射（销售单/履约处理/导入与期初），
   此处未用，按钮对业务用户是不可理解的代号。
5. **确认提交对话框泄漏数据库字段名**
   `inventory-ledger-page.tsx:2131`（effects："不立即修改 on_hand / reserved / available"）。
   术语表规则 7"不把字段名当文案"；`on_hand/reserved/available` 是 DB 列名原值上屏，应改「账面/预占/可用数量不变」。
6. **流水筛选摘要泄漏枚举原值**
   `api.ts:151-153`：`filterSummary` 渲染 `流水类型 PURCHASE_RECEIPT、STOCK_ADJUSTMENT…`。
   该字符串出现在表框架描述（`:1363-1364` 区域）与导出范围说明中，枚举原值直接上屏。
7. **期初说明泄漏旧商城数据库字段名**
   `api.ts:50-51`（`OPENING_STOCK_NOTE`）：「旧商城 stock / total_stock 不作为 ERP 库存记录」——
   用户可见文案出现 `stock` / `total_stock` 字段名，应改「旧商城的库存数量」。

### P2（体验瑕疵）· 10 项

1. **仓库筛选下拉把内部 ID 当仓库编码展示**
   `inventory-ledger-page.tsx:1406-1411`（`warehouseCode: w.id`），种子数据明明有 `code: "WH-E01"`（`mock/inventory.ts:142-144`）；
   `selectors.tsx:127` 会把 code 渲染在选项里 → 用户看到 `wh_east_1`。内部 ID 上屏，且丢掉了业务编码。
2. **「余额版本 N」锁版本概念暴露**
   `inventory-ledger-page.tsx:1984-1988`（调整框元信息卡）与 `:2127`（确认框锁定字段）。
   术语表 §4「lockVersion → 数据已更新，请刷新后重试」，版本号本身对用户无意义；冲突场景已有业务化提示（`:2064`），
   建议删掉数字或改为「已按当前数据版本提交」。
3. **结果未知时把幂等键当「结果编号」展示**
   `inventory-ledger-page.tsx:556`、`:1182`（`reference: result.idempotencyKey`），
   组件渲染为「结果编号：w10-adj-bal_1-1735…」（`feedback.tsx:747-749`）。
   内部幂等键既不是单据号也带内部 draftId，用户无法用于任何后续操作；按术语表应换成「原任务号」口径或隐藏。
4. **导出任务标签与文件名泄漏内部编号**
   `inventory-ledger-page.tsx:1242`（`导出任务 ${exportJob.jobId}`）、`mock/session-state.ts:1724-1732`
   （`jobId = exp-w10-N`、文件名 `库存台账导出-exp-w10-1.csv`）。用户可见字符串含 `exp-w10-1` 内部编号。
5. **指标 detail 泄漏英文原值**
   `inventory-ledger-page.tsx:1311`（`detail="available = 0"`）。应改「可用数量为 0」。
6. **切换视图后排序参数残留，排序控件显示占位符而旧排序仍生效**
   `inventory-ledger-page.tsx:228`（`sortValue` 直接取 URL）、`:1499-1511`（排序下拉 value=sortValue）、
   `option-combobox.tsx:80-81`（值不在选项列表时 selected=null 显示占位符）。
   例：流水视图选了"发生时间（新到旧）"切回余额，URL 里 sort 仍是 occurredAt 键，余额列表按任意序排列，
   下拉框却显示空白占位——用户以为未排序，实际顺序不可预期且无清除入口（allowClear=false）。
7. **调整表单「业务发生时间」用自由文本输入 + 默认值取 UTC**
   `inventory-ledger-page.tsx:425`（`new Date().toISOString().slice(0,16)` 为 UTC 时刻，东八区会早 8 小时）、
   `:2044-2049`（普通 TextField，非 datetime-local，无格式校验约束，用户可键入任意字符串）。
   业务经办填错时点直接写进流水。建议换 datetime-local 输入，默认值用本地时区。
8. **演示控件直通正式操作流**
   `inventory-ledger-page.tsx:1217-1218`（「完成并确认结果（仅演示）」）、`:2071-2094`
   （「演示：强制结果不确定」「演示：模拟余额并发变更」）常驻正式对话框内。
   术语表 G6 允许演示标记，但「强制结果不确定」会真实注入不确定结果，演示环境外存在被误点/误解风险，
   建议收敛为角色开关或隐藏。
9. **常驻库存边界 Alert 偏技术且重复**
   `inventory-ledger-page.tsx:1273-1279`。长文本（排除口径 + 期初说明）与详情抽屉页脚说明（`:1927`）重复，
   建议折叠为 tooltip 或"查看说明"。
10. **刷新失败静默**
    `inventory-ledger-page.tsx:1011`（`isError && !data` 才失败态）；列表刷新/筛选变更失败时旧数据无提示停留，
    也未接入 DataTable 的刷新横幅（`data-table.tsx` 的 loading 态）。

## 6. 改进建议

1. **（P1-1）日期窗口改"动态近 30 天 + 显式可清空"**：默认值改为相对当天计算（如 `today-30d ~ today`），
   日期为空即视为"全部时间"并去掉 URL 回落逻辑；「清除筛选」后应呈现全量，而不是弹回默认窗口。
2. **（P1-2）合并跳转补丁**：把「查看全部流水」改为一次 `patchUrl`（view=movement + 三个过滤参数 + balanceId 清空），
   去掉 closeDetail 的第二次 patch；并为 `skuId` 参数补一个显式控件（或在筛选条显示"当前 SKU：XXX ×"标签）。
3. **（P1-3）隐形参数清零**：`adjustmentId`/`salesOrderLineId` 进入清除按钮的出现条件与清除清单；
   若定位为纯深链参数，落地时在筛选条展示可一键移除的 Chip（如「筛选：调整单 W10123 ×」）。
4. **（P1-4/5/6/7，P2-1/2/3/4/5）文案一轮过术语表**：W 编号走 `workspaceLabel`（`lib/ui-text.ts`）；
   `on_hand/reserved/available`→业务中文；`filterSummary` 的流水类型拼 label 不拼原值；
   期初说明去字段名；仓库下拉传 `w.code`；删除「余额版本」数字；幂等键不展示；
   导出文件名/任务号用业务编号（如"库存台账-20260805-01"）。
5. **（P2-6）视图切换时重置/修正 sort**：`patchUrl({view})` 时若当前 sort 不属于目标视图选项则一并删除，
   保证下拉显示与真实排序一致。
6. **（P2-7）调整表单时区与控件**：默认值改本地时间（不用 `toISOString().slice`），
   输入控件换 `datetime-local`。
7. **（P2-8）演示控件开关化**：`forceUnknownOnce` 演示按钮与「完成并确认结果」移入显式"演示模式"分组或隐藏，
   不与正式提交流并列。
8. **（P2-10）刷新失败感知**：给 `DataTable` 接 `loading`/刷新横幅，或 refetch 失败时展示轻量提示条。

## 7. 结论

页面主体质量高：四态齐全、URL 状态化、键盘路径与焦点管理用心、关键数字突出、移动端只读降级明确。
主要问题集中在两类：**筛选状态可见性**（流水日期窗口不可清除、隐形深链参数、排序残留、跳转竞态——用户
要么看不全数据要么被锁死在过滤视图）与**实现词泄漏**（W 编号按钮、字段名、枚举原值、幂等键、内部 ID 共 9 处，
全部有术语表现成替换口径可一键修）。
