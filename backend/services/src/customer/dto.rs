//! 域 D08 `customer` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；业务日期一律 `YYYY-MM-DD`；时间一律秒级时间戳。

use entities::common::time::BusinessDate;
use entities::customer::{AssignmentRole, CustomerAccount, CustomerAccountStatus, CustomerAssignment};
use entities::ids::PartyId;
use entities::party::AddressType;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 客户角色列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const CUSTOMER_SORT_FIELDS: &[&str] = &["created_at", "updated_at", "customer_no", "status"];
/// 客户归属列表允许的排序字段白名单。
pub(crate) const CUSTOMER_ASSIGNMENT_SORT_FIELDS: &[&str] = &["created_at", "valid_from", "valid_to"];

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
    /// 服务端执行的数据范围。
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
        Self {
            id: account.base.id,
            party_id: account.party_id.to_string(),
            party_no: None,
            legal_name: None,
            short_name: None,
            customer_no: account.customer_no,
            default_payment_term_id: account.default_payment_term_id,
            status: account.stable.status,
            owner_user_id: None,
            owner_user_name: None,
            collaborator_count: 0,
            scope_tags: Vec::new(),
            version: account.base.version,
            created_at: account.base.created_at,
            updated_at: account.base.updated_at,
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
    /// 数据范围；缺省为当前用户负责的客户。
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
        Ok(CustomerListQuery {
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

/// 客户资料根命令中的联系人输入。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomerProfileContactInput {
    /// 既有事实行 ID；提供且内容未变化时原样保留。
    pub existing_id: Option<String>,
    /// 联系人姓名。
    #[validate(custom(function = "non_blank", message = "联系人姓名不能为空"))]
    pub contact_name: String,
    /// 职务或用途。
    pub title: Option<String>,
    /// 手机号明文；只在当前请求内存在。
    pub mobile: Option<String>,
    /// 固话。
    pub telephone: Option<String>,
    /// 邮箱。
    pub email: Option<String>,
    /// 是否默认联系人。
    pub is_default: bool,
}

/// 客户资料根命令中的地址输入。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomerProfileAddressInput {
    /// 既有事实行 ID；提供且内容未变化时原样保留。
    pub existing_id: Option<String>,
    /// 地址类型。
    pub address_type: AddressType,
    /// 地址联系人。
    pub contact_name: Option<String>,
    /// 地址明文；只在当前请求内存在。
    pub address: Option<String>,
    /// 是否默认地址。
    pub is_default: bool,
}

/// 客户资料根命令中的银行账户输入。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomerProfileBankAccountInput {
    /// 既有事实行 ID；提供时保留稳定银行账户，只允许调整默认标记。
    pub existing_id: Option<String>,
    /// 户名。
    #[validate(custom(function = "non_blank", message = "户名不能为空"))]
    pub account_name: String,
    /// 银行名称。
    #[validate(custom(function = "non_blank", message = "银行名称不能为空"))]
    pub bank_name: String,
    /// 支行名称。
    pub bank_branch_name: Option<String>,
    /// 银行账号明文；只在当前请求内存在。
    pub account_number: Option<String>,
    /// 是否默认账户。
    pub is_default: bool,
}

/// 创建或修订完整客户资料的根级命令。
///
/// 修订时 `contacts`、`addresses`、`bank_accounts` 缺省表示保留；显式空数组
/// 表示结束该类全部当前事实；非空数组表示结束旧事实后写入新的当前集合。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SaveCustomerProfileRequest {
    /// 客户端幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
    /// 修订时必填的 Party 乐观锁版本。
    pub expected_party_version: Option<u64>,
    /// 修订时必填的客户乐观锁版本。
    pub expected_customer_version: Option<u64>,
    /// 法定名称。
    #[validate(custom(function = "non_blank", message = "法定名称不能为空"))]
    pub legal_name: String,
    /// 简称；修订时空字符串表示清空。
    pub short_name: Option<String>,
    /// 统一社会信用代码；修订时空字符串表示清空。
    pub unified_credit_code: Option<String>,
    /// 默认付款条件稳定代码；修订时空字符串表示清空。
    pub default_payment_term_id: Option<String>,
    /// 客户状态；修订时缺省表示保留。
    pub status: Option<CustomerAccountStatus>,
    /// 兼容旧客户端的负责销售字段；创建时忽略并由创建人写入 OWNER，修订时不得提交。
    pub owner_user_id: Option<String>,
    /// 联系人当前集合；缺省表示修订时保留。
    pub contacts: Option<Vec<CustomerProfileContactInput>>,
    /// 地址当前集合；缺省表示修订时保留。
    pub addresses: Option<Vec<CustomerProfileAddressInput>>,
    /// 银行账户当前集合；缺省表示修订时保留，提交该字段需要银行账户写权限。
    pub bank_accounts: Option<Vec<CustomerProfileBankAccountInput>>,
    /// 从属事实生效日期。
    pub effective_from: BusinessDate,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
}

