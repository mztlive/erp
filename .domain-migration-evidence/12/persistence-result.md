# 阶段 12 实体与持久化交付合同

## 1. 输入与范围

- 执行树：`/private/tmp/erp-domain-crate-12-fulfillment`；输入源码：`0a297ab0`。
- 本分片迁入并删除 29 个原文件：15 个实体、7 个仓储、1 个访问器、1 个索引、5 个拥有仓储。
- 附加改动仅为财务、销售、库存各一个 `repository/fulfillment_facts.rs` 查询叶；共享注册由唯一集成负责人处理。
- 禁止将本文静态复核记作编译、真实数据库或并发回滚验证。

## 2. 最终出口

- `erp_fulfillment::entity::fulfillment::*`：原履约实体与值对象；原状态、DTO/实体字段与 serde 不变。
- `erp_fulfillment::entity::facts::*`：采购状态、付款门槛、采购版本行、分配关联、收货进度、验收进度、证据级别/保留、预占行与验收销售行/数量事实。
- `erp_fulfillment::repository::FulfillmentExt`：9 个集合关联常量、5 个关联筛选类型、仓储工厂。
- `erp_fulfillment::repository::owned::{CustomerAcceptanceRepository,DeliveryRepository,ElectronicDeliveryRepository,PurchaseReceiptRepository,ServiceFulfillmentRepository}`：5 个拥有仓储。
- `erp_fulfillment::repository::fulfillment::FulfillmentRepository`：本域批量查询、累计合格收货、草稿读取与头行事务内写入。
- `erp_fulfillment::indexes::ensure(db)`：按原 9 集合顺序安装 16 个命名索引。

## 3. 外域查询核销

| 原 FulfillmentRepository 方法 | 最终唯一实现/调用路径 | 固定查询合同 |
| --- | --- | --- |
| `list_payable_accounts_for_purchase_order` | `db.payable_accounts().list_payable_accounts_for_purchase_order`；finance `repository/fulfillment_facts.rs` | 来源采购单 ID + PurchaseOrder 来源类型 + 未删除；返回所有匹配子账，不能替换为现有 find_one |
| `sales_revision_line_for_allocation` | `db.sales_order_revision_lines().sales_revision_line_for_allocation`；sales `repository/fulfillment_facts.rs` | 版本行 ID + 稳定销售行 ID + 未删除；不匹配返回 None |
| `list_sales_revision_lines_by_ids` | 复用 `db.sales_order_revision_lines().list_active_by_ids(&[String], executor)` | ID `$in` + 未删除；空集合直接返回空；默认 FindOptions |
| `list_stock_reservations_for_receipt_lines` | `db.stock_reservations().list_stock_reservations_for_receipt_lines`；inventory `repository/fulfillment_facts.rs` | source_receipt_line_id `$in` + 未删除；空集合返回空；无额外状态过滤 |
| `list_sales_order_lines_by_ids` | 复用 `db.sales_order_lines().list_active_by_ids(&[String], executor)` | 稳定行 ID `$in` + 未删除；空集合直接返回空；默认 FindOptions |

三个新增 provider 叶保持原注释与过滤值。所有入口接收原 `&mut dyn Executor`，不创建事务。

## 4. 原文件、公开符号和测试去向

表内列出原文件测试入口数，不代表运行结果。6 个 `serde_shapes_and_bson_roundtrip` 测试从实体整段迁入 `repository/serialization_contract.rs` 的同名子模块；原测试函数名、断言和夹具均保留。对应实体测试模块与夹具仅 `cfg(test)` 下可在 crate 内访问，没有第二份夹具实现。

