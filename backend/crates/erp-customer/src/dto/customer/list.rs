//! 客户角色列表、创建/更新请求与列表/详情视图。

use application_core::{non_blank, normalized_text, page_or_default, page_size_or_default};
use erp_core::common::time::BusinessDate;
use erp_core::ids::PartyId;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{SortDir, normalize_sort};
use crate::entity::customer::{AssignmentRole, CustomerAccount, CustomerAccountStatus, CustomerAssignment};
use crate::error::{Error, Result};

/// 客户角色列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const CUSTOMER_SORT_FIELDS: &[&str] = &["created_at", "updated_at", "customer_no", "status"];

/// 客户目录的数据范围。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomerScope {
    /// 当前用户为负责销售的客户。
    #[default]
    Mine,
    /// 当前用户为协作销售的客户。
    Collaborating,
    /// 当前用户以负责或协作身份参与的全部客户。
    Assigned,
    /// 当前权限允许读取的全部客户。
    AllAuthorized,
}

impl CustomerScope {
    /// 装配目录范围标签（纯展示规则下沉自 Service）。
    ///
    /// # 参数
    /// * `assignments` - 同一客户的归属行
    /// * `owner_user_id` - 已解析的主负责人
    /// * `actor_user_id` - 当前账号
    /// * `requested` - 页面请求的目录范围
    ///
    /// # 返回
    /// 返回命中原因标签，保证包含请求范围。
    pub(crate) fn tags_for(
        assignments: &[CustomerAssignment],
        owner_user_id: Option<&str>,
        actor_user_id: &str,
        requested: Self,
    ) -> Vec<Self> {
        let mut scope_tags = Vec::new();
        if owner_user_id == Some(actor_user_id) {
            scope_tags.push(Self::Mine);
        }
        if assignments.iter().any(|assignment| {
            assignment.assignment_role == AssignmentRole::Collaborator && assignment.user_id == actor_user_id
        }) {
            scope_tags.push(Self::Collaborating);
        }
        if !scope_tags.contains(&requested) {
            scope_tags.push(requested);
        }
        scope_tags
    }
}

/// 归一化后的客户角色列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CustomerListQuery {
    /// 当前负责人精确身份条件。
    pub owner_user_ids: Option<application_core::QueryIds>,
    /// 当前主负责人所属组织，只收窄授权结果。
    pub org_unit_ids: Option<application_core::QueryIds>,
    /// 组织筛选是否包含有效下级。
    pub include_descendants: Option<bool>,
    /// 客户编号模糊搜索。
    pub keyword: Option<String>,
    /// 共用企业主体 ID。
    pub party_id: Option<PartyId>,
    /// 启停状态筛选。
    pub status: Option<CustomerAccountStatus>,
    /// 目录范围标签，只收窄授权结果。
    pub scope: CustomerScope,
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

/// 客户角色创建请求（HTTP 契约：`{ party_id, customer_no, ... }`）。
///
/// 同事务建立 `customer_account` + 首条 `OWNER` 归属；负责销售固定为创建人。
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
    /// 兼容旧客户端的负责销售字段；服务端忽略该值，首条 OWNER 固定为创建人。
    pub owner_user_id: Option<String>,
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
    /// 企业主体编号。
    pub party_no: Option<String>,
    /// 当前法定名称。
    pub legal_name: Option<String>,
    /// 当前简称。
    pub short_name: Option<String>,
    /// 客户编号。
    pub customer_no: String,
    /// 默认客户付款条件引用。
    pub default_payment_term_id: Option<String>,
    /// 启停状态。
    pub status: CustomerAccountStatus,
    /// 当前负责销售账号 ID。
    pub owner_user_id: Option<String>,
    /// 当前负责销售展示名。
    pub owner_user_name: Option<String>,
    /// 当前协作销售人数。
    pub collaborator_count: u32,
    /// 当前结果行命中的服务端范围标签。
    pub scope_tags: Vec<CustomerScope>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 最后更新时间（秒级时间戳）。
    pub updated_at: u64,
}

impl From<CustomerAccount> for CustomerView {
    /// 从实体构造响应视图。
    fn from(account: CustomerAccount) -> Self {
        Self::from_account_parts(
            account.base.id,
            account.party_id.to_string(),
            account.customer_no,
            account.default_payment_term_id,
            account.stable.status,
            account.base.version,
            account.base.created_at,
            account.base.updated_at,
        )
    }
}

