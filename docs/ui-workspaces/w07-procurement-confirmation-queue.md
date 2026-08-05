# W07 · 采购二次确认队列

> 状态：草稿
> 页面模式：M3 连续处理队列
> 主要路由：`/procurement/confirm`
> 主要角色：采购经办；销售按对象权限查看结果
> 最后更新：2026-08-01

## 1. 定位与目标

### 1.1 用户目标

- 采购连续核对销售提交快照中的供应商、可供数量、最新成本、进项税率、预计交期和履约方式。
- 在不返回列表的情况下完成“通过”“驳回”或“暂挂当前项并处理下一项”。
- 对一个销售明细按多个供应商拆分确认，并在提交前看清数量覆盖、资质、能力、成本和交期差异。
- 需要深挖客户承诺、合同或附件时打开 W05 销售单中心，返回后仍定位到原队列、原筛选和当前任务。
- 驳回完成后清楚告知销售只能改品/改价后重提、照原条件申请低毛利上级确认或不做作废；采购能从后续新任务识别其新提交及来源链。

### 1.2 业务目标

- 把采购二次确认作为销售单生效前的正式行为闸门，而不是业务单据或普通详情页按钮。
- 通过时在同一事务校验完整确认覆盖并使实物与服务销售单生效；驳回时把销售单退回销售处理。
- 以不可变 `sales_order_submission` 作为确认对象，禁止对仍可编辑的销售草稿确认。
- 以 `work_item` 原子领取（数据库条件更新）保障一个任务同一时间只有一个有效处理人，并支持高峰期连续处理。
- 驳回的旧任务永久完成；销售重提只能产生新提交和新的 `PROCUREMENT_CONFIRMATION`，照原条件承接必须经过已注册 `LOW_MARGIN_MANAGER_CONFIRMATION` 且上级通过不能替代采购再次确认。

### 1.3 不在本工作面完成

- 不给采购二次确认生成业务单号，不注册 `business_document`，不提供纸质单据或“确认单列表”。
- 不修改销售提交快照、销售价格、客户、合同或客户承诺交期；发现问题只能驳回销售处理。
- 不创建或修改已生效采购单；首次采购建单进入 W08。
- 不处理销售单生效后发生的供应商、成本、数量或交期变化；生效后走销售变更/采购变更，不回到二次确认。
- 不在前端自行判断销售单已生效；只接受正式事务返回结果。

## 2. 用户、权限与数据范围

### 2.1 角色与动作

| 角色 | 默认入口 | 可见范围 | 主要动作 |
| --- | --- | --- | --- |
| 采购经办 | W07 | 分配给本人或本人有权领取的采购确认任务 | 领取、编辑确认数据、通过、驳回、暂挂 |
| 采购负责人 | W07 / W02 | 本采购责任域任务 | 查看；Q4 确认并配置受控转交规则前，不显示转交或代办动作 |
| 销售 | W05 / W01 | 自己负责销售单的确认结果 | 只读查看结果与驳回原因；不能进入采购成本编辑态 |
| 销售经理 | W02 / W05 | 本团队销售单及有权领取的低毛利任务 | 通过已注册 `LOW_MARGIN_MANAGER_CONFIRMATION` 处理承接决定；通过后只创建新采购确认任务，不替代采购决定 |
| 财务 | W08 / W12 | 财务数据范围 | 不参与二次确认；采购单提交后审核成本与付款条件 |
| 系统管理员 | W19 审计 | 授权审计数据 | 查操作记录，不代替采购完成确认 |

### 2.2 权限表达

| 情况 | W07 行为 |
| --- | --- |
| 无 W07 模块权限 | 侧栏、命令和快捷入口均不展示；直接访问显示无权限页 |
| 有模块权限但角色池为空 | 显示“当前责任范围没有待确认事项”，不是无权限状态 |
| 有查看权但无处理权 | 当前项只读；正式动作可见但禁用并显示 `actionBlocker` |
| 成本或供应商敏感字段无权限 | 不应进入采购确认处理角色；只读跨角色视图按字段权限掩码 |
| 销售提交已失效 | 当前任务标记失效并停止编辑；提供打开最新销售单/返回队列 |
| 页面期间权限收回 | 立即停止写动作、清除成本与联系人敏感缓存，切换无权限态 |

服务端按当前采购角色、组织、任务责任域和对象参与权筛选。前端不得查询全队列后再隐藏无权任务，也不得把 `allowedActions` 当成仅视觉提示。Q4 未确认期间，W07 的处理器注册与 `allowedActions` 必须 fail-closed 排除 `TRANSFER`；不能把 W02 的通用转交能力自动视为 W07 已授权能力。

### 2.3 领取与动作契约

1. 领取 = W02 数据库条件更新原子完成：任务从 `UNCLAIMED` 置为 `IN_PROGRESS` 并归属当前用户，更新影响行数为 0 即表示已被他人领取；不存在租约到期或续租。
2. 通过、驳回、保存和暂挂等动作提交时，服务端统一校验当前领取人和对象版本（`expectedSubjectVersion`）；W07 不定义第二套任务令牌或版本协议。
3. 版本冲突或处理权变化时保留本地输入供查看/复制，但所有写动作按服务端校验结果禁用；重新领取后必须重取当前确认数据再处理冲突。
4. “暂挂”不是采购确认的新业务状态：它通过 W02 非终结任务动作记录原因，任务回到待领取状态，再把队列指针移到下一项，但不得完成任务。

## 3. 入口、路由与任务页签