| 原文件 | 新文件 | 原测试数 | 原公开符号去向 |
| --- | --- | ---: | --- |
| `entities/src/fulfillment/acceptance_eligibility.rs` | `crates/erp-fulfillment/src/entity/fulfillment/acceptance_eligibility.rs` | 9 | `AcceptanceFactEligibility`, `from_fact`, `AcceptanceLineEligibility`, `from_facts`, `is_fully_fulfilled`, `has_acceptance`, `has_remaining_eligible`, `AcceptanceProgress`, `derive` |
| `entities/src/fulfillment/acceptance_fulfillment_allocation.rs` | `crates/erp-fulfillment/src/entity/fulfillment/acceptance_fulfillment_allocation.rs` | 7 | `FulfillmentFactType`, `label`, `as_str`, `AllocationAction`, `AcceptanceFulfillmentAllocationData`, `AcceptanceFulfillmentAllocation`, `new`, `net_quantity_for_fact`, `eligible_quantity_for_fact`, `ensure_apply_within_successful_quantity`, `ensure_reversible_source` |
| `entities/src/fulfillment/customer_acceptance.rs` | `crates/erp-fulfillment/src/entity/fulfillment/customer_acceptance.rs` | 8 | `CustomerAcceptanceState`, `label`, `as_str`, `is_editable`, `AcceptanceResult`, `CustomerAcceptanceData`, `CustomerAcceptanceUpdate`, `CustomerAcceptance`, `new`, `update`, `matches_business_identity`, `is_posted`, `ensure_draft_version`, `ensure_draft`, `ensure_posting_lines`, `ensure_reversible`, `mark_posted`, `reverse`, `CustomerAcceptanceLineData`, `CustomerAcceptanceLine`, `ensure_allocation_conserved` |
| `entities/src/fulfillment/customer_acceptance_line_batch.rs` | `crates/erp-fulfillment/src/entity/fulfillment/customer_acceptance_line_batch.rs` | 4 | `CustomerAcceptanceLineSpec`, `CustomerAcceptanceLineBatch`, `build` |
| `entities/src/fulfillment/delivery.rs` | `crates/erp-fulfillment/src/entity/fulfillment/delivery.rs` | 10 | `DeliveryState`, `label`, `as_str`, `is_editable`, `acceptance_eligible_states`, `is_acceptance_eligible`, `DeliveryType`, `DeliveryData`, `DeliveryUpdate`, `Delivery`, `address_snapshot_fingerprint`, `new`, `registration_context_id`, `acceptance_quantity`, `update`, `mark_shipped`, `mark_signed`, `reverse`, `DeliveryLineData`, `DeliveryLine` |
| `entities/src/fulfillment/delivery_line_batch.rs` | `crates/erp-fulfillment/src/entity/fulfillment/delivery_line_batch.rs` | 5 | `DeliveryLineSpec`, `DeliveryLineBatch`, `build` |
| `entities/src/fulfillment/electronic_delivery.rs` | `crates/erp-fulfillment/src/entity/fulfillment/electronic_delivery.rs` | 7 | `ElectronicDeliveryState`, `label`, `as_str`, `is_editable`, `is_confirmable`, `is_acceptance_eligible`, `FulfillmentResult`, `ElectronicDeliveryData`, `ElectronicDeliveryUpdate`, `ElectronicDelivery`, `recipient_snapshot_fingerprint`, `new`, `registration_context_id`, `ensure_confirmable`, `acceptance_quantity`, `update`, `confirm`, `reverse` |
| `entities/src/fulfillment/electronic_delivery_draft.rs` | `crates/erp-fulfillment/src/entity/fulfillment/electronic_delivery_draft.rs` | 3 | `ElectronicRecipientFingerprint`, `from_precomputed`, `as_str`, `ElectronicDeliveryDraftData`, `ElectronicDeliveryDraft`, `build` |
| `entities/src/fulfillment/fingerprint.rs` | `crates/erp-fulfillment/src/entity/fulfillment/fingerprint.rs` | 4 | `hmac_sha256_hex`, `validate_fingerprint` |
| `entities/src/fulfillment/mod.rs` | `crates/erp-fulfillment/src/entity/fulfillment/mod.rs` | 0 |  |
| `entities/src/fulfillment/purchase_receipt.rs` | `crates/erp-fulfillment/src/entity/fulfillment/purchase_receipt.rs` | 11 | `PurchaseFulfillmentEligibility`, `ensure_order_fulfillable`, `ensure_prepayment_satisfied`, `ensure_allocation_consistent`, `PurchaseReceiptState`, `label`, `as_str`, `is_editable`, `QualityResult`, `from_quantities`, `PurchaseReceiptData`, `PurchaseReceiptUpdate`, `PurchaseReceipt`, `new`, `update`, `ensure_draft_version`, `ensure_posting_lines`, `fulfillment_progress`, `mark_posted`, `reverse`, `PurchaseReceiptLineData`, `PurchaseReceiptLine`, `posting_quantity`, `ensure_within_revision`, `reservation_shares` |
| `entities/src/fulfillment/purchase_receipt_line_batch.rs` | `crates/erp-fulfillment/src/entity/fulfillment/purchase_receipt_line_batch.rs` | 3 | `PurchaseReceiptLineSpec`, `PurchaseReceiptLineBatch`, `build` |
| `entities/src/fulfillment/service_evidence.rs` | `crates/erp-fulfillment/src/entity/fulfillment/service_evidence.rs` | 7 | `ActualServiceLocation`, `parse`, `as_str`, `ServiceEvidencePolicy`, `validate` |
| `entities/src/fulfillment/service_fulfillment.rs` | `crates/erp-fulfillment/src/entity/fulfillment/service_fulfillment.rs` | 9 | `ServiceFulfillmentState`, `label`, `as_str`, `is_editable`, `is_confirmable`, `is_acceptance_eligible`, `ServiceFulfillmentData`, `ServiceFulfillmentUpdate`, `ServiceFulfillmentConfirmation`, `ServiceFulfillmentConfirmationParams`, `new`, `ServiceFulfillment`, `recipient_snapshot_fingerprint`, `service_location_fingerprint`, `registration_context_id`, `ensure_confirmable`, `ensure_draft_version`, `ensure_evidence_present`, `acceptance_quantity`, `update`, `apply_confirmation`, `confirm`, `reverse` |
| `entities/src/fulfillment/service_fulfillment_draft.rs` | `crates/erp-fulfillment/src/entity/fulfillment/service_fulfillment_draft.rs` | 3 | `ServiceRecipientFingerprint`, `from_precomputed`, `as_str`, `ServiceLocationFingerprint`, `ServiceFulfillmentDraftData`, `ServiceFulfillmentDraft`, `build` |
| `database/src/repository/fulfillment/customer_acceptance.rs` | `crates/erp-fulfillment/src/repository/fulfillment/customer_acceptance.rs` | 2 | `CustomerAcceptanceRow`, `CustomerAcceptanceFilter`, `find_by_acceptance_no`, `search_customer_acceptances` |
| `database/src/repository/fulfillment/delivery.rs` | `crates/erp-fulfillment/src/repository/fulfillment/delivery.rs` | 2 | `DeliveryRow`, `DeliveryFilter`, `search_deliveries` |
| `database/src/repository/fulfillment/electronic_delivery.rs` | `crates/erp-fulfillment/src/repository/fulfillment/electronic_delivery.rs` | 2 | `ElectronicDeliveryRow`, `ElectronicDeliveryFilter`, `search_electronic_deliveries` |
| `database/src/repository/fulfillment/purchase_receipt.rs` | `crates/erp-fulfillment/src/repository/fulfillment/purchase_receipt.rs` | 2 | `PurchaseReceiptRow`, `PurchaseReceiptFilter`, `search_purchase_receipts`, `find_by_receipt_no` |
| `database/src/repository/fulfillment/purchase_receipt_totals.rs` | `crates/erp-fulfillment/src/repository/fulfillment/purchase_receipt_totals.rs` | 3 | `load_qualified_received_totals` |
| `database/src/repository/fulfillment/service_fulfillment.rs` | `crates/erp-fulfillment/src/repository/fulfillment/service_fulfillment.rs` | 2 | `ServiceFulfillmentRow`, `ServiceFulfillmentFilter`, `search_service_fulfillments` |
| `database/src/repository/fulfillment/mod.rs` | `crates/erp-fulfillment/src/repository/fulfillment/mod.rs` | 3 | `FulfillmentRepository`, `new`, `list_acceptance_eligible_deliveries`, `list_confirmed_electronic_deliveries`, `list_confirmed_service_fulfillments`, `list_customer_acceptance_history`, `list_deliveries_by_ids`, `list_electronic_deliveries_by_ids`, `list_service_fulfillments_by_ids`, `qualified_received_totals_by_purchase_revision_line`, `draft_delivery_for_sales_order`, `draft_warehouse_delivery`, `receipt_lines_by_receipt_ids`, `delivery_lines_by_delivery_ids`, `acceptance_lines_by_acceptance_ids`, `allocations_by_acceptance_lines`, `allocations_by_fulfillment_fact`, `create_purchase_receipt_with_lines`, `create_delivery_with_lines`, `create_customer_acceptance_with_lines`, `replace_customer_acceptance_lines` |
| `database/src/repository/owned/customer_acceptance.rs` | `crates/erp-fulfillment/src/repository/owned/customer_acceptance.rs` | 0 | `CustomerAcceptanceRepository`, `new`, `database`, `collection`, `create`, `find_by_id`, `update`, `soft_delete`, `restore`, `list_all`, `find_one_by_field`, `find_one`, `find_many`, `find_many_sorted`, `list_active_by_ids`, `exists`, `search` |
| `database/src/repository/owned/delivery.rs` | `crates/erp-fulfillment/src/repository/owned/delivery.rs` | 0 | `DeliveryRepository`, `new`, `database`, `collection`, `create`, `find_by_id`, `update`, `soft_delete`, `restore`, `list_all`, `find_one_by_field`, `find_one`, `find_many`, `find_many_sorted`, `list_active_by_ids`, `exists`, `search` |
| `database/src/repository/owned/electronic_delivery.rs` | `crates/erp-fulfillment/src/repository/owned/electronic_delivery.rs` | 0 | `ElectronicDeliveryRepository`, `new`, `database`, `collection`, `create`, `find_by_id`, `update`, `soft_delete`, `restore`, `list_all`, `find_one_by_field`, `find_one`, `find_many`, `find_many_sorted`, `list_active_by_ids`, `exists`, `search` |
| `database/src/repository/owned/purchase_receipt.rs` | `crates/erp-fulfillment/src/repository/owned/purchase_receipt.rs` | 0 | `PurchaseReceiptRepository`, `new`, `database`, `collection`, `create`, `find_by_id`, `update`, `soft_delete`, `restore`, `list_all`, `find_one_by_field`, `find_one`, `find_many`, `find_many_sorted`, `list_active_by_ids`, `exists`, `search` |
| `database/src/repository/owned/service_fulfillment.rs` | `crates/erp-fulfillment/src/repository/owned/service_fulfillment.rs` | 0 | `ServiceFulfillmentRepository`, `new`, `database`, `collection`, `create`, `find_by_id`, `update`, `soft_delete`, `restore`, `list_all`, `find_one_by_field`, `find_one`, `find_many`, `find_many_sorted`, `list_active_by_ids`, `exists`, `search` |
| `database/src/repository/extensions/fulfillment.rs` | `crates/erp-fulfillment/src/repository/extensions/fulfillment.rs` | 0 | `FulfillmentExt` |
| `database/src/indexes/fulfillment.rs` | `crates/erp-fulfillment/src/indexes/fulfillment.rs` | 5 | `ensure` |

