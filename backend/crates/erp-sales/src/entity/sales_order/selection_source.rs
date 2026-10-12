//! 销售方案来源：仅作为开单追溯，不代替合同与凭证。
use erp_core::ids::CustomerAccountId;
use serde::{Deserialize, Serialize};

use super::BusinessType;
use crate::entity::sales_selection::{SalesSelectionProposal, SubmitMode};
use crate::{Error, Result};

/// 服务端从已授权方案生成的不可编辑来源快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesSelectionSource {
    /// 方案稳定身份。
    pub proposal_id: String,
    /// 方案业务编号。
    pub proposal_no: String,
}

impl SalesSelectionSource {
    /// 核验来源模式、客户和销售业务性质。
    /// # 参数
    /// `proposal` 为已授权方案；`customer_id`、`business_type` 为目标销售单事实。
    /// # 返回
    /// 返回方案来源快照。
    /// # 错误
    /// 非按份采购、客户不一致或非实物与服务销售时拒绝。
    pub fn from_proposal(
        proposal: &SalesSelectionProposal,
        customer_id: &CustomerAccountId,
        business_type: BusinessType,
    ) -> Result<Self> {
        if proposal.submit_mode != SubmitMode::ByQuantity {
            return Err(Error::ValidationError("仅按份采购方案支持创建销售单".into()));
        }
        if &proposal.customer_id != customer_id {
            return Err(Error::ValidationError("销售单客户必须与来源销售方案一致".into()));
        }
        if business_type != BusinessType::GoodsService {
            return Err(Error::ValidationError("按份采购方案仅支持实物与服务销售单".into()));
        }
        Ok(Self { proposal_id: proposal.base.id.clone(), proposal_no: proposal.proposal_no.clone() })
    }
}
