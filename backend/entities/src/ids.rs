//! 34 个域的 ID newtype（P0-1.1 共享基元任务，P0 冻结后禁止在域内自定义 ID 类型）。
//!
//! 生成规则：稳定主表 → `<Entity>Id`；修订表 → `<Entity>RevisionId`；
//! 行表 → `<Entity>LineId`。实体名 = 表名 PascalCase（`source_system` → `SourceSystemId`，
//! `sales_order_line` → `SalesOrderLineId`）。
//! 值由 `id_generator::next_id()` 产生（UUID v4，32 位十六进制），ID 不承载业务含义
//! （数据模型 4.1）；ID 是透明值对象，不校验格式。
//!
//! # 表 → ID 类型映射（分域，P1 各域实施者照抄）
//!
//! D01 `source_registry`：
//! - `source_system` → `SourceSystemId`
//! - `external_identity_map` → `ExternalIdentityMapId`
//! - `external_identity_target` → `ExternalIdentityTargetId`
//!
//! D02 `document_registry`：
//! - `business_document` → `BusinessDocumentId`
//! - `document_relation` → `DocumentRelationId`
//! - `document_participant` → `DocumentParticipantId`
//! - `workflow_action` → `WorkflowActionId`
//!
//! D03 `work_item`：
//! - `work_item` → `WorkItemId`
//!
//! D04 `bulk_job`：
//! - `bulk_selection_snapshot` → `BulkSelectionSnapshotId`
//! - `bulk_selection_item` → `BulkSelectionItemId`
//! - `background_job` → `BackgroundJobId`
//! - `background_job_item` → `BackgroundJobItemId`
//!
//! D05 `file_asset`：
//! - `file_asset` → `FileAssetId`
//! - `document_attachment` → `DocumentAttachmentId`
//!
//! D06 `access_control`：
//! - `role` → 沿用 `entities::rbac::RoleId`（P0 前已存在且带解析校验，本模块不重复定义）
//! - `permission` → `PermissionId`
//! - `user_role` → `UserRoleId`
//! - `data_scope` → `DataScopeId`
//! - `audit_event` → `AuditEventId`
//!
//! D07 `party`：
//! - `party` → `PartyId`；`party_revision` → `PartyRevisionId`
//! - `party_contact` → `PartyContactId`；`party_address` → `PartyAddressId`
//! - `party_tax_profile` → `PartyTaxProfileId`；`party_bank_account` → `PartyBankAccountId`
//!
//! D08 `customer`：
//! - `customer_account` → `CustomerAccountId`
//! - `customer_assignment` → `CustomerAssignmentId`
//!
//! D09 `supplier`：
//! - `supplier_account` → `SupplierAccountId`
//! - `supplier_commercial_profile_revision` → `SupplierCommercialProfileRevisionId`
//! - `supplier_capability` → `SupplierCapabilityId`
//! - `supplier_capability_revision` → `SupplierCapabilityRevisionId`
//! - `supplier_qualification` → `SupplierQualificationId`
//! - `supplier_qualification_revision` → `SupplierQualificationRevisionId`
//! - `supplier_qualification_capability` → `SupplierQualificationCapabilityId`
//! - `supplier_rating_revision` → `SupplierRatingRevisionId`
//!
//! D10 `catalog`：
//! - `product_category` → `ProductCategoryId`；`product_brand` → `ProductBrandId`
//! - `unit_of_measure` → `UnitOfMeasureId`；`sku_attribute` → `SkuAttributeId`
//! - `sku_attribute_value` → `SkuAttributeValueId`
//! - `product_category_attribute` → `ProductCategoryAttributeId`
//! - `product` → `ProductId`；`product_revision` → `ProductRevisionId`
//! - `product_revision_media` → `ProductRevisionMediaId`
//! - `sku` → `SkuId`；`sku_revision` → `SkuRevisionId`
//! - `sku_revision_attribute_value` → `SkuRevisionAttributeValueId`
//! - `voucher_category_profile_revision` → `VoucherCategoryProfileRevisionId`
//!
//! D11 `warehouse`：
//! - `warehouse` → `WarehouseId`；`warehouse_revision` → `WarehouseRevisionId`
//! - `warehouse_sku_policy` → `WarehouseSkuPolicyId`
//!
//! D12 `contract`：
//! - `contract` → `ContractId`；`contract_revision` → `ContractRevisionId`
//!
//! D13 `sales_order`：
//! - `sales_order` → `SalesOrderId`；`sales_order_line` → `SalesOrderLineId`
//! - `sales_order_working_copy` → `SalesOrderWorkingCopyId`
//! - `sales_order_working_copy_line` → `SalesOrderWorkingCopyLineId`
//! - `sales_order_submission` → `SalesOrderSubmissionId`
//! - `sales_order_submission_line` → `SalesOrderSubmissionLineId`
//! - `sales_order_revision` → `SalesOrderRevisionId`
//! - `sales_order_revision_line` → `SalesOrderRevisionLineId`
//! - `sales_order_goods_service_line_revision` → `SalesOrderGoodsServiceLineRevisionId`
//! - `sales_order_voucher_line_revision` → `SalesOrderVoucherLineRevisionId`
//!
//! D14 `sales_review`：
//! - `sales_order_review` → `SalesOrderReviewId`
//! - `procurement_confirmation` → `ProcurementConfirmationId`
//! - `procurement_confirmation_line` → `ProcurementConfirmationLineId`
//! - `sales_change_order` → `SalesChangeOrderId`
//! - `sales_change_submission` → `SalesChangeSubmissionId`
//! - `sales_change_submission_line` → `SalesChangeSubmissionLineId`
//! - `sales_change_review` → `SalesChangeReviewId`
//!
//! D15 `purchase_order`：
//! - `purchase_order` → `PurchaseOrderId`
//! - `purchase_order_submission` → `PurchaseOrderSubmissionId`
//! - `purchase_order_submission_line` → `PurchaseOrderSubmissionLineId`
//! - `purchase_order_revision` → `PurchaseOrderRevisionId`
//! - `purchase_order_revision_line` → `PurchaseOrderRevisionLineId`
//! - `purchase_line_sales_allocation` → `PurchaseLineSalesAllocationId`
//! - `purchase_change_order` → `PurchaseChangeOrderId`
//! - `purchase_change_submission` → `PurchaseChangeSubmissionId`
//! - `purchase_change_submission_line` → `PurchaseChangeSubmissionLineId`
//!
//! D16 `fulfillment`：
//! - `purchase_receipt` → `PurchaseReceiptId`；`purchase_receipt_line` → `PurchaseReceiptLineId`
//! - `delivery` → `DeliveryId`；`delivery_line` → `DeliveryLineId`
//! - `electronic_delivery` → `ElectronicDeliveryId`
//! - `service_fulfillment` → `ServiceFulfillmentId`
//! - `customer_acceptance` → `CustomerAcceptanceId`
//! - `customer_acceptance_line` → `CustomerAcceptanceLineId`
//! - `acceptance_fulfillment_allocation` → `AcceptanceFulfillmentAllocationId`
//!
//! D17 `inventory`：
//! - `stock_movement` → `StockMovementId`；`stock_balance` → `StockBalanceId`
//! - `stock_reservation` → `StockReservationId`
//! - `stock_reservation_entry` → `StockReservationEntryId`
//! - `stock_adjustment` → `StockAdjustmentId`；`stock_adjustment_line` → `StockAdjustmentLineId`
//!
//! D18 `receivable`：
//! - `receivable_account` → `ReceivableAccountId`；`receivable_entry` → `ReceivableEntryId`
//! - `receivable_funds_review` → `ReceivableFundsReviewId`
//! - `receivable_entry_offset` → `ReceivableEntryOffsetId`
//! - `customer_receipt` → `CustomerReceiptId`；`receipt_allocation` → `ReceiptAllocationId`
//! - `invoice` → `InvoiceId`；`sales_invoice_allocation` → `SalesInvoiceAllocationId`
//!
//! D19 `payable`：
//! - `payable_account` → `PayableAccountId`；`payable_entry` → `PayableEntryId`
//! - `payable_entry_offset` → `PayableEntryOffsetId`
//! - `supplier_payment` → `SupplierPaymentId`；`payment_allocation` → `PaymentAllocationId`
//! - `purchase_invoice_allocation` → `PurchaseInvoiceAllocationId`
//!
//! D20 `cost`：
//! - `cost_entry` → `CostEntryId`；`cost_allocation` → `CostAllocationId`
//!
//! D21 `returns`：
//! - `sales_return_case` → `SalesReturnCaseId`；`sales_return_line` → `SalesReturnLineId`
//! - `purchase_return_order` → `PurchaseReturnOrderId`
//! - `purchase_return_line` → `PurchaseReturnLineId`
//! - `customer_refund` → `CustomerRefundId`；`supplier_refund` → `SupplierRefundId`
//! - `receipt_reversal` → `ReceiptReversalId`；`payment_reversal` → `PaymentReversalId`
//!
//! D22 `legacy_import`：
//! - `legacy_import_batch` → `LegacyImportBatchId`
//! - `legacy_import_row` → `LegacyImportRowId`
//! - `legacy_import_confirmation` → `LegacyImportConfirmationId`
//!
//! D23 `mall_sync`：
//! - `mall_sales_sync_job` → `MallSalesSyncJobId`
//! - `mall_sales_sync_cursor` → `MallSalesSyncCursorId`
//! - `mall_sales_order_snapshot` → `MallSalesOrderSnapshotId`
//! - `mall_sales_reconciliation_job` → `MallSalesReconciliationJobId`
//! - `mall_sales_reconciliation_item` → `MallSalesReconciliationItemId`
//! - `master_mapping_task` → `MasterMappingTaskId`
//!
//! D24 `supplier_catalog`：
//! - `supplier_catalog_product` → `SupplierCatalogProductId`
//! - `supplier_catalog_product_revision` → `SupplierCatalogProductRevisionId`
//! - `supplier_catalog_product_revision_media` → `SupplierCatalogProductRevisionMediaId`
//! - `supplier_catalog_sku` → `SupplierCatalogSkuId`
//! - `supplier_catalog_sku_revision` → `SupplierCatalogSkuRevisionId`
//! - `supplier_product_mapping` → `SupplierProductMappingId`
//! - `supplier_catalog_intake_batch` → `SupplierCatalogIntakeBatchId`
//! - `supplier_catalog_intake_item` → `SupplierCatalogIntakeItemId`
//! - `supplier_offering` → `SupplierOfferingId`
//! - `supplier_offering_revision` → `SupplierOfferingRevisionId`
//!
//! D25 `supplier_api`：
//! - `supplier_api_connection` → `SupplierApiConnectionId`
//! - `supplier_api_capability` → `SupplierApiCapabilityId`
//!
//! D26 `publication`：
//! - `product_publication` → `ProductPublicationId`
//! - `product_publication_revision` → `ProductPublicationRevisionId`
//! - `product_publication_revision_media` → `ProductPublicationRevisionMediaId`
//! - `product_publication_delivery` → `ProductPublicationDeliveryId`
//!
//! D27 `projection`：
//! - `sales_order_projection` → `SalesOrderProjectionId`
//! - `sales_order_projection_revision` → `SalesOrderProjectionRevisionId`
//! - `sales_order_projection_delivery` → `SalesOrderProjectionDeliveryId`
//!
//! D28 `card_instance`：
//! - `mall_consumption_cutover` → `MallConsumptionCutoverId`
//! - `mall_card_instance` → `MallCardInstanceId`
//! - `mall_card_instance_correction` → `MallCardInstanceCorrectionId`
//! - `mall_balance_snapshot` → `MallBalanceSnapshotId`
//!
//! D29 `mall_order`：
//! - `mall_order_fact` → `MallOrderFactId`
//! - `mall_order_cancel_fact` → `MallOrderCancelFactId`
//! - `mall_order_completion_fact` → `MallOrderCompletionFactId`
//! - `mall_order` → `MallOrderId`；`mall_order_item` → `MallOrderItemId`
//! - `mall_payment_source` → `MallPaymentSourceId`
//! - `mall_item_funding_allocation` → `MallItemFundingAllocationId`
//! - `mall_consumption_entry` → `MallConsumptionEntryId`
//! - `mall_consumption_cost_assessment` → `MallConsumptionCostAssessmentId`
//!
//! D30 `mall_after_sales`：
//! - `mall_after_sales_request` → `MallAfterSalesRequestId`
//! - `mall_after_sales_request_line` → `MallAfterSalesRequestLineId`
//! - `mall_refund` → `MallRefundId`；`mall_refund_line` → `MallRefundLineId`
//! - `mall_refund_allocation` → `MallRefundAllocationId`
//! - `mall_balance_restoration` → `MallBalanceRestorationId`
//! - `mall_balance_restoration_allocation` → `MallBalanceRestorationAllocationId`
//!
//! D31 `mall_backfill`：
//! - `mall_consumption_backfill_job` → `MallConsumptionBackfillJobId`
//! - `mall_consumption_backfill_item` → `MallConsumptionBackfillItemId`
//!
//! D32 `supplier_fulfillment`：
//! - `supplier_fulfillment_order` → `SupplierFulfillmentOrderId`
//! - `supplier_fulfillment_item` → `SupplierFulfillmentItemId`
//! - `supplier_order_action` → `SupplierOrderActionId`
//! - `supplier_order_action_line` → `SupplierOrderActionLineId`
//! - `supplier_order_status_history` → `SupplierOrderStatusHistoryId`
//! - `supplier_refund_fact` → `SupplierRefundFactId`
//! - `supplier_refund_allocation` → `SupplierRefundAllocationId`
//!
//! D33 `supplier_settlement`：
//! - `supplier_settlement_statement` → `SupplierSettlementStatementId`
//! - `supplier_settlement_item` → `SupplierSettlementItemId`
//! - `supplier_settlement_difference` → `SupplierSettlementDifferenceId`
//!
//! D34 `integration_ops`：
//! - `inbox_message` → `InboxMessageId`
//! - `integration_error_task` → `IntegrationErrorTaskId`
//! - `reconciliation_difference` → `ReconciliationDifferenceId`
//! - `reconciliation_difference_resolution` → `ReconciliationDifferenceResolutionId`

