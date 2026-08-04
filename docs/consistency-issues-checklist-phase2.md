# 一致性审查 · 问题待解决清单（二期）

> 生成日期：2026-08-04（二期）
> 审查方式：3 个独立 agent 并行审查（文档间一致性 A 系列 / 前端代码 vs 文档 FE 系列 / 前后端设计一致性 BE 系列）
> 维护规则：每与用户讨论解决一条，立即更新本清单（状态 → 已决议/已修复）并同步更新对应原始文档。
> 二期基线：一期 38 项（D/C/B 系列）已全部闭环，本清单仅含二期新发现；与一期重叠的主题残留（如 BE-03/BE-04）标注对应一期编号。

## 统计总览

| 来源 | 编号前缀 | 高 | 中 | 低 | 合计 |
| --- | --- | --- | --- | --- | --- |
| 文档间一致性 | A- | 0 | 2 | 5 | 7 |
| 前端代码 vs 文档 | FE- | 0 | 4 | 5 | 9 |
| 前后端设计一致性 | BE- | 1 | 5 | 3 | 9 |
| **合计** | | **1** | **11** | **13** | **25** |

> 注：BE-02 与 A-01 为同一问题（数据模型 §7.6 状态图），合并入 A-01，BE-02 仅作交叉引用，故 BE 系列实际 9 项。

## 状态图例

- 待讨论：已录入，等待与用户商讨解决方案
- 已决议：解决方案已确定（含修改方案），尚未落地到文件
- 已修复：原始文档/代码已同步修改，问题闭环

---

## 一、文档间一致性冲突（A 系列，二期）

### A-01 【中】状态机冲突 · 数据模型内部自相矛盾：§7.6 状态图用 `PENDING/FULL` 简写

- **性质**：状态机冲突（数据模型内部 §7.6 与 §6.19 两种枚举拼写；二期与 D-03 决议不符）
- **涉及文档**：erp-data-model.md §7.6（L3622-3623）vs §6.19（L3174-3175、L3206）、erp-phase-2.md §10.3（L866-881）、w26-supplier-orders.md（L119-120、L341）
- **冲突点**：
  - §7.6 图：`cancel_status: NONE → PENDING → CANCELED | FAILED | MANUAL`、`refund_status: NONE → PENDING → PARTIAL → FULL（↘ FAILED | MANUAL）`
  - §6.19 / phase-2 §10.3 / W26：取消轨 `CANCEL_PENDING`、退款轨 `REFUND_PENDING / REFUNDED / REFUND_FAILED`（D-03 决议已定）
  - §7.6 未随 D-03 决议同步，且缺少 `REFUND_FAILED` 分支图示
- **建议方向**：§7.6 状态图统一为 `CANCEL_PENDING / REFUND_PENDING / REFUNDED / REFUND_FAILED`，与 §6.19、phase-2 §10.3、W26 完全对齐（前端代码侧枚举见 BE-01）
- **状态**：✅ 已修复（2026-08-04）
- **决议**：§7.6 取消/退款双轨图改为完整权威枚举：`cancel_status: NONE → CANCEL_PENDING → CANCELED | FAILED | MANUAL`；`refund_status: NONE → REFUND_PENDING → PARTIAL → REFUNDED（↘ REFUND_FAILED | MANUAL）`。与 §6.19、phase-2 §10.3、w26 逐字一致。BE-02 随本项闭环。已 grep 复核：§7.6 区域无 PENDING/FULL 残留（其余 PENDING 属 §7.7 集成投递等其他域，无关）。

### A-02 【中】字段名/模型冲突 · 二期消费成本字段落点：phase-2 §14.3 与数据模型 §6.17 矛盾（自身前后矛盾）

