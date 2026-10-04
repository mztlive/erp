# 审计业务依赖逐项实施登记

本登记执行[审计业务依赖清单与解耦实施合同](audit-business-dependency-contract.md)，用于固定每项改造的拥有领域、目标事实、回放语义、唯一防护、直接切换映射及验收责任。合同原文的 A、B、C、D、E 编号和 V01—V14 验收编号保持不变。

## 1 执行范围与状态规则

1. 用户于 2026-10-04 明确系统尚未上线，本次不需要历史数据或旧版本兼容。本登记执行合同 §1.0：A、B 直接切到独立领域回执，C 直接切到领域创建人/取消事实，D 使用稳定命令来源，普通写用例接入统一审计边界。
2. 原日志协议、旧审计 ID 和原回放行为作为源码盘点事实保留；本次不交付旧回执双写、legacy alias、历史 parser、回填工具、滚动 writer 共存或旧二进制回退。V03 旧版本重试和 V12 历史回填/回退验收不适用。
3. 表中“源码已实现，统一检查通过；验收未核销”表示当前代码已经建立该项领域事实或保留机制及审计边界，并通过 §10 的统一编译、库单测及质量门禁。该状态不表示每项 V 验收已完整核销；运行证据只覆盖 §10.4 登记的具体入口与分支。
4. 用户于 2026-10-04 明确要求运行 E2E。按 §10.4 执行隔离数据库、真实 API 与浏览器验收，开发源库仅只读复制。新增回执并发、真实事务故障注入及全部批次验收仍未核销；不得补入历史迁移或滚动发布。
5. 下列集合名称已经与当前源码的领域常量和索引入口核对。真实分片的索引样本单独登记到 §10.4，不以启动或索引存在证明并发正确性；名称、身份或结果 schema 调整必须同步修改本登记。
6. 实施者只修改拥有领域及组合层所需路径，遵守[后端规则](../backend/AGENTS.md)。归档、历史证据和无生产调用接口不得重新接入业务。普通审计查询保留其授权边界。

## 2 共用实施约束

以下约束与各行要求共同构成执行合同。原防护记录用于说明被替换的依赖，不要求在未上线版本中保留旧 writer。

| 代码 | 本次实施规则 |
| --- | --- |
| U1 独立回执唯一防护 | 新回执以稳定命令 ID 或拥有领域明确的业务身份建唯一约束，结构化保存 actor、动作、作用域/目标、幂等键摘要、当前请求指纹及 schema、结果及业务/任务版本。回执、正式业务事实和成功审计使用同一 Executor 写入。命中回执重验原入口必须执行的当前授权，同键异载荷明确冲突，未知结果只查证原命令；不写旧日志回执，不查展示审计，不保留历史定位/parser。 |
| U2 领域创建人 | 创建命令从已认证命令人写入不可变 `created_by`，责任交接不覆盖该字段。两个资金投影直接读取同一领域事实。空或非法创建人明确失败且不授予参与资格，不使用审计、当前经办人或当前账号补齐。供应商付款新增字段，五类已有字段直接复用。 |
| U3 既有独立机制 | 沿用现有拥有领域的回执/命令/业务事实集合及唯一索引，补充类型化事件和关联引用；不得增加一次执行的第二份业务成功事件，不得改回审计恢复协议。 |
| U4 稳定业务关联 | 证据、内部幂等键、库存分录来源及结论使用稳定命令身份或正式业务事实引用；新审计 ID 仅用于关联。未上线范围不交付旧 ID 别名或历史来源回填。当前命令的证据、版本、任务和完整强类型结果必须保持可核验。 |
| U5 展示查询 | 审计集合继续用于授权展示、导出和追溯；动作、字段、中文模板独立登记。未知字段显示缺失；不得用当前名称伪造历史，不得泄漏凭据、密文、完整银行信息或完整敏感值。 |

### 2.1 当前领域回执与事实登记

| 拥有领域 | 目标集合/事实 | 状态及要求 |
| --- | --- | --- |
| `erp-finance` | `finance_command_receipts` | A01—A07；A01 使用 `FinanceCommandResult::PurchaseInvoiceRegistered`，A02—A07 使用资源类型可判别的 `ResourceCommitted`；当前对象视图和附件归一化保持各原入口合同。唯一索引 `uk_finance_command_receipts_id`。源码已实现，统一检查通过；验收未核销。 |
| `erp-returns` | `returns_command_receipts` | A08—A13；退款与冲正的结果类型、原请求版本和动作分别匹配。唯一索引 `uk_returns_command_receipts_id`。源码已实现，统一检查通过；验收未核销。 |
| `erp-fulfillment` | `fulfillment_command_receipts` | A14、A15；客户验收及冲正命令独立匹配。唯一索引 `uk_fulfillment_command_receipts_id`。服务履约确认仅接入自动审计，不新增回放语义。源码已实现，统一检查通过；验收未核销。 |
| `erp-supply` | `supply_command_receipts`；`supplier_offering_handover_command_receipts` | 前者用于 A16、B16—B25，B19 使用 `SupplyCommandResult::CapabilitiesUpdated`；后者用于 B04。分别使用 `uk_supply_command_receipts_id`、`uk_supplier_offering_handover_command_receipts_id`。E03 的 `supplier_api_connection_command_receipts` 独立保留。源码已实现，统一检查通过；验收未核销。 |
| `erp-workflow` | `work_item_command_receipts`；`approval_cancellation_facts` | A17/A18 的转交/关闭结果分别登记；C08 保存原审批回执、执行、blocker、终态版本、时间及历史任务。分别使用 `uk_work_item_command_receipts_id`、`uk_approval_cancellation_facts_id`、`uk_approval_cancellation_facts_receipt`。源码已实现，统一检查通过；验收未核销。 |
| `erp-import` | `import_command_receipts` | B01/B02 使用 `ImportCommandResult::Confirmation` / `Execution`，保存完整结果和后台任务引用。唯一索引分别保护 `id`、`identity.command_id`、`audit_event_id`。源码已实现，统一检查通过；验收未核销。 |
| `erp-integration` | `integration_command_receipts` | B03 使用 `TaskAction` / `TaskCompletion` / `DirectDecision` 强类型结果，D03 结论由稳定关闭命令形成。唯一索引 `uk_integration_command_receipts_id`。源码已实现，统一检查通过；验收未核销。 |
| `erp-sales` | `sales_command_receipts` | B05—B07 区分 `Created` / `Submitted` / `HandedOver`，保留提交快照与当前责任视图语义。唯一索引 `uk_sales_command_receipts_id`。源码已实现，统一检查通过；验收未核销。 |
| `erp-procurement` | `purchase_command_receipts` | B08—B12 持久化 `PurchaseCommandReceipt<T>` 完整版本化 payload、业务/任务版本和子命令关联。唯一索引 `uk_purchase_command_receipts_id`。源码已实现，统一检查通过；验收未核销。 |
| `erp-catalog` | `product_handover_command_receipts` | B13 保存结构化身份及首次结果快照，回放返回当前授权视图。唯一索引 `uk_product_handover_command_receipts_id`。源码已实现，统一检查通过；验收未核销。 |
| `erp-supplier` | `supplier_handover_command_receipts` | B14/B15 以强类型结果及 action/resource_type 区分，能力身份同时绑定供应商。唯一索引 `uk_supplier_handover_command_receipts_id`。源码已实现，统一检查通过；验收未核销。 |
| `erp-inventory` | `stock_adjustment_cancellations` | C07 保存原命令、调整单、审批实例、执行、actor、原因、版本、终态时间、历史任务及审批回执引用。唯一索引 `uk_stock_adjustment_cancellations_id`、`uk_stock_adjustment_cancellations_receipt`。源码已实现，统一检查通过；验收未核销。 |

