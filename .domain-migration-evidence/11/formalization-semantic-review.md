# 阶段 11 采购正式化语义核对证据

## 1. 证据身份与适用边界

| 项目 | 固定值 |
| --- | --- |
| 阶段 | 11：采购领域 Crate 迁移 |
| 执行目录 | `/private/tmp/erp-domain-crate-11-procurement` |
| Before | `07da7863ec0fb975d2093e4e97a43c7e0813e607`，下文简称 `07da7863` |
| After | 当前未提交工作树；证据登记时 HEAD 仍为 `07da7863` |
| 核对对象 | 采购首次正式化、当前销售分配、原始应付及确认成本、付款条件解析回调 |
| 执行方式 | 读取 Before Git 对象与 After 源码，逐符号核对；当前补充核对限定为写入参数打包、履约根导出和付款适配器接线；履约函数按命名空间、根导出路径与空白归一化比较 |
| 结论级别 | 静态语义核对；所列范围未发现时间/ID 调用顺序、首错或写入顺序漂移 |
| 执行限制 | 本核对未修改 backend 源码，未执行 Cargo、测试或数据库操作 |

本证据只适用于所列输入提交和 After 文件内容。不得将本证据登记为编译、测试通过、真实事务回滚、并发正确性或真实数据库运行证明。**真实数据库运行未验证。**

## 2. 符号迁移与实际提供方

以下路径均相对仓库根目录。Before 表中 `services/...` 均位于 `backend/services/src/purchase_order/`。

| Before 符号 | After 落点与符号 | 实际提供方及核对要求 | 核对结果 |
| --- | --- | --- | --- |
| `review.rs::PurchaseOrderService::prepare_formalized_order` | `backend/crates/erp-processes/src/procure_to_pay/review.rs::PurchaseOrderProcess::prepare_formalized_order` | 采购订单、提交、提交行仍由 `erp_procurement::repository::PurchaseOrderExt` 提供；保持原加载及预校验顺序 | 一致 |
| `formalization.rs::{next_revision_no, build_effective_revision, build_change_revision}` | `backend/crates/erp-procurement/src/service/purchase_order/formalization.rs` 中同名 `PurchaseOrderService` 方法 | 当前采购版本查询与 `PurchaseOrderRevision` / `PurchaseOrderRevisionLine` 原构造方法 | 一致 |
| `review.rs::ensure_purchase_review_sources` | 采购域 `formalization.rs::ensure_review_sources` | 原函数实际仅调用 `submission.ensure_line_totals(lines)`；原 `db/order/executor` 参数未参与规则 | 移为同步纯规则，校验及错误保持一致 |
| `allocation_maintenance.rs::prepare_current_sales_allocations` | process 同名函数 → 采购域同名函数 → `SalesAllocationPort::current_lines` | `procure_to_pay/adapters/sales_allocation.rs::SalesAllocationAdapter` 实际调用销售 `sales_orders().find_by_id`、`sales_order_revision_lines().list_lines_by_revision` | 读取顺序、版本指针、字段映射一致 |
| `allocation_maintenance.rs::persist_current_sales_allocations` | `backend/crates/erp-procurement/src/service/purchase_order/allocation_maintenance.rs` 中同名函数 | 采购 `purchase_line_sales_allocations().create`，按已准备的 allocation 顺序逐行写入 | 一致 |
| `review.rs::build_payable` | process 同名函数 → `backend/crates/erp-finance/src/service/payable/purchase_initial.rs::prepare` | 原 `PayableAccount::new` 与 `PayableEntry::new`；持久化使用同文件 `persist` 调用 `create_payable_with_entry` | 构造数据、身份顺序与写入一致 |
| `review.rs::build_confirmed_cost_entries` | process 同名函数 → `backend/crates/erp-finance/src/service/cost/purchase_initial.rs::{prepare,persist}` | 原 `CostEntry::new`；逐行 `create_cost_entry_with_allocations(entry, Vec::new(), executor)` | 原行序、金额与成本分类一致 |
| `review.rs::FormalizedOrderPersist::persist_order` | process 同名方法 → 采购域 `formalization.rs::persist_formalized_order(db, FormalizedOrderWrite, actor_id, executor)` | `FormalizedOrderWrite` 在原调用点打包订单、提交、版本、版本行及分配；领域入口解包后调用原采购仓储 | 内部写序一致 |
| Before `backend/crates/erp-processes/src/procure_to_pay/mod.rs::persist_formalized_order_write` | `formalization_root.rs::persist_formalized_order_write` → `formalization_posting.rs::post` | 实际生产入口 `execute` 驱动 `MongoPosting::apply`；财务、付款任务、履约、审计提供方见第 3 节 | 外层写序一致 |

