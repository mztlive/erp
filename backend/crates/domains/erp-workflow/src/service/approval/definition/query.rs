use application_core::AuditActor;
use bpm::ids::ApprovalProcessDefinitionId;
use erp_identity::AccessTarget;
use erp_permissions::approval_process;
use persistence_core::{Executor, Transactional};

use super::super::definition_dto::{DefinitionCatalogItem, DefinitionDetailView, DefinitionVersionItem};
use super::super::policy::{ALL_DOCUMENT_TYPES, DocumentApprovalPolicy, policy_of, require_process_required};
use super::super::process_kind::{document_type_of, process_kind_of};
use super::super::scope::definition_management_visibility_with_executor;
use super::ApprovalDefinitionService;
use super::command::{definition_not_found, require_definition_access};
use super::mapping::{catalog_facts_by_kind, catalog_item, detail_with_copies, version_item};
use crate::entity::document_registry::DocumentType;
use crate::error::{Error, Result};
use crate::repository::BpmExt;

impl ApprovalDefinitionService {
    /// 返回固定单据类型的非敏感目录。
    /// # 参数
    /// actor 为当前认证操作人。
    /// # 返回
    /// 同一授权快照内的类型目录及可执行动作。
    /// # 错误
    /// 缺少读取资格、政策或仓储读取失败时拒绝。
    pub async fn definition_catalog(&self, actor: &AuditActor) -> Result<Vec<DefinitionCatalogItem>> {
        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.catalog_view(&actor, executor).await })
            })
            .await
    }

    async fn catalog_view(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DefinitionCatalogItem>> {
        self.authorization
            .require(actor, &approval_process::DEFINITION_DETAIL, AccessTarget::Operation, executor)
            .await?;
        let visibility = definition_management_visibility_with_executor(
            &self.authorization,
            &self.db,
            &self.identity,
            actor,
            executor,
        )
        .await?;

        let mut required_kinds = Vec::new();
        let mut policies = Vec::with_capacity(ALL_DOCUMENT_TYPES.len());
        for document_type in ALL_DOCUMENT_TYPES {
            let policy = policy_of(document_type)?;
            if matches!(policy, DocumentApprovalPolicy::ProcessRequired(_)) {
                required_kinds.push(policy.process_kind());
            }
            policies.push((document_type, policy));
        }

        let facts = self.db.bpm_workflow().definition_catalog_facts(&required_kinds, executor).await?;
        let by_kind = catalog_facts_by_kind(facts);
        Ok(policies
            .into_iter()
            .map(|(document_type, policy)| catalog_item(document_type, &policy, &visibility, &by_kind))
            .collect())
    }

    /// 列出某单据类型的定义版本。
    /// # 参数
    /// document_type 为服务端单据类型，actor 为当前认证操作人。
    /// # 返回
    /// 同一授权快照内的版本摘要。
    /// # 错误
    /// 缺少入口或类型读取资格、类型无需审批及仓储失败时拒绝。
    pub async fn definition_versions(
        &self,
        document_type: DocumentType,
        actor: &AuditActor,
    ) -> Result<Vec<DefinitionVersionItem>> {
        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.version_list(document_type, &actor, executor).await })
            })
            .await
    }

    async fn version_list(
        &self,
        document_type: DocumentType,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DefinitionVersionItem>> {
        let policy = require_process_required(document_type)?;
        require_definition_access(
            &self.authorization,
            actor,
            &approval_process::DEFINITION_DETAIL,
            &policy,
            executor,
        )
        .await
        .map_err(detail_access_error)?;
        let versions =
            self.db.bpm_workflow().list_definition_versions(process_kind_of(document_type), executor).await?;
        Ok(versions.iter().map(version_item).collect())
    }

    /// 返回定义详情。
    /// # 参数
    /// definition_id 为定义主键，actor 为当前认证操作人。
    /// # 返回
    /// 同一授权快照中的定义、节点及抄送配置。
    /// # 错误
    /// 缺少入口资格、目标不存在或类型无权、仓储读取失败时拒绝。
    pub async fn definition_detail(
        &self,
        definition_id: &str,
        actor: &AuditActor,
    ) -> Result<DefinitionDetailView> {
        let service = self.clone();
        let actor = actor.clone();
        let id = definition_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.detail_view(&id, &actor, executor).await })
            })
            .await
    }

    async fn detail_view(
        &self,
        definition_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<DefinitionDetailView> {
        self.authorization
            .require(actor, &approval_process::DEFINITION_DETAIL, AccessTarget::Operation, executor)
            .await?;

        let graph = self
            .db
            .bpm_workflow()
            .load_definition_graph(&ApprovalProcessDefinitionId::new(definition_id.to_string()), executor)
            .await?
            .ok_or_else(definition_not_found)?;

        let policy = require_process_required(document_type_of(graph.definition.process_kind))?;
        require_definition_access(
            &self.authorization,
            actor,
            &approval_process::DEFINITION_DETAIL,
            &policy,
            executor,
        )
        .await
        .map_err(detail_access_error)?;

        detail_with_copies(&self.db, &graph, executor).await
    }
}

/// 类型读取拒绝沿用不存在语义，不泄露无权类型的定义身份。
fn detail_access_error(error: Error) -> Error {
    if matches!(error, Error::Forbidden(_)) { definition_not_found() } else { error }
}

#[cfg(test)]
mod tests {
    use super::super::super::definition_dto::DefinitionAllowedAction;
    use super::super::super::policy::ApprovalRequirement;
    use super::super::DefinitionManagementVisibility;
    use super::super::mapping::definition_allowed_actions;
    use super::*;

    #[tokio::test]
    async fn reads_reject_before_loading_definition_data() {
        use persistence_core::testing::SnapshotExecutor;

        use super::super::test_support::unavailable_service;

        let (service, actor) = unavailable_service().await;
        let errors = [
            service.catalog_view(&actor, &mut SnapshotExecutor::default()).await.unwrap_err(),
            service
                .version_list(DocumentType::SalesOrder, &actor, &mut SnapshotExecutor::default())
                .await
                .unwrap_err(),
            service.detail_view("definition", &actor, &mut SnapshotExecutor::default()).await.unwrap_err(),
        ];
        for error in errors {
            assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        }
    }

    /// 类型可见范围与已有定义状态共同决定展示动作。
    #[test]
    fn type_visibility_controls_definition_actions() {
        let visibility = DefinitionManagementVisibility::from_type_permissions(
            vec![DocumentType::StockAdjustment],
            Vec::new(),
        );
        assert!(visibility.can_read_detail(DocumentType::StockAdjustment));
        assert!(!visibility.can_read_detail(DocumentType::SalesOrder));
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