新集合不得随审计日志归档或 TTL 删除仍承担去重作用的回执。原 `uk_audit_logs_id` 可作为日志身份约束保留，禁止继续用它承担业务命令防重。

## 3 A 类实施登记

各行的“新语义”是必须达到的行为，非已通过的结果。所有行同时执行 V11、V13、V14；表内 V 编号列出该项额外必须覆盖的验收。V03、V12 的旧版兼容要求按 §1 不适用。

| 编号/批次 | 拥有领域与目标集合 | 原回放 → 新回放语义 | 原防护与本次事实要求 | 验收 | 实施状态 |
| --- | --- | --- | --- | --- | --- |
| A01 / P1 | `erp-finance`；`finance_command_receipts` | 旧审计回执恢复发票 ID，再读取登记对象当前视图 → 独立强类型回执恢复同一 ID，再读取同一当前视图；首次查询和异常查证同步迁移，不改成原请求快照。 | U1；原审计回执依赖 `uk_audit_logs_id`，本次以独立命令唯一索引替代；保留发票规范化号码去重、进项分配序号约束及正式分配事实。 | V01、V02、V04—V06、V09 | 源码已实现，统一检查通过；验收未核销 |
| A02 / P2 | `erp-finance`；`finance_command_receipts` | 恢复付款 ID 与已提交附件返回语义 → 原付款当前允许视图及原附件提交结果；不重复付款、附件或分配。 | U1；保留当前附件归一化、付款/分配引用及付款编号/分配序号唯一约束；原日志唯一防护改为领域回执约束。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| A03 / P2 | `erp-finance`；`finance_command_receipts` | 审计恢复已提交回款 → 独立回执恢复原回款 ID 及既有已提交视图，异常后只查证。 | U1；结构化保存 actor、当前请求指纹、回款及分配引用，不重新登记到账事实；新回执唯一约束承担防重。 | V01、V02、V04—V07 | 源码已实现，统一检查通过；验收未核销 |
| A04 / P2 | `erp-finance`；`finance_command_receipts` | 在调用方 Executor 内校验原单及原请求 → 保留授权先行、精确匹配和同 Executor 查询。 | U1；保存原单和原提交请求版本、指纹、审批及任务结果，不使用较新单据版本重算请求身份；删除旧候选读取。 | V01、V02、V04—V07 | 源码已实现，统一检查通过；验收未核销 |
| A05 / P2 | `erp-finance`；`finance_command_receipts` | 恢复销项发票及附件结果 → 恢复同一发票和原附件归一化语义。 | U1；保存当前附件归一化和发票/分配结果，保留发票业务唯一约束；不保留历史附件 parser。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| A06 / P2 | `erp-finance`；`finance_command_receipts` | 恢复申请 ID、绑定及审批启动结果 → 原申请和原启动/绑定结果，不重复启动。 | U1；保存申请版本、绑定、任务和审批回执关联；原日志防重改为领域回执唯一约束。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| A07 / P2 | `erp-finance`；`finance_command_receipts` | 恢复原申请撤回结果 → 同一申请撤回结果与原授权校验。 | U1；新身份明确保存资源类型与申请 ID，不沿用申请 ID 占 `resource_type` 槽的旧协议；保存撤回原因及结果。 | V01、V02、V04—V07 | 源码已实现，统一检查通过；验收未核销 |
| A08 / P2 | `erp-returns`；`returns_command_receipts` | 恢复退款 ID 且验证读取/提交资格 → 保留退款当前可读结果和同序资格重验。 | U1；保存当前请求指纹、退款 ID、来源关系及原资格要求，保留退款业务唯一约束。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| A09 / P2 | `erp-returns`；`returns_command_receipts` | 恢复供应商退款及原来源 → 独立回执恢复同一退款及原来源，不新建退款。 | U1；保存当前身份、原采购/退款来源和结果引用；不保留审计来源 fallback。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| A10 / P2 | `erp-returns`；`returns_command_receipts` | 恢复回款冲正 ID → 恢复原冲正单及结果，不重复冲正。 | U1；保存请求指纹、原回款关系和结果，保留冲正业务唯一约束。 | V01、V02、V04—V07 | 源码已实现，统一检查通过；验收未核销 |
| A11 / P2 | `erp-returns`；`returns_command_receipts` | 恢复付款冲正 ID 与原付款 → 恢复相同关系和原允许视图。 | U1；保存当前命令身份、付款与冲正引用，领域唯一约束防重。 | V01、V02、V04—V07 | 源码已实现，统一检查通过；验收未核销 |
| A12 / P2 | `erp-returns`；`returns_command_receipts` | 同 Executor 精确匹配已有回款冲正提交 → 保留绑定原请求版本的命令身份和启动结果。 | U1；保存原请求版本、完整 submit payload 和审批/任务结果，不以 BPM 启动回执替代精确命令身份。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| A13 / P2 | `erp-returns`；`returns_command_receipts` | 恢复已有付款冲正提交 → 保留原账号、静态权限、资金来源、经办职责的校验顺序；未知结果只查证。 | U1；付款冲正身份独立于 A12，保存原请求版本和付款来源，不改变失败顺序。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| A14 / P5 | `erp-fulfillment`；`fulfillment_command_receipts` | 恢复验收 ID、销售单及完成任务 → 恢复原验收和完成事实，当前授权继续执行。 | U1；保存验收/销售单/任务关联、当前请求指纹及版本，新领域回执承担防重。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| A15 / P5 | `erp-fulfillment`；`fulfillment_command_receipts` | 恢复反向验收 → 恢复同一反向验收、原验收和原因身份。 | U1；保存原因、原验收、反向验收及结果，不依赖审计消息。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| A16 / P4 | `erp-supply`；`supply_command_receipts` | 恢复异常任务处理结果 → 恢复正式决定及来源，不在回放时恢复供给状态。 | U1；保存任务及正式决定引用，领域回执承担原日志防重，不恢复供给状态。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| A17 / P5 | `erp-workflow`；`work_item_command_receipts` | WorkflowAudit Port 恢复转交 → 领域回执恢复原转交结果，重验管理范围和任务类型守卫。 | U1；保存 actor、工作项、目标人、原版本和原因指纹，当前权限重验保持。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| A18 / P5 | `erp-workflow`；`work_item_command_receipts` | 恢复受控关闭 → 恢复原关闭结果、替代任务及领域关闭证据；D03 的结论身份不变。 | U1、U4；保存关闭原因、工作项、替代任务和关闭证据，D03 结论由稳定命令身份产生。 | V01、V02、V04—V07、V09、V10 | 源码已实现，统一检查通过；验收未核销 |

