# 一致性审查 · 问题待解决清单

> 生成日期：2026-08-04
> 审查方式：3 个独立 agent 并行审查（文档间一致性 / 前端代码 vs 文档 / 前后端设计一致性）
> 维护规则：每与用户讨论解决一条，立即更新本清单（状态 → 已决议/已修复）并同步更新对应原始文档。

## 统计总览

| 来源 | 编号前缀 | 高 | 中 | 低 | 合计 |
| --- | --- | --- | --- | --- | --- |
| 文档间一致性 | D- | 2 | 2 | 3 | 7 |
| 前端代码 vs 文档 | C- | 10 | 8 | 6 | 24 |
| 前后端设计一致性 | B- | 1 | 5 | 1 | 7 |
| **合计** | | **13** | **15** | **10** | **38** |

## 状态图例

- 待讨论：已录入，等待与用户商讨解决方案
- 已决议：解决方案已确定（含修改方案），尚未落地到文件
- 已修复：原始文档/代码已同步修改，问题闭环

---

## 一、文档间一致性冲突（D 系列）

### D-01 【高】`commercial_status` 主状态值归属互相矛盾（含采购确认状态所在字段冲突）

- **性质**：状态机冲突（数据模型内部也自相矛盾）
- **涉及文档**：erp-phase-1.md §9.3（L1048-1055）；erp-data-model.md §6.4（L1002-1003）、§7.1（L3561-3562）、§7.3（L3586-3592）；ui-workspaces/w05-sales-orders.md（L276-277、L511、L578、L619-650）
- **冲突点**：
  - phase-1 §9.3：主状态只保存 4 值（草稿/审核中/已生效/已作废），采购确认、低毛利确认等中间环节属 `review_status`
  - 数据模型 §7.1：`commercial_status` 保存 `DRAFT / PENDING_PROCUREMENT_CONFIRMATION / EFFECTIVE / VOIDED`（把待采购确认算进主状态）；§7.3 二期链出现 `PENDING_SALES_LEAD / PENDING_OPERATIONS`（若为 commercial 值则至少 6 值）
  - 数据模型 §6.4：`review_status` 又列有"待采购确认、待销售领导、待运营"
  - W05：把 `PENDING_PROCUREMENT_CONFIRMATION / PENDING_SALES_LEAD / PENDING_OPERATIONS` 全部用作 `salesOrderReviewStatus`（站在 phase-1 一侧）
- **建议方向**：中间环节统一归 `review_status`；§7.1/§7.3 状态图改为"主状态 + review 轨"双层表达
- **状态**：✅ 已修复（2026-08-04）
- **决议**：主状态 4 值 + review 轨。`commercial_status` 仅保存 `DRAFT / PENDING_REVIEW / EFFECTIVE / VOIDED`；待采购确认/待销售领导/待运营等全部归 `review_status`。已同步修改 erp-data-model.md §7.1/§7.3 状态图，与 phase-1 §9.3、W05 对齐。`PENDING_REVIEW` 命名与数据模型 §7.5 资金单据状态命名风格一致。

### D-02 【高】员工手机号"ERP 不保存"与"收货手机号加密快照保存"直接矛盾

- **性质**：口径冲突（phase-1 绝对禁令被 phase-2/数据模型突破）
- **涉及文档**：erp-phase-1.md §11.1（L1154）；erp-phase-2.md §9.4（L806）；erp-data-model.md §4.5（L226）、§6.17（L2934）；erp-mall-data-mapping.md §10.4（L783）
- **冲突点**：
  - phase-1 §11.1：「ERP 不保存卡号、卡密和员工手机号；员工的绑定和消费信息留在商城」
  - phase-2 §9.4 / 数据模型 L2934 / mall-mapping L783：「收货人、手机号和地址仅在供应商履约需要时保存订单快照，完整值加密存储」
- **建议方向**：phase-1 §11.1 改为"不保存卡实例绑定手机号及其可逆映射；履约收货手机号仅加密快照"
- **状态**：✅ 已修复（2026-08-04）
- **决议**：区分两类手机号：ERP 不保存卡号/卡密/卡实例绑定手机号及其可逆映射；履约收货手机号仅加密快照（与 phase-2 §9.4、数据模型 §6.17、mall-mapping 一致）。