| 场景 | 入口 | URL / 页签行为 | 返回位置 |
| --- | --- | --- | --- |
| 采购默认着陆 | 登录着陆 / 侧栏 | `/procurement/confirm?scope=mine` 打开固定队列页签 | 不适用 |
| 从今日工作台处理 | W01 任务 | 以 `currentWorkItemId=workItemId` 携带 `queueContextId`，聚焦 W07 已有页签 | 完成后可继续队列或回 W01 |
| 从统一待办处理 | W02 行级“处理” | 携带相同 `currentWorkItemId` 与 `queueContextId`，恢复 W02 对应队列上下文 | 关闭 W07 后回 W02 原行 |
| 从销售单查看进度 | W05 下一步/审计 | 只读定位指定任务或确认结果，不抢占他人处理 | 返回 W05 原子区 |
| 深挖销售单 | 当前项“打开销售单” | 新开/聚焦 `sales-order:{salesOrderId}` 页签，W07 保留 | 关闭/返回后恢复当前项 |
| 刷新浏览器 | 任意队列状态 | URL 恢复筛选、排序、`currentWorkItemId`、`queueContextId` 和显式 `autoNext` 临时值；未配置 `preferenceScope` 时不从本地或服务端恢复持久偏好；刷新后按任务状态重新领取或只读展示 | W07 |

建议 URL 状态：

```text
/procurement/confirm?scope=mine&due=active&sort=due_at&currentWorkItemId=wi_123&queueContextId=qc_456&autoNext=1
```

TaskTabs 身份为 `queue:procurement-confirmation:{userId}:{scopeDigest}`。同一队列上下文只保留一个页签；从不同筛选打开时更新同一页签 URL 并保留浏览器后退历史。

临时确认层、驳回 Dialog 和未保存敏感值不进入 URL。刷新后若任务已由他人领取或已完成，页面展示确定结果并定位到队列中下一条有效任务。

## 4. 页面布局

### 4.1 1440×900 基准布局

```text
┌ PageHeader：采购二次确认 · 待我处理 28              数据更新时间 09:36 ───┐
├ SequentialProcessBar：第 3/28 · 仅我的 · 自动下一项 开 [上一项][下一项]│
│ 当前任务：已领取            [暂挂] [打开销售单中心]      │
├─────────────────────────────────────────────┬──────────────────────────┤
│ 销售提交与客户承诺 约 66%                   │ 决策摘要 约 34%（sticky）│
│ 销售单 XS… · 提交 #2 · 客户 · 期望日期      │ 数量覆盖 100%             │
│ 合同 / 付款条件 / 销售金额 / 附件（只读）   │ 供应商 2 家               │
│                                             │ 预计采购含税额             │
│ 明细 1 · 商品 / 承诺量 / 参考供给           │ 预计毛利变化（服务端）     │
│  ├ 供应商甲 60 · 成本 · 税率 · 交期 · 仓发  │ 阻塞项 / 警告              │
│  └ 供应商乙 40 · 成本 · 税率 · 交期 · 代发  │                          │
│ 明细 2 · 服务 ...                            │ 驳回原因（选择驳回后展开） │
├─────────────────────────────────────────────┴──────────────────────────┤
│ ValidationSummary       [暂挂] [驳回] [确认通过并使销售单生效]         │
│ 驳回固定结果：旧任务已完成 · 销售三路提示 · [打开 W05 驳回处理]        │
└────────────────────────────────────────────────────────────────────────┘
```

### 4.2 区域说明

| 区域 | 目的 | 主组件 | 是否固定 |
| --- | --- | --- | --- |
| 页头 | 展示队列名称、有效任务数和数据更新时间 | `PageHeader` `DataFreshness` | 顶部 |
| 连续处理条 | 展示位置、筛选和上下项 | `SequentialProcessBar` | 顶部 sticky |
| 销售提交摘要 | 锁定采购正在确认的不可变客户承诺 | `DocumentSummary` `StatusTrackSummary` | 否 |
| 确认明细编辑器 | 按销售提交明细维护一个或多个供应商确认分行 | `EditableLineItemTable` + 供应商选择器 | 主滚动区 |
| 决策摘要 | 汇总覆盖、能力资质、金额差异、交期与阻塞 | `ValidationSummary` `CostCoverageNotice` | 桌面右栏 sticky |
| 正式动作栏 | 暂挂、驳回、确认通过 | `FormalActionConfirmDialog` | 底部 sticky |
| 固定结果区 | 显示本项正式结果和下一项去向；驳回时展示销售固定三路及 W05 入口 | `FormalActionResult` + `ProcurementRejectionNextSteps` | 动作后替换决策区顶部 |

### 4.3 销售提交只读上下文

- 明确显示提交序号、提交时间、提交人和提交版本的短摘要，避免采购误以为正在确认销售当前草稿。
- 客户、合同、结算主体、付款条件、销售明细、客户交期和销售附件只读。
- 销售金额只用于理解客户承诺；采购不能在确认中修改销售价格。
- 提交已失效或出现更新提交时，整页停止正式处理并提供打开最新提交的入口。

### 4.4 确认分行编辑

- 每个需要外采的销售提交明细至少有一个确认分行，可按多个供应商拆分。
- 每条分行从 W21 当前有效供给中选择 `supplier_offering_revision_id`，完整展示供应商、确认数量、采购含税成本、进项税率、预计交期、履约方式和能力版本。
- 页面实时显示“已确认数量 / 承诺数量”，但最终覆盖校验由服务端完成。
- 供应商选择器使用 `SupplierCombobox`（不自由输入供应商名称）；只返回当前业务日期有效、能力匹配、资质有效且在用户数据范围内的供应商。
- 销售提交只携带 `sku_revision_id` 与销售成交快照（公司商品池只是公司 SKU 集合的查询称呼），不携带或展示采购成本；采购侧自行查询有权查看的有效供给。`sales_visible_price_gross` 属于 SKU 修订，成交价格以提交快照为准。

### 4.5 驳回结果与再次进入队列

- 驳回固定结果明确旧 `PROCUREMENT_CONFIRMATION` 已形成 `REJECTED` 事实、旧 `work_item` 已 `COMPLETED`，只提供打开 W05 驳回处理卡和继续队列，不提供“重新打开”“改成通过”或“复制任务”。
- W05 展示三条固定出路：改品/改价后形成新提交并重提；照原条件承接先进入 `LOW_MARGIN_MANAGER_CONFIRMATION`；不做则作废。W07 只读展示这些下一步，不代销售选择。
- 改品/改价重提形成的新任务必须指向递增提交号和新提交版本；采购重新从零确认供应商、成本、数量、交期与履约方式。
- 低毛利上级通过后形成的新任务也必须指向新的不可变提交/新版本，并展示上级确认的受控证据引用；该证据只说明公司愿意承担低毛利，不能让 W07 自动通过或沿用旧确认分行。