## 4 B 类实施登记

各行同时执行 U1 及 V11、V13、V14。旧审计回执曾依赖 `uk_audit_logs_id`；本次改用拥有领域的回执唯一约束，删除旧 parser，不交付兼容模块或迁移工具。

| 编号/批次 | 拥有领域与目标集合 | 原回放 → 新回放语义 | 原防护与本次事实要求 | 验收 | 实施状态 |
| --- | --- | --- | --- | --- | --- |
| B01 / P5 | `erp-import`；`import_command_receipts` | 解析确认指纹/结果并验证确认记录、决定、任务完成 → 强类型确认结果，保留全部交叉验证。 | U1；将确认结果、批次/任务版本、确认记录和决定保存为结构化字段；删除旧 result 消息 parser。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B02 / P5 | `erp-import`；`import_command_receipts` | 恢复开始应用、取消待处理、失败重试的后台任务 → 保存原动作/批次/任务结果，允许后台进度依既有规则推进。 | U1；按 StartApply/CancelPending/RetryFailed 保存强类型结果、批次/试算/后台任务版本、状态、影响数量和下一步；删除旧 execution parser。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B03 / P5 | `erp-integration`；`integration_command_receipts` | JSON 信封恢复 W29 动作/完成/直接决定 → 完整强类型结果与身份检查，保留非终结和终结分支。 | U1；强类型 payload 直接存储，保存动作、重归集、补偿、证据和终结分支结果；删除审计 JSON 信封恢复。 | V01、V02、V04—V07、V09、V10 | 源码已实现，统一检查通过；验收未核销 |
| B04 / P3 | `erp-supply`；`supplier_offering_handover_command_receipts` | 指纹命中后读取当前维护人/组织/版本 → 仍返回当前供给责任视图及当前访问重验。 | U1；保存目标类型、actor、当前规范化请求指纹和供给引用，回放仍返回当前视图；删除原始键拼接及旧消息 parser。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B05 / P3 | `erp-sales`；`sales_command_receipts` | 创建/创建并提交恢复销售单并核对创建人 → 原销售单当前可读结果，保持 `stable.created_by` 和当前访问校验。 | U1；保存创建/创建并提交分支、actor、销售单和任务结果；维持创建人一致性及正式单据防重。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B06 / P3 | `erp-sales`；`sales_command_receipts` | 恢复提交快照与行并核对销售单/提交人 → 原提交快照及原行，不返回较新的替代快照。 | U1；直接保存原销售单/提交快照 ID、提交人及行版本，不从审计消息恢复。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B07 / P3 | `erp-sales`；`sales_command_receipts` | 指纹后读取当前责任人和开放任务 → 保留当前责任及开放任务视图。 | U1；保存交接身份、销售单及任务结果；创建人不随责任交接改变。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B08 / P3 | `erp-procurement`；`purchase_command_receipts` | `CreationReceipt` 恢复创建结果并验证单据 → 同一强类型创建结果与存在性验证。 | U1；完整保存 CreationReceipt 强类型 payload、schema 和当前身份；删除审计信封 parser。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B09 / P3 | `erp-procurement`；`purchase_command_receipts` | `SaveDraftReceipt` 加版本核对 → 当前版本可高于回执版本，不能低于回执版本。 | U1；完整保存 SaveDraftReceipt、原锁版本及当前身份；保留版本不低于回执的核验规则。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B10 / P3 | `erp-procurement`；`purchase_command_receipts` | `SourcingReceipt` 恢复采购单集合/库存分配/任务 → 完整批量结果及子建单回执，无重复采购或预占。 | U1、U4；完整保存采购单集合、子命令、库存分配和任务；D02 新库存来源直接指向稳定命令/分配事实。 | V01、V02、V04—V07、V09、V10 | 源码已实现，统一检查通过；验收未核销 |
| B11 / P3 | `erp-procurement`；`purchase_command_receipts` | 依候选顺序恢复 `PurchaseSubmitReceipt` → 原审批、任务及版本结果，独立当前命令身份。 | U1；完整保存 PurchaseSubmitReceipt、当前请求身份、审批/任务和版本；不交付历史候选。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B12 / P3 | `erp-procurement`；`purchase_command_receipts` | `VoidDraftReceipt` 且核对当前作废状态/版本 → 相同状态和版本核对，不重作废。 | U1；保存 VoidDraftReceipt、正式单据引用和版本/状态条件，不读取审计信封。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B13 / P3 | `erp-catalog`；`product_handover_command_receipts` | 指纹后读取商品当前责任 → 相同当前视图、目标资格、组织和当前访问控制。 | U1；保存商品目标类型、商品 ID、目标人、组织和当前请求指纹，删除原始键历史 parser。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B14 / P3 | `erp-supplier`；`supplier_handover_command_receipts` | 指纹后读取供应商当前责任 → 相同当前责任和访问重验。 | U1；保存供应商账户目标类型和当前指纹，账户身份独立于能力交接。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B15 / P3 | `erp-supplier`；`supplier_handover_command_receipts` | 指纹后读取能力负责人/版本 → 相同当前能力责任与版本校验。 | U1；保存能力目标类型、能力 ID、目标人和原版本，禁止与账户交接共用身份。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B16 / P4 | `erp-supply`；`supply_command_receipts` | 完成收据恢复终结动作/任务/订单/结论 → 完整原完成结果并核对正式 `supplier_order_actions`。 | U1、U4；直接保存终结动作、任务/订单版本和结论，核对正式动作；D01 内部幂等键由稳定命令产生。 | V01、V02、V04—V07、V09、V10 | 源码已实现，统一检查通过；验收未核销 |
| B17 / P4 | `erp-supply`；`supply_command_receipts` | 指纹后读取当前跟进人/版本 → 相同当前责任视图和权限重验。 | U1；保存 actor、履约订单、目标人、当前请求指纹及原版本；删除指纹说明后缀兼容 parser。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B18 / P4 | `erp-supply`；`supply_command_receipts` | 调查收据恢复证据/操作号/任务版本 → 相同强类型调查结果；直接调查和任务内调查身份继续区分。 | U1、U4；直接保存证据、操作号及订单/任务版本，入口身份区分；D01 证据由稳定命令产生。 | V01、V02、V04—V07、V09、V10 | 源码已实现，统一检查通过；验收未核销 |
| B19 / P4 | `erp-supply`；`supply_command_receipts` / `CapabilitiesUpdated` | 审计 ID/指纹恢复能力修改 → 独立能力命令结果；继续返回可关联的 `audit_event_id`。 | U1、U3；独立登记能力修改身份与结果，不与 E03 通用连接治理混淆；审计编号只关联。 | V01、V02、V04—V07、V09、V10 | 源码已实现，统一检查通过；验收未核销 |
| B20 / P4 | `erp-supply`；`supply_command_receipts` | 恢复操作号/版本及正式差异决定 → 原决定结果，继续联合核验差异、明细和单头。 | U1；直接保存操作号、结算/差异版本及决定结果，删除自由消息恢复。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B21 / P4 | `erp-supply`；`supply_command_receipts` | 恢复刷新来源快照/版本/数量 → 原刷新结果；来源未变化仍形成有效命令回执。 | U1；保存 request ID、来源快照、版本及明细/差异数量；无变化分支仍写一次回执。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B22 / P4 | `erp-supply`；`supply_command_receipts` | 指纹后读取当前制单人/组织/版本 → 相同当前结算责任视图。 | U1；保存结算责任交接动作、目标人和结算 ID，不覆盖创建人。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B23 / P4 | `erp-supply`；`supply_command_receipts` | 指纹后读取当前差异处理人/版本 → 相同当前差异处理视图。 | U1；身份与 B22 区分，保存差异 ID、目标处理人和原版本。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B24 / P4 | `erp-supply`；`supply_command_receipts` | 恢复复核提交的冻结主题/单头版本/任务 → 原任务与提交结果，继续交叉核对任务和结算单。 | U1；直接保存操作号、冻结主题、单头版本和工作项，不启动第二次复核。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |
| B25 / P4 | `erp-supply`；`supply_command_receipts` | 恢复原复核结果及成本差额 → 完整强类型结果、原任务/结算版本和成本差额；保持复核人、制单人分离及成本写序。 | U1；直接保存复核人、决定、任务/结算版本和成本结果，保留职责分离及成本写序。 | V01、V02、V04—V07、V09 | 源码已实现，统一检查通过；验收未核销 |

