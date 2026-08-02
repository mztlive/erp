# ERP 前端文案审查报告：采购 / 供应商域（2026-08-02）

范围：features/ 下 purchase-orders、supplier-orders、supplier-settlements、supplier-payables、supplier-api-connections、procurement-confirmation、external-product-supply 共 13 个 .tsx 文件。
类别：A=冗余、B=车轱辘话、C=技术名词/内部术语。

## features/external-product-supply/external-product-supply-page.tsx

- [C] 1233-1234: 「先进入 `supplier_external_product` 不可变修订区；未经映射与审核不修改 ERP SKU 或商城商品」→ 数据库表名直接暴露 → 「先写入不可变的外部商品修订记录，未经映射与审核不修改 ERP SKU 或商城商品」
- [C] 1219: caption「白名单业务字段对比；成本字段按权限掩码」→ 「白名单」为内部实现词 → 「业务字段对比；成本字段按权限隐藏」
- [C] 1353-1356: 「供给 MOQ 是供应商约束，不等于商城最小购买量（`false` 自动复制）。商城销售价自动更新：`true`。」→ 布尔原值直接上屏 → 「供货 MOQ 是供应商约束，不会自动复制为商城最小购买量；商城销售价将自动更新」
- [C] 1386: 「`{p.publicationId}` · `{p.reason}` · outbox `{p.outboxId}` · `{p.status}`」→ outbox 为内部基础设施术语 → 删去 outbox 字段或改「同步记录」
- [C] 1511: `title="WORK_ITEM_TYPE_UNREGISTERED：无写入入口"`（tooltip）→ 错误码进用户提示 → 「确认入口尚未开放，请先完成映射与供给修订登记」
- [C] 1603: `title="WORK_ITEM_TYPE_UNREGISTERED"` → 同上 → 「尚未开放，需先登记供给修订类型」
- [C] 1617-1618: 「仅证据准备；选定被 `RECOVERY_RESPONSIBILITY_UNCONFIRMED` 阻断」→ 错误码直接上屏 → 「当前仅可准备候选证据，暂不能选定替代供给」
- [C] 1759: 「使用 WorkItemActionEnvelope；成功后任务仍为 PENDING/IN_PROGRESS，不自动下一项。」→ 全句技术黑话 → 「暂挂后任务保留在待处理队列，不会自动进入下一项」
- [C] 1889/1894: 「完成 BUSINESS_EXCEPTION 任务」→ 内部任务类型码 → 「结束当前异常处理任务」
- [C] 1898: 「任务终态不可撤销（演示会话内）」→ 「终态」为内部状态词 → 「处理结果不可撤销（演示会话内）」
- [A] 1427: 「同一外部商品同时点仅一个有效映射；一 SKU 可有多外部供给」→ 「同时点」生造词 → 「同一外部商品同一时间仅一个有效映射；一个 SKU 可有多个外部供给」

## features/external-product-supply/external-product-center-page.tsx

- [C] 254: AlertTitle 直接显示错误码「WORK_ITEM_TYPE_UNREGISTERED」→ 错误码当标题，最严重的一类 → 「任务类型尚未登记」
- [C] 118: 「稳定身份 `{externalProductId}` 不在当前目录观察范围。」→ 「稳定身份」「观察范围」均为内部概念 → 「外部商品 `{externalProductId}` 不在当前目录范围内」
- [C] 180: 「上下文 `{queueContextId}`」→ 内部参数名上屏 → 「队列 `{queueContextId}`」
- [C] 262: 「价格/税率/费用字段已按权限掩码」→ 「掩码」技术词 → 「价格/税率/费用字段已按权限隐藏」
- [C] 467: 「数据版本 `{fingerprint}`（无原始报文/密钥）」→ 「原始报文」为内部术语 → 「数据版本 `{fingerprint}`（不含源数据原文）」
- [C] 482-484: 「聚合任务 `{workItemId}` · `{workItemType}` · handler `{handlerKey}`」→ handler 字段名暴露 → 删除「handler」部分
- [B] 339: 「同一时点仅一个有效映射；历史不原位覆盖」→ 「不原位覆盖」生造 → 「同一时间仅一个有效映射；历史记录不会被覆盖」
- [B] 368: 「不可变修订时间线；供货价变化不覆盖旧版、不自动改商城销售价」→ 与 supply-page 1318 行几乎逐字重复 → 两页统一口径后保留一处