- **性质**：字段落点冲突（文档内部自相矛盾）
- **涉及文档**：erp-phase-2.md §14.3（L1282-1283 vs 同节 L1291）；erp-data-model.md §6.17（L2930-2931）
- **冲突点**：
  - phase-2 §14.3 表目录：`mall_item_funding_allocation` 关键关系含"成本分摊金额、尾差归属"、`mall_consumption_entry` 含"分摊成本"
  - 数据模型 §6.17：本表只表达支付，不保存任何成本字段；成本金额、来源分摊和尾差只进入 `cost_entry + cost_allocation`（`rounding_residual_flag` 在 cost_allocation）
  - phase-2 同节 L1291 又写"成本口径不直接落在消费表上"，与 L1282-1283 自相矛盾
- **建议方向**：phase-2 §14.3 L1282-1283 删除成本字段描述，改为"仅支付分摊；成本分摊与尾差见 `cost_entry/cost_allocation`"
- **状态**：✅ 已修复（2026-08-04，子 agent 执行并自复核）
- **决议**：以数据模型 §6.17 为准：`mall_item_funding_allocation` 仅"商品明细、支付来源、实际支付金额"，成本分摊与尾差归属指向 `mall_consumption_cost_assessment`；`mall_consumption_entry` 删除"分摊成本"。grep 复核 phase-2 全文其余成本表述（分摊规则/毛差口径）均指向 cost_assessment，无其他矛盾。

### A-03 【低】统计不一致 · 数据模型 §5.4 表目录 `mall_consumption_cutover` 重复登记

- **性质**：统计不一致（表目录重复）
- **涉及文档**：erp-data-model.md §5.4（L322-323）
- **冲突点**：`mall_consumption_cutover` 同时列在"卡实例与余额"行和"商城关键事实"行首
- **建议方向**：从"商城关键事实"行移除，只保留在"卡实例与余额"行
- **状态**：✅ 已修复（2026-08-04，子 agent 执行并自复核）
- **决议**：选项 A——保留在"卡实例与余额"行，从"商城关键事实"行移除行首 `mall_consumption_cutover`，该行以 `mall_order_fact` 开头。grep 复核 §5.4 区域仅剩 L322 一处。

### A-04 【低】编号断链 · phase-2 §15 能力编号 P2-P06 缺失

- **性质**：编号断链（疑为 B-01 取消 W24 时未重排）
- **涉及文档**：erp-phase-2.md §15（L1331-1343）
- **冲突点**：P2-P01～P2-P13 中 P2-P06 缺失（P2-P05 直接跳 P2-P07），全文无任何 P2-P06 引用或删除说明
- **建议方向**：补一行删除说明，或将 P2-P07～P2-P13 重排为 P2-P06～P2-P12
- **状态**：✅ 已修复（2026-08-04，3 个子 agent 并行执行并自复核）
- **决议**：随"主责迁移概念取消"决策（见下）一并处理：删除 P2-P05（主责迁移）与 P2-P06（未迁移卡券销售单，原缺失行），P2-P07~P2-P13 重排为 P2-P05~P2-P11，w25 L650 引用同步改为 P2-P05/P2-P06。
- **附：重大概念决策（并入本项）**：用户确认"主责迁移"整个概念不存在——它只是"T 时点 ERP 全面服务业务"的说法；数据始终仅靠商城同步。因此：删除 `sales_order.owner_system` 字段（保留 `origin_system` 创建入口，恒不变）；删除全部迁移机制（无迁移操作/批次/审计动作/幂等/续跑/回退）；T 起商城停止建单、全部 B2B 销售单由 ERP 服务；T 前商城开单单仅同步只读；`mall_consumption_cutover` 删除 `migration_scope_digest`（cutover_check 已存证据快照）；`reconciliation_type` 删"主责"值；§7.2/§7.3 改"T 前/T 后"标题；§7.8 改"销售单服务切换"。涉及：erp-data-model.md（26 处）、erp-phase-2.md（约 40 处）、erp-phase-1.md（13 处，历史语境保留+加注）、erp-mall-data-mapping.md（5 处）、phases-4/8/10、11 个工作面文档、erp-ui-design/ui-flows、前端 12 文件（tsc/eslint 通过，ownerSystem 0 处）。

