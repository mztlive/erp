//! 开票来源及购销双方资料统一读取，支持查询及提交事务复用。
use erp_core::ids::PartyId;
use erp_finance::entity::receivable::ReceivableAccount;
use erp_finance::entity::receivable::invoice_request_source::InvoiceRequestSource;
use erp_party::repository::PartyDomainRepository;
use erp_sales::entity::sales_order::CommercialStatus;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 购方取应收往来主体，销方取来源销售单当前生效版本的我方结算主体。
///
/// # 参数
/// * `account` - 本次申请绑定的应收。
/// * `executor` - 查询执行器或提交事务执行器。
///
/// # 返回
/// 返回准入事实；缺少主体资料以空字段表示，不使用客户简称或其他主体资料补位。
///
/// # 错误
/// 销售单不存在或仓储读取失败时返回错误。
pub async fn load(
    db: &Database,
    account: &ReceivableAccount,
    executor: &mut dyn Executor,
) -> Result<InvoiceRequestSource> {
    let order = db
        .sales_orders()
        .find_by_id(&account.sales_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".into()))?;

    let (invoice_title, tax_number) = party_identity(db, &account.counterparty_party_id, executor).await?;
    let (seller_name, seller_tax_number) = match order.current_revision_id() {
        Some(id) if order.customer_id == account.customer_id => {
            let revision = db
                .sales_order_revisions()
                .find_by_id(id.as_ref(), executor)
                .await?
                .filter(|revision| revision.sales_order_id == account.sales_order_id)
                .ok_or_else(|| Error::ConflictError("来源销售单生效版本不匹配".into()))?;
            party_identity(db, &revision.settlement_party_id, executor).await?
        },
        _ => (String::new(), String::new()),
    };
    Ok(InvoiceRequestSource {
        effective: order.commercial_status == CommercialStatus::Effective
            && order.current_revision_id().is_some()
            && order.customer_id == account.customer_id,
        invoice_title,
        tax_number,
        seller_name,
        seller_tax_number,
    })
}

async fn party_identity(
    db: &Database,
    id: &PartyId,
    executor: &mut dyn Executor,
) -> Result<(String, String)> {
    Ok(match PartyDomainRepository::new(db).find_with_current_revision(id, executor).await? {
        Some((party, revision)) => (
            revision.map(|row| row.legal_name).unwrap_or_default().trim().to_owned(),
            party.unified_credit_code.unwrap_or_default().trim().to_owned(),
        ),
        None => (String::new(), String::new()),
    })
}