## 5 C 类实施登记

C01—C06 的权威事实与资金简报直接读取同一领域创建人，不保留审计回退或核对标记过渡。创建人不替代 RBAC、DataScope、任务责任或审批规则；创建人缺失或非法明确失败。

| 编号/批次 | 拥有领域与目标事实 | 原读取 → 新读取语义 | 原防护与本次事实要求 | 验收 | 实施状态 |
| --- | --- | --- | --- | --- | --- |
| C01 / P1 | `erp-finance`；既有 `customer_receipts.created_by` | 旧创建审计首个 actor → 不可变领域创建人同时进入权威事实和简报；无审计回退。 | U2；原首个创建审计 actor 读取删除，直接读取已认证创建人；缺失不授权。 | V04、V07、V11、V13、V14 | 源码已实现，统一检查通过；验收未核销 |
| C02 / P2 | `erp-finance`；`supplier_payments.created_by` | 旧创建审计 actor → 认证命令人写入的不可变创建人；缺失明确失败。 | U2；新增不可变认证创建人，不回填历史记录，不以付款执行人补齐创建人。 | V04、V07、V11、V13、V14 | 源码已实现，统一检查通过；验收未核销 |
| C03 / P2 | `erp-returns`；既有 `customer_refunds.created_by` | 旧创建审计 actor → 不可变领域创建人；不改用可变化经办人。 | U2；删除创建审计反查，不以当前经办人补齐退款创建人。 | V04、V07、V11、V13、V14 | 源码已实现，统一检查通过；验收未核销 |
| C04 / P2 | `erp-returns`；既有 `supplier_refunds.created_by` | 旧创建审计 actor → 不可变领域创建人，两套投影一致。 | U2；直接复用不可变领域字段，删除旧身份 fallback。 | V04、V07、V11、V13、V14 | 源码已实现，统一检查通过；验收未核销 |
| C05 / P2 | `erp-returns`；既有 `receipt_reversals.created_by` | 旧创建审计 actor → 冲正领域创建人，保留参与资格。 | U2；使用冲正自身创建人，不借用原回款创建人。 | V04、V07、V11、V13、V14 | 源码已实现，统一检查通过；验收未核销 |
| C06 / P2 | `erp-returns`；既有 `payment_reversals.created_by` | 旧创建审计 actor → 冲正领域创建人，权威事实/简报一致。 | U2；使用冲正自身创建人，不借用原付款执行人。 | V04、V07、V11、V13、V14 | 源码已实现，统一检查通过；验收未核销 |
| C07 / P5 | `erp-inventory`；`stock_adjustment_cancellations` | 按动作/实例前缀要求唯一原 actor → 结构化事实恢复原 actor，并与调整单、审批实例和原回执交叉验证。 | 保留原审批回执唯一约束；新撤回事实按稳定命令身份建唯一约束，保存调整单/实例/actor/原因/结果/审批回执；缺失或不匹配明确拒绝，删除消息前缀 parser。 | V01、V02、V04—V08、V10、V13 | 源码已实现，统一检查通过；验收未核销 |
| C08 / P5 | `erp-workflow`；`approval_cancellation_facts` | 唯一取消审计的 execution/reason 加终态校验 → 完整 actor/execution/reason/blocker/时间/终态事实，仍校验任务和实例。 | U3；保留审批回执唯一约束，结构化取消事实按稳定身份唯一；完整保存 execution/reason/blocker/actor/时间/终态，原 `result_ref` 仅关联；删除取消审计 parser。 | V01、V02、V04—V08、V09、V13 | 源码已实现，统一检查通过；验收未核销 |