### A-05 【低】字段名/键冲突 · 供应商退款幂等键缺"连接 + 退款版本"维度

- **性质**：幂等键口径冲突（同供应商多连接会撞键）
- **涉及文档**：erp-phase-2.md §13.1.1（L1162）vs erp-data-model.md §6.19（L3282、L3251）
- **冲突点**：
  - phase-2 消息层幂等依据："供应商 + 外部退款单号"
  - 数据模型 `supplier_refund_fact` 唯一键：`(connection_id, external_refund_no, external_refund_version)`，并强调"同一供应商的不同连接或账号合法复用外部事件号"
- **建议方向**：phase-2 改为"连接 + 外部退款单号 + 退款版本"
- **状态**：⏳ 待讨论

### A-06 【低】幂等键口径不一致 · 快照幂等缺"来源更新时间"维度

- **性质**：幂等键口径冲突
- **涉及文档**：erp-phase-1.md §8.3（L897）vs erp-data-model.md §6.13（L2234、L2239-2240）、erp-mall-data-mapping.md §10.1（L648）
- **冲突点**：
  - phase-1：幂等依据"商城 + 销售单号 + 内容指纹"
  - 数据模型快照唯一键：`(source_system_id, external_order_key, source_updated_at, content_hash)`；mall-mapping：含"来源更新时间"
  - 缺 `source_updated_at` 与"第三次 A 来源更新时间不同必须保留新观测"直接冲突
- **建议方向**：phase-1 §8.3 幂等依据补"来源更新时间"维度
- **状态**：⏳ 待讨论

### A-07 【低】字段名冲突 · W05 文档内部 `salesOrderCommercialStatus` vs `salesOrderStatus` 混用

- **性质**：字段名冲突（文档内部）
- **涉及文档**：w05-sales-orders.md（L530 vs L650/L659）
- **冲突点**：同一文档类型定义中，商业主状态一处叫 `salesOrderCommercialStatus: "VOIDED"`，另一处叫 `salesOrderStatus: "PENDING_OPERATIONS"/"EFFECTIVE"`，均指 `commercial_status` 投影
- **建议方向**：统一为一个字段名（如 `salesOrderStatus`）
- **状态**：⏳ 待讨论

---

## 二、前端代码 vs 文档冲突（FE 系列，二期）

### FE-01 【中】跳转断链 · W25/W30/W28 → W29 钻取参数与 W29 解析不匹配，目标项无法聚焦

- **性质**：跳转断链
- **位置**：
  - 产出侧：features/mall-consumption-orders/consumption-order-center-page.tsx（L550、L1044，`?workItemId=…&from=W25&mallOrderId=…`）、consumption-orders-list-page.tsx（L471）、features/history-backfill/history-backfill-page.tsx（L1821，`?from=W30&jobId=…&factKey=…`）、features/card-business-analytics/api.ts（L440-442，`?reason=NONE_COST&from=…&to=…`）
  - 消费侧：features/integration-errors/url-state.ts（L41-85）只解析 `view/mode/environment/errorClass/owner/q/queueContextId/resolveWorkItemId/taskId/differenceId`，`workItemId/from/mallOrderId/jobId/factKey/reason` 全部被静默忽略
- **冲突点**：w29 文档 §3 规定的错误任务入口为 `?resolveWorkItemId=:workItemId&queueContextId=…`；w25 文档 §11 要求携带"事实/订单/差异 ID"。实际点击"打开接口错误差异"后落在 W29 默认队列首位，既不聚焦目标任务，也不进领域详情
- **建议方向**：W25/W30/W28 链接改 `?resolveWorkItemId=…&queueContextId=…`（或按对象类型用 `/errors/:taskId`、`/differences/:differenceId` 领域路由），与 w29 §3 契约对齐；同时考虑 url-state 对 `workItemId` 做兼容解析
- **状态**：⏳ 待讨论