use std::fmt;

use serde::{de::Deserializer, ser::Serializer, Deserialize, Serialize};

/// 批量生成一个透明 ID newtype。
///
/// 提供：`new`、`Deref<Target = str>`、`AsRef<str>`、`From<String>`、`Display`、
/// `Serialize`/`Deserialize`（透明字符串）、`PartialEq`/`Eq`/`Hash`/`Clone`/`Debug`。
macro_rules! id_type {
    ($name:ident) => {
        /// 主键 ID（32 位十六进制，由 `id_generator::next_id()` 产生，不承载业务含义）。
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            /// 由已生成的主键值构造 ID。
            ///
            /// # 参数
            /// * `value` - 由 `id_generator::next_id()` 产生的值（UUID v4，32 位十六进制）。
            ///
            /// # 返回
            /// 返回新的 ID。ID 是透明值对象，不校验格式。
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }
        }

        impl std::ops::Deref for $name {
            type Target = str;

            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
                Ok(Self(String::deserialize(deserializer)?))
            }
        }
    };
}

// D01 source_registry
id_type!(SourceSystemId);
id_type!(ExternalIdentityMapId);
id_type!(ExternalIdentityTargetId);

// D02 document_registry
id_type!(BusinessDocumentId);
id_type!(DocumentRelationId);
id_type!(DocumentParticipantId);
id_type!(WorkflowActionId);