## features/purchase-orders/purchase-orders-list-page.tsx

- [C] 884: 「只消费采购二次确认/销售单的采购创建依据，不要求未注册的采购建单任务。」→ 「消费」「未注册的建单任务」内部概念 → 「仅使用采购二次确认产生的创建依据，无需额外建单任务」
- [C] 908: 「`{basisFromUrl}` · 来自采购二次确认固定结果（无建单任务）」→ 「无建单任务」内部 → 「来自采购二次确认的固定结果」
- [C] 278: 「已消费创建依据 `{selectedBasisId}`，未创建采购建单任务。」→ 同上 → 「已使用创建依据 `{selectedBasisId}`」
- [C] 684: 「1440×900 紧凑密度；采购单号与行级操作列固定。键盘 j/k 移动…」→ 「1440×900 紧凑密度」为内部设计信息 → 删除该短语或改「紧凑布局」

## features/purchase-orders/purchase-order-detail-page.tsx

- [C] 269: 「服务端已规范化金额：含税 X / 不含税 Y / 税额 Z」→ 「服务端已规范化」技术表述 → 「金额已按系统规范计算」
- [C] 1576-1577: 「采购提交只读；无字段编辑器。决策使用简化演示（非完整待办流程 提交字段回显，但含 submission / lockVersion / 任务号）。」→ 整句为开发备注式说明 → 「以下为采购提交的只读回显，不可修改」
- [C] 1585-1586: 「subjectHash `{…}` · 经办 …」→ 内部 hash 字段名+值暴露 → 删除 subjectHash 展示，保留经办/提交时间
- [C] 1370: 「…拆单维度变更需提示影响（演示中付款条件可改）。」→ 「演示中」为内部说明 → 删括号内容
- [B] 1389/1395: 「供应商（只读 · 拆单维度）」「采购类型 / 履约责任（只读）」→ 与下方拆单维度解释重复 → 只保留「（只读）」
- [B] 857: 「1 销售单 × 1 供应商 × 1 类型 × 1 付款条件 × 1 履约责任」与 preview-panel 121 行「一张采购单 = 一张销售单 × …」重复表述 → 两处统一

## features/purchase-orders/purchase-order-preview-panel.tsx

- [C] 315: 「金额合计（服务端舍入）」→ 「服务端舍入」技术表述（detail 页 1435 行「明细（服务端舍入）」同）→ 「金额合计（系统计算）」
- [C] 309: 「当前角色无成本字段权限：金额标签保留，值已掩码（不返回原值）。」→ 「不返回原值」内部实现 → 「当前角色无成本字段权限：金额已隐藏」
- [C] 121-122: 「一张采购单 = 一张销售单 × 一个供应商 × 一种采购类型 × 一套付款条件 × 一个履约责任。」→ 与 detail 页重复（见上）→ 二选一，建议保留本处完整版
- [B] 58: SectionTitle「审核 / 付款 / 开票 / 履约」与 65-106 行各轨 label（审核、付款、进项票、履约）信息重复 → 标题改为「进度」或删除标题

## features/supplier-orders/supplier-orders-list-page.tsx

- [C] 457: 「重放不可用：须先查询明确无结果且可安全重试」→ 「重放」为内部动作名 → 「不可重试：需先查询确认无结果且系统允许重试」
- [C] 618: 「身份列与操作列固定；履约/取消/退款三轨正交展示。」→ 「三轨正交」内部设计术语 → 「履约/取消/退款三种状态独立展示」

## features/supplier-orders/supplier-order-center-page.tsx