### D-03 【中】供应商订单状态机两套画法（phase-2 扁平 13 值 vs 数据模型正交三轨）

- **性质**：状态机冲突（phase-2 正文未同步数据模型拆轨）
- **涉及文档**：erp-phase-2.md §10.3（L845-863）；erp-data-model.md §7.6（L3662-3672）、§6.19（L3217-3219、L3250-3252）
- **冲突点**：phase-2 列 13 个扁平状态（含 CANCEL_PENDING/CANCELED/REFUND_PENDING/REFUNDED）；数据模型拆为 `fulfillment_status` + `cancel_status` + `refund_status` 三条正交轨道，且 refund_status 含部分退款/退款失败/待人工终态
- **建议方向**：更新 phase-2 §10.3 为"主轨 + 取消轨 + 退款轨"并补充 PARTIAL/FAILED/MANUAL 值
- **状态**：✅ 已修复（2026-08-04）
- **决议**：以数据模型三轨为准。phase-2 §10.3 重写为三轨正交：履约轨 9 值 + 取消轨 5 值（NONE/CANCEL_PENDING/CANCELED/FAILED/MANUAL）+ 退款轨 6 值（NONE/REFUND_PENDING/PARTIAL/REFUNDED/REFUND_FAILED/MANUAL），互不折算，与数据模型 §6.19/§7.6、W26 一致。

### D-04 【中】W09 设计文档与 ui-glossary"W09 已落地"声明互相矛盾（过账/暂挂未同步）

- **性质**：术语冲突 + 过时（设计文档未跟上 glossary 落地决策）
- **涉及文档**：ui-glossary.md §6 G1/G5（L354、L358）、§7（L365-381）；w09-fulfillment-operations.md（L3、L32、L53、L61-64、L104、L106、L175、L212、L307）；ui-workspaces/README.md（L64）
- **冲突点**：glossary 声明"过账→确认入库/发货/交付、暂挂→先跳过，W09 已落地"；W09 文档仍大量使用旧词"已过账/暂挂"，且文件自述"部分已实现（Q1 未定）"与 README 标"草稿"不一致
- **建议方向**：W09 文档按 glossary §7 全面替换过账/暂挂，同步 README 索引状态
- **状态**：✅ 已修复（2026-08-04）
- **决议**：W09 文档按 glossary G1/G5 全面替换（过账→确认入库/仓发/交付、暂挂→先跳过，约 40 处），保留枚举/字段名/实现术语与 Q1 待确认标注；文件状态改为"已实现（Q1 未定）"，README 索引同步。

### D-05 【低】资金/库存单据终态词不一致："已生效/已作废" vs "已过账/已冲正"

- **性质**：术语冲突
- **涉及文档**：erp-phase-1.md §10.1（L1106-1113）；erp-data-model.md §7.5（L3627-3635）、§6.8（L1785）
- **冲突点**：phase-1 通用状态词"已生效/已作废"；数据模型 customer_receipt.status = 草稿/已过账/已冲正
- **建议方向**：统一为数据模型口径（过账/冲正），phase-1 同步
- **状态**：待讨论

### D-06 【低】W13 页面文案使用「正式过账」，违反 ui-glossary 且不在漂移清单内

- **性质**：术语冲突（漂移清单遗漏）
- **涉及文档**：w13-card-funds-review.md（L169）；ui-glossary.md §2 P2（L85）、§7 待跟进（L419-421）
- **冲突点**：W13「登记事实已正式过账」；glossary P2「正式过账 → 过账 / 确认入账」，待跟进清单未含 W13
- **建议方向**：W13 改文案，并补入 glossary 漂移清单
- **状态**：待讨论

### D-07 【低】ui-workspaces/README 索引状态过时（W04/W09/W21 状态值不在规范定义内）