### FE-02 【中】契约违反 · W26 导出为"当前页客户端 CSV"，违反文档后台任务 + 服务端快照契约

- **性质**：契约违反（功能缺口）
- **位置**：features/supplier-orders/supplier-orders-list-page.tsx（L465-499）
- **冲突点**：w26 §7 导出要求 `BatchImpactPreview` 展示范围/字段/过期时间 → 创建后台任务（7 天内下载）→ 执行时重验权限与字段遮罩 → 部分失败逐项报告。代码为纯前端拼 CSV、仅当前页、无服务端选择快照、无遮罩重验、无失败报告
- **建议方向**：改后台导出任务 + 选择快照 + `BackgroundJobProgress`（可参照 W25 列表已实现的导出流程）
- **状态**：⏳ 待讨论

### FE-03 【中】跳转断链 · W28 → W25/W26 钻取使用 `focus=` 参数且 ID 不存在，点击无任何定位效果

- **性质**：跳转断链
- **位置**：mock/card-business-analytics.ts（L256、L257、L276、L295、L313、L332、L351，`?focus=co-mall-90112`、`?focus=spo-4412`）
- **冲突点**：W25 列表（consumption-orders-list-page.tsx L152-160）与 W26（url-state.ts）均不解析 `focus`；且 `co-mall-90112`、`spo-4412` 与 mock 中真实稳定 ID（`mo-90881…`、`sfo-unknown-01…`）不符。w25 §3 规定 W28 下钻应为 `/commerce/consumption-orders/:mallOrderId?section=overview`。点击后落在未过滤列表首屏，无法定位目标订单
- **建议方向**：mock 下钻链接改带稳定 ID 的对象中心路由（或列表 `q=` 精确搜索 + 行定位），删除 `focus` 非契约参数
- **状态**：⏳ 待讨论

### FE-04 【中】功能缺口 · W25 列表缺"列表内读主事实"层：单击行直接整页跳对象中心

- **性质**：功能缺口
- **位置**：features/mall-consumption-orders/consumption-orders-list-page.tsx（L907-927）
- **冲突点**：w25 §6（L240）明确"单击行打开 `detail` 半屏，至少覆盖身份、金额、关键事实、支付构成、履约链、供应商摘要和成本口径"；ui-design §4.3.1 要求单据列表"detail 或纸质层二选一，禁止缺一层"。W26/W27 列表均有 `QuickPreviewSheet size="detail"`，W25 是唯一缺层的单据类 M2 列表
- **建议方向**：补 `QuickPreviewSheet size="detail"`（preview 内容复用对象中心字段），行点击打开半屏，操作列保留"打开中心"
- **状态**：⏳ 待讨论

### FE-05 【低】功能缺口 · W25 缺失文档规定的筛选能力（事实期间/事实类型/供应商状态/数据来源）

- **性质**：功能缺口
- **位置**：features/mall-consumption-orders/consumption-orders-list-page.tsx（L152-160、L217-252、L721-841）；types.ts（L123-142）
- **冲突点**：w25 §4.1 工具栏含"事实时间"，§6 定义 `occurredFrom/occurredTo`（策略未配置时"必须由用户显式选择完整起止时间后才查询，不静默采用 30 天"）、`factType`、`supplierStatus`、`dataSource`。代码无期间控件且无期间查询，事实类型/供应商状态/数据来源筛选也全部缺失（types.ts 注释"mock 允许省略"与文档 Q1 决策不符）
- **建议方向**：补期间选择控件（必填期间才可查询）及其余三组筛选，接 URL 参数
- **状态**：⏳ 待讨论

### FE-06 【低】检查产物过时 · C-21"完全摘除"未彻底：supplier-orders 查询契约残留 maskCost/noSensitive

