//! 银行账户创建、更新、视图与列表查询 DTO。

use application_core::non_blank;
use erp_core::common::time::BusinessDate;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::masked_last4;
use crate::entity::party::{EffectiveRecordStatus, PartyBankAccount};

/// 银行账户创建请求（HTTP 契约为账号明文；实体只保留指纹与密文）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreatePartyBankAccountRequest {
    /// ERP 内部稳定账户编号（全局唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "账户编号不能为空"))]
    pub bank_account_no: String,
    /// 户名。
    #[validate(custom(function = "non_blank", message = "户名不能为空"))]
    pub account_name: String,
    /// 银行名称。
    #[validate(custom(function = "non_blank", message = "银行名称不能为空"))]
    pub bank_name: String,
    /// 支行名称。
    pub bank_branch_name: Option<String>,
    /// 账号（明文入参；低熵敏感值 §4.5.5）。
    #[validate(custom(function = "non_blank", message = "账号不能为空"))]
    pub account_number: String,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 是否为当前默认账户。
    pub is_default: bool,
    /// 启停状态；缺省视为启用。
    pub status: Option<EffectiveRecordStatus>,
}

/// 银行账户更新请求（仅生命周期字段）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdatePartyBankAccountRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 启停状态；`None` 表示不修改。
    pub status: Option<EffectiveRecordStatus>,
    /// 生效结束日期；`None` 表示不修改。
    pub valid_to: Option<BusinessDate>,
    /// 默认标记；`None` 表示不修改。
    pub is_default: Option<bool>,
}

/// 银行账户响应视图（契约形状对齐 `party_bank_account` 投影行；不含敏感字段）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyBankAccountView {
    /// 实体主键。
    pub id: String,
    /// ERP 内部稳定账户编号。
    pub bank_account_no: String,
    /// 所属企业主体 ID。
    pub party_id: String,
    /// 户名。
    pub account_name: String,
    /// 银行名称。
    pub bank_name: String,
    /// 银行账号掩码；列表与详情均不返回明文。
    pub account_number_masked: String,
    /// 支行名称。
    pub bank_branch_name: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认账户。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<PartyBankAccount> for PartyBankAccountView {
    /// 从实体构造响应视图。
    fn from(account: PartyBankAccount) -> Self {
        Self {
            id: account.base.id,
            bank_account_no: account.bank_account_no,
            party_id: account.party_id.to_string(),
            account_name: account.account_name,
            bank_name: account.bank_name,
            account_number_masked: masked_last4(&account.account_number_last4),
            bank_branch_name: account.bank_branch_name,
            valid_from: account.valid_from.to_string(),
            valid_to: account.valid_to.map(|date| date.to_string()),
            is_default: account.is_default,
            status: account.status,
            version: account.base.version,
            created_at: account.base.created_at,
        }
    }
}

/// 银行账户列表查询参数（`party_id` 走路径参数）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PartyBankAccountListParams {
    /// 启停状态筛选。
    pub status: Option<EffectiveRecordStatus>,
    /// 默认标记筛选。
    pub is_default: Option<bool>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`bank_account_no`/`valid_from`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}