## 5. 展示内容与字段

### 5.1 队列与任务身份

| 区域 | 字段 | 用户文案 | 数据来源 | 口径 / 格式 | 权限规则 |
| --- | --- | --- | --- | --- | --- |
| 队列 | `position` / `total` | 第 N/M 项 | 服务端当前队列快照 | 仅代表当前筛选快照，不伪装实时全量序号 | W07 用户可见 |
| 队列 | `filterSummary` | 仅我的 · 今日到期等 | 查询参数服务端回显 | 业务文案，不显示内部 digest | 同上 |
| 任务 | `workItemId` | 任务身份 | `work_item.id` | 不直接面向用户展示原始 ID | 处理者与审计角色 |
| 任务 | `status` | 待领取 / 处理中 | `work_item.status` | 固定状态文案 | 同上 |
| 任务 | `dueAt` | 截止 / 已超期 | `work_item.due_at` | 业务时区；超期有文字 | 同上 |
| 处理 | `claimedByLabel` | 处理人 | `work_item.claimed_by` | 任务已领取时显示处理人 | 当前查看者 |
| 影响 | `impactSummary` | 业务影响 | `work_item.impact_summary` | 例如“影响客户 8 月 5 日交付” | 不含敏感技术信息 |

### 5.2 销售提交摘要

| 字段 | 用户文案 / 表现 | 数据来源 | 说明 |
| --- | --- | --- | --- |
| `salesOrderNo` | 销售单号 | `sales_order` | 可打开 W05 |
| `submissionNo` | 第 N 次提交 | `sales_order_submission.submission_no` | 确认针对该不可变提交 |
| `submittedAt` / `submittedBy` | 提交时间 / 提交人 | 销售提交 | 与当前销售草稿修改时间分开 |
| `customerSnapshot` | 客户 | 提交快照 | 不追随基础资料变化 |
| `contractSnapshot` | 合同 | 提交快照 | 提供只读钻取 |
| `paymentTermSnapshot` | 客户付款条件 | 销售提交表头 | 采购确认不修改 |
| `grossAmount` | 销售含税金额 | 销售提交行汇总 | 金额等宽右齐、明确含税 |
| `submissionVersionLabel` | 提交版本摘要 | `sales_order_submission` 提交版本 | 用于并发说明，不暴露内部版本值 |
| `submissionOrigin` | 初次提交 / 改品改价重提 / 低毛利上级通过后重提 | 提交来源链 | 后两者必须展示上一驳回确认引用；不把来源标签当本次采购结论 |
| `lowMarginApprovalEvidence` | 低毛利上级确认 | `LOW_MARGIN_MANAGER_CONFIRMATION` 正式结果引用 | 仅低毛利路径显示；证明上级已通过，但不预填或跳过本次采购确认 |
| `attachments` | 销售附件 | 提交关联文件 | 下载再校验附件权限 |

### 5.3 销售明细与采购确认分行

| 字段 | 用户文案 | 数据来源 / 提交去向 | 校验与格式 |
| --- | --- | --- | --- |
| `submissionLineId` | 销售明细 | `sales_order_submission_line` | 稳定提交明细身份 |
| `itemSnapshot` | 商品/服务与规格 | 销售提交快照 | 不用当前 SKU 名替换 |
| `committedQuantity` | 客户承诺数量 | 销售提交明细 | 基础单位，最多 6 位 |
| `requestedDeliveryDate` | 客户期望交期 | 销售提交履约字段 | 业务日期 |
| `skuRevisionId` | SKU 版本 | `sales_order_submission_line.sku_revision_id` + 成交快照 | 只读；销售可见价来源于该 SKU 修订，提交价格、品名和规格以成交快照为准；不包含供应商成本 |
| `supplierOfferingRevisionId` | 确认供应商供给 | `procurement_confirmation_line.supplier_offering_revision_id` | 从 W21 当前有效供给选择；需能力/资质有效，可多供应商拆分 |
| `supplierId` | 确认供应商 | 从供给修订固定的供应商 | 不接受名称自由输入，不允许与供给修订供应商不一致 |
| `confirmedQuantity` | 确认可供数量 | `confirmed_quantity` | 分行 >0；明细合计覆盖承诺量才可整单通过 |
| `latestCostGross` | 最新含税成本 | `latest_cost_gross` | 单价最多 4 位；不是销售价格 |
| `inputTaxRate` | 进项税率 | `input_tax_rate` | 最多 6 位；不使用销项税率替代 |
| `expectedDeliveryDate` | 预计交期 | `expected_delivery_date` | 与客户期望交期差异明确提示 |
| `fulfillmentMode` | 履约方式 | `fulfillment_mode` | 入仓 / 供应商直发 / 电子交付 / 线下服务，须在允许方式内 |
| `capabilityRevisionId` | 供应商能力版本 | `supplier_capability_revision_id` | 服务端返回有效版本，UI 展示能力摘要 |
| `qualificationStatus` | 资质状态 | 供应商资质校验投影 | 只读阻塞；失效不得通过 |

### 5.4 决策摘要与结果

| 字段 | 用户文案 | 数据来源 | 规则 |
| --- | --- | --- | --- |
| `coverageByLine` | 数量覆盖 | 服务端校验投影 | 每条需外采明细分别展示，不跨行抵消 |
| `estimatedPurchaseGross` | 预计采购含税额 | 服务端按确认分行计算 | 前端不形成正式成本事实 |
| `estimatedMargin` / `marginDelta` | 预计毛利 / 与参考差异 | 服务端受控计算 | 只作决策提示；低毛利动作由独立规则/任务决定 |
| `blockingIssues` | 不能通过的原因 | 服务端校验 | 资质、能力、数量、交期、版本等结构化原因 |
| `warnings` | 可确认但需注意 | 服务端校验 | 警告不等同于阻塞 |
| `rejectReasonCode` | 驳回原因 | `procurement_confirmation.reject_reason_code` | 无法履约、成本上涨、交期不满足、资质失效等固定码 |
| `comment` | 补充说明 | `procurement_confirmation.comment` | 驳回必填业务说明；不记录技术堆栈 |
| `handledBy` / `handledAt` | 处理人 / 时间 | 正式确认记录 | 通过或驳回后显示 |
| `nextSalesResolutions` | 销售后续三条出路 | 驳回正式结果 | 固定为改品/改价重提、照原条件低毛利承接、不做作废；仅作说明和 W05 导航 |