- **性质**：修复残留（死契约字段）
- **位置**：features/supplier-orders/types.ts（L205-208，`SupplierOrderListQuery` 仍声明 `maskCost?`、`noSensitive?`）
- **冲突点**：一期 C-21 决议"从查询/URL 完全摘除"。当前 url-state/api/list 均已无消费点（全 feature 仅 types.ts 这 2 处），属死契约字段
- **建议方向**：从 `SupplierOrderListQuery` 删除这两个字段
- **状态**：⏳ 待讨论

### FE-07 【低】检查产物过时 · verify-workspaces.mjs 两项检查恒真，产物 Nav/文案列失去校验意义

- **性质**：检查产物过时
- **位置**：scripts/verify-workspaces.mjs（L79-87 `navInShell` 恒 true、L128-131 `noMCodeInUiCopy` 恒 true、L163-165 空 if 块）
- **冲突点**：C-23 只修了 W21 模式，但 Nav 列与"M 代号不上屏"两列在 w-page-coverage.md 中永远为 "yes"，实际从未断言；"Nav groups must include every non-nested main route" 的失败分支（L166-168）仍有效但前一个空块使代码不可读
- **建议方向**：把 `navInShell` 改为真实断言（对比 WORKSPACE_NAV_GROUPS hrefs 与 registry navHrefs），删除恒真分支，重新生成产物
- **状态**：⏳ 待讨论

### FE-08 【低】契约偏离 · W26 履约状态筛选只支持单值，文档规定多选

- **性质**：契约偏离
- **位置**：features/supplier-orders/url-state.ts（L60-64）+ supplier-orders-list-page.tsx（L682-703，`OptionCombobox` 单选）
- **冲突点**：w26 §6"履约状态…支持多选，不与取消/退款混成一个枚举"。代码 URL 与控件均为单值 `fulfillmentStatus=`
- **建议方向**：URL 改逗号分隔多值或 `fulfillmentStatuses` 数组，控件改多选
- **状态**：⏳ 待讨论

### FE-09 【低】内部矛盾 · W25 新增界面文案实例：代号/字段名上屏（一期未扫描到的新位置）

- **性质**：新实例（非重复报告）
- **位置**：consumption-orders-list-page.tsx（L475，行内按钮文字直接渲染 `W29`）；consumption-order-center-page.tsx（L666，`paidAt {时间} < T` 字段名与实现术语上屏）
- **冲突点**：ui-glossary P2/§6：工作面代号与字段名不上屏（一期 C-08 在 W30 同型问题已修复，此处 W25 为新遗漏）
- **建议方向**：按钮改"接口错误"；中心 `paidAt` 用文档 §5.1 用户文案"支付成功时间"，`T` 改业务文案（如"支付时间早于/不早于切换时点"）
- **状态**：⏳ 待讨论

---

## 三、前后端设计一致性冲突（BE 系列，二期）

### BE-01 【高】枚举不匹配 · 供应商订单取消/退款轨枚举值与 D-03 决议不符

- **性质**：决策未同步到代码（枚举）
- **位置**：erp-client/features/supplier-orders/types.ts（L20-33）、api.ts（L929-936）、mock/supplier-orders.ts；docs/ui-workspaces/w26-supplier-orders.md（L119-120）
- **冲突点**：
  - 代码：`CancelStatus = NONE/PROCESSING/CANCELLED/FAILED/MANUAL`；`RefundStatus = NONE/PROCESSING/PARTIAL/FULL/FAILED/MANUAL`；动作成功后写 `"PROCESSING"`
  - 权威（D-03 决议 + 数据模型 §6.19 + phase-2 §10.3）：取消轨 `NONE/CANCEL_PENDING/CANCELED/FAILED/MANUAL`，退款轨 `NONE/REFUND_PENDING/PARTIAL/REFUNDED/REFUND_FAILED/MANUAL`
