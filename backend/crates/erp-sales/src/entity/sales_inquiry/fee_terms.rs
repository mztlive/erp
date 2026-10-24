//! 办理历史费用时核对明确反馈轮次的金额与税率，不替代当前接受及额度守卫。

use erp_core::money::{Amount, Rate};

use super::{InquiryFee, InquiryFeedbackSnapshot, SalesInquiry};
use crate::{Error, Result};

impl SalesInquiry {
    /// 取得明确反馈轮次的冻结快照。
    ///
    /// # 参数
    /// `round` 为办理单据声明的原反馈轮次。
    /// # 返回
    /// 返回对应快照；不推断其是否为当前已接受办理依据。
    /// # 错误
    /// 反馈轮次不存在时拒绝。
    pub fn feedback_snapshot(&self, round: u32) -> Result<&InquiryFeedbackSnapshot> {
        self.feedbacks
            .iter()
            .find(|feedback| feedback.round == round)
            .ok_or_else(|| Error::BusinessLogicError("询价费用反馈不存在".into()))
    }

    /// 核对费用办理金额和税率与指定反馈一致。
    ///
    /// # 参数
    /// `round`、`fee_id` 指定稳定反馈费用；`amount`、`rate` 为办理条件。
    /// # 返回
    /// 返回已核对费用。当前接受、正金额及可用额度仍由占用或调整步骤校验。
    /// # 错误
    /// 轮次、费用不存在，税率不一致或金额超过反馈额时拒绝。
    pub fn fee_for_terms(
        &self,
        round: u32,
        fee_id: &str,
        amount: Amount,
        rate: Option<Rate>,
    ) -> Result<&InquiryFee> {
        let fee = self
            .feedback_snapshot(round)?
            .data
            .fees
            .iter()
            .find(|fee| fee.fee_id == fee_id)
            .ok_or_else(|| Error::BusinessLogicError("询价费用不存在".into()))?;
        if rate != Some(fee.tax_rate) || amount > fee.amount_gross {
            return Err(Error::ValidationError("费用税率或金额不符合已接受结果，请先重新询价".into()));
        }
        Ok(fee)
    }
}