## 3. 根事务及写入顺序合同

### 3.1 根事务与准备读取

1. `formalize_approved_order` 仍先调用 `prepare_formalized_order`，完成准备后进入原 `with_transaction`。
2. `formalize_approved_order_in_transaction` 仍接收审批运行时的 `ClientSession`，不另建事务。
3. 准备函数中原有订单、提交、提交行、版本序号查询继续使用 `NoTransaction`。不得把这些准备读取描述为已经改用根事务快照。
4. `persist_formalized_order_write` 仍在采购写入之前构造 `purchase_order.formalize` 审计对象；审计持久化仍位于全部业务写入之后。
5. 事务内采购、销售分配读取、应付、付款任务、成本、履约和审计沿同一个传入执行器调用。部分 leaf 参数从 `&mut ClientSession` 改为 `&mut dyn Executor`，没有在这些 leaf 中新增事务或执行器实例。

### 3.2 外层写段

| 次序 | Before | After 实际生产调用 | 核对结果 |
| --- | --- | --- | --- |
| 1 | `persist.persist_order` | `Step::Purchase` → 同名方法 → 采购域写入方法 | 一致 |
| 2 | `db.payable().create_payable_with_entry` | `Step::Payable` → 财务 `payable::purchase_initial::persist` → 原仓储方法 | 一致 |
| 3 | `ensure_purchase_payment_task` | `Step::PaymentTask` → `crate::finance_posting::payable::payment_task::ensure_purchase_payment_task` | 同一实际提供方 |
| 4 | 按 `effects.cost_entries()` 原顺序逐条写成本及空分配 | `Step::Costs` → 财务 `cost::purchase_initial::persist` → 相同逐行仓储调用 | 一致 |
| 5 | `effects.persist_fulfillment` | `Step::Fulfillment` → 同名方法 | 一致 |
| 6 | `db.audit_logs().create` | `Step::Audit` → 原审计仓储方法 | 一致 |

每个步骤仍通过 `?` 传播首个失败；失败后不执行后续步骤。本项为生产调用图核对结果，不构成数据库回滚或测试运行证明。

### 3.3 采购内部写段

以下顺序在 Before 与 After 保持一致：

1. 校验冻结提交与行金额合计。
2. 读取来源销售单、其当前版本指针、该版本下的销售行。
3. 按采购商品/服务行顺序生成 allocation ID，构造分配计划并回填采购版本行引用。
4. 创建采购生效版本及版本行。
5. 逐行写入已准备的 allocation。
6. 调用订单 `formalize_with_revision`。
7. 调用提交 `record_review(Approved { comment: None }, Instant::now(), actor_id)`。
8. 更新采购提交，执行原版本 CAS。
9. 更新采购订单，执行原版本 CAS。

### 3.4 领域写入入口合同

当前采购领域入口固定为 `persist_formalized_order(db, write: FormalizedOrderWrite, actor_id, executor)`。原八个参数中的 `order`、`submission`、`revision`、`revision_lines`、`allocations` 收拢为 `FormalizedOrderWrite`；调用方仍在金额校验和销售分配准备完成后的原位置打包并立即调用。

领域入口仅解包结构体，然后执行第 3.3 节中的既有采购写段。结构体打包、解包不查询仓储、不生成时间或 ID、不执行新增校验；版本与分配借用保持原对象，订单与提交仍按原方式移动。此次参数收拢未改变首错或写序。

## 4. 时间、ID 与数据事实合同