- **建议方向**：前端 types/mock/api 与 w26 文档统一改为 `CANCEL_PENDING/CANCELED`、`REFUND_PENDING/REFUNDED/REFUND_FAILED`，tsc 验证
- **状态**：✅ 已修复（2026-08-04，子 agent 执行并自复核）
- **决议**：以 D-03 权威枚举为准：`CancelStatus = NONE/CANCEL_PENDING/CANCELED/FAILED/MANUAL`（原 PROCESSING→CANCEL_PENDING、CANCELLED→CANCELED 单 L）；`RefundStatus = NONE/REFUND_PENDING/PARTIAL/REFUNDED/REFUND_FAILED/MANUAL`（原 PROCESSING→REFUND_PENDING、FULL→REFUNDED、FAILED→REFUND_FAILED）。动作成功后写入 CANCEL_PENDING/REFUND_PENDING。界面中文文案不变。5 文件：types.ts（类型+4 组 label/tone 映射键）、api.ts（11 处字面量，含 tsc 抓出的 supplierRefund.status 漏改）、supplier-orders-list-page.tsx（售后指标筛选 1 处）、mock/supplier-orders.ts（2 处种子）、w26 文档（L119-120 语义表 + L340 resolution 拼写）。`mallRefund.status`（商城侧独立枚举）与 mall-sync sourceStatusCode 不在范围内。tsc 0 错误、eslint 无新增、grep 终态：PROCESSING 0 处 / FULL 仅剩 mallRefund / CANCELLED 0 处。

### BE-02 【中】阶段文档偏差 · 数据模型 §7.6 状态机用 `PENDING/FULL` 简写

- **性质**：与 A-01 为同一问题
- **位置**：docs/erp-data-model.md（L3622-3623）vs §6.19（L3174-3175）、erp-phase-2.md §10.3（L866-881）
- **冲突点**：§7.6 图 `PENDING/FULL` vs 权威枚举 `CANCEL_PENDING / REFUND_PENDING / REFUNDED / REFUND_FAILED`
- **建议方向**：见 A-01（合并处理，不单独立项）
- **状态**：✅ 已修复（随 A-01，2026-08-04）

### BE-03 【中】决策未同步到代码 · B-07 已决议"迟到丢弃"枚举移除，mock/类型/页面仍保留 LATE_DISCARDED

- **性质**：已决议主题残留（代码）
- **位置**：erp-client/mock/mall-sync.ts（L204-208，`mappingStatus: "LATE_DISCARDED"` + `mappingStatusLabel: "迟到丢弃"` + `conflictFlags: ["LATE_ARRIVAL"]`）、features/mall-sync/types.ts（L128-134）、mall-sync-page.tsx（L713、L1484）
- **冲突点**：B-07 决议：迟到快照直接丢弃、`mapping_status` 移除"迟到丢弃"枚举；数据模型 §6.13 与 w17 文档均为 4 值（待映射/已应用/差异/无变化）。代码仍存在"迟到丢弃"快照样例并上屏
- **建议方向**：删除该快照种子及 `LATE_DISCARDED` 类型成员；若需演示迟到行为改为服务端丢弃语义（不入列表）
- **状态**：⏳ 待讨论

### BE-04 【中】决策未同步到文档 · B-04 已统一 `sales_visible_price_gross`，4 处文档仍残留旧拼写

- **性质**：已决议主题残留（文档）
- **位置**：docs/backend/phases/phases-4.md（L51、L92）；docs/erp-phase-1.md（L49）；docs/ui-workspaces/w05-sales-orders.md（L168）；docs/ui-workspaces/w07-procurement-confirmation-queue.md（L139）
- **冲突点**：上述位置仍写 `sales_visible_price`；B-04 决议"文档全部统一 `sales_visible_price_gross`"；权威字段在数据模型 §6.3（L823）
- **建议方向**：4 处统一替换为 `sales_visible_price_gross`
- **状态**：⏳ 待讨论

### BE-05 【中】查询参数不符 · W25 页面传 `sort=occurredAt.desc`，mock 忽略并按 paidAt 排序

