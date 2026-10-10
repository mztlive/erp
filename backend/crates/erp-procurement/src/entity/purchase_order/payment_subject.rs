//! 采购沿精确正式销售版本继承的我方付款主体。

use erp_core::ids::{PartyId, SalesOrderId, SalesOrderRevisionId};
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 采购提交、生效与正式变更保留的付款主体及来源身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentSubjectSnapshot {
    /// 来源正式销售版本的我方结算主体身份。
    pub settlement_party_id: PartyId,
    /// 来源正式销售版本的我方结算主体名称。
    pub settlement_party_name: String,
    /// 真实来源销售单身份。
    pub sales_order_id: SalesOrderId,
    /// 精确来源正式销售版本身份。
    pub sales_order_revision_id: SalesOrderRevisionId,
}

impl PaymentSubjectSnapshot {
    /// 从正式销售版本的已冻结身份和名称构造采购付款主体。
    ///
    /// # 参数
    /// * `settlement_party_id` - 正式销售版本的我方主体身份。
    /// * `settlement_party_name` - 同一正式版本冻结的主体名称。
    /// * `sales_order_id` - 来源销售单身份。
    /// * `sales_order_revision_id` - 来源正式销售版本身份。
    ///
    /// # 返回
    /// 返回完整、可追溯且名称已规范化的主体快照。
    ///
    /// # 错误
    /// 任一身份为空，或名称为空、超长时拒绝构造。
    pub fn new(
        settlement_party_id: PartyId,
        settlement_party_name: String,
        sales_order_id: SalesOrderId,
        sales_order_revision_id: SalesOrderRevisionId,
    ) -> Result<Self> {
        if [settlement_party_id.as_ref(), sales_order_id.as_ref(), sales_order_revision_id.as_ref()]
            .iter()
            .any(|id| id.trim().is_empty())
        {
            return Err(Error::from("采购付款主体及正式销售来源身份不能为空"));
        }
        let settlement_party_name = normalize_required_text(
            settlement_party_name,
            "来源正式销售版本缺少我方结算主体名称",
            256,
            "我方结算主体名称过长",
        )?;
        Ok(Self { settlement_party_id, settlement_party_name, sales_order_id, sales_order_revision_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_formal_source_and_normalizes_name() {
        let snapshot = PaymentSubjectSnapshot::new(
            "our-company".to_string().into(),
            "  我方公司  ".into(),
            "sales-1".to_string().into(),
            "sales-v1".to_string().into(),
        )
        .unwrap();
        assert_eq!(snapshot.settlement_party_id.as_ref(), "our-company");
        assert_eq!(snapshot.settlement_party_name, "我方公司");
        assert_eq!(snapshot.sales_order_revision_id.as_ref(), "sales-v1");
    }

    #[test]
    fn rejects_missing_subject_or_formal_source() {
        for ids in [("", "sales-1", "v1"), ("our-company", "", "v1"), ("our-company", "sales-1", "")] {
            assert!(
                PaymentSubjectSnapshot::new(
                    ids.0.to_string().into(),
                    "我方公司".into(),
                    ids.1.to_string().into(),
                    ids.2.to_string().into()
                )
                .is_err()
            );
        }
        assert!(
            PaymentSubjectSnapshot::new(
                "our-company".to_string().into(),
                " ".into(),
                "sales-1".to_string().into(),
                "v1".to_string().into()
            )
            .is_err()
        );
    }
}
