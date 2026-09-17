use application_core::AuditActor;
use bpm::ids::ApprovalProcessDefinitionId;
use mongodb::Database;
use persistence_core::NoTransaction;

use super::super::definition_dto::{DefinitionCatalogItem, DefinitionDetailView, DefinitionVersionItem};
use super::super::policy::{ALL_DOCUMENT_TYPES, DocumentApprovalPolicy, policy_of, require_process_required};
use super::super::process_kind::{document_type_of, process_kind_of};
use super::command::definition_not_found;
use super::mapping::{catalog_facts_by_kind, catalog_item, detail_view, version_item};
use super::{ApprovalDefinitionService, DefinitionManagementVisibility, definition_management_visibility};
use crate::entity::document_registry::DocumentType;
use crate::error::Result;
use crate::repository::BpmExt;

impl<A: crate::ports::WorkflowAuthorizationPort> ApprovalDefinitionService<A> {
    /// 返回固定 20 行非敏感目录。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `visibility` - 类型级可见范围
    ///
    /// # 错误
    /// 政策或仓储读取失败时返回错误。
    pub async fn definition_catalog(
        &self,
        actor: &AuditActor,
        visibility: &DefinitionManagementVisibility,
    ) -> Result<Vec<DefinitionCatalogItem>> {
        let visibility = enforce_visibility(&self.db, &self.auth, actor, visibility).await?;
        let mut required_kinds = Vec::new();
        let mut policies = Vec::with_capacity(ALL_DOCUMENT_TYPES.len());
        for document_type in ALL_DOCUMENT_TYPES {
            let policy = policy_of(document_type)?;
            if matches!(policy, DocumentApprovalPolicy::ProcessRequired(_)) {
                required_kinds.push(policy.process_kind());
            }
            policies.push((document_type, policy));
        }
        let facts =
            self.db.bpm_workflow().definition_catalog_facts(&required_kinds, &mut NoTransaction).await?;
        let by_kind = catalog_facts_by_kind(facts);
        Ok(policies
            .into_iter()
            .map(|(document_type, policy)| catalog_item(document_type, &policy, &visibility, &by_kind))
            .collect())
    }

    /// 列出某单据类型的定义版本。
    ///
    /// # 参数
    /// * `document_type` - 固定单据类型
    /// * `actor` - 已认证操作人
    /// * `visibility` - 类型级可见范围
    ///
    /// # 错误
    /// 无读取权或类型无需审批时返回错误。
    pub async fn definition_versions(
        &self,
        document_type: DocumentType,
        actor: &AuditActor,
        visibility: &DefinitionManagementVisibility,
    ) -> Result<Vec<DefinitionVersionItem>> {
        let visibility = enforce_visibility(&self.db, &self.auth, actor, visibility).await?;
        require_process_required(document_type)?;
        ensure_can_read_detail(&visibility, document_type)?;
        let versions = self
            .db
            .bpm_workflow()
            .list_definition_versions(process_kind_of(document_type), &mut NoTransaction)
            .await?;
        Ok(versions.iter().map(version_item).collect())
    }

    /// 返回定义详情。
    ///
    /// # 参数
    /// * `definition_id` - 定义主键
    /// * `actor` - 已认证操作人
    /// * `visibility` - 类型级可见范围
    ///
    /// # 错误
    /// 不存在或无权读取时返回不泄露存在性的错误。
    pub async fn definition_detail(
        &self,
        definition_id: &str,
        actor: &AuditActor,
        visibility: &DefinitionManagementVisibility,
    ) -> Result<DefinitionDetailView> {
        let visibility = enforce_visibility(&self.db, &self.auth, actor, visibility).await?;
        let graph = self
            .db
            .bpm_workflow()
            .load_definition_graph(
                &ApprovalProcessDefinitionId::new(definition_id.to_string()),
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(definition_not_found)?;
        let document_type = document_type_of(graph.definition.process_kind);
        ensure_can_read_detail(&visibility, document_type)?;
        Ok(detail_view(&graph))
    }
}

/// 以当前 RBAC 重验类型级范围，禁止调用方扩大权限。
async fn enforce_visibility(
    _db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    actor: &AuditActor,
    visibility: &DefinitionManagementVisibility,
) -> Result<DefinitionManagementVisibility> {
    Ok(definition_management_visibility(rbac, actor).await?.intersect(visibility))
}

/// 详情读取权。
fn ensure_can_read_detail(
    visibility: &DefinitionManagementVisibility,
    document_type: DocumentType,
) -> Result<()> {
    if visibility.can_read_detail(document_type) {
        return Ok(());
    }
    Err(definition_not_found())
}

#[cfg(test)]
mod tests {
    use super::super::super::definition_dto::DefinitionAllowedAction;
    use super::super::super::policy::ApprovalRequirement;
    use super::super::command::ensure_definition_admin_allowed;
    use super::super::mapping::definition_allowed_actions;
    use super::*;
    use crate::error::Error;

    /// 类型级权限由 Service 强制：无管理权不得进入写端口。
    #[test]
    fn type_level_permission_is_enforced_by_helpers() {
        let denied = ensure_definition_admin_allowed(false).unwrap_err();
        assert!(matches!(denied, Error::Forbidden(message) if message.contains("流程定义管理权限")));
        ensure_definition_admin_allowed(true).unwrap();
        let visibility = DefinitionManagementVisibility::from_type_permissions(
            vec![DocumentType::StockAdjustment],
            Vec::new(),
        );
        assert!(ensure_can_read_detail(&visibility, DocumentType::StockAdjustment).is_ok());
        assert!(ensure_can_read_detail(&visibility, DocumentType::SalesOrder).is_err());
        let claimed = DefinitionManagementVisibility::from_type_permissions(
            vec![DocumentType::StockAdjustment, DocumentType::SalesOrder],
            vec![DocumentType::SalesOrder],
        );
        let intersected = visibility.intersect(&claimed);
        assert!(intersected.can_define(DocumentType::StockAdjustment));
        assert!(!intersected.can_define(DocumentType::SalesOrder));
        assert_eq!(
            definition_allowed_actions(ApprovalRequirement::ProcessRequired, false, None, None),
            Vec::new()
        );
        assert_eq!(
            definition_allowed_actions(ApprovalRequirement::ProcessRequired, true, None, None),
            vec![DefinitionAllowedAction::CreateDraft]
        );
        assert_eq!(
            definition_allowed_actions(ApprovalRequirement::ProcessRequired, true, None, Some(1)),
            vec![DefinitionAllowedAction::ReplaceNodes, DefinitionAllowedAction::Publish]
        );
        assert_eq!(
            definition_allowed_actions(ApprovalRequirement::ProcessRequired, true, Some(2), Some(3)),
            vec![
                DefinitionAllowedAction::ReplaceNodes,
                DefinitionAllowedAction::Publish,
                DefinitionAllowedAction::Retire
            ]
        );
        assert_eq!(
            definition_allowed_actions(ApprovalRequirement::ProcessRequired, true, Some(2), None),
            vec![DefinitionAllowedAction::CreateDraft, DefinitionAllowedAction::Retire]
        );
        assert!(definition_allowed_actions(ApprovalRequirement::NoApproval, true, None, None).is_empty());
    }
}
