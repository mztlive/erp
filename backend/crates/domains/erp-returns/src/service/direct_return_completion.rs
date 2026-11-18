//! 原采购改单生效后的直退结案。款票守卫已通过，金额只由该次改单调整。
use erp_core::ids::SalesOrderId;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::returns::{
    PurchaseReturnOrderUpdate, PurchaseReturnStatus, SalesReturnCaseStatus, SalesReturnCaseUpdate,
};
use crate::repository::ReturnsExt;
use crate::repository::direct_return::confirmed_direct_returns;
use crate::{Error, Result};

/// 在采购改单生效的同一事务完成该采购已核实直退的双向处理单。
/// 完成已登记的客户直退；不创建金额分录或改写原发货。
/// `purchase_id` 为已通过数量、来源、应付及票款校验的新生效采购标识；`sales_order_id` 为该采购已冻结的销售来源标识。
/// `actor` 为生效操作人；`executor` 为原生效事务执行器。
/// 关联处理单缺失或状态、仓储异常时整体回滚改单。
pub async fn complete_direct_returns(
    db: &Database,
    purchase_id: &str,
    sales_order_id: &SalesOrderId,
    actor: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    for mut returned in confirmed_direct_returns(db, purchase_id, executor).await? {
        if returned.stable.status == PurchaseReturnStatus::Completed {
            continue;
        }

        let case_id = returned
            .sales_return_case_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("直退记录缺少销售退货依据".into()))?;
        let mut case = db
            .sales_return_cases()
            .find_by_id(case_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("直退关联的销售退货处理单不存在".into()))?;
        if &case.sales_order_id != sales_order_id
            || case.stable.status != SalesReturnCaseStatus::PendingFinance
        {
            return Err(Error::ConflictError("直退处理状态发生变化，请核对销售退货记录".into()));
        }

        returned.update(
            PurchaseReturnOrderUpdate { status: Some(PurchaseReturnStatus::Completed), ..Default::default() },
            actor,
        )?;
        case.update(
            SalesReturnCaseUpdate { status: Some(SalesReturnCaseStatus::Completed), ..Default::default() },
            actor,
        )?;

        db.purchase_return_orders().update(&mut returned, executor).await?;
        db.sales_return_cases().update(&mut case, executor).await?;
    }
    Ok(())
}