## 6. 搜索、筛选、排序与默认视图

### 6.1 默认视图

- 默认 `scope=mine`，先展示已分配本人且有效的待处理任务。
- 默认按已超期优先 → `priority` 降序 → `due_at` 升序 → `created_at` 升序，由服务端排序。
- 未提供显式 `autoNext` 时，本次会话默认自动下一项开启；它只控制动作成功后的队列导航，不改变业务事务，也不表示已形成持久偏好。
- 不提供跨对象自由全文搜索作为处理主路径；精确销售单号可用队列搜索并由服务端定位。

### 6.2 筛选契约

| 能力 | 默认值 | URL 状态 | 行为 |
| --- | --- | --- | --- |
| 责任范围 | `mine` | `scope=mine|role_pool` | 角色池任务需先领取；是否开放由权限决定 |
| 时限 | 有效全部 | `due=active|today|overdue` | 改变队列快照并重新计算位置 |
| 履约方式 | 全部 | `mode=warehouse|direct|electronic|service` | 按至少一条确认明细的允许方式过滤 |
| 供应商 | 全部 | `supplierId` | 仅对当前销售明细存在有效 W21 供给的供应商筛选，不泄露无权供应商 |
| 销售单号 | 空 | `orderNo` | 精确或前缀搜索，回车定位结果 |
| 排序 | 截止优先 | `sort=due_at|submitted_at|priority` | 服务端排序，自动下一项沿相同快照 |
| 自动下一项 | 本次会话默认开 | `autoNext=1|0` | 显式 URL 为本次会话临时值并优先；`preferenceScope` 未配置时不得写本地存储或服务端用户偏好，配置后才按 `DEVICE` 或 `USER` 范围持久化 |

筛选变化前如当前项有未保存输入，必须让用户保存、放弃或取消切换；不得因为改筛选静默丢失确认分行。

## 7. 操作契约

| 操作 | 入口 | 权限 / 前置条件 | 确认 | 成功结果 | 失败恢复 |
| --- | --- | --- | --- | --- | --- |
| 领取任务 | 打开当前项 / 角色池“领取” | `CLAIM` 可用；任务待领取 | 无 | W02 条件更新原子领取，返回当前版本 | 被他人领取则转只读并可去下一项 |
| 保存确认数据 | 自动保存 / `⌘S` | 当前领取人为本人，确认仍待处理，`editVersion` 匹配 | 无 | 返回新编辑版本与校验摘要 | 输入保留；版本冲突显示差异 |
| 新增供应商分行 | 明细行“拆分供应商” | 供应商能力/资质有效且明细可拆 | 无 | 在当前确认编辑数据新增分行 | 校验失败保留原分行 |
| 暂挂 | 连续处理条 / 底栏 | `DEFER` 可用；当前任务仍待处理 | 有未保存输入时确认保存或放弃 | 使用 W02 非终结动作记录原因，任务回到待领取状态，固定结果后打开下一项 | 失败停留当前项；不得假装已暂挂或已完成 |
| 驳回 | 底栏 | `REJECT` 可用；当前领取人为本人、提交版本未变；结构化原因和说明完整 | 确认销售单将退回销售处理，并展示销售后续三条固定出路 | 原子写驳回确认、工作流审计并完成当前任务；不创建后继任务；固定结果提供 W05 驳回处理入口后可继续下一项 | 失败保留输入；结果不确定停留并查询；不得复用当前任务重提 |
| 确认通过 | 底栏主动作 | `APPROVE` 可用；全部外采明细覆盖、能力资质有效、当前领取人和对象版本有效 | 展示供应商拆分、成本、交期、履约方式及“销售单将生效” | 原子形成确认记录、销售正式版本/应收和采购创建依据并完成当前任务；固定结果后仅在当前 `autoNext` 生效时自动下一项，否则停留结果页 | 失败不本地生效；不确定时查询最终结果 |
| 打开销售单中心 | 页头/当前项 | 有 W05 对象权限 | 无 | 新开或聚焦 W05，W07 处理权保持不变 | 返回时重验提交版本 |
| 上一项 / 下一项 | 连续处理条 | 无未处理脏输入；目标仍在快照 | 脏输入时确认 | 打开目标并领取/只读展示 | 目标失效则说明并选择最近有效项 |

### 7.1 通过事务边界

确认通过不是“保存一张确认单”，而是一个跨聚合正式事务。服务端必须：

1. 锁定 `sales_order_submission`、当前 `procurement_confirmation` 与 `work_item`；
2. 校验提交仍有效、当前领取人一致、操作者有当前权限；
3. 校验全部需要外采的提交明细已被一个或多个确认分行完整覆盖，供应商能力和资质仍有效；
4. 写采购确认通过事实、确认分行、处理人和时间；
5. 把提交内容原样形成销售正式版本，更新销售状态并形成应收；
6. 完成当前正式待办，形成不可变、可唯一消费的采购创建依据；
7. 写审计；
8. 返回稳定销售单身份、版本、正式结果与采购创建依据身份。

任何一步失败均不得留下“确认已通过但销售未生效”或“销售已生效但任务仍可重复处理”的半成品。

当前权威 `work_item_type` 注册表没有“采购建单任务”。因此 W07 通过不得创建或等待该类任务，也不能把其缺失作为阻断销售通过的 blocker；W08 只从 W07 固定结果或 W05 销售上下文读取采购创建依据并显式建草稿。未来如需任务化，必须先在权威注册表增加固定类型并补齐处理器契约。

### 7.2 驳回边界

