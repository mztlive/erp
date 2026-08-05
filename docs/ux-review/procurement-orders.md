# UX 评审报告：采购单列表（/procurement/orders）

> 评审视角：产品经理 / UX
> 评审日期：2026-08-05
> 范围：`erp-client/features/purchase-orders/purchase-orders-list-page.tsx` 及其引用组件
> （`purchase-order-preview-panel.tsx`、`components/business/` 的 `data-table.tsx`、`list.tsx`、`page.tsx`、`values.tsx`、`feedback.tsx`、`api.ts`、`url-state.ts`、`types.ts`、`docs/ui-glossary.md`）

---

## 一、页面概述

采购单列表（W08）是采购域的作业台核心页面：顶部为指标筛选条（全部 / 可建单依据 / 草稿 / 待财务审核 / 待履约 / 先款门禁阻塞），中部为搜索 + 状态筛选 + 表格，右侧支持行内预览抽屉（半屏双栏），另含"从采购创建依据建单"对话框、CSV 导出、演示角色视图切换。

整体架构成熟度高：筛选器与 URL 参数一一对应（`q`/`status`/`metric`/`page`/`pageSize`/`sort` 均有对应控件）、TanStack Query 数据层、紧凑表格 + 列固定 + 列设置、行预览 + 键盘导航、跨单据跳转带返回保留筛选。主要问题集中在**内部标识/枚举原值泄漏上屏**与**键盘导航可用性**两处，均属于文案合规与交互完成度问题，无阻断性操作故障。

---

## 二、易用性

| 路径 | 评价 |
| --- | --- |
| 查找采购单 | 搜索框支持"采购单号、供应商、来源销售单"三个维度（list-page.tsx:749），300ms 防抖自动写入 URL（list-page.tsx:182-188），路径顺畅。快捷键 `/` 聚焦搜索（list-page.tsx:222-228） |
| 筛选 | 指标条 + 状态 ToggleGroup 双维度，点击即更新 URL 并重置到第 1 页；指标卡可点击，语义清晰 |
| 进入详情 | 三入口：行内"预览"按钮 / 单击行打开预览抽屉 / "中心"进入详情页（list-page.tsx:523）——**"中心"按钮语义不清**（见问题 P1-4） |
| 键盘操作 | 页面宣称 "键盘 j/k 移动，Enter 预览"（list-page.tsx:733），但**焦点无视觉反馈、不滚动**，实际不可用（见问题 P1-3） |
| 新建入口 | 页头"新建采购单" + 指标条"可建单依据"均可触发建单 Dialog，入口充足；从 W05/W07 跨页跳转携带 `basisId` 自动弹框（list-page.tsx:297-302），且返回时剔除该参数避免误弹（list-page.tsx:103-110），细节到位 |

## 三、信息密度

8 列（采购单号 / 来源销售单 / 类型·履约 / 进度 / 含税金额 / 付款条件 / 负责人 / 操作），数量适中。

- **状态**：采购单号列内嵌状态徽章 + "进度"列双轨（付款/履约）——关键信息突出，多轨进度用文字+颜色而非纯色块，符合规范（values.tsx:104-156）。
- **金额**：含税金额列右对齐等宽数字，无成本权限角色打码为 `•••`（list-page.tsx:469-474），实现良好。
- **供应商**：在采购单号下方次级行展示（list-page.tsx:378-380），弱化但可达——合理，因供应商已在搜索与建单维度中。
- **潜在冗余**："类型 / 履约"列中"履约责任"（入仓/供应商直发/电子交付/线下服务）与"进度"列中"履约"进度轴并存，语义相近、名称易混淆，建议将履约责任并入类型徽章或 tooltip。
- 缺失字段：列表无"更新时间"列（默认排序按 updatedAt），刷新间隔不可感知；预览面板有进项票进度但列表"进度"列只有付款/履约两轨，跨页口径不一致。

## 四、交互合理性