- **性质**：查询参数/响应不符
- **位置**：features/mall-consumption-orders/consumption-orders-list-page.tsx（L238）vs api.ts（L206-211）
- **冲突点**：页面传 `sort: "occurredAt.desc"`（W25 文档：服务端按事实发生时间降序 + 稳定事实 ID）；mock `next.sort((a,b) => b.paidAt - a.paidAt)` 从不消费 `query.sort`；`paidAt` ≠ `occurredAt`（取消/退款事实的 `occurredAt` 晚于支付时间）
- **建议方向**：mock 按 `sort` 参数实现 `occurredAt` 排序（相同时间按事实 ID 稳定排序）
- **状态**：⏳ 待讨论

### BE-06 【中】枚举不匹配 · W27 结算差异"状态/处理结论"字段落点与数据模型 §6.20 冲突

- **性质**：字段值域错位
- **位置**：features/supplier-settlements/types.ts（L35-45）+ mock/supplier-settlements.ts vs erp-data-model.md §6.20（L3340-3341）、w27-api-settlement.md（L122）
- **冲突点**：
  - 数据模型：`supplier_settlement_difference.status = 待处理、供应商认可、ERP 认可、已补偿、关闭`（status 承载 5 值）
  - 前端/W27：`status` 为 `OPEN/EVIDENCE_PENDING/RESOLVED/BLOCKING`，而"供应商认可/ERP 认可/已补偿/关闭"被放进 `resolution` 枚举
  - 两侧 status 值域无交集，且数据模型缺 `EVIDENCE_PENDING/BLOCKING` 两态
- **建议方向**：统一差异状态机：将"供应商认可/ERP 认可/已补偿/关闭"定为状态值（或明确数据模型 status 为 resolution），并在 w27 文档补固定枚举
- **状态**：⏳ 待讨论

### BE-07 【中】决策未同步到代码 · phases-10 §5 已登记的"一期代码契约缺口"仍在代码中，且未入清单跟踪

- **性质**：已知缺口未纳入闭环跟踪
- **位置**：features/supplier-catalog/api.ts（L580-653、L844、L1308、L1393-1426，仍以 `supplierProductId` 入池/定位）、（L1006、L1018、L1192、L1469，`supplyMode: ["BULK"]`，数据模型已取消 `supply_mode`）、（L1187，`inputTaxRate ?? "0.13"` 硬编码）；features/master-data/data.ts（L1457+，`salePrice` 仅存会话快照）
- **冲突点**：phases-10 §5.4/5.8/5.14：入池必须提交精确 `supplier_catalog_sku_id`、供给不设置 `supply_mode`、无可靠来源时空白必填不得硬编码、`salePrice` 必须写入 `sku_revision.sales_visible_price_gross`。代码现状与 phases-10 所述缺口一致（仍未同步），但这些缺口未收录进一致性清单闭环
- **建议方向**：将 phases-10 §5 各项代码缺口录入清单跟踪闭环（至少：入池键、supply_mode、0.13 硬编码三项），按决议修代码
- **状态**：⏳ 待讨论

### BE-08 【低】枚举不匹配 · W25 事实 `processingStatus` 值不在数据模型 5 值域内

- **性质**：枚举不匹配
- **位置**：erp-client/mock/mall-consumption-orders.ts（L130 及 14 处，全部 `"COMMITTED"`）vs erp-data-model.md（L2789）
- **冲突点**：mock `processingStatus: "COMMITTED"`；数据模型 `mall_order_fact.processing_status = 已保存、待归集、已归集、差异、拒绝`（5 值）；前端 types.ts 与 W25 文档未约束为字符串
- **建议方向**：确认"已归集"对应枚举（如 `ATTRIBUTED`）并统一 mock 值；或将 COMMITTED 明确登记为第 6 个状态
- **状态**：⏳ 待讨论

### BE-09 【低】枚举不匹配 · W17 每日核对批次 mock 状态与数据模型矛盾（完成态 + 4 差异）

