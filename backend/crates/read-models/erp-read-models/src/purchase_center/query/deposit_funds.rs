//! 原采购及财务账号的双重授权查询；所有金额在同一只读事务内取得。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_finance::entity::supplier_deposit::{DepositExecutionFundsSummary, SupplierDepositAccount};
use erp_finance::repository::supplier_deposit_query::{reservation_for_execution, usages_for_reservation};
use erp_procurement::entity::purchase_order::{PurchaseOrder, PurchaseOrderRevision};
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::api_order::{ApiOrderExecution, for_purchase};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::purchase_center::PurchaseOrderReadService;
use crate::{Error, Result};

/// 由组合层对真实资金账号执行主体、供应商组织与财务明细资格核验。
#[async_trait]
pub trait PurchaseDepositAccessPort: Send + Sync {
    /// 在原读取事务内核验资金账号归属及当前操作人范围。
    /// # 参数
    /// `account_id` 来自原执行组占用，`actor` 为认证人员，`executor` 为原事务。
    /// # 返回
    /// 已获账号、主体及组织许可的账户。
    /// # 错误
    /// 缺财务明细资格、跨主体或供应商范围时拒绝，不返回金额。
    async fn authorize(
        &self,
        account_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositAccount>;
}

impl PurchaseOrderReadService {
    /// 在采购及逐账号财务范围内读取当前执行组资金阶段。
    /// # 参数
    /// `id`、`expected_version` 来自已授权原采购视图；`actor` 为当前人员。
    /// `funds_access` 必须由组合根装配真实财务权限实现。
    /// # 返回
    /// 占用、扣款、核销及返款分别计量的安全摘要，不返回外部账号或资金凭据。
    /// # 错误
    /// 采购、财务权限、归属或金额不守恒时拒绝；无占用记录的执行不伪造金额摘要。
    pub async fn purchase_order_deposit_funds(
        &self,
        id: &str,
        expected_version: u64,
        actor: &AuditActor,
        funds_access: Arc<dyn PurchaseDepositAccessPort>,
    ) -> Result<Vec<DepositExecutionFundsSummary>> {
        let (db, access, id, actor) = (self.db.clone(), self.access(), id.to_owned(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (order, _, scope) = access.resolve_detail(&actor, &id, executor).await?;
                    if order.base.version != expected_version {
                        return Err(Error::ConflictError("采购版本已变化，请刷新后查看资金进度".into()));
                    }

                    let Some(revision_id) = order.stable.current_revision_id.as_deref() else {
                        scope.validate(executor).await?;
                        return Ok(vec![]);
                    };
                    let revision = db
                        .purchase_order_revisions()
                        .find_by_id(revision_id, executor)
                        .await?
                        .filter(|revision| revision.purchase_order_id.as_ref() == order.base.id)
                        .ok_or_else(|| Error::NotFound("采购当前版本缺失".into()))?;
                    let executions = for_purchase(&db, &order.base.id, Some(revision_id), executor).await?;
                    scope.validate(executor).await?;

                    let context = DepositPurchaseContext {
                        order: &order,
                        revision: &revision,
                        actor: &actor,
                        access: funds_access.as_ref(),
                    };
                    let mut summaries = Vec::new();
                    for execution in executions {
                        if let Some(summary) = context.summary(&db, &execution, executor).await? {
                            summaries.push(summary);
                        }
                    }
                    Ok(summaries)
                })
            })
            .await
    }
}

struct DepositPurchaseContext<'a> {
    order: &'a PurchaseOrder,
    revision: &'a PurchaseOrderRevision,
    actor: &'a AuditActor,
    access: &'a dyn PurchaseDepositAccessPort,
}

impl DepositPurchaseContext<'_> {
    async fn summary(
        &self,
        db: &Database,
        execution: &ApiOrderExecution,
        executor: &mut dyn Executor,
    ) -> Result<Option<DepositExecutionFundsSummary>> {
        if execution.supplier_id != self.order.supplier_id.as_ref() {
            return Err(Error::Forbidden("资金执行与原采购供应商不一致".into()));
        }

        let Some(reservation) = reservation_for_execution(db, &execution.base.id, executor).await? else {
            return Ok(None);
        };
        let account = self.access.authorize(&reservation.account_id, self.actor, executor).await?;
        if reservation.basis.purchase_order_id != self.order.base.id
            || reservation.basis.purchase_revision_id != self.revision.base.id
            || account.identity.supplier_id != self.order.supplier_id.as_ref()
            || account.identity.payer_party_id != self.revision.payment_subject.settlement_party_id.as_ref()
        {
            return Err(Error::Forbidden("资金记录与采购、付款主体或供应商归属不一致".into()));
        }

        let usages = usages_for_reservation(db, &reservation.base.id, executor).await?;
        Ok(Some(
            DepositExecutionFundsSummary::from_records(
                &execution.base.id,
                Some(&reservation),
                &usages,
                Some(&account),
            )
            .map_err(Error::Logic)?,
        ))
    }
}