// D03 work_item
id_type!(WorkItemId);

// D04 bulk_job
id_type!(BulkSelectionSnapshotId);
id_type!(BulkSelectionItemId);
id_type!(BackgroundJobId);
id_type!(BackgroundJobItemId);

// D05 file_asset
id_type!(FileAssetId);
id_type!(DocumentAttachmentId);

// D06 access_control（`role` 沿用 rbac::RoleId，见文件头映射表）
id_type!(PermissionId);
id_type!(UserRoleId);
id_type!(DataScopeId);
id_type!(AuditEventId);

// D07 party
id_type!(PartyId);
id_type!(PartyRevisionId);
id_type!(PartyContactId);
id_type!(PartyAddressId);
id_type!(PartyTaxProfileId);
id_type!(PartyBankAccountId);

// D08 customer
id_type!(CustomerAccountId);
id_type!(CustomerAssignmentId);

// D09 supplier
id_type!(SupplierAccountId);
id_type!(SupplierCommercialProfileRevisionId);
id_type!(SupplierCapabilityId);
id_type!(SupplierCapabilityRevisionId);
id_type!(SupplierQualificationId);
id_type!(SupplierQualificationRevisionId);
id_type!(SupplierQualificationCapabilityId);
id_type!(SupplierRatingRevisionId);

