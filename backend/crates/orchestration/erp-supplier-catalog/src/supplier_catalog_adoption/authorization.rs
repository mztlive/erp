//! 内部选品冻结预览的创建人资格由 identity 执行。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::{AccessContext, FactValue, ObjectFacts};
use erp_identity::{AccessFactsPort, AccessTarget, ResourceTargets, Result as IdentityResult};
use erp_permissions::internal;
use erp_supply::catalog_adoption::{CatalogAdoptionExt, CatalogAdoptionPreview};
use mongodb::Database;
use persistence_core::Executor;

use super::SupplierCatalogAdoptionProcess;
use crate::Result;

const PREVIEW_RESOURCE: &str = "supplier_catalog_adoption_preview";
impl SupplierCatalogAdoptionProcess {
    /// 在当前读取或写事务内核验真实冻结预览的创建人。
    /// # 参数
    /// actor 为真实账号，id 为持久化预览主键，executor 为原执行器。
    /// # 返回
    /// 当前有效账号为实际预览创建人时成功；过期预览仍允许原结果回放。
    /// # 错误
    /// 账号无效、对象缺失、非创建人或读取失败时拒绝。
    pub(super) async fn require_preview_owner(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .authorization
            .require(actor, &internal::CATALOG_PREVIEW_DETAIL, AccessTarget::record(id)?, executor)
            .await?)
    }
}

/// 构造冻结预览的持久化事实读取器。
/// # 参数
/// db 为预览所在数据库。
/// # 返回
/// 不缓存请求目标的资源事实读取器。
/// # 错误
/// 不执行读取。
pub fn preview_facts_port(db: Database) -> Arc<dyn AccessFactsPort> {
    Arc::new(PreviewFacts(db))
}

struct PreviewFacts(Database);
#[async_trait]
impl AccessFactsPort for PreviewFacts {
    async fn load(
        &self,
        _: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        let mut objects = Vec::new();
        for id in ids.ids() {
            if let Some(preview) = self.0.catalog_adoption_previews().find_by_id(id, executor).await? {
                objects.push(preview_facts(preview));
            }
        }
        Ok(objects)
    }
}

fn preview_facts(preview: CatalogAdoptionPreview) -> ObjectFacts {
    ObjectFacts {
        resource: PREVIEW_RESOURCE.into(),
        id: preview.base.id,
        version: preview.base.version.to_string(),
        owner_id: FactValue::Known(Some(preview.actor_id)),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;

    use super::*;

    #[test]
    fn preview_facts_keep_stored_owner_and_version_without_disabling_expired_replay() {
        let mut preview = CatalogAdoptionPreview {
            base: BaseModel::new("preview".into()),
            connection_id: "connection".into(),
            actor_id: "owner".into(),
            expires_at: Instant::from_unix_secs(0),
            items: vec![],
        };
        preview.base.version = 7;
        let facts = preview_facts(preview);

        assert_eq!(facts.id, "preview");
        assert_eq!(facts.version, "7");
        assert_eq!(facts.owner_id, FactValue::Known(Some("owner".into())));
    }
}
