//! 发布规格中的审批链。已发布且节点一致时跳过；节点不一致时说明原因，不覆盖。

use application_core::AuditActor;
use erp_workflow::service::approval::definition::{
    ApprovalDefinitionService, DefinitionManagementVisibility,
};
use erp_workflow::service::approval::definition_dto::{
    CreateDefinitionDraftRequest, DefinitionConfigurationStatus, DefinitionDetailView, DefinitionNodeRequest,
    DraftSource, PublishDefinitionRequest, ReplaceDefinitionNodesRequest,
};
use erp_workflow::service::approval::definition_management_visibility_with_executor;
use persistence_core::NoTransaction;

use super::accounts::PreparedAccounts;
use super::spec::{self, ApprovalSpec};
use super::{DemoFoundationReport, DemoMasterDataService};
use crate::adapters::workflow::{WorkflowAuth, workflow_audit, workflow_auth};
use crate::{Error, Result};

impl DemoMasterDataService {
    pub(super) async fn ensure_approvals(
        &self,
        actor: &AuditActor,
        accounts: &PreparedAccounts,
        report: &mut DemoFoundationReport,
    ) -> Result<()> {
        let (definitions, visibility) = self.definition_session(actor).await?;
        let catalog = definitions.definition_catalog(actor, &visibility).await?;
        for spec in &spec::foundation_spec().approvals {
            let document_type = spec::document_type(&spec.document_type)?;
            let published = catalog.iter().any(|item| {
                item.document_type == document_type
                    && item.configuration_status == DefinitionConfigurationStatus::Published
            });
            if published {
                self.note_published(&definitions, &visibility, actor, spec, accounts, report).await?;
                continue;
            }
            self.publish_approval(&definitions, &visibility, actor, spec, accounts).await?;
            report.approvals_published += 1;
        }
        Ok(())
    }

    async fn note_published(
        &self,
        definitions: &ApprovalDefinitionService<WorkflowAuth>,
        visibility: &DefinitionManagementVisibility,
        actor: &AuditActor,
        spec: &ApprovalSpec,
        accounts: &PreparedAccounts,
        report: &mut DemoFoundationReport,
    ) -> Result<()> {
        let detail = self.published_detail(definitions, visibility, actor, spec).await?;
        if detail.is_some_and(|detail| nodes_match(&detail, spec, accounts)) {
            report.approvals_existing += 1;
            return Ok(());
        }
        report.approvals_mismatched += 1;
        report.notices.push(format!("审批流程「{}」的已发布节点与演示岗位不一致，未改已发布流程", spec.name));
        Ok(())
    }

    async fn published_detail(
        &self,
        definitions: &ApprovalDefinitionService<WorkflowAuth>,
        visibility: &DefinitionManagementVisibility,
        actor: &AuditActor,
        spec: &ApprovalSpec,
    ) -> Result<Option<DefinitionDetailView>> {
        let document_type = spec::document_type(&spec.document_type)?;
        let versions = definitions.definition_versions(document_type, actor, visibility).await?;
        let Some(published) = versions.into_iter().find(|version| version.status == "PUBLISHED") else {
            return Ok(None);
        };
        Ok(Some(definitions.definition_detail(&published.definition_id, actor, visibility).await?))
    }

    async fn publish_approval(
        &self,
        definitions: &ApprovalDefinitionService<WorkflowAuth>,
        visibility: &DefinitionManagementVisibility,
        actor: &AuditActor,
        spec: &ApprovalSpec,
        accounts: &PreparedAccounts,
    ) -> Result<()> {
        let definition_id = self.draft_id(definitions, visibility, actor, spec).await?;
        let detail = definitions.definition_detail(&definition_id, actor, visibility).await?;
        let replaced = definitions
            .replace_definition_nodes(
                ReplaceDefinitionNodesRequest {
                    definition_id: definition_id.clone(),
                    expected_definition_lock_version: detail.definition_lock_version,
                    nodes: approval_nodes(spec, accounts)?,
                    idempotency_key: format!("demo-approval-{}-nodes", spec.document_type),
                },
                actor,
            )
            .await?;
        definitions
            .publish_definition(
                PublishDefinitionRequest {
                    definition_id,
                    expected_definition_lock_version: replaced.definition_lock_version,
                    idempotency_key: format!("demo-approval-{}-publish", spec.document_type),
                },
                actor,
            )
            .await?;
        Ok(())
    }

    async fn draft_id(
        &self,
        definitions: &ApprovalDefinitionService<WorkflowAuth>,
        visibility: &DefinitionManagementVisibility,
        actor: &AuditActor,
        spec: &ApprovalSpec,
    ) -> Result<String> {
        let document_type = spec::document_type(&spec.document_type)?;
        let versions = definitions.definition_versions(document_type, actor, visibility).await?;
        if let Some(draft) = versions.into_iter().find(|version| version.status == "DRAFT") {
            return Ok(draft.definition_id);
        }
        let created = definitions
            .create_definition_draft(
                CreateDefinitionDraftRequest {
                    document_type,
                    name: spec.name.clone(),
                    draft_source: DraftSource::Empty,
                    idempotency_key: format!("demo-approval-{}-draft", spec.document_type),
                },
                actor,
            )
            .await?;
        Ok(created.definition_id)
    }

    async fn definition_session(
        &self,
        actor: &AuditActor,
    ) -> Result<(ApprovalDefinitionService<WorkflowAuth>, DefinitionManagementVisibility)> {
        let definitions = ApprovalDefinitionService::with_audit(
            self.db.clone(),
            workflow_auth(self.db.clone(), self.rbac.clone()),
            workflow_audit(self.db.clone()),
        );
        let visibility = definition_management_visibility_with_executor(
            &workflow_auth(self.db.clone(), self.rbac.clone()),
            actor,
            &mut NoTransaction,
        )
        .await?;
        Ok((definitions, visibility))
    }
}

fn approval_nodes(spec: &ApprovalSpec, accounts: &PreparedAccounts) -> Result<Vec<DefinitionNodeRequest>> {
    spec.nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let assignee = accounts
                .by_key
                .get(&node.assignee)
                .cloned()
                .ok_or_else(|| Error::NotFound(format!("审批人 {} 尚未建号", node.assignee)))?;
            Ok(DefinitionNodeRequest {
                node_id: None,
                node_name: node.name.clone(),
                display_order: u32::try_from(index)
                    .map_err(|_| Error::Internal("审批节点过多".to_string()))?
                    + 1,
                assignee_user_id: assignee,
            })
        })
        .collect()
}

fn nodes_match(detail: &DefinitionDetailView, spec: &ApprovalSpec, accounts: &PreparedAccounts) -> bool {
    let mut live = detail.nodes.clone();
    live.sort_by_key(|node| node.display_order);
    live.len() == spec.nodes.len()
        && live.iter().zip(&spec.nodes).all(|(live, expected)| {
            live.node_name == expected.name
                && accounts.by_key.get(&expected.assignee).is_some_and(|id| id == &live.assignee_user_id)
        })
}
