# W09 · 履约作业

> 状态：草稿
> 页面模式：M3 连续处理队列 + M5 简化作业
> 主要路由：`/fulfillment`
> 主要角色：采购经办、仓储经办；销售按关联对象查看进度
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

- 采购和仓储从同一个工作面处理入库、公司仓发、供应商直发、电子交付和线下服务五类作业，不在五个一级菜单之间切换。
- 每次作业自动带出销售单、采购单、采购销售分配、待处理数量、付款门禁、仓库/预占和客户交付上下文。
- 正式提交后先看到作业单号、数量、库存/预占影响和下一步，再自动进入同一筛选下的下一项。
- 从销售单或采购单追溯时进入同一作业工作面并定位具体任务/事实，返回后原对象页签不丢。

### 1.2 业务目标

- 用一套连续队列和五种受控表单承载第一期实物与服务销售的采购履约，避免五套导航和交互方言。
- 分别写入 `purchase_receipt`、`delivery`、`electronic_delivery`、`service_fulfillment` 等正式事实，不创造通用“履约单”覆盖领域单据。
- 所有采购履约入口遵守同一付款门禁；库存入库、销售预占和仓发出库在正式事务内守恒。
- 已过账/已确认事实不可覆盖，错误通过冲正、退货、库存调整或新履约事实纠正。

### 1.3 五类作业边界

| 作业类型 | 责任角色 | 正式事实 | 关键业务影响 |
| --- | --- | --- | --- |
| 入库 | 仓储 | `purchase_receipt` + 行 | 合格量增加库存并沿采购销售分配形成预占；不合格量不入库存 |
| 公司仓发 | 仓储 | `delivery(type=WAREHOUSE_SHIP)` + 行 | 消耗本销售明细有效预占并减少自有库存 |
| 供应商直发 | 采购 | `delivery(type=SUPPLIER_DIRECT)` + 行 | 记录供应商直接发客户，不写自有库存流水 |
| 电子交付 | 采购 | `electronic_delivery` | 记录交付对象、数量、时间、结果和凭证，不保存卡号/卡密 |
| 线下服务 | 采购 | `service_fulfillment` | 记录服务对象、地点、起止时间、结果、说明和凭证 |

这里的“五类”是统一工作面中的作业类型，不是一个可互换的通用单据状态机。每种正式事实仍遵循 `erp-data-model.md` 的固定状态与约束。

### 1.4 不在本工作面完成

- 不执行客户验收；发货、交付或服务完成后，销售在 W06 登记客户验收。物流签收也不等于 ERP 客户验收通过。
- 不修改采购单、销售单或采购确认；来源条件变化进入 W05/W08 变更流程。
- 不登记供应商付款、进项发票或核销；先款门禁不满足时进入 W12。
- 不直接编辑库存余额或预占；库存只由正式入库、出库、退货、冲正和调整流水改变。
- 不把第二期卡券消费触发的 API 供应商订单并入本工作面；该链路属于 W26。
- 不在前端通过隐藏按钮实现付款门禁；服务端正式过账入口必须重验。

## 2. 用户、权限、数据范围与租约

### 2.1 角色与五类责任

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 仓储经办 | W09，默认类型“入库 + 公司仓发” | 本人所属仓库及任务责任域 | 领取、登记入库、公司仓发、暂挂 |
| 仓储复核 / 负责人 | W09；若 Q1 最终选择 `WORK_ITEM`，才可从 W02 进入 | 本仓储责任域 | 查看、受控复核/转交；不因负责人身份自动获得经办权 |
| 采购经办 | W09，默认类型“直发 + 电子 + 服务” | 本人负责采购单及相关销售分配 | 领取、登记直发/电子/服务、暂挂 |
| 销售 | W05 履约子区 → W09 | 自己负责销售单 | 只读查看作业结果与预计时间；不能替执行部门过账 |
| 财务 | W08/W12 → W09 | 财务范围内采购单 | 查看门禁和履约结果；不执行仓储/采购动作 |
| 管理层 / 审计 | 分析/W19 下钻 | 授权范围 | 只读 |

### 2.2 权限表达

| 情况 | W09 行为 |
| --- | --- |
| 无 W09 模块权限 | 入口隐藏；直接访问显示无权限 |
| 有 W09 权限但无仓库/采购数据范围 | 显示无数据范围专用空态，不展示全公司数量 |
| 仅有部分作业类型权限 | 类型分段只展示授权类型；直接 URL 指向无权类型时显示无权限而非空列表 |
| 有对象查看权但不能执行 | 作业上下文可读，正式动作可见禁用并展示责任角色/门禁原因 |
| 无客户地址/交付对象字段权限 | 标签保留、值掩码；执行需要该字段时由服务端判为关键字段权限不足 |
| 无采购成本权限 | 作业数量和对象仍可见，成本字段不返回原值；仓储无需成本即可作业 |
| 页面期间权限收回 | 停止续租/保存，清除地址、联系人、成本和附件敏感缓存，切无权限态 |

服务端按作业类型、采购责任、仓库范围、销售参与权和字段权限过滤。前端不能读取全量任务后按角色隐藏。

### 2.3 作业任务与租约边界

- 普通履约队列由待确认事项 Q1 在以下两个候选中选择且只选择一个；同一作业类型、同一待处理事实不能同时物化为两种身份，也不能在前端互相映射状态。
- `WORK_ITEM` 候选：后端先在统一数据模型/API 的固定注册表中固化普通履约 `work_item_type`，队列唯一身份是 `workItemId`，领取与租约完整遵循 W02。只有该候选可汇聚到 W01/W02；从 W01/W02 进入的正式过账必须包在 W02 `CompleteWorkItemEnvelope` 中，并把领域事实过账和任务完成放在同一事务。
- `DOMAIN_OPERATION` 候选：队列身份是 `operationTaskId`，它只是由来源单据、剩余可作业量和正式履约事实派生的领域作业投影，不是 `work_item`。它不得出现在 W01/W02，不得调用 `work_item` 领取、转交或完成接口，也不得复制 `work_item.status` 形成第二任务状态机；“可处理/历史”由正式领域事实派生。
- 无论选哪个候选，都必须携带来源对象版本和一次有效处理租约；领域投影租约也须满足原子领取、令牌摘要、版本续期、失效审计和同一投影单处理人语义，但投影 ID 不得成为正式业务事实主键。
- `BUSINESS_EXCEPTION` 只承载异常，不得拿来伪装全部正常履约任务。
- 租约令牌仅存当前会话内存，不写 URL、日志、本地长期存储或埋点。
- 租约保护处理权，不改变采购、库存、履约或销售状态；“暂挂”只释放当前处理权并移动队列指针。