### 5.1 资金经办查询补充约束

原 62 项编号不变；下列聚合查询补充到 C 类身份解耦验收，不能用 C01—C06 的创建人读取代替经办查询检查。

| 查询入口 | 权威身份事实与语义 | 实施与验收要求 | 状态 |
| --- | --- | --- | --- |
| [回款资金范围查询](../backend/crates/erp-read-models/src/finance/funds_scope/repository/receipt.rs) 的 `Register` | `CustomerReceipt.created_by` 是回款登记人；直接匹配领域创建人 | 删除登记动作的审计聚合反查；`Settle` 继续读取 `ApprovalSubjectSnapshot.payload.submitted_by`，表示结算申请提交人，禁止替换成登记人。覆盖 V07、V13。 | 源码已实现，统一检查通过；验收未核销 |
| [付款资金范围查询](../backend/crates/erp-read-models/src/finance/funds_scope/repository/payment.rs) 的付款提交执行人 | `SupplierPayment.posted_by` 与 `posted_at` 是实际付款过账人和发生时间，独立于创建人及可变付款经办人 | 在本次 `PaymentSettlement` 的同 Executor 完成写入不可变事实；只按已过账/已冲回且正式过账事实完整的付款筛选，不从创建审计、创建人或 `paid_at` 推定执行人。覆盖 V04、V07、V13。 | 源码已实现，统一检查通过；验收未核销 |

`customer_receipts` 登记人查询使用 `created_by + deleted_at + created_at + id` 索引；`supplier_payments` 执行人查询使用 `posted_by + deleted_at + created_at + id` 索引。索引定义已登记，真实数据库索引和查询执行计划未验证。创建人、申请提交人、审批人、当前任务责任人和实际付款执行人分别执行其资格与授权合同。

## 6 D 类实施登记

| 编号/批次 | 拥有领域与目标事实 | 原身份 → 新身份语义 | 原防护与本次事实要求 | 验收 | 实施状态 |
| --- | --- | --- | --- | --- | --- |
| D01 / P4 | `erp-supply`；B16/B18 回执中的稳定命令引用及既有证据/动作 | 审计 ID 派生证据与内部幂等键 → 稳定命令身份派生新结果；保留当前命令完整证据和动作结果。 | U1、U4；原确定性审计输入取消，新证据及内部幂等键直接使用稳定命令；保留 supplier_order_actions 幂等键唯一约束。 | V01、V02、V04—V06、V09、V10、V13 | 源码已实现，统一检查通过；验收未核销 |
| D02 / P3 | `erp-procurement` 拥有选源命令；`erp-inventory` 拥有预占分录来源；`purchase_command_receipts` 稳定命令及正式分配引用 | `source_document_id` 保存 audit ID → 新分录明确使用选源命令或正式分配事实；不新增展示审计来源。 | U1、U4；`source_document_id` 保存稳定选源命令 ID，预占的 `source_type=ExistingStock` 和 `source_allocation_id` 保存正式分配身份；不保存 audit ID。 | V01、V02、V04—V06、V09、V10、V13 | 源码已实现，统一检查通过；验收未核销 |
| D03 / P5 | `erp-workflow` 拥有关闭命令；`erp-integration` 拥有对账结论；稳定关闭命令及正式对账结论关联 | 回执 ID 指纹生成 `w29-close-…` → A18 由稳定命令身份生成结论；保留结论序号、关闭类型和证据关系。 | U1、U4；稳定关闭命令直接产生结论，保留正式结论唯一防护、关闭类型和证据关联，不使用随机新审计 ID。 | V01、V02、V04—V06、V09、V10、V13 | 源码已实现，统一检查通过；验收未核销 |

## 7 E 类保留与接入登记

| 编号/批次 | 拥有领域与集合/事实 | 保留语义与接入要求 | 既有防护与本次关联要求 | 验收 | 实施状态 |
| --- | --- | --- | --- | --- | --- |
| E01 / P6 | `erp-audit`；既有 `audit_logs` | 合法操作日志查询；增加结构化中文响应和筛选，前后端同一交付同步更新，不建立旧版兼容窗口。 | U5；保留日志 ID、事件关联和查询授权；领域回执与业务成功事件分离，不重复展示为第二次操作。 | V10、V11、V13、V14 | 源码已实现，统一检查通过；验收未核销 |
| E02 / P6 | `erp-audit` 存储 `audit_events`；`erp-identity` 拥有身份事件语义 | 身份治理审计和业务审计分别登记目录，沿授权合同查询。 | U5；保持身份动作命名空间、原策略版本和事务后刷新语义，不无条件加入业务日志选项。 | V07、V10、V11、V13、V14 | 源码已实现，统一检查通过；验收未核销 |
| E03 / P6 | `erp-supply`；既有 `supplier_api_connection_command_receipts` | 独立连接治理回执继续解释指纹、结果及 Processing/Rejected/Unknown 生命周期；审计编号只是关联。 | U3；保留 `uk_supplier_api_command_receipts_idempotency` 和现有业务身份组成；B19 单独迁移，不由本项核销。 | V01、V02、V04—V06、V09—V11、V13、V14 | 源码已实现，统一检查通过；验收未核销 |
| E04 / P6 | `erp-workflow`；既有 `approval_command_receipts`；`bpm` 纯回执模型 | 继续复用审批独立回执，不引入 ERP/Mongo/HTTP 到 `bpm`；C08 取消终态另行实施。 | U3；保留 `uk_approval_command_receipts_id`、`uk_approval_command_receipts_idempotency`、原结果引用和状态恢复分类。 | V01、V02、V04—V06、V08、V09、V13、V14 | 源码已实现，统一检查通过；验收未核销 |
| E05 / P6 | `erp-supply`；既有 `supplier_offering_commands` 与供给/修订/可供性事实 | 创建、修订、可供性修改先持久化领域命令再同事务审计；继续领域恢复，接入类型化事件。 | U3；保留 `uk_supplier_offering_commands_idempotency_key`、正式版本和原命令关联；A16/B04 单独迁移。 | V01、V02、V04—V07、V09、V11、V13、V14 | 源码已实现，统一检查通过；验收未核销 |
| E06 / P6 | `erp-supply`；既有供应商结算草稿及来源事实 | 草稿创建仍走领域重放；统一持久化边界生成安全中文事件，不再写机器协议。 | U3；保留 `prepare_scoped_statement` 定位及结算业务唯一约束；不与 B21 刷新回执合并。 | V01、V02、V04—V07、V09、V11、V13、V14 | 源码已实现，统一检查通过；验收未核销 |
| E07 / P6 | `erp-supply`、`erp-import` 及各 API 结果拥有领域；既有审计编号关联 | 对外 `audit_event_id` 和导入审计编号保留名称、类型与可关联性；不据此反查审计恢复业务。 | U3、U5；新命令回执显式保存原审计引用，前端 `auditEventId` 映射不变。 | V09—V11、V13、V14 | 源码已实现，统一检查通过；验收未核销 |
| E08 / P6 | 登录协议层及各写用例拥有领域；`erp-audit` 拥有 `audit_attempts`，与成功事件分开持久化 | 登录保留 best-effort；敏感读取和正式写入沿各自失败语义；拒绝、失败和未知尝试在原事务返回后单独记录，不替代成功事件或命令回执。 | U5；保留原请求关联和特殊生命周期；尝试使用独立 `NoTransaction`，写入故障不覆盖首次业务错误或重执行业务，成功写入仍与业务同 Executor。 | V04、V06、V07、V11、V13、V14 | 源码已实现，统一检查通过；验收未核销 |

