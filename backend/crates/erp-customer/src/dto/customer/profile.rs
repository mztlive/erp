//! 客户资料根命令输入、敏感字段揭示与对象中心视图。

use application_core::non_blank;
use erp_core::common::time::BusinessDate;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::assignment::CustomerAssignmentView;
use super::list::CustomerView;
use crate::dto::party_snapshot::{
    AddressType, PartyAddressView, PartyBankAccountView, PartyContactView, PartyRevisionView, PartyStatus,
    PartyTaxProfileView, SensitiveFieldKind,
};
use crate::entity::customer::CustomerAccountStatus;

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

impl CustomerProfileContactInput {
    /// 以联系人姓名构造联系人输入，其余可选字段保持空条件。
    ///
    /// # 参数
    /// * `contact_name` - 联系人姓名
    ///
    /// # 返回
    /// 返回仅携带姓名的联系人输入。
    ///
    /// # 错误
    /// 无。
    pub fn new(contact_name: impl Into<String>) -> Self {
        Self {
            existing_id: None,
            contact_name: contact_name.into(),
            title: None,
            mobile: None,
            telephone: None,
            email: None,
            is_default: false,
        }
    }

    /// 设置手机号明文。
    ///
    /// # 参数
    /// * `mobile` - 手机号明文
    ///
    /// # 返回
    /// 返回更新后的联系人输入。
    ///
    /// # 错误
    /// 无。
    pub fn with_mobile(mut self, mobile: impl Into<String>) -> Self {
        self.mobile = Some(mobile.into());
        self
    }

    /// 设置是否默认联系人。
    ///
    /// # 参数
    /// * `is_default` - 是否默认联系人
    ///
    /// # 返回
    /// 返回更新后的联系人输入。
    ///
    /// # 错误
    /// 无。
    pub fn with_is_default(mut self, is_default: bool) -> Self {
        self.is_default = is_default;
        self
    }
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
    pub kind: SensitiveFieldKind,
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

/// 返回客户状态产生的业务动作阻断原因。
///
/// 阻断动作集合以 [`CustomerAccountStatus::blocked_actions`] 为准，本函数只
/// 负责组装面向操作者的稳定代码与文案。
///
/// # 参数
/// * `status` - 客户角色启停状态
///
/// # 返回
/// 启用客户返回空集合；停用客户返回禁止新合同和新销售单的稳定阻断原因。
pub fn customer_status_blockers(status: CustomerAccountStatus) -> Vec<CustomerActionBlockerView> {
    status
        .blocked_actions()
        .iter()
        .map(|action| CustomerActionBlockerView {
            action: (*action).to_string(),
            code: "CUSTOMER_DISABLED".to_string(),
            message: "客户已停用，请先恢复客户后再发起新业务".to_string(),
        })
        .collect()
}

/// 客户资料对象中心的完整事实视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerProfileDetailView {
    /// 客户角色。
    #[serde(flatten)]
    pub account: CustomerView,
    /// Party 状态。
    pub party_status: PartyStatus,
    /// Party 乐观锁版本。
    pub party_version: u64,
    /// 统一社会信用代码。
    pub unified_credit_code: Option<String>,
    /// 当前名称修订。
    pub current_revision: PartyRevisionView,
    /// 名称修订历史，按修订号倒序。
    pub revisions: Vec<PartyRevisionView>,
    /// 归属历史。
    pub assignments: Vec<CustomerAssignmentView>,
    /// 当前有效联系人。
    pub contacts: Vec<PartyContactView>,
    /// 当前有效地址。
    pub addresses: Vec<PartyAddressView>,
    /// 当前有效税务资料。
    pub tax_profiles: Vec<PartyTaxProfileView>,
    /// 当前有效银行账户摘要。
    pub bank_accounts: Vec<PartyBankAccountView>,
    /// 敏感字段短时揭示入口。
    pub sensitive_fields: Vec<CustomerSensitiveFieldView>,
    /// 由 HTTP 权限与客户状态共同计算的允许动作。
    pub allowed_actions: Vec<String>,
    /// 当前状态导致的动作阻断原因。
    pub action_blockers: Vec<CustomerActionBlockerView>,
}

#[cfg(test)]
mod tests {
    use validator::Validate;

    use super::CustomerProfileContactInput;

    #[test]
    fn contact_input_new_carries_only_name() {
        let input = CustomerProfileContactInput::new("测试联系人");
        assert_eq!(input.contact_name, "测试联系人");
        assert_eq!(input.existing_id, None);
        assert_eq!(input.mobile, None);
        assert!(!input.is_default);
        assert!(input.validate().is_ok());

        let chained = CustomerProfileContactInput::new("联系人").with_mobile("13800000000");
        assert_eq!(chained.mobile.as_deref(), Some("13800000000"));

        let empty = CustomerProfileContactInput::new("   ");
        assert!(empty.validate().is_err());
    }
}
