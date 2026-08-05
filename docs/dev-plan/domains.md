# 域切分矩阵

## 1. 读法

- **域（D01–D34）**是所有权的最小单位：一个域拥有一组表、一个 `entities` 模块目录、
  一个 `repository` 模块、一组 `indexes` 函数、一个 `services` 模块目录、一个 `routes` 文件。
- **批次（G1–G12）**是 worktree 的默认粒度：一个批次通常由一人一次交付。
  若想提高并行度，**可以按域进一步拆分批次**（域之间文件不重叠，天然安全）；
  但不允许把两个批次合并成一个 PR，那会放大回滚半径。
- 表名以 `docs/erp-data-model.md` 第 5 章与第 6 章为准，本表只列归属，不复述字段。
- "依赖域"仅对 **P3 服务层**生效（P1/P2 层无跨域依赖，见 README 3.2）。

模块路径统一规则（`<domain>` = 下表"模块名"）：

```
backend/entities/src/<domain>/                    实体与值对象
backend/database/src/repository/<domain>.rs       仓储（大域可为目录）
backend/database/src/indexes/<domain>.rs          索引声明
backend/database/src/repository/extensions/<domain>.rs   DatabaseExt 分片
backend/services/src/<domain>/                    服务与 dto.rs
backend/apps/web-api/src/core/handler/<domain>/   handler
backend/apps/web-api/src/core/routes/<domain>.rs  路由与权限挂载
```

---

## 2. 一期：平台与单据基础设施（批次 G1）

| 域 | 模块名 | 主要表 | 依赖域 | 页面 |
| --- | --- | --- | --- | --- |
| D01 | `source_registry` | `source_system`、`external_identity_map`、`external_identity_target` | — | W17、W29 |
| D02 | `document_registry` | `business_document`、`document_relation`、`document_participant`、`workflow_action` | D01 | 全部单据页 |
| D03 | `work_item` | `work_item` | D02 | W01、W02 |
| D04 | `bulk_job` | `bulk_selection_snapshot`、`bulk_selection_item`、`background_job`、`background_job_item` | D02 | W02、W18 |
| D05 | `file_asset` | `file_asset`、`document_attachment` | D02 | W04、W18 |
| D06 | `access_control` | `role`、`permission`、`user_role`、`data_scope`、`audit_event` | — | W19 |

> D01 是 P0 的垂直样板域。
> D06 已有 Casbin/RBAC/审计基线（`entities/src/rbac.rs`、`role.rs`、`audit_log.rs`、
> `database/src/casbin_adapter.rs`、`services/src/iam/`），本域只做 `data_scope` 增补
> 与 `audit_log → audit_event` 字段对齐，**不重建**已有能力。

---

## 3. 一期：基础资料（批次 G2、G3）

| 批次 | 域 | 模块名 | 主要表 | 依赖域 | 页面 |
| --- | --- | --- | --- | --- | --- |
| G2 | D07 | `party` | `party`、`party_revision`、`party_contact`、`party_address`、`party_tax_profile`、`party_bank_account` | D01 | W14、W03 |
| G2 | D08 | `customer` | `customer_account`、`customer_assignment` | D07、D06 | W03、W15 |
| G2 | D09 | `supplier` | `supplier_account`、`supplier_commercial_profile_revision`、`supplier_capability`(+`_revision`)、`supplier_qualification`(+`_revision`)、`supplier_qualification_capability`、`supplier_rating_revision` | D07、D05 | W14 |
| G3 | D10 | `catalog` | `product_category`、`product_brand`、`unit_of_measure`、`sku_attribute`、`sku_attribute_value`、`product_category_attribute`、`product`(+`_revision`、`_revision_media`)、`sku`(+`_revision`)、`sku_revision_attribute_value`、`voucher_category_profile_revision` | D05 | W14 |
| G3 | D11 | `warehouse` | `warehouse`、`warehouse_revision`、`warehouse_sku_policy` | — | W14 |

---

## 4. 一期：业务单据与台账（批次 G4–G8）

