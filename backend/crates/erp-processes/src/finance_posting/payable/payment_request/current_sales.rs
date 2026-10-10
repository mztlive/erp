//! 当前正式销售来源证明；销售草稿和当前可变主体不构成付款依据。

use erp_procurement::entity::purchase_order::PaymentSubjectSnapshot;
use erp_sales::repository::SalesOrderExt;

use super::*;

/// 同一事务复验当前正式销售与本笔采购冻结主体一致，申请归组按主体身份。
/// # 参数
/// * `db` / `executor` - 付款资格校验所使用的数据库和原事务执行器
/// * `frozen` - 精确采购正式版本保存的销售来源及付款主体
/// * `request` - 已冻结付款申请的付款主体身份；表头名称不替换各来源历史名称
/// # 返回
/// 当前正式销售来源和主体仍满足原批准依据时返回成功。
/// # 错误
/// 正式来源缺失、版本推进、身份或主体错配时返回冲突，不读取可变草稿。
pub(super) async fn revalidate(
    db: &Database,
    frozen: &PaymentSubjectSnapshot,
    request: &PurchasePaymentRequest,
    executor: &mut dyn Executor,
) -> Result<()> {
    let sales = db
        .sales_orders()
        .find_by_id(frozen.sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("付款申请来源正式销售单不存在".into()))?;
    let revision = db
        .sales_order_revisions()
        .find_by_id(frozen.sales_order_revision_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("付款申请来源正式销售版本不存在".into()))?;
    validate(
        frozen,
        &request.payer_party_id,
        CurrentSalesFacts {
            order_id: &sales.base.id,
            current_revision_id: sales.stable.current_revision_id.as_deref(),
            revision_id: &revision.base.id,
            revision_order_id: revision.sales_order_id.as_ref(),
            payer_party_id: revision.settlement_party_id.as_ref(),
            payer_party_name: revision
                .settlement_party_snapshot
                .as_ref()
                .map(|snapshot| snapshot.settlement_party_name.as_str()),
        },
    )
}

struct CurrentSalesFacts<'a> {
    order_id: &'a str,
    current_revision_id: Option<&'a str>,
    revision_id: &'a str,
    revision_order_id: &'a str,
    payer_party_id: &'a str,
    payer_party_name: Option<&'a str>,
}

fn validate(
    frozen: &PaymentSubjectSnapshot,
    request_payer_id: &str,
    current: CurrentSalesFacts<'_>,
) -> Result<()> {
    if current.order_id != frozen.sales_order_id.as_ref()
        || current.current_revision_id != Some(frozen.sales_order_revision_id.as_ref())
        || current.revision_id != frozen.sales_order_revision_id.as_ref()
        || current.revision_order_id != frozen.sales_order_id.as_ref()
        || current.payer_party_id != frozen.settlement_party_id.as_ref()
        || current.payer_party_name != Some(frozen.settlement_party_name.as_str())
        || request_payer_id != frozen.settlement_party_id.as_ref()
    {
        return Err(Error::ConflictError(
            "付款申请来源正式销售版本或主体已变化，请先完成正式采购变更并处理未付申请".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{PartyId, SalesOrderId, SalesOrderRevisionId};

    use super::*;

    fn frozen() -> PaymentSubjectSnapshot {
        PaymentSubjectSnapshot::new(
            PartyId::new("company-a"),
            "我方公司甲".into(),
            SalesOrderId::new("sales-1"),
            SalesOrderRevisionId::new("sales-v1"),
        )
        .unwrap()
    }

    fn current() -> CurrentSalesFacts<'static> {
        CurrentSalesFacts {
            order_id: "sales-1",
            current_revision_id: Some("sales-v1"),
            revision_id: "sales-v1",
            revision_order_id: "sales-1",
            payer_party_id: "company-a",
            payer_party_name: Some("我方公司甲"),
        }
    }

    #[test]
    fn exact_current_formal_source_authorizes_without_reading_mutable_draft() {
        validate(&frozen(), "company-a", current()).unwrap();
    }

    #[test]
    fn one_company_can_authorize_sources_with_each_historical_frozen_name() {
        let before = frozen();
        let after = PaymentSubjectSnapshot::new(
            PartyId::new("company-a"),
            "我方公司新名称".into(),
            SalesOrderId::new("sales-2"),
            SalesOrderRevisionId::new("sales-2-v1"),
        )
        .unwrap();
        validate(&before, "company-a", current()).unwrap();
        validate(
            &after,
            "company-a",
            CurrentSalesFacts {
                order_id: "sales-2",
                current_revision_id: Some("sales-2-v1"),
                revision_id: "sales-2-v1",
                revision_order_id: "sales-2",
                payer_party_id: "company-a",
                payer_party_name: Some("我方公司新名称"),
            },
        )
        .unwrap();
        assert_ne!(before.settlement_party_name, after.settlement_party_name);
    }

    #[test]
    fn advanced_or_missing_current_formal_revision_rejects_old_frozen_authority() {
        for revision in [None, Some("sales-v2")] {
            let mut facts = current();
            facts.current_revision_id = revision;
            assert!(matches!(validate(&frozen(), "company-a", facts), Err(Error::ConflictError(_))));
        }
    }

    #[test]
    fn mismatched_formal_identity_or_company_cannot_authorize_payment() {
        let mut variants = Vec::new();
        let mut facts = current();
        facts.order_id = "unrelated-sales";
        variants.push(facts);
        let mut facts = current();
        facts.revision_id = "sales-v2";
        variants.push(facts);
        let mut facts = current();
        facts.revision_order_id = "unrelated-sales";
        variants.push(facts);
        let mut facts = current();
        facts.payer_party_id = "company-b";
        variants.push(facts);
        let mut facts = current();
        facts.payer_party_name = Some("我方公司乙");
        variants.push(facts);
        let mut facts = current();
        facts.payer_party_name = None;
        variants.push(facts);
        for facts in variants {
            assert!(matches!(validate(&frozen(), "company-a", facts), Err(Error::ConflictError(_))));
        }
        assert!(matches!(validate(&frozen(), "company-b", current()), Err(Error::ConflictError(_))));
    }
}