- **性质**：过时（索引与文件头部不一致）
- **涉及文档**：ui-workspaces/README.md（L45-50、L59、L64、L76）；w04-contracts.md（L3）；w09-fulfillment-operations.md（L3）；w21-supplier-catalog.md（L3）
- **冲突点**：README 状态定义仅六种（样板/草稿/评审中/已确认/已实现/已验收），索引却标 W04「草稿」（文件自述「执行规范」）、W09「草稿」（自述「部分已实现」）、W21「已确认业务方向」（非规范状态值）
- **建议方向**：README 索引状态与文件头部同步，W21 状态值归入规范
- **状态**：待讨论

---

## 二、前端代码 vs 文档冲突（C 系列）

### P0 文案违规（高，C-01 ~ C-10）：枚举原值 / 字段名 / 内部 ID / Q 代号上屏

> 共性建议方向：按 ui-glossary 第 5 轮决议的替换词修正代码文案（这些都在 glossary 已有明确映射，属"上轮清零不彻底"）。

| 编号 | 位置 | 上屏内容 | 应改为 |
| --- | --- | --- | --- |
| C-01 | features/card-funds-review/api.ts:363 | "任务仍为 PENDING/IN_PROGRESS" | 任务仍在待处理列表 |
| C-02 | features/supplier-settlements/supplier-settlements-page.tsx:1942-1948 | `{workItemId} · subjectHash={...}` | 业务 ID / 数据版本 |
| C-03 | 同上 :2156-2157 | lockedFields 显示 sourceSnapshotHash/subjectHash | 字段名不上屏 |
| C-04 | ~~features/ownership-migration/ownership-migration-page.tsx:1325~~ | "scopeHash / 分面变化…" | W24 已删除，随 B-01 关闭 |
| C-05 | ~~同上 :2061、2105、2121、2124-2133~~ | "T：…"、enabledAt/enabledBy/migrationScopeDigest/confirmationDigest | W24 已删除，随 B-01 关闭 |
| C-06 | features/integration-errors/integration-errors-page.tsx:1370 | "客户端不得传入 originalActionIdempotencyKey" | 系统不得传入…（字段名去除） |
| C-07 | 同上 :1605 | "RESOLVE 已从 allowedActions 排除" | 处理完成已从可操作范围排除 |
| C-08 | features/history-backfill/history-backfill-page.tsx:795、973、1313、1499-1500 | "[rangeStart, T)、occurredAt、rangeEnd = T" | 切换编号/范围起点/截止时点（代码 1651-1658 已改，此处漏改） |
| C-09 | features/access-audit/access-audit-page.tsx:2126、1605 | "Q1 前本命令不携带 workItemId / claimToken"、raw workItemSupport | 策略业务名、内部 ID 不上屏 |
| C-10 | features/sales-orders/acceptance-workspace.tsx:1202 | "REVERSE 分配纠正" | 反向记录/冲减分配纠正 |

- **状态**：✅ 已修复（2026-08-04，批量文案修复，见讨论记录）

### 中严重度（C-11 ~ C-18）

| 编号 | 主题 | 位置 | 冲突点 | 建议方向 |
| --- | --- | --- | --- | --- |
| C-11 | 纸质投影残留 | features/sales-orders/sales-order-paper-dialog.tsx:49 | "纸质投影" | 改"打印件" |
| C-12 | "过账"未按 G5 决议替换 | acceptance-workspace.tsx:1190/1325；customer-receivables 5 处；supplier-payables 3 处；inventory/api.ts:734；inventory-ledger-page.tsx:577 | glossary 已决议不保留"过账" | 按业务类型改"确认入库/确认核销/确认入账"等 |
| C-13 | "暂挂"未按 G1 决议替换 | unified-task-queue、procurement-confirmation、card-funds-review、mall-sync、supplier-order-center、integration-errors 等 20+ 处 | glossary 已决议"暂挂→先跳过"（W09 已落地，其余待跟进） | 批量替换为"先跳过" |
| C-14 | 架构词上屏（服务端/客户端/本地/前端） | 12+ 文件（supplier-settlements、supplier-catalog、master-data、mall-sync、customer-quality、actual-profit-loss、product-publications 等） | glossary P1：服务端→系统、本地→本页/你输入的、客户端→删除 | 按 glossary 替换 |
| C-15 | "核销会话"未按 P2 替换 | customer-receivables、supplier-payables 5 处 | glossary P2：核销会话→本次核销 | 替换 |
| C-16 | "终态"残留 | mall-sync-page.tsx:1316、1944 | glossary P2：终态→处理结果/处理完成 | 替换 |
| C-17 | W09 跳 W06 用 ?tab=acceptance 而 W06 只认 section | fulfillment-operations-page.tsx:1009 vs sales-order-detail-page.tsx:212、app/.../page.tsx | 跳转后验收页不会自动定位 | ✅ 已修复：改 ?section=acceptance（tsc 通过） |
| C-18 | README 索引状态全部"样板/草稿"但 30 页均已实现 | ui-workspaces/README.md:56-85 vs w-page-coverage.md、verify-workspaces.mjs（30/30 通过） | 文档未更新 | 批量更新索引状态为"已实现/已验收" |