## 8 自动审计交付合同

1. 业务动作由拥有领域登记，跨域事务及审计 adapter 位于 `erp-processes`。统一边界接收类型化动作、调用人上下文、执行结果和白名单事实，不要求普通业务步骤拼装完整 `AuditLog`。
2. 执行前校验静态元数据，执行后在同一 Executor 内校验结果事实并写审计；审计失败停止提交。有调用方事务时复用 Executor，不嵌套事务。
3. 服务履约确认样板保留 `service_fulfillments` 的正式状态、服务结果、数量、版本、附件及任务变化。事件保存确认人/安全名称快照、履约单 ID/业务编号、确认前后状态、服务结果、数量和命令执行结果，中文模板区分成功确认命令和被确认的服务失败事实。
4. 服务履约确认没有清单内的日志回放依赖，不新增幂等键、回放生命周期或第二份业务成功事件。
5. 只记录领域明确允许的字段；接收人、服务地点、银行卡、凭据、完整请求体和完整实体不得由通用序列化落入审计。后台、HTTP 和内部入口均登记审计或明确免记原因。
6. 其他迁移写用例同样交付类型化事实、中文事件及统一边界，覆盖多对象顺序、重放零成功审计、失败/拒绝/未知分类和已有特殊任务生命周期。
7. 服务履约确认状态为“源码已实现，统一检查通过；验收未核销”。必验 V04、V07、V11、V14；事件事实和渲染库单测不核销最终浏览器体验或真实事务回滚。

### 8.1 主数据交接执行要求

1. B04、B13—B15 已建立拥有领域回执实体、仓储访问器和稳定命令 ID 唯一索引。首次查证和事务内查证直接读取领域回执；删除审计消息 parser 和机器回执写入。
2. 原载荷规范化仍由现有交接指纹函数执行，共享结构化头保存该规范化结果的版本化摘要，不保存商品/供给 JSON 指纹中的原始幂等键。供应商账户和能力使用不同 action/resource_type；能力请求另绑定所属供应商。
3. 命中回执后读取当前责任视图并执行当前范围权限；能力另核验所属供应商。首次执行仍执行原目标资格、组织启用和版本检查；开放审批任务不改派。
4. 四个动作经 `run_audited_event` 统一提交正式事实、独立回执和一次结构化成功事件。商品原 Service 内的重复手工审计已删除；两类无调用的领域单独事务入口已删除，正式用例通过 Process 进入统一边界。
5. 安全事件记录已完成结果及实际责任人/业务组织变更标记；商品编号、供应商编号和供给的供应商 SKU 编号采用发生时事实。能力缺少正式编号时保持缺失，不伪造编号或记录完整人员资料。
6. 已新增各领域 `matcher_preserves_replay_and_payload_conflict`、`matcher_rejects_corrupted_identity_result_and_schema` 或对应双类型 matcher 测试，以及 Process 指纹规范化和中文完成事件测试；这些库单测已随 §10 的全工作区库测试执行；不核销真实数据库及浏览器验收。
7. 四个交接动作执行对应领域 matcher、指纹规范化、事件白名单和中文完成事件库单测；统一编译、质量门禁及库单测由组合层汇总。真实数据库并发/事务或浏览器证据独立登记，未执行时不核销对应验收项。

### 8.2 全域审计目录与依赖门禁

1. [业务动作目录](../backend/crates/erp-audit/src/catalog.rs) 显式登记 action/resource_type、中文名称和版本；身份域 `audit_events` 保持独立目录及授权合同。
2. [审计执行边界](../backend/crates/erp-processes/src/audit/execution.rs) 的 `persist_log`、`persist_logs` 在原 Executor 中执行目录匹配和安全结构化投影，再经审计领域仓储写入；类型化用例经 `execute_audited` / `run_audited_event` 处理首次执行和回放。未知动作拒绝持久化，不将旧机器消息写入新的业务展示事件。
3. A、B、C 的生产命令恢复和业务身份查询不得读取审计；D 只使用稳定命令或正式业务事实。原未使用的职责分离、创建事实和映射历史查询接口及旧消息 parser 删除，不允许重新接回生产业务。
4. [依赖登记](../backend/scripts/audit-boundaries.json) 区分 `Display`、`Audited` 和明确特殊持久化边界；[入口登记](../backend/scripts/audit-entrypoints.json) 以精确路径/函数登记 `Audited` 或带原因的 `Exempt`，并固定 `MetadataFactory`、`Command`、`Transaction`、`PersistenceAdapter` 或 `SpecialLifecycle` 阶段、实际调用符号、次数和位置数。[门禁执行器](../backend/scripts/audit_boundaries.py) 拒绝未登记/过时登记、旧协议解析或符号/次数扩大。普通业务写入不得默认豁免。
5. M4 的目录、统一持久化及入口门禁已建立并通过 §10 的统一编译、库单测和门禁验证；门禁结果只证明其覆盖的静态约束，不能核销真实事务回滚、并发或浏览器体验。
6. 入口登记只覆盖显式审计调用边界，不证明全仓业务写入集合、间接调用、宏展开、动态分派或外部回调必经审计。V14 必须结合真实 HTTP、后台和内部调用链与实际编排库单测核验，不按登记条数核销。

### 8.3 操作人快照与尝试事件