Q1 确认并写回权威数据模型/API 前，W09 的全部正常履约执行入口都是实施 blocker：不得开放队列、领取、保存、暂挂或过账，也不得上线任何临时模式。下文两套类型只用于设计评审，不是客户端或服务端运行时判别联合。确认后，代码生成与实现必须删除未选候选及所有 `mode` 自选参数；服务端路由只能固化一种身份并拒绝客户端选择模式，不能同时返回 `workItemId` 与 `operationTaskId`，也不能以兼容字段长期维护两套事实源。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 角色默认着陆 | 仓储/采购登录或侧栏 | Q1 确认后 `/fulfillment?scope=mine`，按角色恢复类型偏好；确认前显示实施 blocker，不请求正常履约队列 | 不适用 |
| 类型切换 | 页头分段控件 | `type=receipt|warehouse_ship|supplier_direct|electronic|service`；同一 W09 页签 | 浏览器后退恢复上一类型 |
| 今日工作台/待办 | W01/W02；仅 Q1 选择 `WORK_ITEM` 后 | 将来源 `workItemId` 写入 `currentWorkItemId`，并携带 `queueContextId` 和类型，聚焦 W09 | 事实过账与任务完成同事务后，可继续队列或回来源 |
| 从采购单处理 | W08 履约子区 / 门禁结果 | 携带采购单、版本/行和目标任务；新开或聚焦 W09 | 返回 W08 原履约子区 |
| 从销售单处理/查看 | W05 履约子区 | 携带销售单/明细和作业事实；执行者进任务，销售只读 | 返回 W05 原时间线 |
| 从库存处理 | W10 预占/库存行 | 仓发时携带仓库、SKU、预占和销售明细 | 返回 W10 原筛选 |
| 查看正式作业 | W05/W08 时间线或 W09 历史 | URL 追加 `factType` / `factId` 打开只读详情 | 关闭回队列原位置 |
| 刷新 | 当前作业 | 恢复类型、筛选、当前任务、来源对象和自动下一项；重新领取/校验 | W09 |

候选 URL（仅供 Q1 评审；实现时只保留选中候选的一行）：

```text
/fulfillment?type=warehouse_ship&scope=mine&warehouseId=wh_1&currentWorkItemId=wi_123&queueContextId=qc_456&autoNext=1
/fulfillment?type=warehouse_ship&scope=mine&warehouseId=wh_1&currentOperationTaskId=op_123&queueContextId=qc_456&autoNext=1
```

TaskTabs 身份为 `queue:fulfillment:{userId}:{scopeDigest}`；五种类型共享一个 W09 页签，不为每类创建平行任务页签或一级菜单。打开具体正式事实的对象页签身份由其强类型事实稳定 ID 决定，但默认使用 W09 当前页内详情。

URL 不包含地址、联系人、物流号、数量、权限结论、租约令牌或门禁结果。刷新和从 W05/W08 返回时必须重新查询这些事实。

## 4. 页面布局

### 4.1 1440×900 基准布局

```text
┌ PageHeader：履约作业 · 待我处理 36                     数据水位 09:36 ┐
├ MetricStrip：待入库 8 | 待仓发 12 | 待代发 6 | 待电子 5 | 待服务 5  │
├ 类型：[全部] [入库] [公司仓发] [供应商直发] [电子交付] [线下服务]   │
├ ListToolbar：仅我的 | 仓库 | 超期 | 采购单/销售单号 | 自动下一项 开   │
├────────────── 队列 32% ─────────────┬──────── 当前作业 68% ──────────┤
│ 第 3/36 · 已领取 · 剩余 08:42        │ 销售 XS… / 采购 PO… / 供应商   │
│ [超期] 入库 · PO… · 仓库 A           │ PrepaymentGate / 来源版本      │
│       待处理 100 件                   │                               │
│ [普通] 仓发 · XS… · 仓库 A           │ 当前类型表单                   │
│       预占 80 件                      │ 行项目、数量、仓位/物流/凭证   │
│ ...                                   │                               │
│                                       │ ValidationSummary             │
│                                       │ [暂挂] [保存] [确认过账并下一项]│
└───────────────────────────────────────┴───────────────────────────────┘
```

### 4.2 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 页头与指标 | 看清五类作业水位与更新时间 | `PageHeader` `MetricStrip` `DataFreshness` | 顶部 |
| 类型分段 | 在同一工作面筛选五类作业 | Segmented 控件 | 顶部 sticky |
| 队列列表 | 扫描当前筛选任务、超期和责任 | `WorkTaskItem` | 桌面左栏独立滚动 |
| 连续处理上下文 | 位置、租约、上下项、自动下一项 | `SequentialProcessBar` | 当前作业顶部 sticky |
| 来源上下文 | 销售/采购、供应商、SKU/服务、门禁、剩余量 | `DocumentSummary` `PrepaymentGate` | 否 |
| 类型表单 | 五种作业的受控字段 | M5 简化表单 + 行表 | 右主滚动区 |
| 校验与动作 | 守恒、敏感字段、版本和正式提交 | `ValidationSummary` `FormalActionConfirmDialog` | 右栏底部 sticky |
| 固定结果 | 作业单号、库存/预占影响、下一步 | `FormalActionResult` | 成功后置顶 |

### 4.3 五种表单插槽

#### 入库

- 只读：采购单/版本、供应商、采购行、累计有效收货、剩余可收、付款门禁。
- 可写：入库仓、到货数量、合格数量、不合格数量、质量结果、到货/过账时间、凭证。
- 约束：合格 + 不合格不得超过到货；累计有效收货不超采购数量，超收进入受控审批/采购变更。
- 成功影响：合格量写库存增加流水、库存余额，并沿采购销售分配建立销售预占；不合格量不入库。

#### 公司仓发

- 只读：销售单/明细、仓库、SKU、有效预占、可用量、已发/剩余量、客户交付信息（按权限）。
- 可写：本次发货数量、承运方、物流单号、发货时间、凭证/备注。
- 约束：必须引用本销售明细有效 `stock_reservation_id`，发货量不超过有效预占和变更后销售数量。
- 成功影响：消耗预占、写库存减少流水并形成仓发事实。

#### 供应商直发

- 只读：采购单/版本、销售单/明细、供应商、采购销售分配、剩余可发、付款门禁。
- 可写：发货数量、承运方、物流单号、发货时间、凭证。
- 约束：必须引用同销售明细和采购单的有效 `purchase_line_sales_allocation_id`；不写自有库存流水。
- 成功影响：形成直发事实；后续由销售 W06 验收。

#### 电子交付

- 只读：采购/销售分配、商品、剩余可交付、付款门禁。
- 可写：交付对象加密快照、数量、实际时间、结果（成功/部分成功/失败）、凭证。
- 约束：敏感交付信息加密；失败后重做形成新记录，不覆盖失败事实。
- 成功影响：形成已确认电子交付事实；不改变自有库存。

#### 线下服务

- 只读：采购/销售分配、服务内容、客户约定、剩余服务量、付款门禁。
- 可写：服务对象、地点、开始/结束时间、数量、结果、完成说明、凭证。
- 约束：结束不早于开始；失败/部分成功留正式记录，重做另建记录。
- 成功影响：形成已确认服务履约事实；后续进入 W06 客户验收。

## 5. 展示内容与字段

