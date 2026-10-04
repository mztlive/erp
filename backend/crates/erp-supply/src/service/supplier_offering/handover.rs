//! 供给维护人交接：范围重验、CAS 与责任字段。

use application_core::AuditActor;

use super::SupplierOfferingService;
use crate::dto::{HandoverSupplierOfferingRequest, HandoverSupplierOfferingView};
use crate::entity::supplier_offering::SupplierOffering;
use crate::error::{Error, Result};
use crate::repository::SupplierOfferingExt;

impl SupplierOfferingService {
    /// 在调用方事务内交接；供组合层与幂等收据共用执行器。
    ///
    /// # 参数
    /// * `id` - 供给稳定 ID
    /// * `req` - 已校验交接请求
    /// * `actor` - 已认证操作人
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回交接后视图。
    ///
    /// # 错误
    /// 版本、范围或目标非法时拒绝。
    pub async fn apply_offering_handover(
        &self,
        id: &str,
        req: &HandoverSupplierOfferingRequest,
        actor: &AuditActor,
        executor: &mut dyn persistence_core::Executor,
    ) -> Result<HandoverSupplierOfferingView> {
        persist_handover(&self.db, &self.access(), id, req, actor, executor).await
    }
}

/// 在调用方事务内重验范围、CAS 并写入维护人。
///
/// # 参数
/// * `db` - 供给数据库
/// * `access` - 供给范围访问器
/// * `id` - 供给 ID
/// * `req` - 已校验交接请求
/// * `actor` - 操作人
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回交接后视图。
///
/// # 错误
/// 版本、范围或目标非法时整事务回滚。
async fn persist_handover(
    db: &mongodb::Database,
    access: &crate::service::supplier_offering::OfferingAccess,
    id: &str,
    req: &HandoverSupplierOfferingRequest,
    actor: &AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<HandoverSupplierOfferingView> {
    let mut offering = access.require_offering(actor, "update", id, executor).await?;
    if offering.base.version != req.expected_version {
        return Err(Error::ConflictError("供给责任或版本已变化，请刷新后重试".into()));
    }
    offering
        .handover(req.target_user_id.clone(), req.target_org_unit_id.clone(), actor.id())
        .map_err(Error::from)?;
    db.supplier_offerings().update(&mut offering, executor).await?;
    Ok(handover_view(&offering))
}

/// 构造交接响应。
fn handover_view(offering: &SupplierOffering) -> HandoverSupplierOfferingView {
    HandoverSupplierOfferingView {
        offering_id: offering.base.id.clone(),
        maintainer_user_id: offering.maintainer_user_id.clone(),
        business_org_unit_id: offering.business_org_unit_id.clone(),
        version: offering.base.version,
    }
}
