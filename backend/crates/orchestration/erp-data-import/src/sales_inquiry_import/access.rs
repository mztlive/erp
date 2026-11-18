//! 询价导入预览的本人访问由 identity 执行，确认绑定仍由预览实体核对。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{AccessContext, FactValue, ObjectFacts};
use erp_identity::{AccessFactsPort, AccessTarget, ResourceTargets, Result as IdentityResult};
use erp_permissions::internal;
use erp_sales::entity::sales_inquiry_import::InquiryImportPreview;
use erp_sales::repository::extensions::SalesInquiryImportExt;
use mongodb::Database;
use persistence_core::Executor;

use super::SalesInquiryImportProcess;
use crate::Result;

const PREVIEW_RESOURCE: &str = "sales_inquiry_import_preview";

impl SalesInquiryImportProcess {
    // 在当前读取或写事务内证明询价导入预览的本人资格。
    //
    // actor 为当前操作人，id 为真实预览 ID，executor 为原执行器。
    //
    // 预览属于当前有效账号且仍有询价导入权限时成功。
    //
    // 预览不存在、无权或身份设施失败时拒绝。
    pub(crate) async fn require_preview(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .authorization
            .require(actor, &internal::SALES_INQUIRY_IMPORT_PREVIEW, AccessTarget::record(id)?, executor)
            .await?)
    }
}

pub(crate) struct PreviewFacts(Database);
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
        valid_until: Some(Instant::from_unix_secs(preview.expires_at)),
        owner_id: FactValue::Known(Some(preview.actor_id.clone())),
        ..Default::default()
    }
}

/// 构造询价导入预览的真实事实读取器。
///
/// db 为业务数据库。
///
/// 读取持久化预览发起人及版本的实现。
pub fn preview_facts_reader(db: Database) -> Arc<dyn AccessFactsPort> {
    Arc::new(PreviewFacts(db))
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