// D10 catalog
id_type!(ProductCategoryId);
id_type!(ProductBrandId);
id_type!(UnitOfMeasureId);
id_type!(SkuAttributeId);
id_type!(SkuAttributeValueId);
id_type!(ProductCategoryAttributeId);
id_type!(ProductId);
id_type!(ProductRevisionId);
id_type!(ProductRevisionMediaId);
id_type!(SkuId);
id_type!(SkuRevisionId);
id_type!(SkuRevisionAttributeValueId);
id_type!(VoucherCategoryProfileRevisionId);

// D11 warehouse
id_type!(WarehouseId);
id_type!(WarehouseRevisionId);
id_type!(WarehouseSkuPolicyId);

// D12 contract
id_type!(ContractId);
id_type!(ContractRevisionId);

// D13 sales_order
id_type!(SalesOrderId);
id_type!(SalesOrderLineId);
id_type!(SalesOrderWorkingCopyId);
id_type!(SalesOrderWorkingCopyLineId);
id_type!(SalesOrderSubmissionId);
id_type!(SalesOrderSubmissionLineId);
id_type!(SalesOrderRevisionId);
id_type!(SalesOrderRevisionLineId);
id_type!(SalesOrderGoodsServiceLineRevisionId);
id_type!(SalesOrderVoucherLineRevisionId);