- **状态**：C-11~C-16 ✅ 已修复（2026-08-04）；C-17 ✅ 已修复（2026-08-04）；C-18 待讨论

### 低严重度（C-19 ~ C-24）

| 编号 | 主题 | 位置 | 冲突点 | 建议方向 |
| --- | --- | --- | --- | --- |
| C-19 | W23 对象态在 page 变体下叠 DocumentHeader | execution-projections-page.tsx:696、1095-1127 | 违反 erp-ui-design §4.5.1"列表+对象同页混合壳"字面契约 | 确认 Sheet 浮层场景豁免或调整 |
| C-20 | P2 散词残留（掩码/工作面/会话/快照） | purchase-orders、supplier-api-connections、procurement-confirmation、unified-task-queue、supplier-catalog、master-data、product-publications 等 | glossary P2：掩码→打码、工作面→页面、会话→本次操作、快照→历史记录 | 替换 |
| C-21 | maskCost/noSensitive 被查询消费但无 UI 控件 | supplier-order-center-page.tsx:117-136 | 违反 README 规则 9（隐形状态） | 补控件或从查询摘除 |
| C-22 | 侧栏导航结构漂移 | lib/workspace-registry.ts:329-609 vs erp-ui-design.md §3.3 | IA 演进未回写文档 | 更新 erp-ui-design §3.3 |
| C-23 | 检查产物 W21 模式过时 | w-routes-inventory.txt:21（M3+M4）vs README/registry（M2+M3+M4） | 检查产物未更新 | 重新生成检查产物 |
| C-24 | W09 文档自述状态与 README 矛盾 + 正文仍写"暂挂" | w09-fulfillment-operations.md:3、61-63、104 vs README:64；代码已按 G1 落地 | 文档未更新 | ✅ 已修复（随 D-04）：全文替换为先跳过/确认，状态同步"已实现" |

- **状态**：待讨论

---

## 三、前后端设计一致性冲突（B 系列）

### B-01 【高】主责迁移"主责系统"被实体化为独立表，与数据模型"不建当前主责副表"矛盾

- **性质**：数据模型冲突（实体命名 + 设计决策）
- **涉及文档**：erp-phase-2.md §14.2（L1234-1235）vs erp-data-model.md §6.16（L2595）、§10（L3926）、W24（L29）、erp-mall-data-mapping.md §2.3（L127）
- **冲突点**：
  - phase-2：建 `sales_order_ownership`（当前主责副表）+ `sales_order_ownership_migration`
  - 数据模型/W24/mall-mapping：主责只改 `sales_order.owner_system` 字段，迁移历史写 `sales_order_owner_migration_batch/item`，**不建当前主责副表**
- **建议方向**：phase-2 §14.2 删除 `sales_order_ownership` 副表，迁移表改名 `sales_order_owner_migration_batch`
- **状态**：✅ 已修复（2026-08-04）
- **决议**：彻底简化。主责迁移是一次性运营行为（停止从商城开单，存量单 `owner_system` MALL→ERP），非数据实体：删除全部专用表（`sales_order_ownership`、`sales_order_owner_migration_batch/item`、`sales_order_ownership_migration`），无批次/冻结窗口/scope_hash/基线确认/迁移授权；仅保留 `origin_system` + `owner_system` 字段 + 通用审计；**W24 页面取消**（路由、feature、mock、侧栏、registry 全部删除）；W17 删除冻结逻辑保留封存只读态；执行投影三表保留。涉及 12 个文档 + 前端 10 个文件（verify 29 工作面通过、tsc 通过）。

