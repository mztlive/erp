//! 域 D08 `customer` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；业务日期一律 `YYYY-MM-DD`；时间一律秒级时间戳。

use entities::common::time::BusinessDate;
use entities::customer::{AssignmentRole, CustomerAccount, CustomerAccountStatus, CustomerAssignment};
use entities::ids::PartyId;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 客户角色列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const CUSTOMER_SORT_FIELDS: &[&str] = &["created_at", "customer_no", "status"];
/// 客户归属列表允许的排序字段白名单。
pub(crate) const CUSTOMER_ASSIGNMENT_SORT_FIELDS: &[&str] = &["created_at", "valid_from", "valid_to"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

/// 归一化后的客户角色列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CustomerListQuery {
    /// 客户编号模糊搜索。
    pub keyword: Option<String>,
    /// 共用企业主体 ID。
    pub party_id: Option<PartyId>,
    /// 启停状态筛选。
    pub status: Option<CustomerAccountStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

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
pub(crate) fn normalize_sort(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    allowed_fields: &'static [&'static str],
) -> Result<(&'static str, SortDir)> {
    let sort_by = match sort_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(field) => allowed_fields
            .iter()
            .find(|allowed| **allowed == field)
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("不支持的排序字段: {field}")))?,
        None => "created_at",
    };
    let sort_dir = match sort_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("asc") => SortDir::Asc,
        Some("desc") => SortDir::Desc,
        Some(other) => return Err(Error::ValidationError(format!("非法排序方向: {other}"))),
        None => SortDir::Desc,
    };
    Ok((sort_by, sort_dir))
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
#[derive(Debug, Clone, Serialize)]
pub struct PageView<T> {
    /// 当前页数据。
    pub items: Vec<T>,
    /// 满足筛选条件的总数（非当前页条数）。
    pub total: i64,
    /// 当前页码（1 起）。
    pub page: u64,
    /// 请求的分页大小。
    pub page_size: u32,
}

/// 校验文本去除首尾空白后非空。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 客户角色创建请求（HTTP 契约：`{ party_id, customer_no, owner_user_id, ... }`）。
///
/// 同事务建立 `customer_account` + 首条 `OWNER` 归属（W03：新建客户必带负责销售）；
/// `party` 必须已存在（D07 跨域读校验）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateCustomerRequest {
    /// 共用企业主体 ID。
    pub party_id: PartyId,
    /// 客户编号（全局唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "客户编号不能为空"))]
    pub customer_no: String,
    /// 默认客户付款条件引用（受控码表字典）。
    pub default_payment_term_id: Option<String>,
    /// 当前负责销售（账号或系统身份；D06 账号存在性校验）。
    #[validate(custom(function = "non_blank", message = "负责销售不能为空"))]
    pub owner_user_id: String,
    /// 归属生效开始日期。
    pub valid_from: BusinessDate,
    /// 归属生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 归属调整原因。
    #[validate(custom(function = "non_blank", message = "归属原因不能为空"))]
    pub change_reason: String,
    /// 启停状态；缺省视为启用。
    pub status: Option<CustomerAccountStatus>,
}

/// 客户角色更新请求（乐观锁；`party_id` 与 `customer_no` 为稳定身份不可修改）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateCustomerRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 默认付款条件；`None` 表示不修改，空字符串表示清除。
    pub default_payment_term_id: Option<String>,
    /// 启停状态；`None` 表示不修改。
    pub status: Option<CustomerAccountStatus>,
}

/// 客户角色响应视图（列表用，契约形状对齐 `customer_account` 投影行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerView {
    /// 实体主键。
    pub id: String,
    /// 共用企业主体 ID。
    pub party_id: String,
    /// 客户编号。
    pub customer_no: String,
    /// 默认客户付款条件引用。
    pub default_payment_term_id: Option<String>,
    /// 启停状态。
    pub status: CustomerAccountStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<CustomerAccount> for CustomerView {
    /// 从实体构造响应视图。
    fn from(account: CustomerAccount) -> Self {
        Self {
            id: account.base.id,
            party_id: account.party_id.to_string(),
            customer_no: account.customer_no,
            default_payment_term_id: account.default_payment_term_id,
            status: account.stable.status,
            version: account.base.version,
            created_at: account.base.created_at,
        }
    }
}

/// 客户角色详情视图：客户 + 主体身份 + 当前生效 OWNER。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerDetailView {
    /// 客户角色响应视图。
    #[serde(flatten)]
    pub account: CustomerView,
    /// 企业主体编号（D07 跨域读）。
    pub party_no: Option<String>,
    /// 当前法定名称（D07 当前修订快照）。
    pub legal_name: Option<String>,
    /// 当前生效负责销售。
    pub owner_user_id: Option<String>,
}