// D14 sales_review
id_type!(SalesOrderReviewId);
id_type!(ProcurementConfirmationId);
id_type!(ProcurementConfirmationLineId);
id_type!(SalesChangeOrderId);
id_type!(SalesChangeSubmissionId);
id_type!(SalesChangeSubmissionLineId);
id_type!(SalesChangeReviewId);

// D15 purchase_order
id_type!(PurchaseOrderId);
id_type!(PurchaseOrderSubmissionId);
id_type!(PurchaseOrderSubmissionLineId);
id_type!(PurchaseOrderRevisionId);
id_type!(PurchaseOrderRevisionLineId);
id_type!(PurchaseLineSalesAllocationId);
id_type!(PurchaseChangeOrderId);
id_type!(PurchaseChangeSubmissionId);
id_type!(PurchaseChangeSubmissionLineId);

// D16 fulfillment
id_type!(PurchaseReceiptId);
id_type!(PurchaseReceiptLineId);
id_type!(DeliveryId);
id_type!(DeliveryLineId);
id_type!(ElectronicDeliveryId);
id_type!(ServiceFulfillmentId);
id_type!(CustomerAcceptanceId);
id_type!(CustomerAcceptanceLineId);
id_type!(AcceptanceFulfillmentAllocationId);

// D17 inventory
id_type!(StockMovementId);
id_type!(StockBalanceId);
id_type!(StockReservationId);
id_type!(StockReservationEntryId);
id_type!(StockAdjustmentId);
id_type!(StockAdjustmentLineId);

// D18 receivable
id_type!(ReceivableAccountId);
id_type!(ReceivableEntryId);
id_type!(ReceivableFundsReviewId);
id_type!(ReceivableEntryOffsetId);
id_type!(CustomerReceiptId);
id_type!(ReceiptAllocationId);
id_type!(InvoiceId);
id_type!(SalesInvoiceAllocationId);

// D19 payable
id_type!(PayableAccountId);
id_type!(PayableEntryId);
id_type!(PayableEntryOffsetId);
id_type!(SupplierPaymentId);
id_type!(PaymentAllocationId);
id_type!(PurchaseInvoiceAllocationId);

// D20 cost
id_type!(CostEntryId);
id_type!(CostAllocationId);

// D21 returns
id_type!(SalesReturnCaseId);
id_type!(SalesReturnLineId);
id_type!(PurchaseReturnOrderId);
id_type!(PurchaseReturnLineId);
id_type!(CustomerRefundId);
id_type!(SupplierRefundId);
id_type!(ReceiptReversalId);
id_type!(PaymentReversalId);

// D22 legacy_import
id_type!(LegacyImportBatchId);
id_type!(LegacyImportRowId);
id_type!(LegacyImportConfirmationId);

// D23 mall_sync
id_type!(MallSalesSyncJobId);
id_type!(MallSalesSyncCursorId);
id_type!(MallSalesOrderSnapshotId);
id_type!(MallSalesReconciliationJobId);
id_type!(MallSalesReconciliationItemId);
id_type!(MasterMappingTaskId);

// D24 supplier_catalog
id_type!(SupplierCatalogProductId);
id_type!(SupplierCatalogProductRevisionId);
id_type!(SupplierCatalogProductRevisionMediaId);
id_type!(SupplierCatalogSkuId);
id_type!(SupplierCatalogSkuRevisionId);
id_type!(SupplierProductMappingId);
id_type!(SupplierCatalogIntakeBatchId);
id_type!(SupplierCatalogIntakeItemId);
id_type!(SupplierOfferingId);
id_type!(SupplierOfferingRevisionId);

