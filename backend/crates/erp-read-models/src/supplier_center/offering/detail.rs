//! 单条供给资料：先证明供给范围，再装配商品、供应商与当前条款。

use application_core::AuditActor;
use erp_core::ids::SupplierOfferingId;
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_supply::OfferingAccess;
use persistence_core::Transactional;

use super::{SupplierOfferingReadService, SupplierOfferingView, page_view, repository_page};
use crate::supplier_center::repository::offering::SupplierOfferingListQuery;
use crate::{Error, Result};

impl SupplierOfferingReadService {
    /// 读取授权范围内一条供给的完整当前资料。
    ///
    /// # 参数
    /// * `id` - 供给稳定 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回与列表同口径的当前资料，成本脱敏由 HTTP 边界执行。
    ///
    /// # 错误
    /// 不存在或越权统一返回 NotFound；仓储失败原样传播。
    pub async fn detail(&self, id: &str, actor: &AuditActor) -> Result<SupplierOfferingView> {
        let db = self.db.clone();
        let access = OfferingAccess::new(db.clone(), self.data_scope.clone());
        let id = id.to_owned();
        let actor = actor.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let offering = access.require_offering(&actor, "list", &id, executor).await?;
                    let query = SupplierOfferingListQuery {
                        offering_ids: Some(vec![SupplierOfferingId::new(offering.base.id)]),
                        page_size: 1,
                        ..Default::default()
                    };
                    let bundle = repository_page(&db, &query, executor).await?;
                    let mut view = page_view(bundle, &query)?
                        .items
                        .into_iter()
                        .next()
                        .ok_or_else(|| Error::NotFound("供给不存在或无权查看".into()))?;
                    let names =
                        db.accounts().names_by_ids(&[view.maintainer_user_id.clone()], executor).await?;
                    view.maintainer_user_name = names.get(&view.maintainer_user_id).cloned();
                    Ok(view)
                })
            })
            .await
    }
}
