//! 询价导入预览的本人访问由 identity 执行，确认绑定仍由预览实体核对。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::{AccessContext, FactKind, FactValue, ObjectFacts};
use erp_identity::{
    AccessFactsPort, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget,
    AuthorizationRegistry, AuthorizationService, DataAccess, Permission, ResourceTargets,
    Result as IdentityResult, SharedRbacService,
};
use erp_sales::entity::sales_inquiry_import::InquiryImportPreview;
use erp_sales::repository::extensions::SalesInquiryImportExt;
use mongodb::Database;
use persistence_core::Executor;

use super::SalesInquiryImportProcess;
use crate::Result;

const PREVIEW_RESOURCE: &str = "sales_inquiry_import_preview";

pub(super) fn authorization(db: Database, rbac: SharedRbacService) -> AuthorizationService {
    let registry = AuthorizationRegistry::new(vec![AccessRegistration {
        permission: Permission::parse("sales_inquiry_import_preview:detail").expect("固定预览读取权限"),
        required_permissions: vec![],
        access: AccessMode::Object {
            data: DataAccess {
                subject: AccessSubject::Direct,
                required_facts: [FactKind::Owner].into(),
                policy: AccessPolicy::Account {
                    policy: Box::new(AccessPolicy::All(vec![
                        AccessPolicy::Owned,
                        AccessPolicy::Permission(
                            Permission::parse("sales_inquiry:import").expect("固定询价导入权限"),
                        ),
                    ])),
                },
            },
            facts: Arc::new(PreviewFacts(db.clone())),
        },
    }])
    .expect("固定询价导入预览授权目录");
    AuthorizationService::new(db, rbac, registry)
}

impl SalesInquiryImportProcess {
    /// 在当前读取或写事务内证明询价导入预览的本人资格。
    /// # 参数
    /// actor 为当前操作人，id 为真实预览 ID，executor 为原执行器。
    /// # 返回
    /// 预览属于当前有效账号且仍有询价导入权限时成功。
    /// # 错误
    /// 预览不存在、无权或身份设施失败时拒绝。
    pub(crate) async fn require_preview(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .preview_access
            .require(
                actor,
                &Permission::parse("sales_inquiry_import_preview:detail")?,
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
            if let Some(preview) = self.0.inquiry_import_previews().find_by_id(id, executor).await? {
                objects.push(preview_facts(&preview));
            }
        }
        Ok(objects)
    }
}

fn preview_facts(preview: &InquiryImportPreview) -> ObjectFacts {
    ObjectFacts {
        resource: PREVIEW_RESOURCE.into(),
        id: preview.base.id.clone(),
        version: preview.base.version.to_string(),
        owner_id: FactValue::Known(Some(preview.actor_id.clone())),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_sales::entity::sales_inquiry::InquiryRequirementData;

    use super::*;

    #[test]
    fn preview_owner_and_version_come_from_persisted_preview_instead_of_inquiry_owner() {
        let mut preview = InquiryImportPreview {
            base: BaseModel::fake(),
            inquiry_id: "inquiry".into(),
            actor_id: "uploader".into(),
            creation_customer_id: None,
            expected_version: 3,
            input_fingerprint: "a".repeat(64),
            errors: vec![],
            merges: vec![],
            data: InquiryRequirementData::default(),
            expires_at: 100,
            result: None,
        };
        preview.base.id = "preview".into();
        preview.base.version = 7;
        let facts = preview_facts(&preview);
        assert_eq!(facts.resource, PREVIEW_RESOURCE);
        assert_eq!(facts.id, "preview");
        assert_eq!(facts.version, "7");
        assert_eq!(facts.owner_id, FactValue::Known(Some("uploader".into())));
        assert_eq!(facts.assignee_id, FactValue::Missing);
    }
}
