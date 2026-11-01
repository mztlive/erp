//! 开票来源和购方资料统一读取，支持查询及提交事务复用。
use erp_finance::entity::receivable::ReceivableAccount;
use erp_finance::entity::receivable::invoice_request_source::InvoiceRequestSource;
use erp_party::repository::PartyDomainRepository;
use erp_sales::entity::sales_order::CommercialStatus;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 读取本应收对应销售来源及结算主体当前法定名称、统一社会信用代码。
/// # 参数
/// * `db` - 数据库。
/// * `account` - 本次申请绑定的应收。
/// * `executor` - 查询执行器或提交事务执行器。
/// # 返回
/// 返回准入事实；缺少主体资料以空字段表示，不使用客户简称或其他主体资料补位。
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
    let party = PartyDomainRepository::new(db)
        .find_with_current_revision(&account.counterparty_party_id, executor)
        .await?;
    let (invoice_title, tax_number) = match party {
        Some((party, revision)) => (
            revision.map(|revision| revision.legal_name).unwrap_or_default(),
            party.unified_credit_code.unwrap_or_default(),
        ),
        None => (String::new(), String::new()),
    };
    Ok(InvoiceRequestSource {
        effective: order.commercial_status == CommercialStatus::Effective
            && order.current_revision_id().is_some()
            && order.customer_id == account.customer_id,
        invoice_title,
        tax_number,
    })
}
