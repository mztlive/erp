//! 费用商品归属与明确反馈轮次的商业条款；历史条款核对不替代当前接受及额度守卫。

use erp_core::common::time::Instant;
use erp_core::money::{Amount, Rate};

use super::{InquiryFee, InquiryFeedbackSnapshot, InquiryReservationState, SalesInquiry};
use crate::{Error, Result};

impl SalesInquiry {
    /// 核对已接受费用与来源销售已办理商品的归属关系。
    /// 结果：销售存在已办理商品，且至少一项落在费用商品范围内时通过；费用无指定行时适用全部商品。
    /// 失败条件：先拒绝未接受反馈或不存在的费用，再拒绝缺少匹配已办理商品的销售。
    ///
    /// 输入约定：`round`、`fee_id` 为已接受反馈费用；`sales_order_id` 为待分摊销售，`now` 为业务时刻。
    pub fn ensure_fee_sales_attribution(
        &self,
        round: u32,
        fee_id: &str,
        sales_order_id: &str,
        now: Instant,
    ) -> Result<()> {
        let feedback = self.accepted_feedback(round, now)?;
        let fee = feedback
            .data
            .fees
            .iter()
            .find(|fee| fee.fee_id == fee_id)
            .ok_or_else(|| Error::NotFound("询价费用不存在".into()))?;

        let matches = self
            .quantity_ledger
            .iter()
            .filter(|row| {
                row.sales_order_id == sales_order_id && row.state == InquiryReservationState::Handled
            })
            .flat_map(|row| &row.selections)
            .any(|selection| fee.line_ids.is_empty() || fee.line_ids.contains(&selection.line_id));
        if !matches {
            return Err(Error::BusinessLogicError("销售单不在本费用关联的商品范围内".into()));
        }
        Ok(())
    }

    /// 取得明确反馈轮次的冻结快照。
    /// 返回对应快照；不推断其是否为当前已接受办理依据。
    /// 反馈轮次不存在时拒绝。
    ///
    /// 输入约定：`round` 为办理单据声明的原反馈轮次。
    pub fn feedback_snapshot(&self, round: u32) -> Result<&InquiryFeedbackSnapshot> {
        self.feedbacks
            .iter()
            .find(|feedback| feedback.round == round)
            .ok_or_else(|| Error::BusinessLogicError("询价费用反馈不存在".into()))
    }

    /// 核对费用办理金额和税率与指定反馈一致。
    /// 返回已核对费用。当前接受、正金额及可用额度仍由占用或调整步骤校验。
    /// 轮次、费用不存在，税率不一致或金额超过反馈额时拒绝。
    ///
    /// 输入约定：`round`、`fee_id` 指定稳定反馈费用；`amount`、`rate` 为办理条件。
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