// D25 supplier_api
id_type!(SupplierApiConnectionId);
id_type!(SupplierApiCapabilityId);

// D26 publication
id_type!(ProductPublicationId);
id_type!(ProductPublicationRevisionId);
id_type!(ProductPublicationRevisionMediaId);
id_type!(ProductPublicationDeliveryId);

// D27 projection
id_type!(SalesOrderProjectionId);
id_type!(SalesOrderProjectionRevisionId);
id_type!(SalesOrderProjectionDeliveryId);

// D28 card_instance
id_type!(MallConsumptionCutoverId);
id_type!(MallCardInstanceId);
id_type!(MallCardInstanceCorrectionId);
id_type!(MallBalanceSnapshotId);

// D29 mall_order
id_type!(MallOrderFactId);
id_type!(MallOrderCancelFactId);
id_type!(MallOrderCompletionFactId);
id_type!(MallOrderId);
id_type!(MallOrderItemId);
id_type!(MallPaymentSourceId);
id_type!(MallItemFundingAllocationId);
id_type!(MallConsumptionEntryId);
id_type!(MallConsumptionCostAssessmentId);

// D30 mall_after_sales
id_type!(MallAfterSalesRequestId);
id_type!(MallAfterSalesRequestLineId);
id_type!(MallRefundId);
id_type!(MallRefundLineId);
id_type!(MallRefundAllocationId);
id_type!(MallBalanceRestorationId);
id_type!(MallBalanceRestorationAllocationId);

// D31 mall_backfill
id_type!(MallConsumptionBackfillJobId);
id_type!(MallConsumptionBackfillItemId);

// D32 supplier_fulfillment
id_type!(SupplierFulfillmentOrderId);
id_type!(SupplierFulfillmentItemId);
id_type!(SupplierOrderActionId);
id_type!(SupplierOrderActionLineId);
id_type!(SupplierOrderStatusHistoryId);
id_type!(SupplierRefundFactId);
id_type!(SupplierRefundAllocationId);

// D33 supplier_settlement
id_type!(SupplierSettlementStatementId);
id_type!(SupplierSettlementItemId);
id_type!(SupplierSettlementDifferenceId);

// D34 integration_ops
id_type!(InboxMessageId);
id_type!(IntegrationErrorTaskId);
id_type!(ReconciliationDifferenceId);
id_type!(ReconciliationDifferenceResolutionId);

#[cfg(test)]
mod tests {
    use super::*;

    /// 序列化透明性：serde_json 中 ID 是普通字符串，反序列化还原为同一新类型。
    #[test]
    fn ids_serialize_as_transparent_strings() {
        let value = "00112233445566778899aabbccddeeff";
        let id = SalesOrderId::new(value);

        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{value}\""));

        let back: SalesOrderId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    /// 代表性 ID 集（跨域引用面）的构造、Deref、Display 与比较。
    #[test]
    fn representative_ids_construct_display_compare() {
        let customer = CustomerAccountId::new("c100");
        let sku = SkuId::new("s200");
        let contract = ContractId::new("k300");
        let order = SalesOrderId::new("o400");
        let line = SalesOrderLineId::new("l500");
        let revision = SalesOrderRevisionId::new("r600");

        assert_eq!(customer.to_string(), "c100");
        assert_eq!(format!("{order}"), "o400");
        assert_eq!(sku.as_ref(), "s200");
        assert_eq!(&*contract, "k300");
        assert_ne!(SalesOrderId::new("o400"), SalesOrderId::new("o401"));
        assert_ne!(line, SalesOrderLineId::new("l501"));
        assert_eq!(revision.to_string(), "r600");

        let from_string: SalesOrderLineId = "l500".to_string().into();
        assert_eq!(from_string, line);
        assert_eq!(SalesOrderId::from("o400".to_string()), order);
    }

    /// 批量构造（Deref）与从字符串反序列化。
    #[test]
    fn ids_deref_and_deserialize() {
        let party = PartyId::new("p1");
        assert_eq!(party.len(), 2);

        let ids: Vec<InvoiceId> = serde_json::from_str(r#"["inv1","inv2","inv3"]"#).unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(ids[2].as_ref(), "inv3");
    }
}