/// 客户角色列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomerListParams {
    /// 客户编号模糊搜索。
    pub keyword: Option<String>,
    /// 共用企业主体 ID（精确匹配）。
    pub party_id: Option<PartyId>,
    /// 启停状态筛选。
    pub status: Option<CustomerAccountStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`customer_no`/`status`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

impl CustomerListParams {
    /// 归一化客户角色列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<CustomerListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, CUSTOMER_SORT_FIELDS)?;
        Ok(CustomerListQuery {
            keyword: normalized_text(self.keyword.as_deref()),
            party_id: self.party_id.clone(),
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

/// 归属变更动作（W03：结束旧归属并建立新归属，不原地修改）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentAction {
    /// 建立新归属（同时结束同一客户同一角色的重叠旧归属；OWNER 恰好一人）。
    Assign,
    /// 提前结束既有归属（只允许结束有效期）。
    End,
}

/// 归属写入请求。
///
/// - `Assign`：`user_id`/`assignment_role`/`valid_from`/`valid_to` 必填；
///   同一客户同一角色、同一客户的 OWNER 均不允许重叠区间（§6.2）。
/// - `End`：`assignment_id` 必填，`valid_to` 必填（晚于该归属 `valid_from`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomerAssignmentRequest {
    /// 归属变更动作。
    pub action: AssignmentAction,
    /// 销售人员（`Assign` 必填）。
    pub user_id: Option<String>,
    /// 归属角色（`Assign` 必填）。
    pub assignment_role: Option<AssignmentRole>,
    /// 归属生效开始日期（`Assign` 必填）。
    pub valid_from: Option<BusinessDate>,
    /// 归属生效结束日期（`Assign`/`End` 均可选；`End` 必填）。
    pub valid_to: Option<BusinessDate>,
    /// 目标归属 ID（`End` 必填）。
    pub assignment_id: Option<String>,
    /// 调整原因。
    #[validate(custom(function = "non_blank", message = "调整原因不能为空"))]
    pub change_reason: String,
    /// 期望的乐观锁版本（`End` 必填）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
}

/// 归属响应视图（契约形状对齐 `customer_assignment` 投影行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerAssignmentView {
    /// 实体主键。
    pub id: String,
    /// 客户角色 ID。
    pub customer_id: String,
    /// 销售人员。
    pub user_id: String,
    /// 归属角色。
    pub assignment_role: AssignmentRole,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 调整原因。
    pub change_reason: String,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<CustomerAssignment> for CustomerAssignmentView {
    /// 从实体构造响应视图。
    fn from(assignment: CustomerAssignment) -> Self {
        Self {
            id: assignment.base.id,
            customer_id: assignment.customer_id.to_string(),
            user_id: assignment.user_id,
            assignment_role: assignment.assignment_role,
            valid_from: assignment.valid_from.to_string(),
            valid_to: assignment.valid_to.map(|date| date.to_string()),
            change_reason: assignment.change_reason,
            version: assignment.base.version,
            created_at: assignment.base.created_at,
        }
    }
}

/// 客户归属列表查询参数（`customer_id` 走路径参数）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomerAssignmentListParams {
    /// 销售人员筛选。
    pub user_id: Option<String>,
    /// 归属角色筛选。
    pub assignment_role: Option<AssignmentRole>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`valid_from`/`valid_to`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{normalize_sort, CustomerListParams, SortDir};
    use serde_json::json;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        let (field, direction) = normalize_sort(
            &Some(" customer_no ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "customer_no"],
        )
        .unwrap();
        assert_eq!(field, "customer_no");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn list_params_normalize_paging_filters_and_sort_defaults() {
        let params = CustomerListParams {
            keyword: Some(" C-20 ".to_string()),
            party_id: None,
            status: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.keyword.as_deref(), Some("C-20"));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn create_customer_request_deserializes_contract_shape() {
        let request: super::CreateCustomerRequest = serde_json::from_value(json!({
            "party_id": "party-1",
            "customer_no": "C-2026-001",
            "owner_user_id": "admin-1",
            "valid_from": "2026-01-01",
            "change_reason": "首次建档",
        }))
        .unwrap();
        assert_eq!(request.customer_no, "C-2026-001");
        assert!(request.status.is_none(), "status 缺省由 Service 按启用处理");
    }
}