- [C] 633: 「`{o.paymentOccurredNotice}` 支付键摘要 `{o.paymentFactKey}` · 支付时间 …」→ 「支付键摘要」为内部概念 → 「支付凭证 `{paymentFactKey}`」
- [C] 642: AlertTitle「结果未知 — 唯一主路径是查询原结果」→ 「主路径」技术表述 → 「结果未知 — 请先查询原结果」
- [C] 655: 「…「安全重放」在明确无结果且可安全重试前保持禁用。」→ 「安全重放」技术词 → 「重试按钮在确认无结果前保持禁用」
- [C] 665-666: 「履约轨保持「已完成」，退款轨为「部分」。三轨正交，不用单一综合状态覆盖记录。」→ 「三轨正交」内部术语 → 「履约与退款状态独立记录，互不覆盖」
- [C] 695: 「版本 `{detail.workItem.subjectHash}`」→ hash 直接展示 → 删除或显示为「版本号」
- [C] 1236-1237: 「沿用原下单任务号（服务端）」「任务保持 PENDING/IN_PROGRESS，不自动完成」→ 「（服务端）」与状态码均为内部 → 「沿用原下单任务号」「任务保持待处理，不会自动完成」
- [B] 563-564: 「查询已明确无结果，可安全重放」与「须先查询明确无结果且服务端确认可安全重试」同句语义重复（557 行按钮、655 行、preview 页 137 行又各出现一次）→ 全局统一为一句「需先查询确认无结果后，方可重试」
- [C] 1172: 「技术摘要：`{a.techSummary}`」→ 审计表内面向运营也展示 → 改「摘要」并输出业务描述

## features/supplier-orders/supplier-order-preview-panel.tsx

- [C] 84: 「三轨正交：部分退款不会回退履约「已完成」状态。」→ 同 center 页 → 「部分退款不会影响履约「已完成」状态」
- [C] 137: 「唯一主路径是「查询原结果」。在取得明确无结果且服务端确认可安全重试前，不可直接重放或再次下单。」→ 「主路径」「重放」内部 → 「请先「查询原结果」；确认无结果且系统允许重试前，不要再次下单」
- [B] 140: 「最近查询：`{outcomeLabel}` — `{…}`」→ 与 136 行「结果未知」Alert 中信息重复 → 合并

## features/supplier-settlements/supplier-settlements-page.tsx

- [C] 581/1109: 「PERIOD_POLICY_UNCONFIGURED：不得新建草稿…」「PERIOD_POLICY_UNCONFIGURED 或策略不可用，不创建草稿。」→ 错误码直接上屏 → 「结算期间策略未配置，暂不能新建草稿」
- [C] 689: AlertTitle「期间策略未配置（PERIOD_POLICY_UNCONFIGURED）」→ 同上 → 删括号错误码
- [C] 1611: 「仍未知，请稍后用原任务号再查。」→ 「原任务号」内部查询凭证 → 「结果仍未返回，请稍后重试」
- [C] 1629: 卡描述「…全部 `{taxBasisLabel}` · 服务端舍入，前端不重算」→ 「服务端舍入，前端不重算」为内部实现说明 → 删除后半句
- [C] 1743-1749: 「`sourceSnapshotHash=` `{…}` · `subjectHash=` `{…}`」→ hash 字段名+值直接暴露 → 删除，保留「来源数据 · 更新时间」
- [C] 1759-1761: 「W26 数据仅展示（`{…}`），不参与正式取数」→ W26 内部工作区编号 → 「以下数据仅供参考，不参与正式结算」
- [C] 1836: 「前端禁用态仅解释；服务端岗位分离校验为准」→ 「前端/服务端」内部 → 「系统将按岗位权限最终校验」
- [C] 1848: 「…金额只读（canEditBillOrOrder=…）」→ 字段名暴露 → 「金额只读，不可修改」
- [C] 2175: 「创建 SUPPLIER_SETTLEMENT_REVIEW 待办」→ 内部任务类型码 → 「创建结算复核待办」
- [C] 2209: 「追加成本差额 cost_entry」→ 内部记录类型码 → 「追加成本差额记录」

## features/supplier-payables/supplier-accounts-page.tsx

- [C] 918: 「…请返回来源页重查门禁；附件或未核销付款不满足先款条件。」→ 「门禁」为内部校验概念（allocation-session 474 行、1335 行同）→ 「请返回来源页重新校验付款条件；未核销付款不满足先款要求」
- [C] 973: detail="服务端口径"（MetricStrip）→ 「服务端口径」内部 → 「系统口径」
- [C] 1335: AlertTitle「付款门禁（服务端）」→ 同上 → 「付款条件（系统校验）」

## features/supplier-payables/allocation-session.tsx