- **URL 契约**：`q/status/metric/page/pageSize/sort/basisId` 均有控件消费；`basisId` 仅服务建单 Dialog 且返回时被剔除，处理规范。**例外**：预览面板回跳 URL 携带 `currentId`（preview-panel.tsx:196），列表页无任何消费（见问题 P2-6）。
- **分页**：URL 驱动 + 服务端分页，页码越界时自动回写修正页（list-page.tsx:199-203）；每页 20/50/100 可选。良好。
- **排序**：4 列可排序（单号/来源/金额/负责人），金额排序对无成本角色返回 0（api.ts:109-112）——排序按钮仍可点但结果不变，语义略歧义，建议对打码角色禁用金额排序。
- **空/错误状态**：加载骨架屏、错误态整页替换 + 重试按钮均存在；但**空态无引导**（见问题 P2-4）。
- **批量操作**：未启用行选择（enableRowSelection 默认 false），列表无批量操作——当前页面仅有导出/建单，可接受，但"导出当前筛选"与"逐条预览后导出"之间缺"选中若干行导出"能力，高频诉求未覆盖。
- **按钮语义**："去审核/去编辑/去交付/预览"均为动作词，良好；但"中心"与"履约（禁用）"两处不合格（见问题 P1-4 / P2-2）。
- **角色视图切换**：页内"演示角色视图" ToggleGroup（list-page.tsx:679-701）为演示功能，切换会改变成本打码与数据范围，但**不在 URL 中**，刷新即重置、无法跨页面分享状态；演示环境按 G6 保留可接受，建议后续标注清楚。
- **全局键盘拦截**：window keydown 处理器在 Sheet/Dialog 打开时仍生效（list-page.tsx:205-253），在预览抽屉或建单弹框中按 j/k/Enter 会悄悄改动后台列表的焦点索引，属于状态污染（见问题 P2-11）。

## 五、问题清单（按严重度）

### P0（阻断操作）— 0 个

无。

### P1（明显阻碍效率）— 5 个

1. **内部 ID 泄漏上屏（违反术语表 §2 规则 7）**
   - 建单依据 `basisId`（形如 `pcb_*`）直接渲染：成功提示 `已使用创建依据 ${selectedBasisId}`（purchase-orders-list-page.tsx:315）、URL 带参场景选项 `来自采购二次确认的固定结果`（purchase-orders-list-page.tsx:974）、下拉选项前缀（purchase-orders-list-page.tsx:980）、预览面板"创建依据"字段（purchase-order-preview-panel.tsx:164）。
   - 明细行渲染 `确认分行 {line.procurementConfirmationLineId}`（形如 `cl_xg_1`，见 mock/purchase-orders.ts:340）（purchase-order-preview-panel.tsx:258）。
   - 建单成功结果编号直接展示内部 `po_*` ID（list-page.tsx:317，`result.reference` 即 purchaseOrderId，见 api.ts:414）。
   - 用户完全无法从 `pcb_open_01` / `cl_xg_1` 理解业务含义，应替换为"销售单号 + 供应商 + 品名/数量"等业务表达。
2. **筛选摘要输出英文枚举原值**
   - `当前筛选：${metricKey} · ${statusFilter}`（purchase-orders-list-page.tsx:734）会渲染出 `当前筛选：gate_blocked · EFFECTIVE`、`review · DRAFT` 这类英文原值（`metricKey` 为 `pending_create`/`gate_blocked` 等枚举 key，见 types.ts:33-39）。应改为中文映射（如"待履约 · 已生效"），或干脆只在非默认筛选时显示中文。
3. **键盘 j/k 导航实际不可用**
   - 页面帮助文案宣称"键盘 j/k 移动"（list-page.tsx:733），实现只更新 `focusedIndex` 状态并写 `tabIndex`/`data-focused` 属性（list-page.tsx:232-253, 347-358），但全仓无 `[data-focused]` 样式、无 `scrollIntoView`、也不调用 `.focus()`——焦点行无任何视觉反馈，向下移动后焦点还会因 tabIndex 变为 -1 而丢失。宣称的键盘效率路径实际无效，且误导用户。
4. **行操作按钮"中心"语义不明**
   - `中心`（purchase-orders-list-page.tsx:523）是内部"对象中心"架构词，采购用户无法推测其含义（实际打开详情页）。应改为"详情"。
5. **工作面编号泄漏（违反术语表 §1.3-6 / §3.6）**
   - 指标明细 `W07 固定结果`（api.ts:156）把工作面编号当用户文案；导出结果编号 `EXP-W08-${rows.length}`（purchase-orders-list-page.tsx:291）同样泄漏 W08。应改为业务名（"采购二次确认产生的固定结果"）或去掉编号。

### P2（体验瑕疵）— 11 个

