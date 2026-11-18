//! 商品维护人交接：范围重验、CAS 与责任字段。

use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_identity::entity::authorization::{FactValue, ObjectFacts};
use erp_permissions::{catalog as catalog_permissions, catalog};
use mongodb::Database;
use persistence_core::Executor;

use super::{CatalogAccess, CatalogService};
use crate::dto::{HandoverProductRequest, HandoverProductView};
use crate::entity::catalog::Product;
use crate::error::{Error, Result};
use crate::repository::CatalogExt;

impl CatalogService {
    /// 组合层交接商品责任时，复用用于幂等回执的事务更新维护人，并返回交接前实体与交接后视图供审计。
    /// 原对象权限、目标归属、乐观锁版本不满足时拒绝。
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

// 在原写事务中先授权当前商品、核对期望版本，再授权拟写入的责任归属并执行 CAS。
// 返回修改前实体及修改后视图，供同一用例记录审计；不另开事务。
async fn persist_handover(
    db: &Database,
    access: &CatalogAccess,
    id: &str,
    req: &HandoverProductRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<(Product, HandoverProductView)> {
    let mut product =
        access.require_product(actor, &catalog_permissions::PRODUCT_PRODUCT_UPDATE, id, executor).await?;
    if product.base.version != req.expected_version {
        return Err(Error::ConflictError("商品责任或版本已变化，请刷新后重试".into()));
    }

    let before = product.clone();
    product
        .handover(req.target_user_id.clone(), req.target_org_unit_id.clone(), actor.id())
        .map_err(Error::from)?;
    access
        .authorization
        .require(
            actor,
            &catalog::PRODUCT_PRODUCT_UPDATE,
            AccessTarget::OwnershipChange {
                id: id.into(),
                candidate: ObjectFacts {
                    resource: "product".into(),
                    owner_id: FactValue::Known(Some(product.maintainer_user_id.clone())),
                    org_unit_id: FactValue::Known(Some(product.business_org_unit_id.clone())),
                    ..Default::default()
                },
            },
            executor,
        )
        .await?;

    db.products().update(&mut product, executor).await?;
    Ok((before, handover_view(&product)))
}

// 交接写入成功后，从更新后的商品提取维护人、业务组织与乐观锁版本，供响应和回执投影。
fn handover_view(product: &Product) -> HandoverProductView {
    HandoverProductView {
        product_id: product.base.id.clone(),
        maintainer_user_id: product.maintainer_user_id.clone(),
        business_org_unit_id: product.business_org_unit_id.clone(),
        version: product.base.version,
    }
}