- 驳回只适用于销售单生效前的当前提交。
- 驳回记录结构化原因和补充说明，形成该次采购确认的正式 `REJECTED` 结论并完成当前任务，销售单回到销售可处理草稿；不创建变更单。
- 驳回事务不创建任何后继 `work_item`。旧 `PROCUREMENT_CONFIRMATION` 任务保持 `COMPLETED`，其提交、确认分行、结论和审计永久留痕且不得复用。
- “改品或改价”出路：销售与客户重新确认后，由 W05 冻结递增 `submission_no` 的新提交和新提交版本，原子创建新的 `PROCUREMENT_CONFIRMATION`。
- “照原条件承接”出路：W05 先冻结新提交、创建已注册 `LOW_MARGIN_MANAGER_CONFIRMATION`；只有上级通过事务才能完成该任务并创建新的 `PROCUREMENT_CONFIRMATION`，且采购仍须重新确认。
- “不做”出路：由 W05 原子作废生效前销售单，不创建任务。
- 无论改品/改价还是低毛利上级通过，新 W07 任务都只能关联新 `submissionId` / 新提交版本。不得改绑旧任务、复制旧确认分行、把上级同意当采购通过或绕过采购生效闸门。
- 驳回结果不得表述成“采购单已驳回”，因为此时尚未形成采购单。

## 8. 数据契约

本节定义 UI 所需语义，不固定具体 HTTP 路径。

### 8.1 队列查询

```ts
type ProcurementConfirmationQueueQuery = {
  scope: "mine" | "role_pool"
  due?: "active" | "today" | "overdue"
  fulfillmentMode?: "WAREHOUSE" | "SUPPLIER_DIRECT" | "ELECTRONIC" | "SERVICE"
  supplierId?: string
  orderNo?: string
  sort: "due_at" | "submitted_at" | "priority"
  currentWorkItemId?: string
  queueContextId?: string
  pageSize: number
}
```

```ts
type ProcurementConfirmationQueueView = {
  preferences: {
    autoNextDefault: boolean
    preferenceScope?: "DEVICE" | "USER"
  }
  context: {
    queueContextId: string
    position: number
    total: number
    currentWorkItemId?: string
    previousWorkItemId?: string
    nextWorkItemId?: string
    filterSummary: string
    queueContextUpdatedAt: string
  }
  current?: ProcurementConfirmationTaskView
  emptyReason?: "NO_TASKS" | "FILTER_NO_RESULT" | "NO_DATA_SCOPE"
}

type ProcurementConfirmationTaskView = {
  workItem: {
    workItemId: string
    workItemType: "PROCUREMENT_CONFIRMATION"
    status: WorkItemStatus
    priority: number
    dueAt?: string
    impactSummary: string
    subjectVersion: string
  }
  claimedByLabel?: string
  salesSubmission: {
    salesOrderId: string
    salesOrderNo: string
    submissionId: string
    submissionNo: number
    submittedAt: string
    submittedByLabel: string
    customerSnapshot: string
    contractSnapshot?: string
    paymentTermLabel: string
    grossAmount: string
    lines: ProcurementSubmissionLineView[]
    resubmissionContext?: {
      origin: "CHANGED_TERMS_AFTER_REJECTION" | "LOW_MARGIN_MANAGER_APPROVED"
      previousRejectedConfirmationId: string
      previousRejectedSubmissionId: string
      lowMarginManagerConfirmationEvidenceReference?: string
    }
  }
  confirmation: {
    confirmationId: string
    status: "PENDING"
    editVersion: number
    lines: ProcurementConfirmationLineDraft[]
  }
  decisionSummary: {
    coverageByLine: Array<{ submissionLineId: string; confirmed: string; required: string; complete: boolean }>
    estimatedPurchaseGross: string
    estimatedMargin?: string
    marginDelta?: string
    blockingIssues: Array<{ code: string; message: string; lineId?: string }>
    warnings: Array<{ code: string; message: string; lineId?: string }>
  }
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
}
```

`WorkItemStatus` 直接复用 W02 的固定状态联合类型。`queueContextId` 是跨 W01/W02/W07 的唯一队列上下文；任务与邻接项身份只使用 `workItemId`、`currentWorkItemId`、`previousWorkItemId` 和 `nextWorkItemId`。

查询 View 只返回业务可读的处理信息（如 `claimedByLabel`）和对象版本；领取权是否有效由服务端在每次动作提交时按条件更新重新校验，不依赖查询响应中的任何令牌。

Query Key 至少包含用户、当前角色、权限/数据范围版本、全部筛选、排序、`queueContextId` 和 `currentWorkItemId`。列表总数和位置由服务端队列上下文提供；客户端当前缓存任务数不能冒充总数。

`preferenceScope` 缺失表示持久化范围尚未决定：切换“自动下一项”只更新显式 URL / 当前会话状态，不能写 `localStorage`、IndexedDB 或服务端用户偏好。服务端明确返回 `DEVICE` 后才允许写设备本地偏好，返回 `USER` 后才允许通过用户偏好接口跨设备保存；无论哪种范围，显式 URL 仍只作当前会话覆盖。

### 8.2 非终结任务动作

```ts
type SaveProcurementConfirmationAction = {
  confirmationId: string
  submissionId: string
  expectedEditVersion: number
  lines: Array<{
    submissionLineId: string
    supplierId: string
    confirmedQuantity: string
    latestCostGross: string
    inputTaxRate: string
    expectedDeliveryDate: string
    fulfillmentMode: "WAREHOUSE" | "SUPPLIER_DIRECT" | "ELECTRONIC" | "SERVICE"
    supplierCapabilityRevisionId: string
  }>
}

type SaveProcurementConfirmationCommand =
  WorkItemActionCommand<SaveProcurementConfirmationAction>

type DeferProcurementConfirmationAction = {
  type: "DEFER"
  queueContextId: string
  reasonCode?: string
  comment?: string
}

type DeferProcurementConfirmationCommand =
  WorkItemActionCommand<DeferProcurementConfirmationAction>

type DeferProcurementConfirmationResult =
  WorkItemActionResult<{
    queueContextId: string
    nextWorkItemId?: string
  }>
```