### 5.1 队列与公共上下文

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 队列 | `position` / `total` | 第 N/M 项 | 服务端当前队列快照 | 当前筛选位置，不是全局固定序号 | W09 用户 |
| 队列 | `operationType` | 入库/仓发/代发/电子/服务 | 服务端作业投影 | 固定 UI 映射，不用五个菜单 | 按类型权限 |
| 队列 | `priority` / `dueAt` | 优先级 / 截止 | 任务/作业投影 | 超期文字 + 时长 | 同上 |
| 租约 | `claimedBy` / `leaseExpiresAt` | 处理人 / 占用到期 | 任务租约 | 不显示令牌 | 当前查看者 |
| 来源 | `purchaseNo` / `salesOrderNo` | 采购单 / 销售单 | `business_document` 与强类型表 | 稳定业务号，可钻取 | 按对象权限 |
| 来源 | `sourceRevision` | 来源版本 | 采购/销售当前有效版本 | 过账时重验 | 同上 |
| 主体 | `supplierSnapshot` / `customerSnapshot` | 供应商 / 客户 | 正式版本快照 | 不追随当前主数据改名 | 字段权限裁剪 |
| 数量 | `remainingQuantity` | 待处理数量 | 服务端按有效事实/冲正/变更计算 | 基础单位；不跨单位求和 | 执行角色 |
| 门禁 | `prepaymentGate` | 付款条件 | 采购版本快照 + 有效付款净核销 | 服务端 SATISFIED/BLOCKED/NA | 金额可掩码但 blocker 可见 |

### 5.2 入库字段

| 字段 | 用户文案 | 数据来源 / 提交去向 | 规则 |
| --- | --- | --- | --- |
| `warehouseId` | 入库仓 | `purchase_receipt.warehouse_id` | 仓库有效且用户有作业范围 |
| `receiptLineId` / `purchaseRevisionLineId` | 入库行 / 采购明细 | 入库草稿、采购版本行 | 必须属于当前采购版本 |
| `receivedQuantity` | 到货数量 | `purchase_receipt_line.received_quantity` | >0，最多 6 位 |
| `qualifiedQuantity` | 合格数量 | `qualified_quantity` | 非负；仅此数量入库和预占 |
| `rejectedQuantity` | 不合格数量 | `rejected_quantity` | 非负；不直接创建采购退货 |
| `qualityResult` | 质量结果 | `quality_result` | 受控代码 + 业务说明 |
| `postedAt` | 入库时间 | `purchase_receipt.posted_at` | 业务时间与记录时间分开 |
| `evidenceAttachmentId` | 收货/质检凭证 | 附件引用 | 权限和安全扫描通过 |

### 5.3 仓发与直发字段

| 字段 | 用户文案 | 数据来源 / 提交去向 | 规则 |
| --- | --- | --- | --- |
| `deliveryType` | 发货方式 | `delivery.delivery_type` | 仓发或供应商直发，表单中不可互换 |
| `salesOrderLineId` | 销售明细 | `delivery_line.sales_order_line_id` | 属于当前有效销售单 |
| `quantity` | 发货数量 | `delivery_line.quantity` | >0；累计不超当前有效销售量 |
| `stockReservationId` | 销售预占 | 仓发 `delivery_line.stock_reservation_id` | 仓发必填且归属本销售明细 |
| `purchaseLineSalesAllocationId` | 采购销售分配 | 直发对应字段 | 直发必填；仓发为空 |
| `warehouseId` | 发货仓 | 仓发 `delivery.warehouse_id` | 仓发必填；直发为空 |
| `carrier` / `trackingNo` | 承运方 / 物流单号 | `delivery` | 根据业务规则必填；物流号可受权限掩码 |
| `shippedAt` | 发货时间 | `delivery.shipped_at` | 业务时间 |
| `evidenceAttachmentId` | 发货凭证 | 关联附件 | 下载再次鉴权 |

### 5.4 电子交付与服务字段

| 字段 | 用户文案 | 数据来源 / 提交去向 | 规则 |
| --- | --- | --- | --- |
| `fulfillmentNo` | 履约记录号 | 过账后正式编号 | 结果固定展示 |
| `purchaseLineSalesAllocationId` | 采购销售分配 | 电子/服务正式事实 | 必须与销售明细、采购单和有效数量一致 |
| `recipientSnapshot` | 交付/服务对象 | 加密快照 | 按字段权限短时展示，日志不记录原文 |
| `quantity` | 交付/服务数量 | 正式事实 | >0，最多 6 位 |
| `occurredAt` | 实际发生时间 | 正式事实 | 页面按业务时区格式化 |
| `result` | 履约结果 | 成功 / 部分成功 / 失败 | 失败不是可覆盖草稿；重做新建记录 |
| `serviceLocation` | 服务地点 | 服务事实 | 仅服务类型；敏感时加密/掩码 |
| `startedAt` / `endedAt` | 服务起止 | 服务事实 | 结束时间不早于开始 |
| `completionNote` | 完成说明 | 服务事实 | 业务语言，不写技术日志 |
| `evidenceAttachmentId` | 交付/服务凭证 | 正式事实附件 | 安全扫描、权限和保留规则 |

### 5.5 正式结果与后续

| 字段 | 用户文案 | 数据来源 | 规则 |
| --- | --- | --- | --- |
| `factNo` / `factId` | 入库/发货/履约编号 | 正式事务结果 | 成功唯一依据 |
| `postedOrConfirmedAt` | 过账/确认时间 | 正式事实 | 与表单业务发生时间分开展示 |
| `inventoryDelta` | 库存变化 | 入库/仓发事务结果 | 直发/电子/服务显示“不影响自有库存” |
| `reservationDelta` | 预占建立/消耗 | 库存预占事务结果 | 仅相关类型展示 |
| `remainingQuantity` | 剩余待处理 | 服务端正式投影 | 前端不从当前表单相减形成事实 |
| `acceptanceNextStep` | 待客户验收 | 服务端下一步动作 | 指向 W06；不宣称已验收 |
| `nextTask` | 下一作业 | 服务端选定契约的任务引用 | 成功固定结果后才导航；运行时只存在 Q1 选中的一种身份 |

## 6. 搜索、筛选、排序与默认视图

### 6.1 默认视图

- Q1 确认并完成唯一契约实现后，默认 `scope=mine`、`view=actionable`、`roundView=remaining`，类型按当前角色偏好恢复；仓储默认入库+仓发，采购默认直发+电子+服务。未确认前不生成默认正常履约队列。
- 默认排序：已超期 → 高优先级 → 截止时间 → 来源提交时间，由服务端完成。
- 默认“成功后自动下一项”开启；正式结果不确定、版本冲突或租约丢失时强制暂停。
- 队列每批预取下一项的非敏感身份摘要，但不得提前领取或返回敏感地址/联系人。

