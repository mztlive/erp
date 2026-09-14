//! 域 D15 `purchase_order` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳，业务日期 `YYYY-MM-DD`；
//! 金额/数量以字符串传输（`entities::money` 的 serde 字符串形态）。
//!
//! 与 `erp-client/features/purchase-orders/api.ts` 的差异（契约变更）：
//! - 列表状态枚举沿用实体代码（`PENDING_FINANCE_REVIEW`/`PARTIALLY_EXECUTED`/
//!   `VOIDED`），前端 mock 使用 `PENDING_REVIEW`/`PARTIAL`/`VOID`；
//! - 列表/详情同时返回 `sales_order_id` 与 `sales_order_no`：前者只用于路由，
//!   后者是用户可见的跨单据业务引用，禁止把内部 ID 当单号展示；
//! - 草稿 `purchase_no` 为空，首次提交事务分配不可复用正式号；
//! - 表单类写操作（创建/保存/提交/审核）统一返回稳定业务结果，不再返回
//!   `FormalActionResponse` 信封（由 HTTP 统一信封承载）。

mod change_order;
mod command;
mod query;

pub use crate::entity::purchase_order::SupplySourceType;
pub(crate) use application_core::normalize_sort;
pub use application_core::PageView;
pub use application_core::SortDir;

pub use self::change_order::{
    CancelPurchaseChangeApprovalRequest, EffectPurchaseChangeRequest, PurchaseChangeEffectResult,
    PurchaseChangeOrderListParams, PurchaseChangeSubmitResult, StartPurchaseChangeRequest,
    StartPurchaseChangeResult, SubmitPurchaseChangeRequest,
};
pub use self::command::{
    CancelPurchaseOrderApprovalRequest, CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderLineRequest,
    CreatePurchaseOrderResult, CreatePurchaseOrdersFromSourcingRequest,
    CreatePurchaseOrdersFromSourcingResult, ExistingStockReservationResult, PurchaseReviewResult,
    SavePurchaseOrderDraftRequest, SavePurchaseOrderDraftResult, SavePurchaseOrderLine,
    SavePurchaseOrderLinePatch, SourcingLineAssignment, SubmitPurchaseOrderRequest,
    SubmitPurchaseOrderResult, VoidPurchaseOrderRequest, VoidPurchaseOrderResult,
};
pub use self::command::{
    CREATE_ACTION, CREATE_SOURCING_ACTION, PURCHASE_SUBMIT_ACTION, SAVE_ACTION, VOID_ACTION,
};
pub use self::query::{
    PageParams, PurchaseChangeSummaryView, PurchaseOrderLineView, PurchaseOrderListParams,
    PurchaseOrderListQuery, PurchaseSalesAllocationView, TotalsView, PURCHASE_ORDER_SORT_FIELDS,
};

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::entity::purchase_order::PurchaseOrderStatus;
    use erp_core::common::time::BusinessDate;
    use erp_core::money::Quantity;
    use serde_json::json;
    use validator::Validate;

    use super::command::submit_request_shape;
    use super::{
        normalize_sort, CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderLineRequest,
        CreatePurchaseOrdersFromSourcingRequest, PurchaseOrderListParams, SavePurchaseOrderDraftRequest,
        SavePurchaseOrderLine, SavePurchaseOrderLinePatch, SortDir, SourcingLineAssignment,
        SubmitPurchaseOrderRequest, SupplySourceType, VoidPurchaseOrderRequest,
    };
    use crate::entity::purchase_order::{
        PurchaseLineType, PurchaseOrderSubmissionLine, RequestedLine, SourcingAssignment,
    };
    use crate::Error;

    /// 作废路径指纹金值：锁定历史算法绝对摘要。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 指纹算法或载荷形态变化导致摘要漂移时测试失败。
    #[test]
    fn void_fingerprint_golden() {
        let request = VoidPurchaseOrderRequest {
            expected_lock_version: 4,
            reason: " 重复采购 ".to_string(),
            idempotency_key: "void-key-1".to_string(),
        };
        assert_eq!(
            request.request_fingerprint("po-1").unwrap(),
            "bcb616e27bcce0b0d3e09c5a26d1ac15d386f7672ebcf85757b0c52d10761b8b"
        );
    }

    /// 保存草稿路径指纹金值：锁定历史算法绝对摘要。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 指纹算法或载荷形态变化导致摘要漂移时测试失败。
    #[test]
    fn save_fingerprint_golden() {
        let request = SavePurchaseOrderDraftRequest {
            expected_lock_version: 3,
            payment_term_code: Some(" NET-30 ".to_string()),
            lines: vec![SavePurchaseOrderLine {
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: None,
                sku_id: Some("sku-1".to_string()),
                sku_revision_id: Some("sku-rev-1".to_string()),
                product_name: Some("产品".to_string()),
                specification: None,
                quantity: Some("1".to_string()),
                base_unit_code: Some("EA".to_string()),
                unit_cost_gross: Some("10".to_string()),
                input_tax_rate: Some("0.13".to_string()),
                expected_delivery_date: Some("2026-08-25".to_string()),
                sales_order_line_id: Some("sales-line-1".to_string()),
                sales_order_revision_line_id: Some("sales-revision-line-1".to_string()),
                sales_order_submission_line_id: Some("sales-submission-line-1".to_string()),
                allocated_quantity: Some("1".to_string()),
                gross_amount: None,
            }],
            line_patches: vec![],
            idempotency_key: "save-key-1".to_string(),
        };
        assert_eq!(
            request.request_fingerprint("po-1").unwrap(),
            "28ffd5720e0a0bd90a07358d39f81a313c3b10746c037a927ceb4b635068a8f5"
        );
    }

    /// 依据创建路径指纹金值：锁定历史算法绝对摘要。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 指纹算法或载荷形态变化导致摘要漂移时测试失败。
    #[test]
    fn create_fingerprint_golden() {
        let request = CreatePurchaseOrderFromBasisRequest {
            work_item_id: "wi-1".to_string(),
            basis_id: "basis-1".to_string(),
            purchase_type: crate::entity::purchase_order::PurchaseType::Physical,
            payment_term_code: "NET-30".to_string(),
            target_warehouse_id: Some("wh-1".to_string()),
            lines: vec![CreatePurchaseOrderLineRequest {
                sales_order_line_id: "sol-1".to_string(),
                quantity: "10".to_string(),
                expected_delivery_date: "2026-08-25".to_string(),
            }],
            idempotency_key: "create-key-1".to_string(),
        };
        let lines = vec![RequestedLine {
            sales_order_line_id: "sol-1".to_string(),
            quantity: Quantity::from_str("10").unwrap(),
            expected_delivery_date: BusinessDate::from_str("2026-08-25").unwrap(),
        }];
        assert_eq!(
            request.request_fingerprint(&lines),
            "e225f0fda8a15fdf6b332cf1bb97ee893e9f0fd9092367986446a4e3f87f5a2f"
        );
    }

    /// 选源创建路径指纹金值：锁定历史算法绝对摘要。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 指纹算法或载荷形态变化导致摘要漂移时测试失败。
    #[test]
    fn sourcing_fingerprint_golden() {
        let request = CreatePurchaseOrdersFromSourcingRequest {
            work_item_id: "wi-1".to_string(),
            sales_order_id: "so-1".to_string(),
            lines: vec![
                SourcingLineAssignment {
                    sales_order_line_id: "sol-1".to_string(),
                    basis_id: "basis-1".to_string(),
                    source_type: SupplySourceType::Purchase,
                    target_warehouse_id: Some("wh-1".to_string()),
                    quantity: "10".to_string(),
                    expected_delivery_date: "2026-08-25".to_string(),
                },
                SourcingLineAssignment {
                    sales_order_line_id: "sol-2".to_string(),
                    basis_id: "basis-2".to_string(),
                    source_type: SupplySourceType::ExistingStock,
                    target_warehouse_id: None,
                    quantity: "5".to_string(),
                    expected_delivery_date: "2026-08-26".to_string(),
                },
            ],
            idempotency_key: "sourcing-key-1".to_string(),
        };
        let assignments = vec![
            SourcingAssignment {
                sales_order_line_id: "sol-1".to_string(),
                basis_id: "basis-1".to_string(),
                source_type: SupplySourceType::Purchase,
                target_warehouse_id: Some("wh-1".to_string()),
                quantity: Quantity::from_str("10").unwrap(),
                expected_delivery_date: BusinessDate::from_str("2026-08-25").unwrap(),
            },
            SourcingAssignment {
                sales_order_line_id: "sol-2".to_string(),
                basis_id: "basis-2".to_string(),
                source_type: SupplySourceType::ExistingStock,
                target_warehouse_id: None,
                quantity: Quantity::from_str("5").unwrap(),
                expected_delivery_date: BusinessDate::from_str("2026-08-26").unwrap(),
            },
        ];
        assert_eq!(
            request.request_fingerprint(&assignments).unwrap(),
            "189640d37d07d808e3fc76001481775cade3f948f9873ddfdf9af4ecd87cfa1b"
        );
    }

    /// 提交路径指纹金值：锁定历史算法绝对摘要。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 指纹算法或形态编码变化导致摘要漂移时测试失败。
    #[test]
    fn submit_fingerprint_golden() {
        let request = SubmitPurchaseOrderRequest {
            expected_lock_version: 3,
            payment_term_code: Some("NET-30".to_string()),
            line_patches: vec![SavePurchaseOrderLinePatch {
                line_id: "line-1".to_string(),
                line_type: PurchaseLineType::ItemService,
                quantity: Some("10".to_string()),
                unit_cost_gross: Some("100.00".to_string()),
                input_tax_rate: Some("0.13".to_string()),
            }],
            idempotency_key: "submit-key-1".to_string(),
        };
        assert_eq!(
            request.request_fingerprint("po-1"),
            "377e75901732461d599f88b8297478c5c9bb9c0dfbe34d0506b701f12d4f1ee6"
        );
    }

    /// 提交形态文本必须与历史 Debug 派生字节逐字一致。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 显式编码与历史 Debug 输出不一致时测试失败。
    #[test]
    fn submit_shape_matches_historical_debug_bytes() {
        let payment_term_code = Some("NET-30".to_string());
        let line_patches = vec![SavePurchaseOrderLinePatch {
            line_id: "line-1".to_string(),
            line_type: PurchaseLineType::ItemService,
            quantity: Some("10".to_string()),
            unit_cost_gross: Some("100.00".to_string()),
            input_tax_rate: Some("0.13".to_string()),
        }];
        assert_eq!(
            submit_request_shape(&payment_term_code, &line_patches),
            format!("{:?}|{:?}", payment_term_code, line_patches)
        );
        let empty_term = None;
        let empty_patches = vec![];
        assert_eq!(
            submit_request_shape(&empty_term, &empty_patches),
            format!("{:?}|{:?}", empty_term, empty_patches)
        );
        let logistics = vec![SavePurchaseOrderLinePatch {
            line_id: "line-2".to_string(),
            line_type: PurchaseLineType::LogisticsFee,
            quantity: None,
            unit_cost_gross: Some("50".to_string()),
            input_tax_rate: None,
        }];
        assert_eq!(
            submit_request_shape(&None, &logistics),
            format!("{:?}|{:?}", None::<String>, logistics)
        );
    }

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("amount".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" purchase_no ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "purchase_no"],
        )
        .unwrap();
        assert_eq!(field, "purchase_no");
        assert_eq!(direction, SortDir::Asc);

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn list_params_normalize_paging_filters_and_sort_defaults() {
        let params = PurchaseOrderListParams {
            scope_version: None,
            owner_user_ids: None,
            q: Some(" PO-2026 ".to_string()),
            sales_order_id: None,
            supplier_id: Some(" sup-1 ".to_string()),
            status: Some(PurchaseOrderStatus::PendingFinanceReview),
            review_status: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.q.as_deref(), Some("PO-2026"));
        assert_eq!(query.supplier_id.as_deref(), Some("sup-1"));
        assert_eq!(query.status, Some(PurchaseOrderStatus::PendingFinanceReview));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = PurchaseOrderListParams {
            scope_version: Some("v1".to_string()),
            owner_user_ids: None,
            q: None,
            sales_order_id: None,
            supplier_id: None,
            status: None,
            review_status: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn create_request_rejects_blank_basis_and_keys() {
        let request: super::CreatePurchaseOrderFromBasisRequest = serde_json::from_value(json!({
            "work_item_id": "wi-1",
            "basis_id": "   ",
            "purchase_type": "PHYSICAL",
            "payment_term_code": "NET-30",
            "lines": [{
                "sales_order_line_id": "sol-1",
                "quantity": "1",
                "expected_delivery_date": "2026-09-01"
            }],
            "idempotency_key": "k-1",
        }))
        .unwrap();
        assert!(request.validate().is_err());
    }

    #[test]
    fn sourcing_line_defaults_legacy_requests_to_purchase() {
        let line: super::SourcingLineAssignment = serde_json::from_value(json!({
            "sales_order_line_id": "sol-1",
            "basis_id": "basis-1",
            "quantity": "1",
            "expected_delivery_date": "2026-09-01"
        }))
        .unwrap();
        assert_eq!(line.source_type, SupplySourceType::Purchase);
    }

    /// 构造当前草稿商品行。
    fn draft_line(id: &str, stable_line_id: &str, quantity: &str) -> PurchaseOrderSubmissionLine {
        use crate::entity::purchase_order::PurchaseOrderSubmissionLineData;
        use erp_core::ids::{
            ProcurementConfirmationLineId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId,
            SalesOrderLineId, SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId, SkuRevisionId,
        };
        use erp_core::money::{Rate, UnitPrice};
        let quantity = Quantity::from_str(quantity).unwrap();
        let (gross, net, tax) = erp_core::money::line_amounts(
            UnitPrice::from_str("5").unwrap(),
            quantity,
            Rate::from_str("0").unwrap(),
        );
        PurchaseOrderSubmissionLine::new(
            PurchaseOrderSubmissionLineId::new(id),
            PurchaseOrderSubmissionLineData {
                purchase_order_submission_id: PurchaseOrderSubmissionId::new("sub-1"),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: Some(SkuRevisionId::new("skur-1")),
                product_name_snapshot: Some("商品".to_string()),
                specification_snapshot: Some("规格".to_string()),
                quantity: Some(quantity),
                base_unit_code: Some("件".to_string()),
                unit_cost_gross: Some(UnitPrice::from_str("5").unwrap()),
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
                input_tax_rate: Some(Rate::from_str("0").unwrap()),
                expected_delivery_date: None,
                sales_order_line_id: Some(SalesOrderLineId::new(stable_line_id)),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("sorl-1")),
                sales_order_submission_line_id: Some(SalesOrderSubmissionLineId::new("sosl-1")),
                allocated_quantity: Some(quantity),
            },
        )
        .unwrap()
    }

    /// 构造空行载荷的保存请求。
    fn draft_request(line_patches: Vec<SavePurchaseOrderLinePatch>) -> SavePurchaseOrderDraftRequest {
        SavePurchaseOrderDraftRequest {
            expected_lock_version: 1,
            payment_term_code: None,
            lines: vec![],
            line_patches,
            idempotency_key: "save-key-1".to_string(),
        }
    }

    /// 完整行与行补丁必须且只能提供一种。
    #[test]
    fn save_request_shape_requires_exactly_one_line_payload() {
        let empty = draft_request(vec![]);
        assert!(empty.ensure_shape().is_err());

        let mut both = empty.clone();
        both.lines = vec![SavePurchaseOrderLine {
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: None,
            sku_id: None,
            sku_revision_id: None,
            product_name: None,
            specification: None,
            quantity: Some("1".to_string()),
            base_unit_code: None,
            unit_cost_gross: None,
            input_tax_rate: None,
            expected_delivery_date: None,
            sales_order_line_id: None,
            sales_order_revision_line_id: None,
            sales_order_submission_line_id: None,
            allocated_quantity: Some("1".to_string()),
            gross_amount: None,
        }];
        both.line_patches = vec![SavePurchaseOrderLinePatch {
            line_id: "subl-1".to_string(),
            line_type: PurchaseLineType::ItemService,
            quantity: None,
            unit_cost_gross: None,
            input_tax_rate: None,
        }];
        assert!(both.ensure_shape().is_err());

        let mut lines_only = both.clone();
        lines_only.line_patches = vec![];
        assert!(lines_only.ensure_shape().is_ok());

        let mut patches_only = both.clone();
        patches_only.lines = vec![];
        assert!(patches_only.ensure_shape().is_ok());

        let mut blank_patch = patches_only.clone();
        blank_patch.line_patches[0].line_id = "  ".to_string();
        assert!(blank_patch.ensure_shape().is_err());
    }

    /// 完整行路径原样返回，补丁路径合并冻结字段与可编辑字段。
    #[test]
    fn resolve_lines_merges_patches_with_frozen_draft_fields() {
        let existing = vec![draft_line("subl-1", "sol-1", "2")];
        let mut request = draft_request(vec![SavePurchaseOrderLinePatch {
            line_id: " subl-1 ".to_string(),
            line_type: PurchaseLineType::ItemService,
            quantity: Some(" 5 ".to_string()),
            unit_cost_gross: Some("8".to_string()),
            input_tax_rate: Some("0.13".to_string()),
        }]);
        let merged = request.resolve_lines(&existing).unwrap();
        assert_eq!(merged.len(), 1);
        let line = &merged[0];
        // 补丁字段原样合并；空白由领域校验统一规范化。
        assert_eq!(line.quantity.as_deref(), Some(" 5 "));
        assert_eq!(line.allocated_quantity.as_deref(), Some(" 5 "));
        assert_eq!(line.unit_cost_gross.as_deref(), Some("8"));
        assert_eq!(line.input_tax_rate.as_deref(), Some("0.13"));
        assert_eq!(line.sku_id.as_deref(), Some("sku-1"));
        assert_eq!(line.sales_order_line_id.as_deref(), Some("sol-1"));
        assert_eq!(line.sales_order_submission_line_id.as_deref(), Some("sosl-1"));
        assert_eq!(line.product_name.as_deref(), Some("商品"));

        request.line_patches[0].line_id = "subl-1".to_string();
        request.lines = vec![SavePurchaseOrderLine {
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: None,
            sku_id: Some("sku-9".to_string()),
            sku_revision_id: None,
            product_name: None,
            specification: None,
            quantity: Some("9".to_string()),
            base_unit_code: None,
            unit_cost_gross: None,
            input_tax_rate: None,
            expected_delivery_date: None,
            sales_order_line_id: Some("sol-9".to_string()),
            sales_order_revision_line_id: None,
            sales_order_submission_line_id: None,
            allocated_quantity: Some("9".to_string()),
            gross_amount: None,
        }];
        let full = request.resolve_lines(&existing).unwrap();
        assert_eq!(full[0].quantity.as_deref(), Some("9"));
        assert_eq!(full[0].sales_order_line_id.as_deref(), Some("sol-9"));
    }

    /// 补丁必须覆盖全部当前草稿行且不得重复。
    #[test]
    fn resolve_line_patches_rejects_partial_and_duplicate_patches() {
        let existing = vec![
            draft_line("subl-1", "sol-1", "2"),
            draft_line("subl-2", "sol-2", "1"),
        ];
        let patch = |line_id: &str| SavePurchaseOrderLinePatch {
            line_id: line_id.to_string(),
            line_type: PurchaseLineType::ItemService,
            quantity: None,
            unit_cost_gross: None,
            input_tax_rate: None,
        };
        let partial = draft_request(vec![patch("subl-1")]);
        assert!(SavePurchaseOrderLinePatch::resolve_all(&partial.line_patches, &existing).is_err());

        let duplicate = draft_request(vec![patch("subl-1"), patch("subl-1")]);
        assert!(SavePurchaseOrderLinePatch::resolve_all(&duplicate.line_patches, &existing).is_err());

        let unknown = draft_request(vec![patch("subl-9"), patch("subl-2")]);
        assert!(matches!(
            SavePurchaseOrderLinePatch::resolve_all(&unknown.line_patches, &existing),
            Err(Error::ConflictError(_))
        ));

        let mut changed_type = draft_request(vec![patch("subl-1"), patch("subl-2")]);
        changed_type.line_patches[0].line_type = PurchaseLineType::LogisticsFee;
        assert!(SavePurchaseOrderLinePatch::resolve_all(&changed_type.line_patches, &existing).is_err());
    }

    /// 草稿行编辑请求保留全部字段供领域校验。
    #[test]
    fn to_draft_edit_carries_all_line_fields() {
        let line = SavePurchaseOrderLine {
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some("pcl-1".to_string()),
            sku_id: Some("sku-1".to_string()),
            sku_revision_id: Some("skur-1".to_string()),
            product_name: Some("商品".to_string()),
            specification: Some("规格".to_string()),
            quantity: Some("5".to_string()),
            base_unit_code: Some("件".to_string()),
            unit_cost_gross: Some("8".to_string()),
            input_tax_rate: Some("0.13".to_string()),
            expected_delivery_date: Some("2026-09-01".to_string()),
            sales_order_line_id: Some("sol-1".to_string()),
            sales_order_revision_line_id: Some("sorl-1".to_string()),
            sales_order_submission_line_id: Some("sosl-1".to_string()),
            allocated_quantity: Some("5".to_string()),
            gross_amount: None,
        };
        let edit = line.to_draft_edit();
        assert_eq!(edit.line_type, PurchaseLineType::ItemService);
        assert_eq!(edit.quantity.as_deref(), Some("5"));
        assert_eq!(edit.allocated_quantity.as_deref(), Some("5"));
        assert_eq!(edit.sku_id.as_deref(), Some("sku-1"));
        assert_eq!(edit.sales_order_line_id.as_deref(), Some("sol-1"));
        assert_eq!(edit.sales_order_submission_line_id.as_deref(), Some("sosl-1"));
    }
}