1. [后台认证](../backend/apps/web-api/src/core/middleware/authentication.rs) 复用 `validate_session` 返回的 `AccountCore.name`，写入 `AuditActor` 的可选冻结名称；执行上下文、领域 Prepared 结果和 adapter 沿调用链传递该名称，不重新读取身份集合。
2. 名称去除首尾空白、不超过 128 个字符且拒绝控制字符；未知名称保持 `None`，禁止用展示时的当前名称补成历史。名称不参与领域回执指纹，不替代 actor ID、账号类型或原授权。
3. 成功事件与尝试事件使用同一冻结身份和安全关联快照。[`audit_attempts`](../backend/crates/erp-audit/src/repository/attempt.rs) 保存结构化 `Rejected` / `Failed` / `Unknown` 分类，不复制完整错误正文，不承担业务结果查证或去重。
4. [尝试写入边界](../backend/crates/erp-processes/src/audit/attempt.rs) 在原事务返回后用独立 `NoTransaction` 保存记录；尝试写入失败只记安全诊断，返回首次业务错误，不再次执行命令。成功执行及回放不追加失败尝试，未知结果仍只查证原命令。
5. 身份策略独立任务的提交后刷新和未知结果失败关闭、登录 best-effort、敏感读取独立 `NoTransaction` 按各自入口登记执行；不得用普通业务成功事务或尝试事件统一替换这些生命周期。状态保持“源码已实现，统一检查通过；验收未核销”。

## 9 核销与证据登记要求

1. 每项交付记录列实际修改路径、库单元测试名称、执行命令和结果，以及仍未验证的 V 项。编译、单测、边界门禁和运行验收分别登记，不得互相替代。
2. 文档交付只执行路径/内容核对与 `git diff --check`。表中 V 编号表示验收义务，不表示对应测试已通过。V03 和 V12 的旧版验收不适用；真实运行范围按 §10.3、§10.4 单独登记。
3. 按合同 M0 固定事实、M1 建立领域回执/事实、M2 直接切读、M3 自动审计、M4 删除全部旧 parser/业务反查及门禁、M5 汇总适用验收执行。不得新增旧日志双写、fallback 或迁移工具。
4. 新领域回执具备稳定身份唯一约束和完整结果；C 两套投影及取消终态事实保持授权；D 新业务引用不使用审计 ID；E 既有独立机制和合法展示保留正确边界后，才允许逐项核销。
5. 全清单交付覆盖 A01—A18、B01—B25、C01—C08、D01—D03 及 E01—E08。P1 样板不作为整份合同完成证据；未取得真实运行证据的项目继续标为未验证。


## 10 本次统一检查与未核销范围

本节分别登记源码质量检查和实际运行证据。每项实施状态只按适用验收义务核销；后续修改必须重跑受影响检查并更新本节。

### 10.1 检查结果

| 范围 | 实际命令/检查 | 结果及证据边界 |
| --- | --- | --- |
| 后端格式 | `cargo fmt --all -- --check` | 通过。 |
| 后端编译 | `cargo check --workspace --locked` | 通过；覆盖全部活动 workspace。 |
| 后端静态质量 | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | 通过；不替代运行验收。 |
| 后端库单测 | `env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked` | 33 组，4782 通过、0 失败、63 忽略。忽略用例未执行，不计入通过；包含 Web API 库内认证、HTTP 追踪及审计上下文用例。 |
| BPM 边界 | `./scripts/check-bpm-boundaries.sh` | 通过；纯 BPM 与 ERP 持久化边界保持。 |
| 领域与审计边界 | `./scripts/check-domain-boundaries.sh --cutover` | 通过；18 个领域门禁夹具及 8 个审计门禁夹具通过。实际源码无未登记审计读取/写入、旧业务审计反查或过时入口。 |
| 权限生成物 | `./scripts/check-permissions-drift.sh` | 通过；权限和身份审计动作生成物无漂移。构建执行文件，未启动服务。 |
| 前端质量 | `npm run lint`、`npx tsc --noEmit`、`npm run build` | 通过；生产构建生成 52/52 页面。最终 `event_sequence` 仅补必需 TypeScript 字段，之后定向格式与 TypeScript 复验通过，无运行代码变化。 |
| 前端格式 | 本次 9 个审计文件的定向 oxfmt 检查 | 通过。先前全量格式检查有 6 个未修改文件的存量格式问题；未将定向结果表述为全量格式通过。 |
| Rust 体积增量 | 实际 `rust_size_gate` 规则，对照 `56dbd001f9fea576f56c9d75ee55f44f005ac6d1` | 344 个变更生产文件，其中新增 61 个；新增方法越线、存量方法新增越线或扩大、新增/扩大文件越线均为 0。仓库仍有 207 个存量超限方法和 2 个存量超限文件，不宣称全量体积门禁通过。 |
| 空白与文档 | `git diff --check`、62 行编号/状态/链接核对 | 通过；A18、B25、C8、D3、E8 全部保留。 |

上述命令在各子项目规定目录执行。前端没有新增、修改或执行单测；后端没有执行 `tests/`、`examples/` 或 `--include-ignored`。隔离 E2E 的真实 API、数据库和浏览器证据按 §10.4 单独登记，不计入库单元测试结果。

### 10.2 事件上下文与顺序

1. 当前 HTTP `X-Trace-Id` 以同一安全值进入请求扩展、审计操作人、成功事件及独立尝试；后台及内部无请求入口保持 `None`。不得生成伪造的历史请求号。
2. 业务事件保存正值 `event_sequence`，单事件默认 1；缺失或零序号拒绝反序列化。多对象事件共享命令关联，不以事件 ID 代替命令身份。
3. 采购选源在首个 guard 写入前检查 `2N+1` 容量；每张采购单沿原提交事件、创建事件顺序编号，批次主事件最后编号。创建并提交的销售及逆向命令沿各自实际事件写序编号 1、2；采购责任转交先采购事件 1、再工作项事件 2，普通工作项转交保持 1。合同上传的文件与合同事件共享本次独立调用身份，编号分别为 1、2，不新增回放协议。
4. 操作人名称、请求关联、事件序号及原 BaseModel 通过领域 Prepared/adapter 保留；缺失名称和请求号保持缺失，普通事件自由正文不进入安全事实。
5. 工作流尝试委托 `MongoAuditAttemptSink` 的唯一普通尝试持久化边界；在原事务返回后使用 `NoTransaction`，不重新开启正式写事务、不覆盖首次错误。
6. 当前入口登记为 399 个精确 path/function、617 次显式审计调用，其中 395 项 `Audited`、4 项具备限定原因的 `Exempt`。该数量只核对明确调用的登记，不核销全仓业务写入或 V14 运行覆盖。
7. HTTP 权限中间件沿原请求 tracing 链记录独立 `authorization_attempt` 拒绝/未知分类，不从权限码推导业务动作或伪造业务目标。拥有领域的执行异常由对应业务边界记录到 `audit_attempts`；身份策略、登录及敏感读取继续执行各自生命周期。