impl CustomerView {
    /// 从投影行构造响应视图的内核（erp-customer-007）。
    ///
    /// 实体路径（字段留空待 hydrate）与行投影路径共用同一内核；
    /// hydrate 补齐（主体身份、负责人、范围标签）由调用方后续步骤完成。
    ///
    /// # 参数
    /// * `id` - 实体主键
    /// * `party_id` - 共用企业主体 ID
    /// * `customer_no` - 客户编号
    /// * `default_payment_term_id` - 默认付款条件引用
    /// * `status` - 启停状态
    /// * `version` - 乐观锁版本
    /// * `created_at` - 创建时间
    /// * `updated_at` - 最后更新时间
    ///
    /// # 返回
    /// 返回待 hydrate 的客户视图。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_account_parts(
        id: String,
        party_id: String,
        customer_no: String,
        default_payment_term_id: Option<String>,
        status: CustomerAccountStatus,
        version: u64,
        created_at: u64,
        updated_at: u64,
    ) -> Self {
        Self {
            id,
            party_id,
            party_no: None,
            legal_name: None,
            short_name: None,
            customer_no,
            default_payment_term_id,
            status,
            owner_user_id: None,
            owner_user_name: None,
            collaborator_count: 0,
            scope_tags: Vec::new(),
            version,
            created_at,
            updated_at,
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CustomerListParams {
    /// 跨页与导出必须使用前一页的当前授权和业务版本。
    #[validate(length(min = 1, max = 256))]
    pub scope_version: Option<String>,
    /// 当前业务负责人 ID，逗号分隔，最多 100 项；只收窄授权结果。
    pub owner_user_ids: Option<application_core::QueryIds>,
    /// 当前主负责人所属组织，逗号分隔，最多 100 项；只收窄授权结果。
    pub org_unit_ids: Option<application_core::QueryIds>,
    /// 组织筛选是否包含有效下级；缺省为 false。
    pub include_descendants: Option<bool>,
    /// 客户编号模糊搜索。
    pub keyword: Option<String>,
    /// 共用企业主体 ID（精确匹配）。
    pub party_id: Option<PartyId>,
    /// 启停状态筛选。
    pub status: Option<CustomerAccountStatus>,
    /// 目录范围标签；缺省为当前用户负责的客户，只收窄授权结果。
    #[serde(default)]
    pub scope: CustomerScope,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`updated_at`/`customer_no`/`status`）。
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
        if self.include_descendants == Some(true) && self.org_unit_ids.is_none() {
            return Err(Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        Ok(CustomerListQuery {
            owner_user_ids: self.owner_user_ids.clone(),
            org_unit_ids: self.org_unit_ids.clone(),
            include_descendants: self.include_descendants,
            keyword: normalized_text(self.keyword.as_deref()),
            party_id: self.party_id.clone(),
            status: self.status,
            scope: self.scope,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{CustomerListParams, CustomerScope, SortDir, normalize_sort};
    use crate::entity::customer::AssignmentRole;

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
            scope: CustomerScope::Mine,
            ..Default::default()
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.keyword.as_deref(), Some("C-20"));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn list_params_accept_scope_version_and_org_filters() {
        let params: CustomerListParams = serde_json::from_value(json!({
            "scope_version": "v1",
            "org_unit_ids": "org-2,org-1",
            "include_descendants": true,
            "page": 2
        }))
        .unwrap();
        let query = params.normalized().unwrap();
        assert_eq!(params.scope_version.as_deref(), Some("v1"));
        assert_eq!(query.org_unit_ids.unwrap().as_slice(), &["org-1", "org-2"]);
        assert_eq!(query.include_descendants, Some(true));
        assert!(
            CustomerListParams {
                include_descendants: Some(true),
                scope: CustomerScope::Mine,
                ..Default::default()
            }
            .normalized()
            .is_err()
        );
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
        assert_eq!(request.owner_user_id.as_deref(), Some("admin-1"));
        assert!(request.status.is_none(), "status 缺省由 Service 按启用处理");
    }

    #[test]
    fn create_customer_request_accepts_missing_owner() {
        let request: super::CreateCustomerRequest = serde_json::from_value(json!({
            "party_id": "party-1",
            "customer_no": "C-2026-002",
            "valid_from": "2026-01-01",
            "change_reason": "首次建档",
        }))
        .unwrap();
        assert!(request.owner_user_id.is_none());
    }

    #[test]
    fn assigned_scope_accepts_stable_wire_code() {
        let params: CustomerListParams = serde_json::from_value(json!({
            "scope": "assigned"
        }))
        .unwrap();
        assert_eq!(params.scope, CustomerScope::Assigned);
    }

    #[test]
    fn scope_tags_cover_mine_collaborating_and_requested() {
        use erp_core::common::time::BusinessDate;
        use erp_core::ids::CustomerAccountId;

        use crate::entity::customer::{CustomerAssignment, CustomerAssignmentData, CustomerAssignmentId};

        let owner = CustomerAssignment::new(
            CustomerAssignmentId::new("o"),
            CustomerAssignmentData {
                customer_id: CustomerAccountId::new("c-1"),
                user_id: "actor-1".to_string(),
                assignment_role: AssignmentRole::Owner,
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: None,
                change_reason: "首次指派".to_string(),
            },
        )
        .unwrap();
        let tags = CustomerScope::tags_for(&[owner], Some("actor-1"), "actor-1", CustomerScope::Mine);
        assert_eq!(tags, vec![CustomerScope::Mine]);
        let tags = CustomerScope::tags_for(&[], None, "actor-1", CustomerScope::AllAuthorized);
        assert_eq!(tags, vec![CustomerScope::AllAuthorized]);
    }
}