/// 客户资料根命令的稳定结果，也用于幂等查询。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerProfileMutationView {
    /// 发起命令的账号 ID，仅供服务端授权判断，不进入响应 JSON。
    #[serde(skip_serializing)]
    pub initiated_by: String,
    /// 客户角色 ID。
    pub customer_id: String,
    /// 客户编号。
    pub customer_no: String,
    /// Party ID。
    pub party_id: String,
    /// 当前 Party 修订 ID。
    pub revision_id: String,
    /// 当前 Party 修订号。
    pub revision_no: u32,
    /// 保存后的客户乐观锁版本。
    pub customer_version: u64,
    /// 保存后的 Party 乐观锁版本。
    pub party_version: u64,
    /// 从属事实生效日期。
    pub effective_from: String,
    /// 命令记录时间（秒级时间戳）。
    pub recorded_at: u64,
    /// 变更原因。
    pub change_reason: String,
}

/// 客户详情中的单个敏感字段揭示入口。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerSensitiveFieldView {
    /// 敏感字段类型。
    pub kind: crate::party::SensitiveFieldKind,
    /// 事实行 ID。
    pub record_id: String,
    /// 掩码展示值。
    pub masked_value: String,
    /// 受字段、事实行和客户约束的短时令牌。
    pub reveal_token: String,
    /// 令牌过期时间（Unix 秒）。
    pub expires_at: u64,
}

/// 敏感字段揭示请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RevealCustomerSensitiveRequest {
    /// 详情接口签发的短时令牌。
    #[validate(custom(function = "non_blank", message = "揭示令牌不能为空"))]
    pub reveal_token: String,
}

/// 敏感字段揭示结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerSensitiveRevealView {
    /// 解密后的明文；仅返回给已通过字段权限校验的当前请求。
    pub value: String,
}

/// 页面动作阻断原因。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerActionBlockerView {
    /// 页面动作稳定代码。
    pub action: String,
    /// 阻断原因稳定代码。
    pub code: String,
    /// 面向操作者的原因与下一步。
    pub message: String,
}

/// 客户资料对象中心的完整事实视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerProfileDetailView {
    /// 客户角色。
    #[serde(flatten)]
    pub account: CustomerView,
    /// Party 状态。
    pub party_status: entities::party::PartyStatus,
    /// Party 乐观锁版本。
    pub party_version: u64,
    /// 统一社会信用代码。
    pub unified_credit_code: Option<String>,
    /// 当前名称修订。
    pub current_revision: crate::party::PartyRevisionView,
    /// 名称修订历史，按修订号倒序。
    pub revisions: Vec<crate::party::PartyRevisionView>,
    /// 归属历史。
    pub assignments: Vec<CustomerAssignmentView>,
    /// 当前有效联系人。
    pub contacts: Vec<crate::party::PartyContactView>,
    /// 当前有效地址。
    pub addresses: Vec<crate::party::PartyAddressView>,
    /// 当前有效税务资料。
    pub tax_profiles: Vec<crate::party::PartyTaxProfileView>,
    /// 当前有效银行账户摘要。
    pub bank_accounts: Vec<crate::party::PartyBankAccountView>,
    /// 敏感字段短时揭示入口。
    pub sensitive_fields: Vec<CustomerSensitiveFieldView>,
    /// 由 HTTP 权限与客户状态共同计算的允许动作。
    pub allowed_actions: Vec<String>,
    /// 当前状态导致的动作阻断原因。
    pub action_blockers: Vec<CustomerActionBlockerView>,
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
    pub version: Option<u64>,
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
    /// 销售人员展示名。
    pub user_name: String,
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
        let user_name = assignment.user_id.clone();
        Self {
            id: assignment.base.id,
            customer_id: assignment.customer_id.to_string(),
            user_id: assignment.user_id,
            user_name,
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
            scope: super::CustomerScope::Mine,
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
    fn assigned_scope_and_assign_action_accept_stable_wire_codes() {
        let params: CustomerListParams = serde_json::from_value(json!({
            "scope": "assigned"
        }))
        .unwrap();
        assert_eq!(params.scope, super::CustomerScope::Assigned);

        let request: super::CustomerAssignmentRequest = serde_json::from_value(json!({
            "action": "assign",
            "user_id": "admin-2",
            "assignment_role": "COLLABORATOR",
            "valid_from": "2026-08-08",
            "change_reason": "联合跟进"
        }))
        .unwrap();
        assert!(request.version.is_none());
    }
}