| 对象 | 保留的生成及消费位置 | 核对结果 |
| --- | --- | --- |
| 采购生效版本 | 先校验提交归属，再生成版本 ID，并在原 `from_submission` 实参位置调用 `Instant::now()`；随后按提交原行序生成版本行 ID | 一致 |
| 原始应付 | 先按商品/服务行求最晚预计交付日，以原 `BusinessDate::today()` 计算到期日；计算成功后生成账户 ID，再生成分录 ID，分录 `posted_at` 仍在原构造位置调用 `Instant::now()` | 一致 |
| 确认成本 | 保持提交原行序；每行先取得税率或零税率，再生成成本 ID，在成本数据构造位置调用 `Instant::now()` | 一致 |
| 成本事实映射 | 新增的 `PurchaseCostLine` 映射只复制原 ID、金额、税率及物流行标记；不生成时间或 ID，不增加可返回错误的规则 | 未改变可观察步骤顺序 |
| 销售分配 | 销售事实读取成功后，才按采购商品/服务行顺序生成 allocation ID；不从历史销售版本补查 | 一致 |
| 审核结论 | `record_review` 的时间仍在版本及 allocation 写入之后、提交与订单 CAS 之前取得 | 一致 |
| 履约草稿 | 仓库、供应商直发、服务三分支沿用原分支条件、文档号提供方、逐行 ID、服务统一 `now` 与 `fact_no` 位置 | 一致 |

`create_receipt_draft_for_order`、`create_delivery_draft_for_order`、`create_service_fulfillment_draft_for_order` 已对当前函数体完成定向比较：归一化采购实体及服务命名空间、履约根导出路径和空白后，三者均与 Before 相等。

当前实际调用 `services::fulfillment::{next_purchase_receipt_no, next_delivery_no, ensure_fulfillment_task, FulfillmentTaskObject}`。`backend/services/src/fulfillment/mod.rs` 通过窄 `pub use` 导出原 `document_number` 和 `task` 符号，未新增转发函数或第二实现；文档号与履约任务的实际提供方保持原函数，现有履约仓储调用不变。

## 5. 付款条件回调与首错合同

真实付款条件提供方为 `erp_supplier::SupplierPaymentTerm`。组合适配器位于 `backend/crates/erp-processes/src/procure_to_pay/adapters/payment_term.rs`：

- `parse` 调用原 `SupplierPaymentTerm::parse`，输出 `canonical_code`、`prepay_gate`、`days_after_delivery` 三项消费事实。
- `parse_snapshot` 先调用原 `split_encoded_payment_term_snapshot`，再调用 `parse`。

| 消费符号 | Before 执行位置 | After 执行位置 | 首错核对 |
| --- | --- | --- | --- |
| `PurchaseOrder::new` | 采购单号及付款文本规范化之后，创建依据、责任人和目标仓库校验之前解析付款条件 | 相同位置调用 `resolve_term`，实际传入 `parse` | 保持原序 |
| `PurchaseOrder::update` / `apply_payment_term` | 先 `ensure_draft`；只有提供付款更新值时才规范化并解析；随后处理其他字段并 touch | 相同位置调用 `resolve_term`；未提供付款值时不调用回调 | 保持原序 |
| `PaymentTermSnapshot::new` | 文本规范化后先拆历史编码并解析，再校验先款门禁及金额/比例门槛 | 相同位置调用实际 `parse_snapshot` 回调，再执行原门禁与门槛校验 | 保持原序 |
| `PaymentTermSnapshot::payable_due_date` | 先拆历史编码并解析，再判断是否后付；后付条件随后校验预计交付日和日期范围 | 相同位置调用实际 `parse_snapshot`，继续原分支与日期计算 | 保持原序 |
| `review.rs::build_payable` | 到期日计算失败时，尚未生成应付账户或分录 ID | 回调和到期日计算仍在财务 `purchase_initial::prepare` 之前 | 保持原序 |
| `shared.rs::payment_term_snapshot` | 先直接调用 `SupplierPaymentTerm::parse`，再将规范代码、门禁和空金额/比例门槛传入 `PaymentTermSnapshot::new` | 原调用位置改为统一 `adapters::payment_term::parse`；使用其 `canonical_code` 和 `prepay_gate` 构造快照，实体回调仍为 `parse_snapshot` | 保留原有前置解析和实体解析次序 |

迁后未新增提前解析。`shared::payment_term_snapshot` 保留原有的快照构造前解析，只将同一提供方调用集中至统一适配器；采购实体和快照内部的解析回调仍在各自原位置。未知条件、先款门禁不一致、负门槛、后付缺少预计交付日的相对失败顺序保持原实现。