### B-02 【中】二期消费事实实体名冲突：`mall_consumption_fact` vs `mall_consumption_entry`

- **性质**：数据模型冲突（表名 + 成本口径字段落点不一致）
- **涉及文档**：erp-phase-2.md §14.3（L1261）vs erp-data-model.md §6.17（L2978、2998）
- **冲突点**：phase-2 用 `mall_consumption_fact` 且把成本口径直接放表上；数据模型用 `mall_consumption_entry` + 成本口径拆入 `mall_consumption_cost_assessment`（追加不修改原消费）
- **建议方向**：phase-2 §14.3 同步数据模型命名与拆表决策
- **状态**：待讨论

### B-03 【中】二期对账表命名冲突：`integration_reconciliation_*` vs `reconciliation_*`

- **性质**：数据模型冲突（W29 与数据模型一致，phase-2 §14.5 单独偏离）
- **涉及文档**：erp-phase-2.md §14.5（L1290-1291）vs erp-data-model.md §6.21（L3482、3494、3504）、W29（L20、170）
- **建议方向**：phase-2 §14.5 改为 `reconciliation_job / reconciliation_difference / reconciliation_difference_resolution`
- **状态**：待讨论

### B-04 【中】`sales_visible_price` vs `sales_visible_price_gross` 字段名混用

- **性质**：数据模型冲突（前端契约两种拼写；W21 文件内部自相矛盾）
- **涉及文档**：erp-data-model.md §6.3（L823，权威 `sales_visible_price_gross`）vs erp-mall-data-mapping.md §2.2（L101-102）、W14（L154、187）、W21（L27、114 vs L258）
- **建议方向**：统一为 `sales_visible_price_gross`（mall-mapping、W14、W21 同步修正）
- **状态**：待讨论

### B-05 【中】一期卡券快照/指纹字段集不一致（phase-1 缺"项目名称/业务备注"）

- **性质**：数据模型冲突（指纹由商城按同一规则计算，字段集不一致会造成每日核对系统性偏差）
- **涉及文档**：erp-phase-1.md §8.2（L863-870，自述为指纹权威）vs erp-data-model.md §6.13（L2248）、§6.4（L1073-1075）、erp-mall-data-mapping.md §3.3（L203-206）、§11.4（L3993-3994）、phases-8.md §5.1（L87-88）
- **建议方向**：phase-1 §8.2 指纹快照字段集补入"项目名称、业务备注"
- **状态**：✅ 已修复（2026-08-04）
- **决议**：以 phase-1 §8.2 为权威，从指纹/快照字段集移除项目名称、业务备注（`project_name`/`business_remark` 仍随销售版本保存同步，但不参与 content_hash，变化不产生版本差异）。已同步数据模型 §6.13/L1071/L3948、mall-mapping §3.3/L266、phases-8 §5.1。

### B-06 【低】phases-3 §6.1 关于"供应商商品库列为二期扩展"的陈述已过时

- **性质**：文档间引用过时（矛盾已解决，残留旧陈述）
- **涉及文档**：phases-3.md（L132-135）vs erp-data-model.md §5.3（L308-314）、§10（L3927）、phases-10.md（L102-106）
- **建议方向**：删除/修正 phases-3 过时陈述
- **状态**：待讨论

### B-07 【中】迟到快照：phase-1 说"丢弃"，phases-8/数据模型/mall-mapping 说"保留为迟到证据"