保存的是待处理确认的工作数据，不形成业务单据、不使销售生效、不完成或转交待办。`SaveProcurementConfirmationCommand` 直接复用 W02 `WorkItemActionCommand`（`kind="WORK_ITEM_ACTION"`），共享命令统一提供 `workItemId`、`expectedSubjectVersion` 和 `action`；W07 不复制这些任务字段，也不另建私有领取或版本协议。`action.expectedEditVersion` 保护确认工作数据版本。成功结果的任务状态仍为 `IN_PROGRESS`，并返回新 `editVersion`、规范化数值和完整校验摘要。

`DeferProcurementConfirmationCommand` 追加暂挂动作证据并携带当前 `queueContextId`；返回 `DeferProcurementConfirmationResult`。成功结果的任务回到待领取（`UNCLAIMED`）状态，前端只在结果的 `queueContextId` 与当前上下文一致时，才把 `currentWorkItemId` 切到服务端返回的 `nextWorkItemId`。

### 8.3 正式决策

```ts
type ProcurementConfirmationDecision =
  | {
      reviewResult: "APPROVED"
      confirmationId: string
      submissionId: string
      expectedConfirmationEditVersion: number
    }
  | {
      reviewResult: "REJECTED"
      confirmationId: string
      submissionId: string
      expectedConfirmationEditVersion: number
      rejectReasonCode: "UNFULFILLABLE" | "COST_INCREASE" | "DELIVERY_UNMET" | "QUALIFICATION_INVALID"
      comment: string
    }

type CompleteProcurementConfirmationCommand =
  WorkItemActionCommand<ProcurementConfirmationDecision>

type ProcurementConfirmationBusinessResult =
  | {
      outcome: "APPROVED_AND_SALES_EFFECTIVE"
      procurementConfirmationId: string
      salesOrderId: string
      submissionId: string
      salesOrderRevisionId: string
      receivableAccountId: string
      procurementCreationBasisId: string
    }
  | {
      outcome: "REJECTED_TO_SALES"
      procurementConfirmationId: string
      salesOrderId: string
      rejectedSubmissionId: string
      workflowActionId: string
      nextSalesResolutions: [
        "RESUBMIT_CHANGED_TERMS",
        "REQUEST_LOW_MARGIN_ACCEPTANCE",
        "VOID_AFTER_REJECTION",
      ]
      successorWorkItemId?: never
    }

type CompleteProcurementConfirmationResult =
  WorkItemActionResult<ProcurementConfirmationBusinessResult>
```

`CompleteProcurementConfirmationCommand` 直接复用 W02 `WorkItemActionCommand`，不得在 W07 重定义同名或等价私有命令。请求携带 `workItemId`、`expectedSubjectVersion` 和 `decision`；其中 `expectedSubjectVersion` 对应不可变销售提交版本，`expectedConfirmationEditVersion` 保护采购确认工作数据版本。

通过或驳回时，服务端在同一事务重读提交与确认版本，写该次采购确认的正式 `APPROVED` / `REJECTED` 事实及销售生效/退回结果、追加 `workflow_action`，并把当前 `work_item` 置为 `COMPLETED`。驳回结果以 `successorWorkItemId?: never` 明确禁止本事务创建后继任务，只返回三条固定销售出路。销售在 W05 完成合法出路后，才可能为新的不可变提交创建全新的采购确认任务。任一部分失败均整体回滚；前端不得在业务请求成功后再补发“完成任务”。

新 W07 任务的 `resubmissionContext` 由服务端从提交谱系生成。`CHANGED_TERMS_AFTER_REJECTION` 必须关联改品/改价后的新提交；`LOW_MARGIN_MANAGER_APPROVED` 必须同时关联已完成上级确认的受控证据。两种来源都使用本任务的新 `submissionId`，并在提交时重新校验；旧驳回任务或上级任务均不能作为本任务的完成依据。

网络超时或断网后先查询最终结果，未确认前不重复提交；同一正式事务由服务端唯一约束兜底，重复请求只返回同一结果：

- 已成功：展示同一销售单版本和处理结果，再按原队列快照寻找下一项；
- 明确失败：保留当前输入，确认未生效后重试；
- 仍未知：停在当前项，继续受控查询或转人工支持，不在本工作面临时创建新的任务类型，也不自动下一项。

服务端必须对同一不可变提交的有效 `PROCUREMENT_CONFIRMATION` 做唯一约束。重复查询或重放旧驳回只能返回同一 `REJECTED` 事实和三条销售出路，不能顺带创建后继任务；只有 W05 的改品/改价重提事务或 `LOW_MARGIN_MANAGER_CONFIRMATION` 上级通过事务，才能针对各自新提交原子创建一个新采购确认任务。

### 8.4 前端边界

- 前端只做输入格式、覆盖提示和服务端结果展示；供应商有效性、资质、数量守恒、正式成本/毛利、销售生效均由服务端决定。
- 前端不得把 SKU 修订上的销售可见价或销售成交快照当作采购成本；从 W21 供给修订带入的成本必须标明版本和有效期，并要求采购确认。
- `allowedActions` 和 blockers 必须从服务端当前响应取得，不从历史页面缓存推断。
- 任务完成后不由客户端删除；服务端正式事务更新后重新查询队列。
- TanStack Query 管理队列和对象缓存，TanStack Form 管理确认分行与驳回表单；组件内不得裸 `fetch` 管理竞态。

## 9. 页面状态矩阵