1. **状态筛选缺"已作废"**：`PurchaseOrderStatus` 含 `VOID`、`PO_STATUS_LABEL` 有"已作废"，但筛选 ToggleGroup 与 `STATUS_VALUES` 均无该项（purchase-orders-list-page.tsx:767-773；url-state.ts:17-24），作废单只能靠"全部"翻找。
2. **双维度筛选可组合出静默空结果**：指标"待财务审核" + 状态"草稿"等矛盾组合无提示（api.ts:62-80 双条件叠加），描述行 `当前筛选：review · DRAFT` 既不中文也不解释，用户不知道为何为空。
3. **"履约"被用作动作按钮词**：阻断态按钮文字为"履约"（title 为阻断原因，purchase-orders-list-page.tsx:566-575），与可用态"去交付"（list-page.tsx:560）、预览面板"去交付与代发"（purchase-order-preview-panel.tsx:199）三处不一致。按术语表 §7，"履约"仅作状态/进度词，不做入口词；应统一为"去交付"或"去履约处理"。
4. **空态无引导**：复用 DataTable 默认 `当前筛选没有结果`（data-table.tsx:865），没有"去采购二次确认建单"或"清除筛选"的 CTA。
5. **导出文案内部词 + 口径不一致**：`已按角色遮罩成本字段`（purchase-orders-list-page.tsx:290）——"遮罩"为内部词（术语表 §2：掩码/遮罩 → 打码/隐藏），且与同页提示"成本金额已打码"（list-page.tsx:698）不一致。
6. **`currentId` 死参数**：预览面板"去交付与代发"回跳 URL 携带 `currentId`（purchase-order-preview-panel.tsx:196），列表页不消费也无控件——违反"URL 参数与界面控件一一对应"契约，返回列表后该参数永远残留在 URL 上。
7. **"可建单依据"指标卡行为混用**：点击后直接打开建单 Dialog（purchase-orders-list-page.tsx:715-719）而非筛选；但 `metric=pending_create` 合法存在于 URL 且服务端不做该值过滤（api.ts:68-80 未处理），URL 直链时该卡高亮却不筛选，状态失真。
8. **数据新鲜度时间硬编码**：`updatedAt="刚刚"` 写死（purchase-orders-list-page.tsx:621），从不显示真实更新时间，可信度低。
9. **帮助文案为开发者视角**：`紧凑布局；采购单号与行级操作列固定`（purchase-orders-list-page.tsx:733）描述的是列固定实现细节；对用户应只保留操作说明（搜索/预览/键盘）。
10. **单号兜底泄漏内部 ID 隐患**：`displayNo` 兜底 `purchaseOrderId`（purchase-orders-list-page.tsx:76；api.ts:59），一旦草稿单同时缺 purchaseNo 与 draftLabel 即上屏 `po_*`。
11. **全局键盘拦截污染弹层状态**：Sheet/Dialog 打开时按 j/k/Enter 仍会改动后台列表 `focusedIndex` 并触发预览（purchase-orders-list-page.tsx:205-253），应仅在列表可见且焦点不在弹层内时生效。

## 六、改进建议（按优先级）

1. **P1 文案合规批量整改**（问题 1/2/5）：建立中文映射表渲染 `metric`/`status`；`basisId`/`procurementConfirmationLineId`/`purchaseOrderId` 一律换业务表达（销售单号 + 供应商 + 品名数量）；结果编号去掉 W08 前缀与内部 ID。
2. **修复键盘导航**（问题 3）：对焦点行 `scrollIntoView({ block: "nearest" })` + 加高亮样式（`[data-focused="true"]` 背景色），并在弹层打开时暂停全局键处理（问题 11 同改）。
3. **按钮语义统一**（问题 4/3）："中心"→"详情"；履约类入口统一"去交付"；阻断态给出原因按钮"交付已阻断"而非裸词"履约"。
4. **筛选体系收敛**（问题 1/2/7）：状态筛选补"已作废"；矛盾组合时在描述行提示"无结果，请调整筛选"；移除或实现 `metric=pending_create` 的过滤语义，把"可建单依据"明确为动作卡而非筛选卡。
5. **空态与导出补强**（问题 4/5）：空态按场景给 CTA（无依据→"去采购二次确认"；有筛选→"清除筛选"）；导出文案改"已按角色打码成本字段"。
6. **一致性清理**（问题 5/8/9/10）：`currentId` 参数由列表页消费（滚动定位到该行并高亮）或移除；DataFreshness 展示真实 `freshness.updatedAt`；帮助文案去实现词；`displayNo` 兜底改为业务化标签。

---

### 统计

- 问题总数：**16**（P0：0 / P1：5 / P2：11）
- Top 3：
  1. 内部 ID（`pcb_*`/`cl_*`/`po_*`/`EXP-W08`）多处直接上屏，违反术语表；
  2. 筛选摘要渲染英文枚举原值（"当前筛选：gate_blocked · EFFECTIVE"）；
  3. 宣称的键盘 j/k 导航无焦点反馈、不滚动，实际不可用。
