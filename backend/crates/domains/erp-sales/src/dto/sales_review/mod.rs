//! 域 D14 `sales_review` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；金额/数量/单价/税率按 P0 约定字符串序列化；
//! 时间一律秒级时间戳；业务日期 `YYYY-MM-DD`。
//!
//! 契约来源：erp-client `features/sales-orders`（W05 变更轨）。

mod draft;
/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use application_core::PageView;
/// 排序方向。
pub use application_core::SortDir;
/// 校验文本去除首尾空白后非空。
use application_core::non_blank;
pub use draft::{SalesChangeDraftView, SaveSalesChangeDraftRequest};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
pub use crate::dto::PageParams;
use crate::entity::sales_review::SalesChangeType;

// ---------------------------------------------------------------------------
// sales_change_order（销售变更单，W05 变更轨）
// ---------------------------------------------------------------------------

/// 销售变更单列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesChangeOrderListParams {
    /// 跨页必须使用前一页的当前授权和业务版本。
    #[validate(length(min = 1, max = 256))]
    pub scope_version: Option<String>,
    /// 原销售单筛选。
    pub sales_order_id: Option<erp_core::ids::SalesOrderId>,
    /// 变更状态筛选。
    pub status: Option<crate::entity::sales_review::SalesChangeOrderStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 创建销售变更单请求。
///
/// 客户端只提交变更意图和当前已见的正式版本号；变更工作副本的
/// 表头、明细、合同与商业快照必须由服务端从当前生效版本派生。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSalesChangeOrderRequest {
    /// 原销售单。
    pub sales_order_id: erp_core::ids::SalesOrderId,
    /// 变更类型。
    pub change_type: SalesChangeType,
    /// 客户端已见的当前正式版本号；已变更时拒绝以防止从过期页面发起。
    #[validate(range(min = 1, message = "基准版本号必须大于 0"))]
    pub expected_base_revision_no: u32,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub reason: String,
    /// 幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 发起销售变更影响确认请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SubmitSalesChangeRequest {
    /// 驳回后的重新提交说明；首次提交可省略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[validate(length(max = 1000, message = "重新提交说明不得超过1000字"))]
    pub resubmission_reason: Option<String>,
    /// 期望的乐观锁版本；与当前版本不一致时拒绝提交（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 作废销售变更单请求（乐观锁：携带期望版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct VoidSalesChangeOrderRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
}

/// 销售变更单列表行视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesChangeOrderView {
    /// 实体主键。
    pub id: String,
    /// 原销售单。
    pub sales_order_id: String,
    /// 发起时当前版本。
    pub base_revision_id: String,
    /// 变更类型。
    pub change_type: SalesChangeType,
    /// 变更状态。
    pub status: crate::entity::sales_review::SalesChangeOrderStatus,
    /// 当前不可变目标提交。
    pub current_submission_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 撤回销售变更审批请求。原因必填。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CancelSalesChangeApprovalRequest {
    /// 期望的单据乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 非空撤回原因。
    #[validate(length(min = 1, max = 512, message = "撤回原因不能为空"))]
    pub reason: String,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

#[cfg(test)]
mod tests {
    use application_core::normalize_sort;

    use crate::entity::sales_review::SalesChangeType;

    #[test]
    fn resubmission_reason_allows_one_thousand_characters() {
        use validator::Validate;
        let mut request = super::SubmitSalesChangeRequest {
            resubmission_reason: Some("字".repeat(1000)),
            version: 1,
            idempotency_key: "retry-1".into(),
        };
        assert!(request.validate().is_ok());
        request.resubmission_reason = Some("字".repeat(1001));
        assert!(request.validate().unwrap_err().field_errors().contains_key("resubmission_reason"));
    }

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("sideways".to_string()), &["created_at"]).is_err());
        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, super::SortDir::Desc);
    }

    #[test]
    fn change_type_serializes_with_stable_code() {
        assert_eq!(serde_json::to_string(&SalesChangeType::Quantity).unwrap(), "\"QUANTITY\"");
    }

    #[test]
    fn change_list_params_consume_scope_version() {
        let params: super::SalesChangeOrderListParams = serde_json::from_value(serde_json::json!({
            "page": 2,
            "scope_version": "v1",
            "sales_order_id": "so-1"
        }))
        .unwrap();
        assert_eq!(params.scope_version.as_deref(), Some("v1"));
        assert_eq!(params.page, Some(2));
    }
}