## 6. 错误映射合同

1. 金额合计和销售来源规则继续映射为原 `BusinessLogicError` 或 `NotFound`，文本不变。
2. `CurrentSalesAllocationPlanError::InvalidAllocation` 继续映射为 `Logic`；ID 数量不匹配继续映射为 `Internal`；其余分配规则继续映射为 `BusinessLogicError`。
3. 财务实体构造的原 `erp_core::Error` 经 `erp_finance::Error::Logic` 后，仍逐项映射为 `services::Error::Logic`。
4. 采购与财务领域错误在 `backend/services/src/errors.rs` 中按枚举变体映射，保留冲突、瞬态事务、结果未知及仓储错误类别。
5. 本链路涉及的采购、应付及成本唯一键冲突、乐观锁冲突文本与 Before 一致。此结论限于本链路，不外推至未核对的领域。

## 7. After 文件指纹

以下 SHA-256 在本证据登记时从当前未提交工作树计算。内容变化后，执行负责人必须核对差异是否影响本证据结论；指纹仅标识文件内容，不代替测试或运行验收。

| After 文件 | SHA-256 |
| --- | --- |
| `backend/crates/erp-processes/src/procure_to_pay/review.rs` | `1dd8df208a617727e8d20ad8e67ba133aa37cb5a7d283e02860235edfcba5638` |
| `backend/crates/erp-processes/src/procure_to_pay/formalization_root.rs` | `a8e59e53a8a201476586a914115253d75d3a30944b2fe1c344c26181582e1578` |
| `backend/crates/erp-processes/src/procure_to_pay/formalization_posting.rs` | `5a9fb16a0e7d520dc5f246bfd0e28e86232b0cbc1e870a5b1a4486b9c899ecc8` |
| `backend/crates/erp-processes/src/procure_to_pay/allocation_maintenance.rs` | `54693c4300f9aa79dff158562aa1c7faac89cab1eec6d4207a45932e599cdcd2` |
| `backend/crates/erp-processes/src/procure_to_pay/adapters/sales_allocation.rs` | `b37faa644c66e10bc8c5f3ab85e0a12aa4ef85cc784c71890227d7e3b7d7f113` |
| `backend/crates/erp-processes/src/procure_to_pay/adapters/payment_term.rs` | `9648515edf15c3c5299887d81361ab338daeaf794195dea42563eaefd98d258e` |
| `backend/crates/erp-procurement/src/service/purchase_order/formalization.rs` | `c99b65b07c7f118b99654c38de4297c1d375a5bf7fb6430539392eeb40df2bbe` |
| `backend/crates/erp-procurement/src/service/purchase_order/allocation_maintenance.rs` | `e5062fc79a5c32283b8b754fd5162c5c3863cda753ab926df6c093f26b37737a` |
| `backend/crates/erp-procurement/src/entity/purchase_order/order.rs` | `4f682c166c60e65fe2ce1e738913273151e7937406767d3fb36f59e824a10ac9` |
| `backend/crates/erp-procurement/src/entity/purchase_order/snapshot.rs` | `74fa74de882f6cc8b5055c7e0a9baa5e5b725f2fad0108722dea3a973233465b` |
| `backend/crates/erp-finance/src/service/payable/purchase_initial.rs` | `ec703accdf5024376fd6c62ab9bb34cc25b435f5d55b8ab77af681e2e308af05` |
| `backend/crates/erp-finance/src/service/cost/purchase_initial.rs` | `cf93450aa42533e66cfb5c255966fd85c724c8ac0ef42e20452f2cc8cfbe9184` |
| `backend/services/src/errors.rs` | `f5fa4957c5907321e0b6fec2b1584b15da11a17e739a6e204b6c3f157a2ab97f` |

## 8. 验收登记规则

- 本证据可登记为“正式化静态语义核对完成；范围内未发现漂移”。
- Cargo、纯内联测试及统一门禁结果必须引用集成负责人实际执行的独立日志。
- 非零大小执行器替身和生产步骤测试的源码存在，不等于这些测试已经在本核对中执行。
- 真实数据库运行未验证；不得据此声明真实 MongoDB 事务回滚、并发或提交结果恢复已经验证。
