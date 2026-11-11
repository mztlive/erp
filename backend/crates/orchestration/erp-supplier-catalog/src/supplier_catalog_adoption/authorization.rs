//! 内部选品冻结预览的创建人资格由 identity 执行。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::{AccessContext, FactKind, FactValue, ObjectFacts};
use erp_identity::{
    AccessFactsPort, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget,
    AuthorizationRegistry, AuthorizationService, DataAccess, Permission, ResourceTargets,
    Result as IdentityResult,
};
use erp_supply::catalog_adoption::{CatalogAdoptionExt, CatalogAdoptionPreview};
use mongodb::Database;
use persistence_core::Executor;

use super::SupplierCatalogAdoptionProcess;
use crate::Result;

const PREVIEW_RESOURCE: &str = "supplier_catalog_adoption_preview";
const PREVIEW_ENTRY: &str = "supplier_catalog_adoption_preview:detail";

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
        let permission = Permission::parse(PREVIEW_ENTRY)?;
        let registry = AuthorizationRegistry::new(vec![AccessRegistration {
            permission: permission.clone(),
            required_permissions: vec![],
            access: AccessMode::Object {
                data: DataAccess {
                    subject: AccessSubject::Direct,
                    required_facts: [FactKind::Owner].into(),
                    policy: AccessPolicy::Account { policy: Box::new(AccessPolicy::Owned) },
                },
                facts: Arc::new(PreviewFacts(self.db.clone())),
            },
        }])?;
        Ok(AuthorizationService::new(self.db.clone(), self.rbac.clone(), registry)
            .require(
                actor,
                &permission,
                AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
                executor,
            )
            .await?)
    }
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
