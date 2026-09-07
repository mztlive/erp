//! 域 D14 `sales_review` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；金额/数量/单价/税率按 P0 约定字符串序列化；
//! 时间一律秒级时间戳；业务日期 `YYYY-MM-DD`。
//!
//! 契约来源：erp-client `features/sales-orders`（W05 变更轨）。

use crate::entity::sales_review::SalesChangeType;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::Result;
use application_core::{page_or_default, page_size_or_default};

/// 销售变更单列表允许的排序字段白名单。
pub(crate) const SALES_CHANGE_ORDER_SORT_FIELDS: &[&str] = &["created_at"];

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

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use application_core::PageView;

/// 校验文本去除首尾空白后非空。
use application_core::non_blank;

// ---------------------------------------------------------------------------
// sales_change_order（销售变更单，W05 变更轨）
// ---------------------------------------------------------------------------

/// 销售变更单列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SalesChangeOrderListParams {
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

/// 归一化后的销售变更单列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SalesChangeOrderListQuery {
    /// 原销售单筛选。
    pub sales_order_id: Option<String>,
    /// 变更状态筛选。
    pub status: Option<crate::entity::sales_review::SalesChangeOrderStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SalesChangeOrderListParams {
    /// 归一化销售变更单列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SalesChangeOrderListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, SALES_CHANGE_ORDER_SORT_FIELDS)?;
        Ok(SalesChangeOrderListQuery {
            sales_order_id: self.sales_order_id.as_ref().map(ToString::to_string),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
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
    /// 期望的乐观锁版本；与当前版本不一致时拒绝提交（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键长度必须在1-128之间"))]
    pub idempotency_key: String,
}

/// 变更复核决策请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChangeReviewDecisionRequest {
    /// 当前复核待办。
    #[validate(custom(function = "non_blank", message = "复核待办ID不能为空"))]
    pub work_item_id: String,
    /// 期望的待办乐观锁版本。
    #[validate(range(min = 1, message = "待办版本必须大于 0"))]
    pub expected_task_version: u64,
    /// 期望的不可变销售变更提交版本。
    #[validate(custom(function = "non_blank", message = "提交版本不能为空"))]
    pub expected_subject_version: String,
    /// 复核意见（通过时可空；驳回必填且非空白）。
    pub decision_reason: Option<String>,
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
    use super::normalize_sort;
    use crate::entity::sales_review::SalesChangeType;

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
        assert_eq!(
            serde_json::to_string(&SalesChangeType::Quantity).unwrap(),
            "\"QUANTITY\""
        );
    }
}
