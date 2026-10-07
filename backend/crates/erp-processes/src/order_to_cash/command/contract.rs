//! 销售单后补合同：仅变更稳定关联，不改动已冻结销售内容。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_contract::{ContractExt, ContractStatus};
use erp_core::ids::ContractRevisionId;
use erp_read_models::sales_center::order::dto::SalesOrderDetailView;
use erp_sales::dto::sales_order::BindSalesOrderContractRequest;
use erp_sales::entity::sales_order::contract_terms::{ContractBindingCheck, ContractTerms};
use erp_sales::entity::sales_order::{InvoiceRequirementSnapshot, PaymentTermSnapshot, SalesOrder};
use erp_sales::repository::SalesOrderExt;
use erp_sales::service::sales_order::SalesOrderService;
use erp_sales::service::sales_order::contract_binding::ContractBindingBasis;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::super::SalesOrderCommandProcess;
use super::super::authorization::SalesCommandAccess;
use crate::audit::persist_log;
use crate::{Error, Result};

impl SalesOrderCommandProcess {
    /// 在原草稿保存事务内持久化首次合同绑定，历史提交和正式版本保持不变。
    ///
    /// # 参数
    /// * `db` / `access` / `executor` - 当前授权与草稿事务
    /// * `order` - 服务端准备完成的稳定关系
    /// * `revision_id` - 本次工作副本引用的合同修订
    /// # 返回
    /// 原单未绑定而本次准备已绑定时更新稳定对象，其余情况无写入。
    /// # 错误
    /// 版本变化、已有合同变化或更新失败时拒绝。
    pub(in crate::order_to_cash) async fn persist_first_contract_binding(
        db: &Database,
        access: &SalesCommandAccess,
        order: &SalesOrder,
        revision_id: Option<&ContractRevisionId>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = access.current(&order.base.id, executor).await?;
        if current.contract_id == order.contract_id {
            return Ok(());
        }
        if !current.matches_version(order.base.version) || current.contract_id.is_some() {
            return Err(Error::ConflictError("销售单合同或版本已变化，请刷新后重试".into()));
        }
        Self::check_first_contract_binding(db, access, &current, order, revision_id, executor).await?;
        let mut updated = order.clone();
        db.sales_orders().update(&mut updated, executor).await?;
        Ok(())
    }

    /// 给未关联合同的销售单首次补录合同，保持原单客户、结算主体和商业快照。
    ///
    /// # 参数
    /// * `id` - 销售单稳定身份
    /// * `req` - 当前单据版本及合同当前有效修订
    /// * `actor` - 当前具有原单 update 范围的修改人
    /// # 返回
    /// 返回更新后的销售单详情。
    /// # 错误
    /// 越权、版本冲突、合同无效、已有合同或关系不一致时拒绝。
    pub async fn bind_sales_order_contract(
        &self,
        id: &str,
        req: BindSalesOrderContractRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        req.validate()?;
        let access = self.command_access(actor, "update")?;
        let db = self.db.clone();
        let client = db.client().clone();
        let id_owned = id.to_string();
        let actor = actor.clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut order = access.current(&id_owned, executor).await?;
                    if !order.matches_version(req.version) {
                        return Err(Error::ConflictError("销售单版本已变化，请刷新后重试".into()));
                    }
                    verify_and_bind_contract(&db, &access, &mut order, &req, &actor, executor).await?;
                    db.sales_orders().update(&mut order, executor).await?;
                    let audit = actor.resource_log("sales_order.bind_contract", "sales_order", id_owned)?;
                    persist_log(&db, &audit, executor).await?;
                    Ok::<(), Error>(())
                })
            })
            .await?;
        self.read_model().sales_order_detail(id, None).await.map_err(Error::from)
    }
}

impl SalesOrderCommandProcess {
    /// 核对后补合同条款，供确认前展示差异。
    ///
    /// # 参数
    /// * `id` / `req` - 原单身份、版本及所选合同修订
    /// * `actor` - 具有原单修改权限的当前用户
    /// # 返回
    /// 返回条款来源及逐项核对结果。
    /// # 错误
    /// 越权、关系冲突、来源缺失或版本变化时拒绝。
    pub async fn check_sales_order_contract(
        &self,
        id: &str,
        req: BindSalesOrderContractRequest,
        actor: &AuditActor,
    ) -> Result<ContractBindingCheck> {
        req.validate()?;
        let access = self.command_access(actor, "update")?;
        let order = access.current(id, &mut NoTransaction).await?;
        let (check, _) = contract_binding_check(&self.db, &access, &order, &req, &mut NoTransaction).await?;
        Ok(check)
    }