### 6.2 筛选契约

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 作业类型 | 角色偏好 | `type` 可多选 | 分段筛选五类，始终同一 W09 工作面 |
| 责任范围 | `mine` | `scope=mine|role_pool` | 角色池任务打开前领取 |
| 仓库 | 当前仓储范围 | `warehouseId` | 入库/仓发适用；其它类型自动忽略并显示摘要 |
| 采购/销售单号 | 空 | `q` | 服务端精确/前缀搜索稳定业务号 |
| 供应商 | 全部 | `supplierId` | 入库、直发、电子、服务适用 |
| 时限 | 有效全部 | `due=active|today|overdue` | 使用作业责任时限，不改销售履约期限 |
| 付款门禁 | 全部 | `gate=blocked|satisfied` | 对入库/直发/电子/服务筛选；仓发显示不适用 |
| 任务视图 | 可处理 | `view=actionable|history` | 选 `WORK_ITEM` 时映射正式任务有效/终态；选 `DOMAIN_OPERATION` 时按来源与履约正式事实派生可处理/历史。实现只保留选中语义；历史均只读且不与待处理混排 |
| 本轮队列 | 未跳过 | `roundView=remaining|skipped` | `skipped` 始终只是当前 `queueContextId` 的派生游标视图，不是 `work_item.status`、领域投影状态或新业务状态 |
| 自动下一项 | 开 | `autoNext=1|0` | URL 显式值优先于用户偏好 |

切换类型或筛选时，如当前表单有未保存输入，必须保存、放弃或取消切换。隐藏已选行时显示摘要，不静默删除。

### 6.3 队列指标

- 五类数量由服务端按同一权限/数据范围与截止水位聚合，不能由当前加载队列求和。
- 指标可点击时具备按钮语义、选中态和筛选摘要；无权限类型整项不展示。
- “付款门禁阻塞”是跨入库/直发/电子/服务的可处理前置异常，不是新的履约状态。

## 7. 操作契约

下表是 Q1 确认后的统一交互要求；确认前全部领取、续租、保存、暂挂和正式主动作均返回 `FULFILLMENT_TASK_MODEL_UNCONFIRMED`。表内两个候选的差异用于评审，运行时实现只能保留选中的一个分支。

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 领取作业 | 队列项 | 类型动作权限；无有效他人租约 | 无 | 返回租约、当前来源版本和表单草稿 | 被领取时转只读并定位下一项 |
| 续租 | 系统自动/倒计时 | 当前领取人、页签前台、租约有效 | 无 | 新 `leaseVersion` / 到期时间 | 失败停止写入，保留输入 |
| 保存作业草稿 | 自动保存 / `⌘S` | 租约有效、来源版本未变、草稿版本匹配 | 无 | 返回新草稿版本与校验摘要 | 输入保留；冲突时不覆盖 |
| 暂挂 | 连续处理条/底栏 | 当前事实尚未过账、选中契约的租约有效 | 脏输入确认保存或放弃，并填写受控原因 | 选 `WORK_ITEM` 时使用 `WorkItemActionEnvelope<DeferFulfillmentAction>` 并保持 `PENDING | IN_PROGRESS`；选 `DOMAIN_OPERATION` 时使用独立暂挂命令释放领域租约。实现只保留一条路径，且只移动当前 `queueContextId` 游标 | mutation 失败停留当前项、保留令牌与游标；结果不确定按同幂等键查询，不能先跳下一项 |
| 入库过账 | 入库主动作 | W09 入库权限、门禁满足、数量/仓库/版本/租约有效 | 展示合格/不合格、库存与预占影响 | 原子写入库、库存增加、余额、销售预占、成本与进度；固定结果后下一项 | 不确定时不更新本地库存，查询结果 |
| 确认仓发 | 仓发主动作 | 仓发权限、有效预占、库存/销售数量/版本/租约有效 | 展示发货、预占消耗和库存减少 | 原子写发货、预占消耗、库存流水和余额；固定结果后下一项 | 失败保留草稿；冲突重取预占 |
| 确认供应商直发 | 直发主动作 | 采购权限、付款门禁满足、有效采购销售分配 | 展示发货和“不影响自有库存” | 写直发事实，更新履约进度；固定结果后下一项 | 不确定时查询同幂等结果 |
| 确认电子交付 | 电子主动作 | 采购权限、门禁满足、分配/敏感字段/凭证有效 | 展示对象、数量、结果、隐私提示 | 写不可覆盖电子交付事实；固定结果后下一项 | 失败输入保留，敏感值按策略清除/重填 |
| 确认服务履约 | 服务主动作 | 采购权限、门禁满足、分配/时间/结果/凭证有效 | 展示服务对象、地点、时间、结果 | 写不可覆盖服务事实；固定结果后下一项 | 失败保留非敏感输入；重验来源 |
| 去登记付款 | `PrepaymentGate` | W12 权限 | 无 | 打开供应商往来并预选采购应付 | 返回 W09 重查门禁和租约 |
| 去客户验收 | 固定结果/历史 | W06 与销售单权限 | 无 | 聚焦 W05 验收子区并携带履约事实 ID | 返回 W09 队列保留 |
| 查看/发起纠正 | 历史事实详情 | 对应退货/冲正/调整权限 | 强确认原事实与影响 | 进入原对象变更/异常流程或形成受控反向事实 | 失败不改原事实；不确定时查询 |

暂挂不是本地队列操作。若选择 `WORK_ITEM`，由 W02 通用动作协议记录动作并保持正式任务 `PENDING | IN_PROGRESS`；客户端仅在 mutation 明确返回租约已释放时清除会话令牌。若选择 `DOMAIN_OPERATION`，实现不读写 `work_item`，释放领域租约后只在当前 `queueContextId` 标记“本轮已跳过”。未选路径必须从生成代码和服务端接口中删除；任何实现都不得写入 `paused` 业务/任务/投影状态。

若选择 `WORK_ITEM`，五类正式主动作必须同时完成对应 `work_item`，不能先过账再由前端补调任务完成；若选择 `DOMAIN_OPERATION`，正式主动作只提交领域事实并让服务端重算投影资格，绝不调用 `work_item` 完成。两套候选的固定结果与自动下一项交互一致，但客户端不能在运行时选择或混用任务事实源。

### 7.1 付款门禁

- `PREPAY` 采购在入库、供应商直发、电子交付或线下服务正式过账前，服务端锁定采购单及付款分配，按有效已过账付款的净核销金额重算门槛。
- 付款申请、银行附件或未核销付款不算满足门禁。
- `POSTPAY` 在财务审核通过后允许上述履约，后续仍按付款条件生成付款待办。
- 付款冲正/反核销导致门槛不再满足时，不回退既有履约事实，只阻断新过账并生成财务异常任务。
- 公司仓发以有效库存与销售预占作为直接门禁；不得在前端另造一套付款判断。若服务端返回其它 blocker，W09 原样展示。

### 7.2 库存与预占事务

入库过账必须原子完成：入库头行 → 库存增加流水 → 库存余额 → 沿采购销售分配建立预占 → 实际成本 → 采购履约进度。

仓发过账必须原子完成：校验预占归属 → 预占消耗 → 库存减少流水 → 库存余额 → 仓发事实。

任何完成后的库存均须满足 `on_hand >= 0`、`reserved >= 0`、`available = on_hand - reserved >= 0`。前端不得先乐观扣库存再等待服务端纠正。

### 7.3 已发生事实与纠正