| 状态 | 页面表现 | 可执行动作 | 恢复方式 |
| --- | --- | --- | --- |
| 初载 | 页头、连续条、提交摘要和双栏 Skeleton | 应用壳导航可用 | 查询完成原位替换 |
| 领取中 | 当前项只读，显示“正在取得处理权” | 打开销售单 | 领取成功进入编辑；失败说明占用者 |
| 刷新 | 保留当前数据，显示数据更新时间；不重置表单 | 查看；正式动作等待重验 | 成功更新未编辑字段 |
| 队列为空 | 明确“本筛选项已处理完” | 清除筛选、返回 W01/W02 | 新任务到达或切换筛选 |
| 筛选无结果 | 展示筛选摘要 | 清除筛选 | 返回有效队列 |
| 无数据范围 | 不展示 0 数量和对象信息 | 查看当前角色/申请范围 | 权限更新后重查 |
| 查询失败且无缓存 | `BusinessFailureState` | 重试、返回工作台 | 重试成功 |
| 查询失败但有缓存 | 保留旧内容并标陈旧 | 只读查看；正式动作禁用 | 取到当前事实后恢复 |
| 保存中 | 草稿保存指示，正式动作暂禁用 | 继续编辑其它字段 | 返回新编辑版本 |
| 保存失败 | 输入保留，错误靠近动作区 | 重试、复制输入 | 重试成功 |
| 校验失败 | `ValidationSummary` + 行内错误 | 修正、暂挂 | 校验通过 |
| 提交已失效 | 显示旧/新提交摘要，全部写动作禁用 | 打开最新销售单、去下一项 | 服务端关闭旧任务并创建新任务 |
| 版本冲突 | 显示当前服务端确认分行与本地差异 | 重载、逐行重新应用 | 基于新版本保存 |
| 任务已被他人领取 | 当前项只读，显示处理人 | 去下一项、打开销售单 | 暂挂/转交或任务完成后重查 |
| 处理权丢失（版本冲突或已转交） | 本地输入只读保留，正式动作禁用 | 重新领取、复制输入 | 重取数据并处理冲突 |
| 正式动作进行中 | 锁定当前项与动作，禁止重复点击 | 无其它正式动作 | 返回确定结果 |
| 正式动作成功 | 固定结果展示销售单状态/版本、确认结果和采购创建依据 | 自动/手动下一项、打开销售单/W08 | 用户明确继续 |
| 驳回正式成功 | 固定结果展示旧提交、`REJECTED`、已完成旧任务和销售三条出路；不显示后继任务 | 打开 W05 驳回处理、继续队列 | 销售选择合法出路后，可能以新任务再次进入队列 |
| 改品/改价后新任务 | 来源 Banner 展示新提交/新版本及上一驳回引用；不加载旧确认分行 | 领取、重新填写并独立通过/驳回 | 按新任务正式事务处理 |
| 低毛利上级通过后新任务 | 来源 Banner 展示新提交/新版本和上级确认受控证据，明确“仍待采购确认” | 领取、重新填写并独立通过/驳回 | 上级证据不预选供应商、不自动通过 |
| 旧驳回任务被再次打开 | 只读固定结果，写动作隐藏 | 打开 W05、返回队列 | 不恢复、转交或改绑旧任务 |
| 正式结果不确定 | 不显示成功，不跳下一项 | 查询最终结果 | 确定成功或失败 |
| 字段级隐藏 | 只读跨角色视图掩码；采购处理角色缺关键字段权限时不可处理 | 查看 blocker | 权限修正后重查 |
| 权限收回 | 停止写动作，清除敏感缓存，显示无权限 | 返回有权模块 | 权限恢复后重查 |

## 10. 响应式、键盘与无障碍

### 10.1 响应式

| 视口 | 布局变化 | 必须保留 | 允许降级 |
| --- | --- | --- | --- |
| 1440×900 | 66/34 双栏；连续条和底栏固定；至少两条确认分行同屏 | 队列位置、销售单/提交身份、数量覆盖、主动作 | 无 |
| 1280×800 | 62/38 双栏；参考信息折叠 | 提交序号、供应商、数量、成本、交期、履约方式 | 附件与审计进入折叠区 |
| 1024×768 | 单列：销售摘要 → 明细 → 决策摘要；底栏固定 | 当前项、阻塞、通过/驳回/暂挂 | 右栏改可展开摘要 |
| 768×1024 | 导航抽屉；分行改卡片；连续条两行 | 队列位置、对象、覆盖、正式动作 | 供应商资质详情用 Sheet；次要参考值折叠 |
| 375×812 | 只保证只读核对、简单单供应商确认和驳回；多供应商拆分提示转桌面 | 对象身份、提交版本、数量/交期、结果查看 | 多分行成本编辑、复杂拆分不作为手机主路径 |

### 10.2 键盘与焦点

- 队列内 `j/k` 或 `↑/↓` 在无输入焦点时切换项；有脏输入先触发保存/放弃确认。
- `⌘S` 保存当前确认数据；校验通过时 `⌘↵` 打开通过确认层，不绕过确认。
- Tab 顺序：队列筛选 → 连续处理条 → 销售摘要 → 确认分行 → 决策摘要 → 暂挂/驳回/通过。
- 新增供应商分行后焦点落到新分行供应商选择器；删除分行后回到相邻分行标题。
- 驳回 Dialog 关闭后焦点回“驳回”；通过成功后焦点到固定结果标题。
- 自动下一项时新对象标题获得焦点，`aria-live=polite` 播报“第 N/M 项，销售单号”。
- 处理权告警、校验、成本变化和状态均使用文字，不仅依赖颜色；触控目标至少 44×44px。

## 11. 与其他工作面的关系

| 来源 / 去向 | Wxx | 携带上下文 | 返回规则 |
| --- | --- | --- | --- |
| 今日工作台 | W01 | `currentWorkItemId=workItemId`、`queueContextId` | 完成后 W01 正式待办立即刷新，指标统计按数据更新时间刷新 |
| 统一待办队列 | W02 | `currentWorkItemId`、`queueContextId`；低毛利任务使用独立 `LOW_MARGIN_MANAGER_CONFIRMATION` handler | 返回恢复 W02 原筛选和行焦点；上级通过后由服务端创建新 W07 任务 |
| 销售单 | W05 | 销售单 ID、提交 ID、来源 `workItemId`；驳回后只传稳定确认/提交身份 | W07 页签和队列位置保持；销售在 W05 选择固定三路，新任务形成后以新提交重新进入 W07 |
| 采购单 | W08 | 通过结果中的销售单版本、确认分行与采购创建依据稳定 ID；不携采购建单 `workItemId` | 建单完成可回 W07 固定结果或继续队列 |
| 履约处理 | W09 | 确认履约方式仅作为采购建单依据 | W07 不直接创建履约事实 |
| 公司商品池（公司 SKU 集合）/供应商 | W14 | 供应商、能力/资质版本、SKU 修订 | 只读钻取；修改基础资料后返回必须重验 |
| 权限与审计 | W19 | 工作流动作、任务、处理人和请求追踪号 | 只读返回原确认结果 |

