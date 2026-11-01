//! 条款历史读取，复用供给列表动作与单对象范围判定。

use application_core::AuditActor;
use erp_core::ids::SupplierOfferingId;
use persistence_core::Transactional;
use validator::Validate;

use super::SupplierOfferingService;
use crate::Result;
use crate::dto::offering_history::{OfferingHistoryPage, OfferingHistoryParams};
use crate::repository::SupplierOfferingExt;
use crate::repository::supplier_offering::history::OfferingHistoryRepositoryExt;

impl SupplierOfferingService {
    /// 读取单条供给授权范围内的一页历史版本。
    ///
    /// # 参数
    /// * `id` - 供给稳定 ID
    /// * `params` - 可选的版本游标
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回至多 20 条不可变条款和下一页游标。
    ///
    /// # 错误
    /// 无效游标拒绝；越权对象与不存在对象统一返回 NotFound。
    pub async fn history(
        &self,
        id: &str,
        params: OfferingHistoryParams,
        actor: &AuditActor,
    ) -> Result<OfferingHistoryPage> {
        params.validate()?;
        let access = self.access();
        let db = self.db.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let offering = access.require_offering(&actor, "list", &id, executor).await?;
                    let revisions = db
                        .supplier_offering_revisions()
                        .history_page(&SupplierOfferingId::new(id), params.before_revision_no, executor)
                        .await?;
                    Ok(OfferingHistoryPage::from_revisions(
                        revisions,
                        offering.stable.current_revision_id.as_deref(),
                    ))
                })
            })
            .await
    }
}