- 已过账入库不可编辑，只能冲正或采购退货；已发货事实不因销售/采购变更删除。
- 已确认电子/服务记录不可覆盖；失败重做创建新记录。
- 库存盘盈、盘亏或损坏走 W10 库存调整及岗位分离，不用 W09 修改数量。
- 供应商直发/客户拒收等后续处理走 W05/W08 变更与异常，原履约事实保留。

## 8. 数据契约

本节定义 UI 所需语义，不固定具体 HTTP 路径，也不在本文新增正常履约 `work_item_type`。所有带 `Candidate` 后缀的类型都是 Q1 设计评审材料，不得直接生成客户端或服务端运行时代码。Q1 确认后，只把选中的一组候选转为正式契约，并彻底删除另一组、`Candidate` 比较别名及任何 `mode` 请求字段；Q1 未确认时不提供正常履约 API。

### 8.1 队列查询

```ts
type WorkItemTaskRefCandidate = {
  workItemId: string
  operationTaskId?: never
}

type DomainOperationTaskRefCandidate = {
  workItemId?: never
  operationTaskId: string
}

// 仅供文档检查两个候选互斥；不得成为正式 API 的宽联合。
type FulfillmentTaskRefCandidate =
  | WorkItemTaskRefCandidate
  | DomainOperationTaskRefCandidate

type FulfillmentQueueItemBase = {
  operationType: "RECEIPT" | "WAREHOUSE_SHIP" | "SUPPLIER_DIRECT" | "ELECTRONIC" | "SERVICE"
  priority: number
  dueAt?: string
  sourceNo: string
  remainingQuantity: string
  unitCode: string
}

type WorkItemFulfillmentQueueItemCandidate = FulfillmentQueueItemBase & {
  taskRef: WorkItemTaskRefCandidate
}

type DomainOperationFulfillmentQueueItemCandidate = FulfillmentQueueItemBase & {
  taskRef: DomainOperationTaskRefCandidate
}

type FulfillmentQueueQueryBase = {
  operationTypes?: Array<"RECEIPT" | "WAREHOUSE_SHIP" | "SUPPLIER_DIRECT" | "ELECTRONIC" | "SERVICE">
  scope: "mine" | "role_pool"
  warehouseId?: string
  supplierId?: string
  q?: string
  due?: "active" | "today" | "overdue"
  gate?: "blocked" | "satisfied"
  view: "actionable" | "history"
  roundView?: "remaining" | "skipped"
  queueContextId?: string
  pageSize: number
}

type WorkItemFulfillmentQueueQueryCandidate = FulfillmentQueueQueryBase & {
  currentWorkItemId?: string
  currentOperationTaskId?: never
}

type DomainOperationFulfillmentQueueQueryCandidate = FulfillmentQueueQueryBase & {
  currentWorkItemId?: never
  currentOperationTaskId?: string
}

type FulfillmentQueueContextCandidate<TTaskRef extends FulfillmentTaskRefCandidate> = {
  queueContextId: string
  position: number
  total: number
  previousTask?: TTaskRef
  nextTask?: TTaskRef
  filterSummary: string
  snapshotUpdatedAt: string
}

type FulfillmentQueueMetrics = Array<{
  operationType: string
  label: string
  count: number
  visible: boolean
}>

type WorkItemFulfillmentQueueViewCandidate = {
  context: FulfillmentQueueContextCandidate<WorkItemTaskRefCandidate>
  metrics: FulfillmentQueueMetrics
  items: WorkItemFulfillmentQueueItemCandidate[]
  current?: WorkItemFulfillmentOperationViewCandidate
  emptyReason?: "NO_TASKS" | "FILTER_NO_RESULT" | "NO_DATA_SCOPE"
}

type DomainOperationFulfillmentQueueViewCandidate = {
  context: FulfillmentQueueContextCandidate<DomainOperationTaskRefCandidate>
  metrics: FulfillmentQueueMetrics
  items: DomainOperationFulfillmentQueueItemCandidate[]
  current?: DomainOperationFulfillmentOperationViewCandidate
  emptyReason?: "NO_TASKS" | "FILTER_NO_RESULT" | "NO_DATA_SCOPE"
}
/*
禁止把两个 Candidate 合成运行时 FulfillmentQueueView 联合，也禁止让请求携带
mode 选择分支；否则客户端可自选事实源，响应也可能夹入另一候选身份。
*/
```

正式 Query Key 至少包含用户、当前角色、权限/数据范围版本、五类筛选、仓库/供应商、任务视图、本轮派生游标、排序、队列上下文和选中契约的当前任务 ID。服务端路由由部署时选中的契约固定，不接受 `mode`，并保证 `previousTask`、`nextTask`、`items[].taskRef` 和 `current` 只含该契约身份；发现另一候选字段时整次响应 fail-closed，客户端不得猜测或丢弃单项后继续。队列位置、总数、剩余量和指标均由服务端提供。“本轮已跳过”只记录某个 `queueContextId` 中暂挂后的队列指针；选择 `WORK_ITEM` 时按正式 `work_item.status` 返回，选择 `DOMAIN_OPERATION` 时按最新来源与履约事实重新派生，任何实现都不持久化 `skipped`/`paused` 状态。

### 8.2 当前作业视图

```ts
type FulfillmentTaskCommon = {
  operationType: "RECEIPT" | "WAREHOUSE_SHIP" | "SUPPLIER_DIRECT" | "ELECTRONIC" | "SERVICE"
  priority: number
  dueAt?: string
  sourceVersion: string
  editVersion: number
}

type FulfillmentLeaseView = {
  claimedByLabel?: string
  claimedByCurrentUser: boolean
  expiresAt?: string
  leaseVersion?: number
  hasValidClaim: boolean
}

type WorkItemFulfillmentTaskContextCandidate = FulfillmentTaskCommon & {
  workItem: {
    workItemId: string
    workItemType: string // 必须来自后端固定注册表
    subjectVersion?: string
    subjectHash: string
  }
  operation?: never
  lease?: FulfillmentLeaseView
}

type DomainOperationFulfillmentTaskContextCandidate = FulfillmentTaskCommon & {
  workItem?: never
  operation: { operationTaskId: string }
  lease?: FulfillmentLeaseView
}

type FulfillmentOperationViewBase = {
  source: {
    purchaseOrderId?: string
    purchaseNo?: string
    purchaseRevisionId?: string
    salesOrderId: string
    salesOrderNo: string
    salesRevisionId: string
    supplierLabel?: string
    customerLabel: string
    warehouseId?: string
  }
  gate: {
    state: "SATISFIED" | "BLOCKED" | "NOT_APPLICABLE"
    message: string
    effectivePaidAmount?: string
    requiredAmount?: string
  }
  lines: FulfillmentSourceLineView[]
  draft: FulfillmentDraft
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string; lineId?: string }>
  fieldVisibility: Record<string, "full" | "masked" | "hidden">
}

type WorkItemFulfillmentOperationViewCandidate =
  FulfillmentOperationViewBase & WorkItemFulfillmentTaskContextCandidate

type DomainOperationFulfillmentOperationViewCandidate =
  FulfillmentOperationViewBase & DomainOperationFulfillmentTaskContextCandidate
```