| 批次 | 域 | 模块名 | 主要表 | 依赖域 | 页面 |
| --- | --- | --- | --- | --- | --- |
| G4 | D12 | `contract` | `contract`、`contract_revision` | D08、D05 | W04 |
| G4 | D13 | `sales_order` | `sales_order`、`sales_order_line`、`sales_order_working_copy`(+`_line`)、`sales_order_submission`(+`_line`)、`sales_order_revision`(+`_line`)、`sales_order_goods_service_line_revision`、`sales_order_voucher_line_revision` | D08、D10、D12、D02 | W05 |
| G4 | D14 | `sales_review` | `sales_order_review`、`procurement_confirmation`(+`_line`)、`sales_change_order`、`sales_change_submission`(+`_line`)、`sales_change_review` | D13、D03、D18 | W05、W07 |
| G5 | D15 | `purchase_order` | `purchase_order`、`purchase_order_submission`(+`_line`)、`purchase_order_revision`(+`_line`)、`purchase_line_sales_allocation`、`purchase_change_order`、`purchase_change_submission`(+`_line`) | D09、D14、D24、D19、D20 | W08 |
| G5 | D24 | `supplier_catalog` | `supplier_catalog_product`(+`_revision`、`_revision_media`)、`supplier_catalog_sku`(+`_revision`)、`supplier_product_mapping`、`supplier_catalog_intake_batch`(+`_item`)、`supplier_offering`(+`_revision`) | D09、D10 | W21 |
| G6 | D16 | `fulfillment` | `purchase_receipt`(+`_line`)、`delivery`(+`_line`)、`electronic_delivery`、`service_fulfillment`、`customer_acceptance`(+`_line`)、`acceptance_fulfillment_allocation` | D15、D17、D13 | W06、W09 |
| G6 | D17 | `inventory` | `stock_movement`、`stock_balance`、`stock_reservation`(+`_entry`)、`stock_adjustment`(+`_line`) | D11、D10 | W10 |
| G7 | D18 | `receivable` | `receivable_account`、`receivable_entry`、`receivable_funds_review`、`receivable_entry_offset`、`customer_receipt`、`receipt_allocation`、`invoice`、`sales_invoice_allocation` | D13、D08 | W11、W13 |
| G7 | D19 | `payable` | `payable_account`、`payable_entry`、`payable_entry_offset`、`supplier_payment`、`payment_allocation`、`purchase_invoice_allocation` | D15、D09 | W12 |
| G7 | D20 | `cost` | `cost_entry`、`cost_allocation` | D15、D16、D13 | W16 |
| G7 | D21 | `returns` | `sales_return_case`、`sales_return_line`、`purchase_return_order`、`purchase_return_line`、`customer_refund`、`supplier_refund`、`receipt_reversal`、`payment_reversal` | D16、D17、D18、D19 | W05、W09、W11、W12 |
| G8 | D22 | `legacy_import` | `legacy_import_batch`、`legacy_import_row`、`legacy_import_confirmation` | D04、D05、D07 | W18 |
| G8 | D23 | `mall_sync` | `mall_sales_sync_job`、`mall_sales_sync_cursor`、`mall_sales_order_snapshot`、`mall_sales_reconciliation_job`(+`_item`)、`master_mapping_task` | D01、D13、D08 | W17 |

> `invoice` 由 D18 拥有实体与仓储；D19 只拥有 `purchase_invoice_allocation`。
> 这是唯一一处跨批次共享聚合，D19 的实施者必须在 P3 通过 D18 的 Repository 访问 `invoice`，
> 不得复制发票实体。

---

## 5. 二期扩展（批次 G9–G12）

