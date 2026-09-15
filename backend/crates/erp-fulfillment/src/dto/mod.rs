//! 域 D16 `fulfillment` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；数量一律十进制字符串
//! （`erp_core::money::Quantity` 自定义序列化）。页面：W06 客户验收、
//! W01 履约任务作业面使用的收货、发货与交付 DTO。
//!
//! 履约对象快照（电子交付接收对象、服务地点）以不透明值传输：服务端用
//! `app.secret` 作为 HMAC 密钥计算查询指纹后落库；明文字段级加密由边界
//! （P4 前端或接入层）在传入前完成，P3 不引入新的加密原语（地基修订候选）。

/// 采购入库单列表允许的排序字段白名单（api-contract §4：Service 层校验，禁止任意字段透传）。
pub(crate) const PURCHASE_RECEIPT_SORT_FIELDS: &[&str] = &["created_at", "posted_at"];
/// 发货单列表允许的排序字段白名单。
pub(crate) const DELIVERY_SORT_FIELDS: &[&str] = &["created_at", "shipped_at"];
/// 电子交付列表允许的排序字段白名单。
pub(crate) const ELECTRONIC_DELIVERY_SORT_FIELDS: &[&str] = &["occurred_at", "recorded_at", "created_at"];
/// 服务履约列表允许的排序字段白名单。
pub(crate) const SERVICE_FULFILLMENT_SORT_FIELDS: &[&str] = &["occurred_at", "recorded_at", "created_at"];
/// 客户验收单列表允许的排序字段白名单。
pub(crate) const CUSTOMER_ACCEPTANCE_SORT_FIELDS: &[&str] = &["accepted_at", "created_at"];

/// 排序方向。
pub use application_core::SortDir;

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use application_core::PageView;
/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串
/// 不生效，空单号需要按「空白视为空」拒绝，落入 HTTP 400）。
use application_core::non_blank;
/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) use application_core::normalize_sort;

mod purchase_receipt;
pub use purchase_receipt::{
    CreatePurchaseReceiptRequest, PostPurchaseReceiptRequest, PurchaseReceiptDetailView,
    PurchaseReceiptLineInput, PurchaseReceiptLineView, PurchaseReceiptListParams, PurchaseReceiptView,
    UpdatePurchaseReceiptRequest,
};
mod delivery;
pub use delivery::{
    CreateDeliveryRequest, DeliveryDetailView, DeliveryLineInput, DeliveryLineView, DeliveryListParams,
    DeliveryView, PostDeliveryRequest, UpdateDeliveryRequest,
};
mod electronic_delivery;
pub use electronic_delivery::{
    ConfirmElectronicDeliveryRequest, CreateElectronicDeliveryRequest, ElectronicDeliveryListParams,
    ElectronicDeliveryView,
};
mod service_fulfillment;
pub use service_fulfillment::{
    ConfirmServiceFulfillmentRequest, CreateServiceFulfillmentRequest, ServiceFulfillmentListParams,
    ServiceFulfillmentView,
};
mod customer_acceptance;
pub use customer_acceptance::{
    AcceptanceAllocationInput, AcceptanceAllocationView, AcceptanceLineInput,
    CommitCustomerAcceptanceRequest, CreateCustomerAcceptanceRequest, CustomerAcceptanceDetailView,
    CustomerAcceptanceLineView, CustomerAcceptanceListParams, CustomerAcceptanceView,
    PostAcceptanceLineInput, PostCustomerAcceptanceRequest, ReverseCustomerAcceptanceRequest,
};

#[cfg(test)]
mod tests {
    use erp_core::ids::SalesOrderId;
    use validator::Validate;

