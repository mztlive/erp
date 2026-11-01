//! 询价费用来源引用；只有组合流程核验已接受结果与余额后才能采用。

use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 费用来源及分摊身份。与收款供应商分开保存，允许安全追溯。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryExpenseSource {
    /// 询价稳定身份。
    pub inquiry_id: String,
    /// 销售接受的反馈轮次。
    pub feedback_round: u32,
    /// 跨反馈轮次保持稳定的费用身份。
    pub fee_id: String,
    /// 费用在本销售单上的分摊身份。
    pub allocation_id: String,
}

impl InquiryExpenseSource {
    /// 比较跨反馈版本保持不变的费用来源身份。
    /// # 参数
    /// `other` 为新旧采购内容中的来源引用。
    /// # 返回
    /// 询价、费用和销售分摊身份一致时返回真；反馈轮次由采用事务另行核验。
    /// # 错误
    /// 不返回错误。
    pub fn same_identity(&self, other: &Self) -> bool {
        self.inquiry_id == other.inquiry_id
            && self.fee_id == other.fee_id
            && self.allocation_id == other.allocation_id
    }
    /// 校验来源身份；实际资格与额度由事务内流程重验。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回规范化的来源引用。
    /// # 错误
    /// 身份为空、超长或反馈轮次为零时拒绝。
    pub fn normalized(&self) -> Result<Self> {
        if self.feedback_round == 0 {
            return Err(Error::from("询价反馈轮次无效"));
        }
        let text = |value: &String| {
            normalize_required_text(value.clone(), "询价费用来源不能为空", 128, "询价费用来源过长")
        };
        Ok(Self {
            inquiry_id: text(&self.inquiry_id)?,
            feedback_round: self.feedback_round,
            fee_id: text(&self.fee_id)?,
            allocation_id: text(&self.allocation_id)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_requires_stable_identity_and_positive_round() {
        let source = InquiryExpenseSource {
            inquiry_id: " iq-1 ".into(),
            feedback_round: 1,
            fee_id: "fee-1".into(),
            allocation_id: "allocation-1".into(),
        };
        assert_eq!(source.normalized().unwrap().inquiry_id, "iq-1");
        assert!(InquiryExpenseSource { feedback_round: 0, ..source.clone() }.normalized().is_err());
        assert!(InquiryExpenseSource { fee_id: " ".into(), ..source }.normalized().is_err());
    }
}