- **性质**：行为/状态机冲突（是否持久化迟到快照，直接决定同步表保留策略与每日核对能力）
- **涉及文档**：erp-phase-1.md §8.3（L898）、§8.5（L938）、§12（L1203）vs phases-8.md §5.2（L95-96）、erp-data-model.md §6.13（mapping_status=迟到丢弃）、erp-mall-data-mapping.md §3.4（L224-225）
- **建议方向**：phase-1 改为"迟到快照保留为『迟到丢弃』证据，不回退当前版本"
- **状态**：✅ 已修复（2026-08-04）
- **决议**：以 phase-1 为权威，迟到快照**直接丢弃**（不持久化、不保留证据、不回退当前版本）；`mapping_status` 移除"迟到丢弃"枚举值。已同步数据模型 §6.13/L2228、mall-mapping §3.4/L224/L908、phases-8 §5.2/L96、W17 L166。

---

## 四、讨论记录

（每解决一条，在此追加一行：编号、决议内容、修改的文件、日期）

| 日期 | 编号 | 决议 | 修改文件 |
| --- | --- | --- | --- |
| 2026-08-04 | D-01 | commercial_status 仅保存 4 值主状态（DRAFT/PENDING_REVIEW/EFFECTIVE/VOIDED），中间审核环节全部归 review_status | erp-data-model.md §7.1、§7.3 |
| 2026-08-04 | D-02 | 区分两类手机号：不保存卡实例绑定手机号及可逆映射；履约收货手机号仅加密快照 | erp-phase-1.md §11.1 L1154、w03-customer-center.md L33 |
| 2026-08-04 | B-01 | 主责迁移=一次性运营行为：删全部专用表（ownership/batch/item），仅 origin_system+owner_system 字段+通用审计；W24 页面取消（前端 feature/路由/registry/mock 全删，verify 29 工作面通过）；W17 删冻结逻辑保留封存态；执行投影三表保留 | erp-data-model.md §6.16/§7.8/§10、erp-phase-2.md §14.2/§15/§17/§20、w17 全文、w24 改为取消说明、README 索引、mall-mapping §10、w05/w18/w23/w25/w29/w30、ui-design/ui-flows/glossary、phase-1 L856、erp-client 10 文件 |
| 2026-08-04 | C-01~C-16、C-20 | 全部文案一批修复：P0 枚举/字段名/内部ID上屏清零（C-01~C-10，其中 C-04/C-05 随 W24 删除关闭），过账/暂挂/架构词/核销会话/终态/散词按 glossary 映射替换（C-11~C-16、C-20），tsc/eslint 通过 | erp-client/features 约 48 个文件（card-funds-review、supplier-settlements、integration-errors、history-backfill、access-audit、sales-orders、customer-receivables、supplier-payables、inventory、unified-task-queue、procurement-confirmation、mall-sync、supplier-orders、supplier-catalog、master-data、purchase-orders、supplier-api-connections、actual-profit-loss、product-publications、workspace-kit 等） |
| 2026-08-04 | D-03 | 供应商订单状态机以数据模型三轨为准：phase-2 §10.3 重写为履约轨(9值)+取消轨(5值)+退款轨(6值)，互不折算 | erp-phase-2.md §10.3 |
| 2026-08-04 | C-17 | W09→W06 跳转参数 tab 改 section，与 W06 路由契约对齐（tsc 通过） | erp-client/features/fulfillment-operations/fulfillment-operations-page.tsx:1009 |
| 2026-08-04 | B-05 | 指纹字段集移除项目名称/业务备注（仍随版本保存但不参与 content_hash），以 phase-1 §8.2 为权威 | erp-data-model.md §6.13/L1071/L3948、erp-mall-data-mapping.md §3.3/L266、phases-8.md §5.1 |
| 2026-08-04 | D-04+C-24 | W09 文档按 glossary G1/G5 全面替换（过账→确认入库/仓发/交付、暂挂→先跳过约40处），保留枚举/字段名与 Q1 待确认标注；文件状态与 README 索引同步"已实现" | w09-fulfillment-operations.md、ui-workspaces/README.md |
| 2026-08-04 | B-07 | 迟到快照直接丢弃（以 phase-1 为权威）：不持久化、不保留证据、不回退当前版本；mapping_status 移除"迟到丢弃"枚举 | erp-data-model.md §6.13/L2228/L2237、erp-mall-data-mapping.md §3.4/L224/L908、phases-8.md §5.2/L96、w17 L166 |