    use super::{
        CreateDeliveryRequest, CreatePurchaseReceiptRequest, DeliveryListParams, DeliveryView,
        PurchaseReceiptListParams, PurchaseReceiptView, SortDir, normalize_sort,
    };
    use crate::entity::fulfillment::{DeliveryState, DeliveryType, PurchaseReceiptState};

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("quantity".to_string()), &None, &["created_at", "posted_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" posted_at ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "posted_at"],
        )
        .unwrap();
        assert_eq!(field, "posted_at");
        assert_eq!(direction, SortDir::Asc);

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn list_params_normalize_paging_and_reject_unbounded_page_size() {
        let receipt = PurchaseReceiptListParams {
            purchase_order_id: None,
            status: Some(PurchaseReceiptState::Posted),
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("created_at".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = receipt.normalized().unwrap();
        assert_eq!(query.status, Some(PurchaseReceiptState::Posted));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);

        let invalid = PurchaseReceiptListParams {
            purchase_order_id: None,
            status: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(invalid.validate().is_err());

        let delivery = DeliveryListParams {
            sales_order_id: Some(SalesOrderId::new("so-1")),
            status: Some(DeliveryState::Shipped),
            page: Some(1),
            page_size: Some(30),
            sort_by: None,
            sort_dir: None,
        };
        let query = delivery.normalized().unwrap();
        assert_eq!(query.sales_order_id.as_deref(), Some("so-1"));
        assert_eq!(query.paging.page_size, 30);
    }

    /// 采购收货创建请求拒绝定义 ID / 审批人；视图不暴露审批区。
    #[test]
    fn purchase_receipt_create_and_view_have_no_approval_surface() {
        let valid = serde_json::json!({
            "receipt_no": "PR-1",
            "purchase_order_id": "po-1",
            "warehouse_id": "wh-1",
            "lines": [{
                "purchase_order_revision_line_id": "porl-1",
                "received_quantity": "10",
                "qualified_quantity": "10",
                "rejected_quantity": "0"
            }]
        });
        assert!(serde_json::from_value::<CreatePurchaseReceiptRequest>(valid).is_ok());
        let forged = serde_json::json!({
            "receipt_no": "PR-1",
            "purchase_order_id": "po-1",
            "warehouse_id": "wh-1",
            "lines": [{
                "purchase_order_revision_line_id": "porl-1",
                "received_quantity": "10",
                "qualified_quantity": "10",
                "rejected_quantity": "0"
            }],
            "definition_id": "forged",
            "assignee": "forged"
        });
        assert!(serde_json::from_value::<CreatePurchaseReceiptRequest>(forged).is_err());

        let view = PurchaseReceiptView {
            id: "pr-1".into(),
            receipt_no: "PR-1".into(),
            purchase_order_id: "po-1".into(),
            warehouse_id: "wh-1".into(),
            status: PurchaseReceiptState::Draft,
            posted_at: None,
            version: 1,
            created_at: 1,
        };
        let value = serde_json::to_value(&view).expect("视图可序列化");
        let object = value.as_object().expect("视图为对象");
        assert!(!object.contains_key("approval"));
        assert!(!object.contains_key("definition_id"));
        assert!(!object.contains_key("assignee"));
    }

    /// 发货创建请求拒绝定义 ID / 审批人；视图不暴露审批区。
    #[test]
    fn delivery_create_and_view_have_no_approval_surface() {
        let valid = serde_json::json!({
            "delivery_no": "DV-1",
            "delivery_type": "WAREHOUSE_SHIP",
            "sales_order_id": "so-1",
            "warehouse_id": "wh-1",
            "lines": [{
                "sales_order_line_id": "so-line-1",
                "quantity": "2",
                "stock_reservation_id": "rsv-1"
            }]
        });
        assert!(serde_json::from_value::<CreateDeliveryRequest>(valid).is_ok());
        let forged = serde_json::json!({
            "delivery_no": "DV-1",
            "delivery_type": "WAREHOUSE_SHIP",
            "sales_order_id": "so-1",
            "warehouse_id": "wh-1",
            "lines": [{
                "sales_order_line_id": "so-line-1",
                "quantity": "2",
                "stock_reservation_id": "rsv-1"
            }],
            "definition_id": "forged",
            "assignee": "forged"
        });
        assert!(serde_json::from_value::<CreateDeliveryRequest>(forged).is_err());

        let view = DeliveryView {
            id: "dv-1".into(),
            delivery_no: "DV-1".into(),
            delivery_type: DeliveryType::WarehouseShip,
            sales_order_id: "so-1".into(),
            purchase_order_id: None,
            warehouse_id: Some("wh-1".into()),
            status: DeliveryState::Draft,
            carrier: None,
            tracking_no: None,
            shipped_at: None,
            version: 1,
            created_at: 1,
        };
        let value = serde_json::to_value(&view).expect("视图可序列化");
        let object = value.as_object().expect("视图为对象");
        assert!(!object.contains_key("approval"));
        assert!(!object.contains_key("definition_id"));
        assert!(!object.contains_key("assignee"));
    }
}