跨工作面只传稳定身份与来源上下文；提交字段、成本、资质、权限和状态在目标工作面重新查询。W07 驳回结果不跨页传“允许重提”布尔值，也不把旧 `workItemId` 当后继任务；W05 与 W02 的正式事务返回新 `submissionId` 和新任务身份后，W07 才能查询并处理。

## 12. 验收清单

### 12.1 体验与布局

- [x] 采购默认着陆后无需返回列表，可连续处理通过、驳回和暂挂。
- [x] 二次确认页面和文案始终表达为“行为/任务”，不生成确认单号或纸质单据。
- [x] 1440×900 下队列位置、不可变销售提交、至少两条确认分行、覆盖摘要和主动作同屏可见。
- [x] 多供应商拆分能逐明细说明确认数量，不能用总量掩盖单行缺口。
- [x] 打开 W05 深挖后返回仍恢复队列位置、筛选、当前项和显式 URL / 当前会话的自动下一项临时值；`preferenceScope` 未配置时不产生本地或服务端持久偏好。

### 12.2 数据、权限与正式动作

- [x] 全部确认引用具体 `submissionId` 和提交版本，不读取可变销售草稿。
- [ ] 供应商、能力、资质、数量覆盖、成本、交期与履约方式均由服务端提交时重验。
- [ ] 通过事务原子完成确认、销售生效、正式版本/应收、当前任务完成和采购创建依据；未注册“采购建单任务”不成为销售通过 blocker。
- [x] 驳回形成本次采购确认的正式 `REJECTED` 结论并完成当前任务，不创建采购单、变更单或任何后继任务；固定结果完整展示销售三条出路。
- [ ] 改品/改价重提只以新 `submissionId`、递增提交号、新提交版本和新 `PROCUREMENT_CONFIRMATION` 返回 W07；旧确认分行与旧任务不复用。
- [ ] 照原条件承接先进入已注册 `LOW_MARGIN_MANAGER_CONFIRMATION`；上级通过前 W07 不产生新任务，通过后仍创建全新采购确认且不能自动通过。
- [ ] 不做路径在 W05 作废销售单且不创建任务；W07 只读保留旧驳回结果和审计链。
- [x] 查询 View 不返回任何会话令牌；领取权只在服务端动作时按条件更新校验。
- [ ] 领取、保存、暂挂和正式决策均使用 W02 统一动作命令；暂挂置回待领取，正式决策由处理器 `completionAction` 约束，不存在第二次“标记完成”调用。
- [x] 重复点击和超时重试不重复推进销售状态或生成多个采购创建依据。
- [ ] 驳回结果重放不创建后继任务；新提交与固定任务类型存在服务端唯一约束，低毛利上级任务和采购任务的唯一性彼此独立。

### 12.3 状态与无障碍

- [ ] §9 全部状态完成组件测试或浏览器验证。
- [ ] 1440、1280、1024、768、375 五档视口符合 §10.1。
- [x] 正式动作结果不确定时停留当前项，不自动下一项。
- [x] 键盘可完成分行编辑、保存、驳回/通过和连续切换。
- [x] 读屏可识别队列位置、处理权变化、行覆盖、错误与固定结果。

## 13. 待确认事项

| ID | 问题 | 影响 | 建议决策人 | 当前建议 |
| --- | --- | --- | --- | --- |
| Q1 | 暂挂后任务立即回到待领取池，是否需要在队列中为原处理人短暂优先展示？ | 队列公平性、并发与恢复 | 采购负责人 + 产品 | 暂挂即回到待领取，原处理人可再次领取，不做独占保留 |
| Q2 | 自动下一项偏好跨设备同步，还是只存当前设备？ | URL、用户偏好接口 | 产品 + 安全负责人 | 未确认前只使用显式 URL 临时值 / 当前会话，不写本地或服务端持久偏好；确认后由服务端配置 `preferenceScope=DEVICE|USER`，显式 URL 仍临时覆盖 |
| Q3 | 当前供给成本相对上一采购确认成本偏离到何种程度时，必须要求采购填写说明并形成“成本上涨”驳回提示？ | W07 校验摘要、采购说明和驳回引导 | 销售/采购负责人 + 财务 | 阈值由服务端业务规则配置，W07 不硬编码百分比；销售端只接收风险结论，不接收原始成本。只有销售在 W05 明确选择“照原条件承接”时才创建低毛利任务 |
| Q4 | 角色池任务是否允许采购负责人直接转交给指定经办？ | W07/W02 操作与任务转交审计 | 采购负责人 + 权限负责人 | 允许受控转交并使用正式后继任务链，不直接覆盖责任人 |

确认后应把结论写回相应章节并删除问题，不能让建议与正式契约长期并存。

## 14. 业务依据

- `erp-phase-1.md` §4.4、§4.6、§7.1–§7.4：二次确认是销售生效闸门和处理行为，不是业务单据；确认字段与驳回路径。
- `erp-phase-1.md` §4.5：供应商能力与资质控制新确认和采购可用性。
- `erp-data-model.md` §6.1 `work_item`：固定任务类型、领取、转交、完成与关闭约束。
- `erp-data-model.md` §6.5：不可变销售提交、`procurement_confirmation` 及多供应商确认分行规则。
- `erp-data-model.md` §7.1、§8.1：销售固定状态机与采购确认通过事务不变量。
- `erp-ui-design.md` §3.4–§3.5、§4.4、§5.1–§5.2、§7.2、§9–§11：连续处理队列、TaskTabs、五档响应式与正式结果规则。
- `erp-ui-flows.md` §2、§4：确认队列的只读/可写/决策信息架构及通过后的 W08 衔接。
