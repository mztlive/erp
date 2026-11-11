//! 商品维护人交接：范围重验、CAS 与责任字段。

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::Executor;

use super::{CatalogAccess, CatalogService};
use crate::dto::{HandoverProductRequest, HandoverProductView};
use crate::entity::catalog::Product;
use crate::error::{Error, Result};
use crate::repository::CatalogExt;

impl CatalogService {
    /// 在调用方事务内交接；供组合层与幂等收据共用执行器。
    ///
    /// # 参数
    /// * `id` - 商品稳定 ID
    /// * `req` - 已校验交接请求
    /// * `actor` - 已认证操作人
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回交接前实体与交接后视图，供调用方记录审计事实。
    ///
    /// # 错误
    /// 版本、范围或目标非法时拒绝。
    pub async fn apply_product_handover(
        &self,
        id: &str,
        req: &HandoverProductRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(Product, HandoverProductView)> {
        persist_handover(&self.db, &self.access(), id, req, actor, executor).await
    }
}

/// 在调用方事务内重验范围、CAS 并写入维护人。
///
/// # 参数
/// * `db` - 商品数据库
/// * `access` - 商品范围访问器
/// * `id` - 商品 ID
/// * `req` - 已校验交接请求
/// * `actor` - 操作人
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回交接前实体与交接后视图，供调用方记录审计事实。
///
/// # 错误
/// 版本、范围或目标非法时整事务回滚。
async fn persist_handover(
    db: &Database,
    access: &CatalogAccess,
    id: &str,
    req: &HandoverProductRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<(Product, HandoverProductView)> {
    let mut product = access.require_product(actor, "update", id, executor).await?;
    if product.base.version != req.expected_version {
        return Err(Error::ConflictError("商品责任或版本已变化，请刷新后重试".into()));
    }
    let before = product.clone();
    product
        .handover(req.target_user_id.clone(), req.target_org_unit_id.clone(), actor.id())
        .map_err(Error::from)?;
    db.products().update(&mut product, executor).await?;
    Ok((before, handover_view(&product)))
}

/// 构造交接响应。
fn handover_view(product: &Product) -> HandoverProductView {
    HandoverProductView {
        product_id: product.base.id.clone(),
        maintainer_user_id: product.maintainer_user_id.clone(),
        business_org_unit_id: product.business_org_unit_id.clone(),
        version: product.base.version,
    }
}
