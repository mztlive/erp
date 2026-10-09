//! 选品册访问与个人选品明细合同。

use erp_core::common::time::Instant;
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{ProposalSkuLineView, PublicSelectionPageView};
use crate::entity::sales_selection::SelectionRecipient;

/// 密码与提货券解锁请求；禁止将请求调试打印到日志。
#[derive(Clone, Serialize, Deserialize, Validate)]
pub struct UnlockSelectionRequest {
    /// 选品册访问密码。
    pub password: String,
    /// 提货券模式必填的个人券码。
    pub voucher_code: Option<String>,
}

/// 解锁结果；授权仅对应本册、本参与人与当前密码版本。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicSelectionAccessView {
    /// 后续公开请求携带的短期访问令牌。
    pub access_token: String,
    /// 本人的公开选品页。
    pub page: PublicSelectionPageView,
}

/// 设置或更换选品册访问密码。
#[derive(Clone, Serialize, Deserialize, Validate)]
pub struct SalesSelectionPasswordRequest {
    /// 册版本。
    pub expected_version: u64,
    /// 幂等键。
    pub idempotency_key: String,
    /// 新访问密码。
    #[validate(length(min = 8, max = 64))]
    pub access_password: String,
}

/// 提货券分发视图；仅授权的管理端可读取券码。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectionVoucherView {
    /// 个人提货券代码。
    pub voucher_code: String,
    /// 参与人身份。
    pub participant_id: String,
    /// 是否已提交。
    pub submitted: bool,
    /// 本人生成的销售方案。
    pub proposal_id: Option<String>,
}

/// 导出的一项商品明细，沿用销售方案冻结的 SKU 事实。
pub type ProposalItem = ProposalSkuLineView;

/// 全册选品明细导出行，每行对应一人的完整提交。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectionDetailView {
    /// 参与人身份。
    pub participant_id: String,
    /// 个人券码；普通选品方案为空。
    pub voucher_code: Option<String>,
    /// 销售方案身份。
    pub proposal_id: String,
    /// 销售方案编号。
    pub proposal_no: String,
    /// 提交时间。
    pub submitted_at: Instant,
    /// 提交时冻结的收件信息。
    pub recipient: Option<SelectionRecipient>,
    /// 根据服务器价格重算的方案合计。
    pub total_amount: Option<Amount>,
    /// 此人已选商品明细。
    pub items: Vec<ProposalItem>,
}
