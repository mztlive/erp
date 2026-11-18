//! 条款历史读取，复用供给列表动作与单对象范围判定。

use application_core::AuditActor;
use erp_core::ids::SupplierOfferingId;
use erp_permissions::supplier_offering;
use persistence_core::Transactional;
use validator::Validate;

use super::SupplierOfferingService;
use crate::Result;
use crate::dto::offering_history::{OfferingHistoryPage, OfferingHistoryParams};
use crate::repository::SupplierOfferingExt;
use crate::repository::supplier_offering::history::OfferingHistoryRepositoryExt;

impl SupplierOfferingService {
    /// 按供给稳定 ID 查询不可变商业条款历史：校验版本游标，在同一事务先按供给列表动作授权，再倒序交付至
    /// 多 20 个版本和续页游标。无效游标或授权错误不返回历史，缺失对象返回 `NotFound`。
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
                    let offering =
                        access.require_offering(&actor, &supplier_offering::LIST, &id, executor).await?;

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