确认 Q1 后，正式 View 只能采用其中一个 Candidate，且请求与响应均无 `mode` 自选字段。选 `WORK_ITEM` 不返回 `operationTaskId`；选 `DOMAIN_OPERATION` 不返回任何 `workItemId` / `workItemType` / `work_item.status`。查询 View 只返回领取人、到期、租约版本和 `hasValidClaim` 等安全投影，不返回 `claimToken` / `operationLeaseToken`；令牌只由选中契约的 Claim / 续租 mutation 返回并只存当前会话内存，不进入 URL、持久化 Query 缓存、日志或埋点。保存、暂挂和正式过账响应也不得回显原始令牌。

### 8.3 五类草稿与提交

```ts
type FulfillmentDraft =
  | {
      type: "RECEIPT"
      warehouseId: string
      occurredAt: string
      lines: Array<{
        purchaseRevisionLineId: string
        receivedQuantity: string
        qualifiedQuantity: string
        rejectedQuantity: string
        qualityResult: string
        evidenceAttachmentId?: string
      }>
    }
  | {
      type: "WAREHOUSE_SHIP"
      warehouseId: string
      carrier?: string
      trackingNo?: string
      shippedAt: string
      lines: Array<{
        salesOrderLineId: string
        stockReservationId: string
        quantity: string
      }>
    }
  | {
      type: "SUPPLIER_DIRECT"
      carrier?: string
      trackingNo?: string
      shippedAt: string
      lines: Array<{
        salesOrderLineId: string
        purchaseLineSalesAllocationId: string
        quantity: string
      }>
    }
  | {
      type: "ELECTRONIC"
      occurredAt: string
      recipientInput: SensitiveInputReference
      result: "SUCCESS" | "PARTIAL" | "FAILED"
      lines: Array<{
        salesOrderLineId: string
        purchaseLineSalesAllocationId: string
        quantity: string
        evidenceAttachmentId?: string
      }>
    }
  | {
      type: "SERVICE"
      startedAt: string
      endedAt: string
      serviceLocationInput: SensitiveInputReference
      result: "SUCCESS" | "PARTIAL" | "FAILED"
      completionNote: string
      lines: Array<{
        salesOrderLineId: string
        purchaseLineSalesAllocationId: string
        quantity: string
        evidenceAttachmentId?: string
      }>
    }
```

```ts
type SaveFulfillmentDraftAction = {
  action: "SAVE_DRAFT"
  expectedSourceVersion: string
  expectedEditVersion: number
  draft: FulfillmentDraft
}

type SaveFulfillmentWorkItemDraftCommandCandidate =
  WorkItemActionEnvelope<SaveFulfillmentDraftAction>

type SaveFulfillmentDomainOperationDraftCommandCandidate = {
  operationTaskId: string
  operationLeaseToken: string
  operationLeaseVersion: number
  expectedSourceVersion: string
  expectedEditVersion: number
  draft: FulfillmentDraft
  idempotencyKey: string
}

type DeferFulfillmentAction = {
  action: "DEFER_FULFILLMENT"
  queueContextId: string
  reason: { code: string; note?: string }
}

type DeferFulfillmentWorkItemCommandCandidate =
  WorkItemActionEnvelope<DeferFulfillmentAction>

type DeferFulfillmentDomainOperationCommandCandidate = {
  operationTaskId: string
  operationLeaseToken: string
  operationLeaseVersion: number
  queueContextId: string
  reason: { code: string; note?: string }
  idempotencyKey: string
}

type DeferFulfillmentWorkItemResultCandidate = {
  workItemId: string
  workItemStatus: "PENDING" | "IN_PROGRESS"
  actionRecordId: string
  subjectVersion?: string
  subjectHash: string
  queueContextId: string
  leaseReleased: boolean
  lease?: FulfillmentLeaseView
  nextTask?: WorkItemTaskRefCandidate
}

type DeferFulfillmentDomainOperationResultCandidate = {
  operationTaskId: string
  idempotencyKey: string
  queueContextId: string
  leaseReleased: true
  nextTask?: DomainOperationTaskRefCandidate
}

type FulfillmentDecision = {
  expectedSourceVersion: string
  expectedEditVersion: number
  draft: FulfillmentDraft
}

type PostFulfillmentWorkItemCommandCandidate =
  CompleteWorkItemEnvelope<FulfillmentDecision>

type PostFulfillmentDomainOperationCommandCandidate = {
  operationTaskId: string
  operationLeaseToken: string
  operationLeaseVersion: number
  decision: FulfillmentDecision
  idempotencyKey: string
}
```

以上带 `Candidate` 的类型不是可同时调用的运行时端点。Q1 确认后仅保留选中候选对应的保存、暂挂和过账命令，并移除 `Candidate` 后缀；服务端不接收 `mode` 让客户端切换另一套命令。

服务端保存后返回新 `editVersion`、规范化字段和完整校验摘要。若选择 `WORK_ITEM`，保存直接复用 W02 `WorkItemActionEnvelope`，成功只返回 `PENDING | IN_PROGRESS` 任务状态与可选安全租约投影，不完成任务、不返回原始 `claimToken`；若选择 `DOMAIN_OPERATION`，保存使用自己的 `idempotencyKey`，不读写 `work_item`、不返回 `operationLeaseToken`。

若选择 `WORK_ITEM`，暂挂复用 W02 `WorkItemActionEnvelope<DeferFulfillmentAction>`，结果中的正式任务状态只能是 `PENDING | IN_PROGRESS`。客户端以 mutation 返回的 `leaseReleased` 和安全 `lease` 投影为准：已释放就清除旧令牌；未释放时保留现有会话令牌，需要刷新令牌只能调用续租 mutation。暂挂响应本身不得返回原始 `claimToken`。若选择 `DOMAIN_OPERATION`，暂挂必须使用选中的领域命令；服务端验证领域租约、记录幂等动作、释放领域租约并只移动 `queueContextId` 游标，不写 `work_item`、`paused` 状态或独立完成状态，也不回显 `operationLeaseToken`。

正式过账按 `draft.type` 调用强类型领域服务；不得用一个通用表吞掉各类型事务约束。

若选择 `WORK_ITEM`，服务端必须校验完整 `CompleteWorkItemEnvelope`，并在同一事务原子写强类型履约事实、库存/预占等关联事实、审计以及任务完成；任一写入失败则全部回滚，前端不再调用“标记完成”。若选择 `DOMAIN_OPERATION`，服务端只校验领域投影租约并过账正式事实，随后从事实重算队列资格；它既不写 `work_item`，也不持久化一套与 `work_item` 同构的完成状态。

### 8.4 正式结果