- [C] 262: 「拟分配合计超过本次记录金额（前端仅提示，服务端将再次校验）」→ 前后端分工暴露 → 「拟分配合计超过本次记录金额，最终以系统校验为准」
- [C] 474: 「…由服务端重新查询付款门禁；未核销付款不算满足。」→ 同上 → 「…将重新校验付款条件；未核销付款不满足先款要求」
- [C] 596: 「开放余额（服务端）」→ 删「（服务端）」
- [C] 637: 「拟分配仅作表单提示；未分配余额以提交后服务端结果为准」→ 「服务端」内部 → 「未分配余额以提交后的系统结果为准」
- [C] 845: 「…服务端将校验供应商一致、余额与策略版本。」→ 「服务端」内部 → 「提交时系统将校验供应商、余额与策略版本」
- [C] 858: 「同步更新应付开放余额（服务端）」→ 删「（服务端）」

## features/supplier-api-connections/supplier-api-connections-page.tsx

- [C] 1224: 「…请按原任务号 / operationId 查询最终结论。」→ operationId 技术词 → 「请按原任务号查询最终结论」
- [C] 1283: 「健康检查 / 目录同步以任务号固定结果；HTTP 返回不等于业务完成。」→ HTTP 技术表述 → 「请求成功返回不代表业务处理完成，请以任务号查询最终结果」
- [C] 1585: 「采购仅见就绪摘要，不显示引用别名」→ 「引用别名」内部概念（1862-1864 行「（采购不显示别名/版本）」同）→ 「采购角色仅查看就绪状态」
- [C] 2153: 「跨工作面只传连接稳定身份；目标页重新查询健康与能力，不信任来源布尔值。」→ 纯内部实现说明 → 「进入相关工作面时将重新获取最新状态」
- [C] 2163: 「配置变更与业务确认追加式只读 ·」→ 「追加式只读」内部术语 → 「配置变更与业务确认均保留审计记录」
- [C] 1392: `sensitiveFields={["密钥正文", "签名材料"]}`（停用影响预览）→ 字段组名上屏且无解释 → 增补说明「以下敏感信息将受影响：密钥配置、签名材料」

## features/procurement-confirmation/procurement-confirmation-page.tsx

- [C] 937: 「仅写入显式 URL 与当前会话，未配置 preferenceScope 时不持久化」→ preferenceScope、显式 URL 全技术 → 「该偏好仅在本次会话内生效」
- [C] 941: 「偏好范围：未配置（会话临时）」→ 同上 → 「该偏好仅在本次会话内生效」
- [C] 1116: label「提交身份 submissionId」→ 字段名暴露（1059 行「新 submissionId」同）→ 「提交编号」
- [C] 1667: 「生成不可变采购创建依据（不创建采购建单任务）」→ 「采购建单任务」内部概念 → 「生成采购创建依据（无需单独建单）」

## 汇总 · Top 10 高优先问题

| # | 位置 | 问题 | 类型 |
|---|------|------|------|
| 1 | external-product-center-page.tsx:254 | Alert 标题直接显示错误码 WORK_ITEM_TYPE_UNREGISTERED | C |
| 2 | external-product-supply-page.tsx:1759 | 「使用 WorkItemActionEnvelope；…PENDING/IN_PROGRESS…」全句技术黑话 | C |
| 3 | external-product-supply-page.tsx:1353-1356 | 布尔原值 true/false 直接上屏 | C |
| 4 | supplier-settlements-page.tsx:1743-1749 | sourceSnapshotHash/subjectHash 字段名+值暴露 | C |
| 5 | supplier-settlements-page.tsx:1848 | canEditBillOrOrder 字段名上屏 | C |
| 6 | procurement-confirmation-page.tsx:937 | preferenceScope 等内部参数说明上屏 | C |
| 7 | supplier-api-connections-page.tsx:2153 | 「跨工作面只传连接稳定身份…不信任来源布尔值」内部实现说明 | C |
| 8 | supplier-settlements-page.tsx:1759-1761 | W26 内部编号暴露 | C |
| 9 | external-product-supply-page.tsx:1233 | supplier_external_product 表名暴露 | C |
| 10 | 多页重复 | 「三轨正交」「重放」「前端/服务端」等内部术语跨 7 个文件重复出现 | C/B |

共发现 60 处问题（C 类 47、B 类 9、A 类 4）。其中 hash/错误码/字段名直接上屏 8 处为最高危；「前端/服务端」「掩码」「门禁」「重放」「三轨正交」「消费」「终态」等内部术语在多个文件重复出现，建议统一术语表后全局替换。