    /// 保存、补开草稿及提交事务内的首次绑定准入。
    ///
    /// # 参数
    /// * `db` / `access` / `executor` - 原命令数据库、授权与事务
    /// * `current` / `prepared` - 持久化原单与本次准备的稳定关系
    /// * `revision_id` - 本次草稿实际引用的合同修订
    /// # 返回
    /// 无首次绑定或全部条款一致时通过；由调用方继续写入草稿及原单。
    /// # 错误
    /// 条款不同、合同修订漂移或缺少依据时拒绝。
    pub(in crate::order_to_cash) async fn check_first_contract_binding(
        db: &Database,
        access: &SalesCommandAccess,
        current: &SalesOrder,
        prepared: &SalesOrder,
        revision_id: Option<&ContractRevisionId>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if current.contract_id == prepared.contract_id {
            return Ok(());
        }
        let req = BindSalesOrderContractRequest {
            version: current.base.version,
            contract_id: prepared
                .contract_id
                .clone()
                .ok_or_else(|| Error::ConflictError("不能移除销售单合同".into()))?,
            requested_contract_revision_id: revision_id
                .cloned()
                .ok_or_else(|| Error::ConflictError("缺少所选合同版本，请刷新后重试".into()))?,
        };
        let (check, _) = contract_binding_check(db, access, current, &req, executor).await?;
        check.ensure_matches()?;
        Ok(())
    }
}

/// 在原事务重验关联和已保存条款；写回草稿以串行化并发编辑。
async fn verify_and_bind_contract(
    db: &Database,
    access: &SalesCommandAccess,
    order: &mut SalesOrder,
    req: &BindSalesOrderContractRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let (check, basis) = contract_binding_check(db, access, order, req, executor).await?;
    check.ensure_matches()?;
    if let Some(mut copy) = basis.working_copy {
        db.sales_order_working_copies().update(&mut copy, executor).await?;
    }
    order.bind_contract(
        req.contract_id.clone(),
        &order.customer_id.clone(),
        &order.settlement_party_id.clone(),
        actor.id(),
    )?;
    Ok(())
}

/// 与最终写入共用同一校验入口，预检只返回差异，不产生业务写入。
async fn contract_binding_check(
    db: &Database,
    access: &SalesCommandAccess,
    order: &SalesOrder,
    req: &BindSalesOrderContractRequest,
    executor: &mut dyn Executor,
) -> Result<(ContractBindingCheck, ContractBindingBasis)> {
    if !order.matches_version(req.version) {
        return Err(Error::ConflictError("销售单版本已变化，请刷新后重试".into()));
    }
    let contract = access.load_contract(req.contract_id.as_ref(), executor).await?;
    access.related(req.contract_id.as_ref(), contract.customer_id.as_ref(), executor).await?;
    if contract.stable.status != ContractStatus::Effective {
        return Err(Error::BusinessLogicError("只能补录当前已生效合同".into()));
    }
    if contract.stable.current_revision_id.as_deref() != Some(req.requested_contract_revision_id.as_ref()) {
        return Err(Error::ConflictError("所选合同版本已变化，请刷新后重试".into()));
    }
    let revision = db
        .contract_revisions()
        .find_by_id(req.requested_contract_revision_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("合同版本不存在".into()))?;
    if !revision.belongs_to_contract(&req.contract_id)
        || !revision.matches_settlement_party(&contract.settlement_party_id)
    {
        return Err(Error::ConflictError("合同版本归属或结算主体已变化，请刷新后重试".into()));
    }
    // 在副本上执行领域关系守卫；预检不修改原对象。
    order.clone().bind_contract(
        req.contract_id.clone(),
        &contract.customer_id,
        &contract.settlement_party_id,
        "contract-check",
    )?;
    let basis = SalesOrderService::new(db.clone()).contract_binding_basis(order, executor).await?;
    let terms = ContractTerms {
        payment: PaymentTermSnapshot {
            payment_term_code: revision.payment_term_snapshot.payment_term_code,
            payment_term_name: revision.payment_term_snapshot.payment_term_name,
        },
        invoice: InvoiceRequirementSnapshot {
            invoice_type: revision.invoice_requirement_snapshot.invoice_type,
            tax_point: revision.invoice_requirement_snapshot.tax_point,
        },
    };
    Ok((basis.terms.compare(&terms, basis.label.clone()), basis))
}
