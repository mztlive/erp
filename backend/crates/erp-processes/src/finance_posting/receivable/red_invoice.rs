//! 按原蓝票一次开具红票并红冲分配。

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::ids::{InvoiceId, ReceivableAccountId};
use erp_finance::entity::receivable::{
    AllocationAction, Invoice, InvoiceData, InvoiceDirection, InvoiceKind,
};
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_finance::service::receivable::red_invoice_plan::{
    purchase_red_invoice_allocation_plan, sales_red_invoice_allocation_plan,
};
use erp_read_models::finance::receivable::snapshot::zero_amount;
use id_generator::next_id;
use persistence_core::Transactional;
use sha2::{Digest, Sha256};
use validator::Validate;

use super::dto::{CommitRedInvoiceRequest, InvoiceView};
use super::invoice::register_created_invoice_document;
use super::{ReceivableProcess, invoice_task};
use crate::{Error, Result};

impl ReceivableProcess {
    /// 按原蓝票一次开具红票并红冲（§8.3-3 事务不变量）。
    ///
    /// 服务端在同一事务内读取原票的有效分配、计算本次反向行、创建红票、
    /// 冲减应收或应付子账进度并写审计。客户端不得提交分配 ID、净额或税额。
    /// 部分红冲时原蓝票保持已登记；全部剩余金额红冲后才置为已红冲。
    ///
    /// # 参数
    /// * `id` - 原蓝票 ID
    /// * `req` - 红票业务意图与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建红票视图。
    ///
    /// # 错误
    /// * `NotFound` - 原蓝票或有效分配不存在
    /// * `ConflictError` - 红票号码重复
    /// * `BusinessLogicError` - 红冲累计超过原分配或超额红冲
    ///
    /// # 约束
    /// 领域计划只计算金额；ID 生成、事务、写入、任务同步和审计继续由 Service 持有。
    pub async fn issue_red_invoice(
        &self,
        id: &str,
        req: CommitRedInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<InvoiceView> {
        req.validate()?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let object_read = std::sync::Arc::clone(&self.object_read);
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let digest = hex::encode(Sha256::digest(
            format!("{}|{}|{}", actor.id(), id, req.idempotency_key.trim()).as_bytes(),
        ));
        let red_no = req
            .invoice_no
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("HT-{}", &digest[..12]));
        let requested_amount = req.amount;
        let reason = req.reason.trim().to_string();
        let original_id = id.to_string();
        let red_invoice_id = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let original = db
                        .invoices()
                        .find_by_id(&original_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("原蓝票不存在".to_string()))?;
                    if !original.is_registered() || original.invoice_kind != InvoiceKind::Blue {
                        return Err(Error::BusinessLogicError("只有已登记的蓝票可以被红冲".to_string()));
                    }
                    let allocation_plan = match original.invoice_direction {
                        InvoiceDirection::Sales => {
                            let blue = db
                                .sales_invoice_allocations()
                                .find_allocations_by_invoices(
                                    &[InvoiceId::new(original.base.id.clone())],
                                    executor,
                                )
                                .await?;
                            let account_ids = blue
                                .iter()
                                .filter(|line| line.allocation_action == AllocationAction::Apply)
                                .map(|line| line.receivable_account_id.clone())
                                .collect::<Vec<_>>();
                            let related = db
                                .sales_invoice_allocations()
                                .find_allocations_by_accounts(&account_ids, executor)
                                .await?;
                            sales_red_invoice_allocation_plan(&blue, &related, requested_amount)?
                        },
                        InvoiceDirection::Purchase => {
                            let blue = db
                                .purchase_invoice_allocations()
                                .find_allocations_by_invoices(
                                    &[InvoiceId::new(original.base.id.clone())],
                                    executor,
                                )
                                .await?;
                            let account_ids = blue
                                .iter()
                                .filter(|line| {
                                    line.allocation_action
                                        == erp_finance::entity::payable::AllocationAction::Apply
                                })
                                .map(|line| line.payable_account_id.clone())
                                .collect::<Vec<_>>();
                            let related = db
                                .purchase_invoice_allocations()
                                .find_allocations_by_accounts(&account_ids, executor)
                                .await?;
                            purchase_red_invoice_allocation_plan(&blue, &related, requested_amount)?
                        },
                    };
                    let (red_gross, red_net, red_tax) = allocation_plan.totals();

                    if let Some(existing) = db
                        .invoices()
                        .find_by_direction_and_normalized_no(
                            original.invoice_direction,
                            &red_no.to_uppercase(),
                            executor,
                        )
                        .await?
                    {
                        if existing.invoice_kind == InvoiceKind::Red
                            && existing.original_invoice_id.as_ref()
                                == Some(&InvoiceId::new(original.base.id.clone()))
                            && existing.gross_amount == red_gross
                            && existing.net_amount == red_net
                            && existing.tax_amount == red_tax
                        {
                            return Ok::<String, crate::Error>(existing.base.id);
                        }
                        return Err(Error::ConflictError("红票号码已登记，请勿重复提交".to_string()));
                    }

                    let red_invoice_id = InvoiceId::new(next_id());
                    let mut red_mut = Invoice::new(
                        red_invoice_id.clone(),
                        InvoiceData {
                            invoice_direction: original.invoice_direction,
                            invoice_kind: InvoiceKind::Red,
                            party_id: original.party_id.clone(),
                            invoice_code: original.invoice_code.clone(),
                            invoice_no: red_no.clone(),
                            invoice_date: erp_core::common::time::BusinessDate::today(),
                            gross_amount: red_gross,
                            net_amount: red_net,
                            tax_amount: red_tax,
                            rounding_adjustment_amount: zero_amount(),
                            rounding_reason: None,
                            original_invoice_id: Some(original.base.id.clone().into()),
                        },
                        &actor_id,
                    )?;
                    red_mut.mark_registered(&actor_id)?;
                    let mut original_mut = original;
                    register_created_invoice_document(
                        &db,
                        &rbac,
                        object_read.as_ref(),
                        &red_mut,
                        &actor_owned,
                        executor,
                    )
                    .await?;
                    let mut sales_order_account_ids =
                        erp_finance::service::receivable::red_invoice_posting::persist_red_invoice(
                            &db,
                            &red_mut,
                            &mut original_mut,
                            &allocation_plan,
                            &actor_id,
                            executor,
                        )
                        .await?;
                    let audit = actor_owned.clone().resource_log_with_message(
                        "invoice.red_issue",
                        "invoice",
                        red_mut.base.id.clone(),
                        Some(reason.clone()),
                    )?;
                    db.audit_logs().create(&audit, executor).await?;
                    if original_mut.invoice_direction == InvoiceDirection::Sales {
                        sales_order_account_ids.sort();
                        sales_order_account_ids.dedup();
                        for account_id in &sales_order_account_ids {
                            invoice_task::sync_sales_invoice_task(
                                &db,
                                &ReceivableAccountId::new(account_id.clone()),
                                invoice_task::SalesInvoiceTaskChange::RedInvoiceIssued,
                                executor,
                            )
                            .await?;
                        }
                        let mut sales_order_ids = Vec::new();
                        for account in db
                            .receivable_accounts()
                            .find_accounts_by_ids(&sales_order_account_ids, executor)
                            .await?
                        {
                            sales_order_ids.push(account.sales_order_id.to_string());
                        }
                        sales_order_ids.sort();
                        sales_order_ids.dedup();
                        for sales_order_id in sales_order_ids {
                            crate::order_to_cash::progress::update_sales_order_money_progress(
                                &db,
                                executor,
                                &erp_core::ids::SalesOrderId::new(sales_order_id),
                                actor_id.clone(),
                                None,
                            )
                            .await?;
                        }
                    }
                    Ok::<String, crate::Error>(red_invoice_id.to_string())
                })
            })
            .await?;

        self.finance.invoice_detail(&red_invoice_id).await.map_err(Error::from)
    }
}
