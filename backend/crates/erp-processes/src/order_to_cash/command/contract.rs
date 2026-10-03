//! 销售单后补合同：仅变更稳定关联，不改动已冻结销售内容。

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_contract::{ContractExt, ContractStatus};
use erp_read_models::sales_center::order::dto::SalesOrderDetailView;
use erp_sales::dto::sales_order::BindSalesOrderContractRequest;
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::super::SalesOrderCommandProcess;
use super::super::authorization::SalesCommandAccess;
use crate::{Error, Result};

impl SalesOrderCommandProcess {
    /// 在原草稿保存事务内持久化首次合同绑定，历史提交和正式版本保持不变。
    ///
    /// # 参数
    /// * `db` / `access` / `executor` - 当前授权与草稿事务
    /// * `order` - 服务端准备完成的稳定关系
    /// # 返回
    /// 原单未绑定而本次准备已绑定时更新稳定对象，其余情况无写入。
    /// # 错误
    /// 版本变化、已有合同变化或更新失败时拒绝。
    pub(in crate::order_to_cash) async fn persist_first_contract_binding(
        db: &Database,
        access: &SalesCommandAccess,
        order: &SalesOrder,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = access.current(&order.base.id, executor).await?;
        if current.contract_id == order.contract_id {
            return Ok(());
        }
        if !current.matches_version(order.base.version) || current.contract_id.is_some() {
            return Err(Error::ConflictError("销售单合同或版本已变化，请刷新后重试".into()));
        }
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
                    db.audit_logs().create(&audit, executor).await?;
                    Ok::<(), Error>(())
                })
            })
            .await?;
        self.read_model().sales_order_detail(id, None).await.map_err(Error::from)
    }
}

/// 在原事务重验合同有效修订、独立客户权限及原销售关系后执行领域绑定。
async fn verify_and_bind_contract(
    db: &Database,
    access: &SalesCommandAccess,
    order: &mut SalesOrder,
    req: &BindSalesOrderContractRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
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
    order.bind_contract(
        req.contract_id.clone(),
        &contract.customer_id,
        &contract.settlement_party_id,
        actor.id(),
    )?;
    Ok(())
}