```ts
type FulfillmentFormalResultBase = {
  idempotencyKey: string
  factType: "PURCHASE_RECEIPT" | "DELIVERY" | "ELECTRONIC_DELIVERY" | "SERVICE_FULFILLMENT"
  factId: string
  factNo: string
  formalStatus: string
  occurredAt: string
  inventoryDelta?: Array<{ warehouseId: string; skuId: string; quantity: string }>
  reservationDelta?: Array<{ reservationId: string; quantity: string; action: "CREATE" | "CONSUME" }>
  remainingByLine: Array<{ salesOrderLineId: string; quantity: string }>
  acceptanceRequired: boolean
}

type WorkItemFulfillmentFormalResultCandidate = FulfillmentFormalResultBase & {
  queueOutcome: {
    completedWorkItemId: string
    nextTask?: WorkItemTaskRefCandidate
  }
}

type DomainOperationFulfillmentFormalResultCandidate = FulfillmentFormalResultBase & {
  queueOutcome: { nextTask?: DomainOperationTaskRefCandidate }
}
```

正式实现只保留一个结果 Candidate，不返回 `mode`。同一幂等键重复请求返回同一正式事实和同一 `queueOutcome`。超时先按幂等键查询；结果未确定前不乐观改库存、不完成任务、不自动下一项。若选择 `DOMAIN_OPERATION`，结果不返回“任务已完成”，只返回按最新正式事实派生的下一队列引用。

### 8.5 前端边界

- 前端只做格式、必填、明显数量关系预检查和服务端结果展示。
- 可作业量、有效预占、库存可用量、付款门禁、采购销售分配、累计履约和状态迁移完全采用服务端结果。
- 前端不得跨类型复用字段生成错误事实，例如直发写 `stockReservationId` 或仓发写采购销售分配代替预占。
- 敏感地址/交付对象使用受控字段组件，原值不进入错误日志、埋点或普通 Query 缓存。
- TanStack Query 管理队列/事实缓存，TanStack Form 管理五种作业表单；组件内不得裸 `fetch` 管竞态。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 任务模型未确认 | `BusinessFailureState` 显示 `FULFILLMENT_TASK_MODEL_UNCONFIRMED`；不请求正常履约队列 | 只读返回来源对象、查看 Q1 决策说明 | Q1 写回权威模型/API 且部署唯一契约后重新进入 |
| 初载 | 页头、五指标、队列和当前作业 Skeleton | 应用壳导航可用 | 查询完成原位替换 |
| 切换类型/筛选刷新 | 保留旧队列并标刷新；当前脏表单不重置 | 保存/放弃当前输入 | 新快照到达后切换 |
| 队列为空 | “本筛选作业已处理完” + 五类摘要 | 切换类型、清筛选、回 W01 | 新任务到达 |
| 筛选无结果 | 展示当前筛选摘要 | 清除筛选 | 返回可处理队列 |
| 无数据范围 | 不显示全公司 0 数量/对象 | 查看当前仓库/责任范围 | 权限更新后重查 |
| 查询失败无缓存 | `BusinessFailureState` | 重试、回工作台 | 重试成功 |
| 查询失败有缓存 | 保留旧队列/事实并标陈旧 | 只读查看；过账禁用 | 取到当前事实 |
| 领取中 | 当前项只读，显示“正在取得处理权” | 查看来源对象 | 领取成功或显示占用者 |
| 他人租约 | 当前项只读，显示处理人/到期 | 去下一项 | 租约释放或完成 |
| 租约即将到期 | 连续条倒计时 warning | 续租、保存、暂挂 | 续租成功 |
| 租约丢失 | 本地非敏感输入只读保留，敏感值按策略清除 | 重新领取、复制允许字段 | 重取来源和版本 |
| 保存中 | 保存指示，正式动作禁用 | 继续编辑非冲突字段 | 返回新编辑版本 |
| 保存失败 | 输入保留，错误靠近保存区 | 重试 | 重试成功 |
| 暂挂中 | 锁定当前项和队列导航，不提前显示“本轮已跳过” | 无其它租约动作 | mutation 返回确定租约与游标结果 |
| 暂挂结果不确定 | 停留当前项，保留会话令牌且不移动游标 | 按同幂等键查询最终结果 | 明确已暂挂后再清令牌并打开下一项，或明确失败后恢复操作 |
| 校验失败 | `ValidationSummary` + 行内错误 | 修正、暂挂 | 校验通过 |
| 来源版本冲突 | 显示采购/销售/预占变化和受影响行 | 重载并重新分配 | 基于新版本保存 |
| 付款门禁阻塞 | `PrepaymentGate` 显示条件与缺口 | 去 W12、暂挂 | 有效付款核销后重查 |
| 库存/预占并发冲突 | 不更新本地数量，显示最新可用/预占 | 刷新、调整本次数量 | 重验通过 |
| 正式动作进行中 | 锁定当前任务和主动作 | 无其它正式动作 | 返回确定结果 |
| 正式动作成功 | 固定结果显示事实号、库存/预占影响、剩余量、验收下一步 | 自动/手动下一项、去 W06/W05/W08 | 用户继续 |
| 正式结果不确定 | 停留当前项，不宣称库存/履约变化 | 查询最终结果、同幂等键重试 | 确定成功或失败 |
| 字段级隐藏 | 标签保留、值掩码；关键字段无权时动作阻塞 | 查看 blocker | 权限更新后重查 |
| 权限收回 | 停止续租、清理敏感缓存、切无权限态 | 返回有权模块 | 权限恢复后重查 |

## 10. 响应式、键盘与无障碍

### 10.1 响应式

| 视口 | 布局变化 | 必须保留 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 32/68 双栏；类型/筛选和动作固定；至少 5 条队列项可见 | 五类水位、当前类型、来源销售/采购、门禁、数量、主动作 | 无 |
| 1280×800 | 28/72 双栏；队列项更紧凑 | 任务身份、超期、来源、表单与校验 | 次要来源说明折叠 |
| 1024×768 | 队列收为可开合侧栏；当前作业单列 | 队列位置、租约、类型、来源、门禁、动作 | 历史/附件列表折叠 |
| 768×1024 | 导航抽屉；队列与作业上下布局；行改卡片 | 稳定单号、数量、仓库/物流、验证与结果 | 指标 2×3；次要参考字段隐藏 |
| 375×812 | 保证任务阅读、简单单行入库/发货/结果查看 | 当前对象、数量、门禁、结果 | 多行入库、复杂预占选择、敏感电子/服务交付和冲正转桌面 |

### 10.2 键盘与焦点

