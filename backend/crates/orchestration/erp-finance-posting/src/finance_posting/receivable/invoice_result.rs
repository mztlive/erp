//! 发票命令回读复用原过账入口及数据库中的完整实际销售来源。

use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_core::ids::InvoiceId;
use erp_finance::dto::receivable::InvoiceView;
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use erp_finance::service::receivable::ReceivableService;
use erp_finance::service::receivable::invoice_commit::ensure_sales_invoice;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::ReceivableProcess;
use crate::{Error, Result};

impl ReceivableProcess {
    /// 按原命令的统一授权入口回读提交或重放结果。
    /// # 参数
    /// id 为回执绑定的实际发票身份，actor 为当前操作人。
    /// # 返回
    /// 同一事务内完成来源授权和财务读取的结果。
    /// # 错误
    /// 撤权、来源缺失、空来源及读取失败保持错误；不要求详情功能权限。
    pub(super) async fn authorized_invoice_result(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<InvoiceView> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let sources = invoice_sources(&db, &id, executor).await?;
                    rbac.funds.require_sales_sources(&actor, "invoice:post", &sources, executor).await?;
                    let view = ReceivableService::new(db.clone())
                        .invoice_detail_with_executor(&id, executor)
                        .await?;
                    rbac.funds.require_sales_sources(&actor, "invoice:post", &sources, executor).await?;
                    Ok(view)
                })
            })
            .await
    }
}

/// 请求保留其真实销售来源；实际分配全部展开，任何缺失子账整体拒绝。
async fn invoice_sources(db: &Database, id: &str, executor: &mut dyn Executor) -> Result<Vec<String>> {
    let invoice =
        db.invoices().find_by_id(id, executor).await?.ok_or_else(|| Error::NotFound("发票不存在".into()))?;
    ensure_sales_invoice(&invoice)?;
    let allocations =
        db.sales_invoice_allocations().find_allocations_by_invoices(&[InvoiceId::new(id)], executor).await?;
    let mut accounts =
        allocations.into_iter().map(|line| line.receivable_account_id.to_string()).collect::<BTreeSet<_>>();
    let mut sources = BTreeSet::new();
    if let Some(request_id) = invoice.sales_invoice_request_id {
        let request = db
            .sales_invoice_requests()
            .find_by_id(&request_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("发票关联开票申请不存在".into()))?;
        accounts.insert(request.receivable_account_id.to_string());
        sources.insert(request.sales_order_id.to_string());
    }
    for account_id in accounts {
        let account = db
            .receivable_accounts()
            .find_by_id(&account_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("发票实际来源子账不存在".into()))?;
        sources.insert(account.sales_order_id.to_string());
    }
    Ok(sources.into_iter().collect())
}