### 10.3 未核销范围

| 验收范围 | 状态 | 核销要求 |
| --- | --- | --- |
| V03、V12 的旧版兼容、历史回填及回退 | 不适用 | 系统未上线；不得追加旧协议 writer、历史 parser、alias 或回填工具。 |
| V05 真并发及唯一索引 | 索引存在已验证，新增回执并发未验证 | §10.4 保留一个真实分片的索引样本；新增回执同命令并发 writer 仍须独立验收，既有合同申请并发场景不核销新集合。 |
| V04/V06 真实事务回滚及未知提交数据库场景 | 未验证 | 库单测只证明实际编排的同 Executor、顺序、停止和错误保留；不证明 MongoDB 提交或回滚效果。 |
| V07 真实授权场景、V09 外部任务/附件完整链路 | 部分场景通过，未完整核销 | 单权限审计账号隔离及客户票款岗位分离已取得 §10.4 的真实 API/浏览器证据；六类创建人矩阵、全部专用结果及外部服务失败场景须单独验收。 |
| V11 最终中文浏览器体验 | 供给创建/交接及审计查询通过，未完整核销 | §10.4 覆盖中文动作、名称快照、编号、执行结果、安全事实及真实详情；其余动作、敏感值类型和历史缺失字段矩阵不得据此核销。 |
| V14 全业务入口运行覆盖 | 未核销 | 入口门禁和库编排用例只覆盖已登记的明确边界；宏、动态回调及运行调用链须取得独立证据，不凭登记条数核销。 |
| P1—P6 批次与全清单 | 未核销 | 维持各行适用 V 项及本节未验证项；禁止将统一检查通过改写为全部业务运行验收完成。 |

### 10.4 隔离 E2E 执行与验收范围

1. 执行前在 `backend/` 运行 `cargo build -p web-api --locked`，在根目录运行 `E2E_SKIP_BACKEND=1 bash scripts/ensure-services.sh`。本次后端构建通过，前端生产构建包含最终权限查询修复；前端 `npm run lint`、`npx tsc --noEmit` 和六个变更文件的定向格式检查通过。
2. 全量入口固定为 `bash scripts/run-flow.sh all` 的默认隔离模式；定向入口使用 `python3 scripts/run-e2e-parallel.py e2e/tests/<文件>.spec.ts`。禁止用共享库单流程重置替代隔离复验。
3. E2E 清理须包含新增的九类普通命令回执、三类主数据交接回执、两类取消事实及 `audit_attempts`，保留主数据、账号、RBAC、审计日志与已发布审批定义的既定边界。`bash backend/scripts/test-reset-dev-business-data.sh` 的四模式离线合同检查通过。

| 执行范围 | 命令与实际报告 | 结果 |
| --- | --- | --- |
| 最终全量回归 | `bash scripts/run-flow.sh all`；[总报告](../logs/e2e/20261004t112811_9d580888d884/summary.json) | 6 个隔离分片、34 个文件、48/48 通过；失败、跳过及 flaky 均为 0，全部用例仅执行 1 次。runner 总耗时 353.678 秒，含服务准备的入口耗时 354 秒；34 份逐文件报告与总报告一致。六个临时库删除，API/Mongo 端口关闭，托管 Mongo 数据目录移除。 |
| 客户票款 | `python3 scripts/run-e2e-parallel.py e2e/tests/flow-06-customer-funds.spec.ts`；[报告](../logs/e2e/20261004t112020_2dd6fa0f8f43/summary.json) | 1/1 通过；等待权限加载后真实提交，确认 `400 / INVALID_REQUEST` 与“提交人不得审批自己的单据”。临时数据库删除、托管 Mongo 清理完成。 |
| 业务审计与独立查询权限 | `python3 scripts/run-e2e-parallel.py e2e/tests/flow-32-business-audit.spec.ts`；[报告](../logs/e2e/20261004t112732_c38f221e376c/summary.json) | 2/2 通过；实际用例共 5.9 秒，含准备和清理共 26.0 秒。临时数据库删除、托管 Mongo 清理完成。 |
| 真实分片索引 | 只读 `getIndexes()`；[索引样本](../logs/e2e/20261004t112811_9d580888d884/observations/audit-indexes.json) | 当前运行第 1 分片的 14 个新回执/取消事实集合均存在 `_id` 之外的显式唯一索引；另保存 `audit_attempts` 索引。未执行并发 writer。 |

4. [业务审计用例](../e2e/tests/flow-32-business-audit.spec.ts) 核对 E05 供给创建与 B04 供给交接：同键回放保持原结果、业务版本及成功审计数量；异载荷拒绝；交接命令 ID 与成功事件 ID 分离，保存业务编号、认证名称快照、请求关联和序号 1。原因与原提交键不进入成功事件的安全投影；真实列表和详情使用保存的中文事件。
5. 仅有 `audit_log:list`、仅有 `audit_event:list` 的账号分别取得其事件接口的 200，另一类事件、销售对象、人员及角色接口返回 403。正常审计 URL 和携带真实 `subjectId` 的 URL 均显示允许的表格；不得发起人员/角色读取、误开对象解释，或通过侧栏角标请求未授予的待办统计。
6. 最终全量还提供以下限定回归证据：`flow-06/07` 的 A04/A12/A13 原提交重放保持单据、版本与审批实例；`flow-04` 的 A17 责任转交原请求回放保持任务版本；`flow-26` 的 B01/B02 确认及开始应用重放保持原 `audit_receipt`，应用部分失败后只重试失败行；`flow-10` 的 C07 正向撤回保留原调整单并允许新审批重提。这些用例未完整计数全部资金流水、任务、附件及成功审计，不核销各入口的全部义务。
7. 本节顺序回放与界面验收不证明新回执并发唯一性、服务端未知提交故障恢复、真实事务回滚、多事件序号、B21 来源无变化刷新、C07/C08 错配取消事实拒绝矩阵或 D01—D03 的稳定身份派生。`flow-29` 通过的并发申请属于既有合同编号机制，不核销新增回执的并发验收。S3 按运行前缀隔离；runner 不清理 S3 对象，不得将数据库和进程清理表述为 S3 已清理。