## 5. 关键合同复核

- 原内联测试 111 个：实体 90、仓储 16、索引 5。迁后原测试仍为 111；其中实体测试 84、仓储原测试 16、仓储迁入序列化测试 6、索引测试 5。
- 新增 1 个 `fulfillment_progress_preserves_total_quantity_contract`：驱动真实 `PurchaseReceipt::fulfillment_progress`，固定多行总收货达到总目标即完成、六位精度未满为部分、空/零目标为部分。当前分片合计 112 个测试入口。
- 原序列化属性逐文件静态比较一致；9 个集合名与全部索引字符串字面量逐项一致；单号和头行唯一性、索引键/顺序/选项不变。
- 收货累计仍为未删除 POSTED 表头 → 未删除行按采购版本行汇总 qualified_quantity；同一 Executor 且保留 Decimal128 溢出错误。
- 头行创建顺序、delete_many → insert_many 替换顺序保持；所有纯仓储方法不另开事务。
- 采购资格规则仍在履约，状态 Effectiveness 映射由组合适配器执行；先款金额门槛先于比例门槛；分配先检查采购当前行，再检查销售两端关联。
- 收货行先检查版本行关联，再检查是否有数量，再检查已收加本次数量上限；采购进度保持总数量求和，未改为逐行收满。
- 验收进度 `derive([])` 仍为 None，跨销售输出仅由 process 显式转换；证据校验保持 MIME → 敏感级别 → 长期保留 → destroyed。
- EvidenceRetention 将提供方两个短期策略统一映射 Other，领域测试验证同一拒绝规则；提供方适配器须分别验证 ThirtyDays/SevenDays 映射，已交 D 分片。

## 6. 静态验证与运行边界

- 定向 `rustfmt --edition 2021 --check`：通过，范围为本分片 entity/repository/indexes 与三个 provider 叶。
- 本分片 Entity 与 Repository 源码外域/旧三层引用扫描：零；Entity BSON/MongoDB 引用：零。
- 原测试名称与数量、原 serde 属性、索引字面量、5 owned 存在性：静态复核通过。
- Cargo：未执行（遵守阶段 11 编译计时隔离）；MongoDB：未运行；环境 ERP_TEST_MONGO_URI：unset。
- 真实数据库运行未验证。最终门禁与全 workspace 失败轨迹由唯一集成负责人统一执行。

## 7. 首轮领域检查修正

- 根代理报告领域检查退出码 0；本分片清理跨域查询迁出后残留的 `PurchaseReceiptLineId` 未使用导入。
- 定向 Rustfmt 检查与 diff whitespace 检查通过；本次仍未运行 Cargo 或 Mongo。