| 批次 | 域 | 模块名 | 主要表 | 依赖域 | 页面 |
| --- | --- | --- | --- | --- | --- |
| G9 | D25 | `supplier_api` | `supplier_api_connection`、`supplier_api_capability` | D09、D01 | W20 |
| G9 | D26 | `publication` | `product_publication`(+`_revision`、`_revision_media`)、`product_publication_delivery` | D10、D24 | W22 |
| G9 | D27 | `projection` | `sales_order_projection`(+`_revision`、`_delivery`) | D13、D14 | W23 |
| G10 | D28 | `card_instance` | `mall_consumption_cutover`、`mall_card_instance`(+`_correction`)、`mall_balance_snapshot` | D13、D27 | W28 |
| G10 | D29 | `mall_order` | `mall_order_fact`、`mall_order_cancel_fact`、`mall_order_completion_fact`、`mall_order`、`mall_order_item`、`mall_payment_source`、`mall_item_funding_allocation`、`mall_consumption_entry`、`mall_consumption_cost_assessment` | D28、D26、D20 | W25、W28 |
| G10 | D30 | `mall_after_sales` | `mall_after_sales_request`(+`_line`)、`mall_refund`(+`_line`)、`mall_refund_allocation`、`mall_balance_restoration`(+`_allocation`) | D29、D28 | W25 |
| G10 | D31 | `mall_backfill` | `mall_consumption_backfill_job`、`mall_consumption_backfill_item` | D29、D04 | W30 |
| G11 | D32 | `supplier_fulfillment` | `supplier_fulfillment_order`、`supplier_fulfillment_item`、`supplier_order_action`(+`_line`)、`supplier_order_status_history`、`supplier_refund_fact`、`supplier_refund_allocation` | D25、D29、D24 | W26 |
| G11 | D33 | `supplier_settlement` | `supplier_settlement_statement`、`supplier_settlement_item`、`supplier_settlement_difference` | D32、D19 | W27 |
| G12 | D34 | `integration_ops` | `inbox_message`、`integration_error_task`、`reconciliation_difference`(+`_resolution`) | D01、D29、D32 | W29 |

> 集成表是普通表组。禁止实现 outbox、消息中间件或投递状态机（数据模型 5.4 末条）。
> 商城主动拉取与核对继续使用 D23 的专用表。

---

## 6. 页面 ↔ 域 反查（P4 用）

| 页面 | 名称 | `erp-client` feature | 后端域 |
| --- | --- | --- | --- |
| W01 | 今日工作台 | `workspace` | D03、投影（P5） |
| W02 | 待办队列（统一） | `unified-task-queue` | D03、D04 |
| W03 | 客户中心 | `customers` | D07、D08 |
| W04 | 合同 | `contracts` | D12、D05 |
| W05 | 销售单（统一） | `sales-orders` | D13、D14、D21 |
| W06 | 客户验收 | `sales-orders`（验收工作台） | D16 |
| W07 | 二次确认队列 | `procurement-confirmation` | D14 |
| W08 | 采购单 | `purchase-orders` | D15 |
| W09 | 收货与发货 / 交付与代发 | `fulfillment-operations` | D16 |
| W10 | 库存台账 | `inventory` | D17 |
| W11 | 客户往来 | `customer-receivables` | D18 |
| W12 | 供应商往来 | `supplier-payables` | D19 |
| W13 | 卡券票款复核 | `card-funds-review` | D18 |
| W14 | 公司商品池、商品、类目、供应商与仓库 | `master-data` | D07、D09、D10、D11 |
| W15 | 客户经营质量 | `customer-quality` | D08、投影（P5） |
| W16 | 实际经营盈亏 | `actual-profit-loss` | D20、投影（P5） |
| W17 | 商城同步与映射 | `mall-sync` | D23、D01 |
| W18 | 导入与期初 | `import-opening` | D22、D04 |
| W19 | 权限与审计 | `access-audit` | D06 |
| W20 | API 供应商连接 | `supplier-api-connections` | D25 |
| W21 | 供应商商品库与供给管理 | `supplier-catalog` | D24 |
| W22 | 商品发布 | `product-publications` | D26 |
| W23 | 执行信息 | `execution-projections` | D27 |
| W25 | 商城消费订单 | `mall-consumption-orders` | D29、D30 |
| W26 | 供应商订单 | `supplier-orders` | D32 |
| W27 | API 结算 | `supplier-settlements` | D33 |
| W28 | 卡券消费台账与经营分析 | `card-business-analytics` | D28、D29、投影（P5） |
| W29 | 接口错误与对账中心 | `integration-errors` | D34 |
| W30 | 历史消费回填 | `history-backfill` | D31 |