- **性质**：枚举不匹配（阶段文档偏差）
- **位置**：erp-client/mock/mall-sync.ts（L512-519 `status: "SUCCEEDED"` + `differenceCount: 4`；L568-570 `status: "UNCONFIRMABLE"`）vs erp-data-model.md（L2263：运行中/完成/有差异/失败；L2274：待处理/补拉中/已解决/确认无误）
- **冲突点**：数据模型要求有差异的批次状态为"有差异"而非"完成"；"无法确认"（UNCONFIRMABLE）不在数据模型 4 值内（w17 文档 L140 有"无法确认"说法，与数据模型 L2274 不一致）
- **建议方向**：mock 批次状态改 `DIFFERENCE/有差异`；数据模型与 w17 文档统一第 4 态为"无法确认/确认无误"二选一
- **状态**：⏳ 待讨论

### BE-10 【低】查询参数不符 · W25 `occurredFrom/occurredTo` 契约未落地（无控件、mock 不消费）

- **性质**：查询参数不符（与 FE-05 关联）
- **位置**：features/mall-consumption-orders/types.ts（L127-128）、api.ts（L106-214）vs w25-mall-consumption-orders.md（L229）
- **冲突点**：文档要求"默认策略缺失时须由用户显式选择完整起止时间后才查询，不静默采用默认区间"；代码类型注释自称"mock 允许省略"，页面无时间控件、mock 无时间过滤
- **建议方向**：按 W25 Q1 决议在 mock/页面补齐时间范围门禁，或将该行为登记为已知 mock 简化
- **状态**：⏳ 待讨论（与 FE-05 合并处理）

---

## 四、讨论记录

（每解决一条，在此追加一行：编号、决议内容、修改的文件、日期）

| 日期 | 编号 | 决议 | 修改文件 |
| --- | --- | --- | --- |
| 2026-08-04 | BE-01 | 供应商订单取消/退款轨枚举统一 D-03 权威值：CancelStatus 改 CANCEL_PENDING/CANCELED（单L）、RefundStatus 改 REFUND_PENDING/REFUNDED/REFUND_FAILED；动作后写 CANCEL_PENDING/REFUND_PENDING；界面中文不动 | erp-client/features/supplier-orders/types.ts、api.ts、supplier-orders-list-page.tsx、mock/supplier-orders.ts、docs/ui-workspaces/w26-supplier-orders.md |
| 2026-08-04 | A-01（含BE-02） | 数据模型 §7.6 取消/退款双轨图由 PENDING/FULL 简写改为完整权威枚举（CANCEL_PENDING/REFUND_PENDING/REFUNDED/REFUND_FAILED），与 §6.19/§10.3/w26 逐字对齐 | docs/erp-data-model.md §7.6（L3622-3624） |
| 2026-08-04 | A-02 | phase-2 §14.3 表目录与数据模型 §6.17 对齐：mall_item_funding_allocation 删除"成本分摊金额、尾差归属"改为指向 cost_assessment；mall_consumption_entry 删除"分摊成本" | docs/erp-phase-2.md §14.3（L1282-1283） |
| 2026-08-04 | A-03 | 表目录重复登记：mall_consumption_cutover 保留在"卡实例与余额"行，从"商城关键事实"行移除（选项A） | docs/erp-data-model.md §5.4（L323） |
| 2026-08-04 | A-04 + 概念决策 | 主责迁移概念取消：删 owner_system 字段（留 origin_system）；删全部迁移机制；T 起商城停单、ERP 全面服务；cutover 删 migration_scope_digest；reconciliation_type 删"主责"；§7.2/7.3 改 T 前/T 后、§7.8 改服务切换；P2 编号重排 P2-P05~P2-P11；w25 引用同步；phase-1 历史句加注、mall-mapping/phases-4/8/10/工作面 11 文档/UI 文档同步；前端 12 文件（tsc/eslint 通过） | erp-data-model.md、erp-phase-2.md、erp-phase-1.md、erp-mall-data-mapping.md、phases-4/8/10、w05/w13/w14/w17/w18/w19/w20/w21/w23/w25/w30、ui-design、ui-flows、erp-client 12 文件 |