- 无输入焦点时 `j/k` 或方向键移动队列；Enter 领取/打开当前作业。
- `⌘S` 保存作业草稿；校验通过时 `⌘↵` 打开正式确认，不绕过影响预览。
- Tab 顺序：类型 → 筛选 → 队列项 → 连续处理条 → 来源摘要/门禁 → 表单 → 校验 → 暂挂/主动作。
- 类型切换使用单选/多选语义和可读标签，不只靠图标或颜色。
- 新任务打开后焦点落对象标题，并通过 `aria-live=polite` 播报类型、位置和单号。
- 正式结果焦点落固定结果标题；关闭详情/确认层回触发源。
- 数量输入关联单位，敏感掩码说明可读；所有触控目标至少 44×44px。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 今日工作台 / 统一待办 | W01 / W02；仅 Q1 选择 `WORK_ITEM` 后 | `workItemId`、固定注册类型、队列上下文 | 履约事实与任务完成同事务后刷新，来源队列恢复焦点；若选择 `DOMAIN_OPERATION`，W01/W02 不提供该入口 |
| 销售单 / 客户验收 | W05 / W06 | 销售单/明细、履约事实 ID、来源任务 | 返回 W09 保留类型/筛选；验收不改原履约事实 |
| 采购单 | W08 | 采购单/版本/行、付款门禁、履约责任 | 返回 W08 履约子区并重查正式进度 |
| 库存台账 | W10 | 仓库、SKU、库存余额、预占稳定 ID | 返回 W09 重取库存/预占，不用旧数量提交 |
| 供应商往来 | W12 | 供应商、采购应付、门禁来源 | 返回重查有效付款净核销和租约 |
| 主数据 | W14 | 仓库、SKU、供应商能力/资质 | 返回作业时重验版本和权限 |
| 权限与审计 | W19 | 正式事实、请求追踪号、处理人、租约审计 | 只读返回原作业 |
| API 供应商订单 | W26 | 第二期商城消费订单/供应商子订单身份 | 仅关联钻取；不把 W26 自动履约任务纳入 W09 |

跨工作面只传稳定身份与来源上下文；数量、库存、预占、门禁、地址、状态和权限必须在目标页重查。

## 12. 验收清单

### 12.1 信息架构与效率

- [ ] 侧栏只有一个“履约作业”入口；入库、仓发、代发、电子、服务用同页分段筛选。
- [ ] 五类作业复用同一队列、租约、结果和自动下一项语言，但分别调用强类型正式事务。
- [ ] 采购/仓储从默认着陆到处理第一项不超过两次点击。
- [ ] 1440×900 下五类水位、至少 5 条队列、当前来源、关键表单和主动作同屏可见。
- [ ] 从 W05/W08/W10 进入时无需再次搜索对象，返回仍保留来源页签。

### 12.2 业务、数据与权限

- [ ] 入库合格量原子形成库存增加和销售预占，不合格量不入库存。
- [ ] 仓发必须消耗本销售明细预占；直发不得写自有库存流水。
- [ ] 电子/服务事实引用有效采购销售分配，敏感交付数据不进入日志/普通缓存。
- [ ] 入库、直发、电子、服务使用同一服务端付款门禁；仓发不由前端另造付款规则。
- [ ] W09 不把物流签收或履约结果当成 W06 客户验收通过。
- [ ] 已过账/确认事实不可覆盖，纠错使用反向/追加事实和对应原单流程。
- [ ] 普通履约只采用 §2.3 的一种任务事实源；若落 `work_item`，先在统一模型固化类型且仅使用 `workItemId`；若落领域投影，则不进入 W01/W02、不调用 `work_item` 接口且不复制任务状态机。

### 12.3 正式动作、状态与响应式

- [ ] Q1 未确认时所有正常履约入口返回 `FULFILLMENT_TASK_MODEL_UNCONFIRMED`，不开放队列、领取、保存、暂挂或过账。
- [ ] Q1 确认后生成代码只保留一个 Candidate，服务端接口不接收 `mode`，客户端依赖中不存在未选命令、结果或任务 ID。
- [ ] 若选择 `WORK_ITEM`，正式过账完整使用 W02 `CompleteWorkItemEnvelope` 且履约事实与任务完成同事务；保存/暂挂复用 W02 动作协议并保持 `PENDING | IN_PROGRESS`。
- [ ] 若选择 `DOMAIN_OPERATION`，命令使用独立领域租约与幂等键，不进入 W01/W02、不调用任何 `work_item` 接口，也不复制任务状态机。
- [ ] 查询 View 只返回领取人、到期、版本和 `hasValidClaim` 等安全租约投影；选中契约的原始令牌仅由 Claim / 续租 mutation 返回并留在会话内存。
- [ ] 选中契约的暂挂携带原因和幂等键，只按服务端结果释放租约与移动本轮游标，不写 `paused` 或第二任务状态。
- [ ] 结果不确定时不乐观修改库存、预占、履约进度或队列位置。
- [ ] 正式成功固定显示强类型事实号、库存/预占影响、剩余量和验收下一步。
- [ ] §9 全部状态完成组件测试或浏览器验证。
- [ ] 1440、1280、1024、768、375 五档视口符合 §10.1。
- [ ] 键盘可完成队列切换、保存、正式确认和结果继续；焦点恢复正确。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | 五类正常履约任务统一使用哪些固定 `work_item_type`，还是使用独立领域作业投影？ | 队列事实源、租约、W01/W02 汇聚和审计 | 架构负责人 + 采购/仓储负责人 | 确认前全部正常履约入口 blocker；确认后按 §2.3 二选一并在后端固化，生成/实现时裁剪未选 Candidate，服务端不接收 `mode` 自选且禁止混合兼容层 |
| Q2 | 一次作业允许跨多少行/多少采购单批量过账？ | 表单布局、事务锁范围和失败语义 | 采购 + 仓储 + 架构负责人 | 单次只处理一个采购/销售上下文，可多行但不跨采购单 |
| Q3 | 五类作业分别哪些场景必须上传凭证，物流单号何时必填？ | 校验、移动端、附件保留 | 采购/仓储负责人 + 内控 | 按作业类型和结果配置；电子/服务失败及直发默认需凭证 |
| Q4 | 采购超收的容差与审批路径是什么？ | 入库校验、额外待办、采购变更 | 采购 + 仓储 + 财务 | 默认不允许超收；需要时先完成明确采购变更/审批，不在 W09 硬编码容差 |
| Q5 | 入库、发货、电子或服务误录的冲正是否均需经办/复核分离？ | 历史详情动作、异常队列与权限 | 内控 + 各执行部门 | 影响库存或金额的冲正必须岗位分离，其它类型按风险规则返回 blocker |

确认后把结论写回正式章节并删除对应问题；尤其 Q1 必须先在统一数据模型/API 固化唯一模式，再实现任务代码，不能让前端运行时猜测或同时维护两套状态。

## 14. 业务依据

- `erp-phase-1.md` §4.3.1、§6.2、§7.1–§7.4：实物/服务履约方式、责任部门、先款后货与统一前后段。
- `erp-phase-1.md` §6.3–§6.5、§10：退货、拒收、库存调整、冲正和已发生事实不可回退。
- `erp-data-model.md` §6.7：采购入库、仓发/直发、电子交付、服务履约、客户验收与采购销售分配字段和约束。
- `erp-data-model.md` §7.4–§7.5、§8.1–§8.2：采购/库存固定状态、付款门禁及库存/履约事务不变量。
- `erp-ui-design.md` §4.4、§4.6、§5.3、§9–§11：统一 M3/M5 作业、正式结果、队列恢复和五档响应式。
- `erp-ui-flows.md` §2、§2.5、§7：五类作业统一入口、单次作业字段、销售单时间线和付款门禁。
- `erp-phase-2.md` §10、§15：API 供应商订单属于第二期 W26，不能与第一期 W09 手工履约混为同一事实源。
