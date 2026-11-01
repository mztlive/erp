//! 从精确正式销售版本读取和复验采购付款主体。

use erp_core::ids::{SalesOrderId, SalesOrderRevisionId};
use erp_procurement::entity::purchase_order::PaymentSubjectSnapshot;
use erp_sales::entity::sales_order::SalesOrderRevision;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 读取采购引用的正式销售版本并保留其主体身份和名称。
///
/// # 参数
/// * `db` - 数据库。
/// * `sales_order_id` - 真实来源销售单。
/// * `revision_id` - 采购引用的精确正式版本。
/// * `executor` - 当前事务执行器。
///
/// # 返回
/// 返回正式销售来源证明完整的采购主体快照。
///
/// # 错误
/// 版本不存在、来源身份错配或正式主体资料缺失时拒绝。
pub(super) async fn load_payment_subject(
    db: &Database,
    sales_order_id: &SalesOrderId,
    revision_id: &SalesOrderRevisionId,
    executor: &mut dyn Executor,
) -> Result<PaymentSubjectSnapshot> {
    let revision = db
        .sales_order_revisions()
        .find_by_id(revision_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源正式销售版本不存在".into()))?;
    subject_from_revision(sales_order_id, &revision)
}

fn subject_from_revision(
    sales_order_id: &SalesOrderId,
    revision: &SalesOrderRevision,
) -> Result<PaymentSubjectSnapshot> {
    if revision.sales_order_id != *sales_order_id || revision.base.is_deleted() {
        return Err(Error::ConflictError("采购引用的正式销售版本与来源销售单不一致".into()));
    }
    let name = revision
        .settlement_party_snapshot
        .as_ref()
        .ok_or_else(|| Error::ConflictError("来源正式销售版本缺少我方结算主体名称".into()))?;
    Ok(PaymentSubjectSnapshot::new(
        revision.settlement_party_id.clone(),
        name.settlement_party_name.clone(),
        sales_order_id.clone(),
        revision.base.id.clone().into(),
    )?)
}

/// 复验已冻结采购主体，禁止用当前销售资料补写历史依据。
///
/// # 参数
/// * `db` - 数据库。
/// * `frozen` - 已冻结采购主体与正式销售来源。
/// * `executor` - 当前事务执行器。
///
/// # 返回
/// 精确正式来源仍与冻结值一致时成功。
///
/// # 错误
/// 来源缺失或任一冻结字段发生变化时拒绝执行。
pub(super) async fn revalidate_payment_subject(
    db: &Database,
    frozen: &PaymentSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<()> {
    let proven =
        load_payment_subject(db, &frozen.sales_order_id, &frozen.sales_order_revision_id, executor).await?;
    if proven != *frozen {
        return Err(Error::ConflictError("采购付款主体与冻结正式销售依据不一致".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::PartyId;
    use erp_core::money::Amount;
    use erp_sales::entity::sales_order::{HeaderSnapshotData, RevisionSource, SalesOrderRevisionData};

    use super::*;

    fn revision() -> SalesOrderRevision {
        let zero: Amount = "0".parse().unwrap();
        SalesOrderRevision::new(
            SalesOrderRevisionId::new("sales-v1"),
            SalesOrderRevisionData {
                sales_order_id: SalesOrderId::new("sales-1"),
                settlement_party_id: PartyId::new("our-company"),
                revision_no: 1,
                revision_source: RevisionSource::ErpApproval,
                previous_revision_id: None,
                content_hash: "sub:submission-1".into(),
                customer_revision_id: None,
                contract_revision_id: None,
                snapshot: HeaderSnapshotData {
                    customer_name: "客户".into(),
                    settlement_party_name: Some("我方公司".into()),
                    payment_term_code: "NET30".into(),
                    payment_term_name: "月结30天".into(),
                    invoice_type: "增值税专用发票".into(),
                    tax_point: "6".into(),
                    ..Default::default()
                },
                project_name: None,
                business_remark: None,
                voucher_category_sku_id: None,
                voucher_expiry_at: None,
                gross_amount: zero,
                net_amount: zero,
                tax_amount: zero,
                effective_at: Instant::from_unix_secs(1),
                recorded_at: Instant::from_unix_secs(1),
            },
        )
        .unwrap()
    }

    #[test]
    fn subject_uses_exact_formal_revision_and_preserves_frozen_identity() {
        let mut formal = revision();
        let frozen = subject_from_revision(&SalesOrderId::new("sales-1"), &formal).unwrap();
        assert_eq!(frozen.settlement_party_id.as_ref(), "our-company");
        assert_eq!(frozen.sales_order_revision_id.as_ref(), "sales-v1");
        formal.settlement_party_id = PartyId::new("other-company");
        let changed = subject_from_revision(&SalesOrderId::new("sales-1"), &formal).unwrap();
        assert_ne!(frozen, changed);
        assert_eq!(frozen.settlement_party_name, "我方公司");
    }

    #[test]
    fn subject_rejects_unrelated_source_and_missing_frozen_name() {
        let mut formal = revision();
        assert!(subject_from_revision(&SalesOrderId::new("unrelated-sales"), &formal).is_err());
        formal.settlement_party_snapshot = None;
        assert!(subject_from_revision(&SalesOrderId::new("sales-1"), &formal).is_err());
    }
}
