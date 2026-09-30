//! 发布规格中的审批链。已发布且节点一致时跳过；节点不一致时说明原因，不覆盖。

use application_core::AuditActor;
use erp_workflow::service::approval::definition::{
    ApprovalDefinitionService, DefinitionManagementVisibility,
};
use erp_workflow::service::approval::definition_dto::{
    CreateDefinitionDraftRequest, DefinitionConfigurationStatus, DefinitionDetailView, DefinitionNodeRequest,
    DraftSource, PublishDefinitionRequest, ReplaceDefinitionNodesRequest,
};
use erp_workflow::service::approval::{
    approval_participant_permissions_with_executor, definition_management_visibility_with_executor,
};
use persistence_core::NoTransaction;

use super::accounts::PreparedAccounts;
use super::spec::{self, ApprovalSpec};
use super::{DemoFoundationReport, DemoMasterDataService};
use crate::adapters::workflow::{WorkflowAuth, workflow_audit, workflow_auth};
use crate::{Error, Result};

impl DemoMasterDataService {
    /// 按当前工作流权限发布全部缺失定义，已有发布版本只核对。
    /// # 参数
    /// 操作人、已准备账号及基础准备报告。
    /// # 返回
    /// 缺失定义完成发布，已有定义差异写入报告。
    /// # 错误
    /// 岗位资格、定义管理或发布失败时返回错误。
    pub(super) async fn ensure_approvals(
        &self,
        actor: &AuditActor,
        accounts: &PreparedAccounts,
        report: &mut DemoFoundationReport,
    ) -> Result<()> {
        self.validate_approval_accounts(accounts).await?;
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

    /// 保留人工发布版本，报告与演示链条的差异。
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

    /// 读取当前已发布版本，不受并存草稿影响。
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

    /// 命令键绑定定义 ID 与锁版本，重试和退役后重建不回放旧定义。
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
                    idempotency_key: format!("demo-nodes-{definition_id}-{}", detail.definition_lock_version),
                },
                actor,
            )
            .await?;
        definitions
            .publish_definition(
                PublishDefinitionRequest {
                    definition_id: definition_id.clone(),
                    expected_definition_lock_version: replaced.definition_lock_version,
                    idempotency_key: format!(
                        "demo-publish-{definition_id}-{}",
                        replaced.definition_lock_version
                    ),
                },
                actor,
            )
            .await?;
        Ok(())
    }

    /// 复用活动草稿；新草稿键包含下一个业务版本，避免命中退役定义的旧回执。
    async fn draft_id(
        &self,
        definitions: &ApprovalDefinitionService<WorkflowAuth>,
        visibility: &DefinitionManagementVisibility,
        actor: &AuditActor,
        spec: &ApprovalSpec,
    ) -> Result<String> {
        let document_type = spec::document_type(&spec.document_type)?;
        let versions = definitions.definition_versions(document_type, actor, visibility).await?;
        if let Some(draft) = versions.iter().find(|version| version.status == "DRAFT") {
            return Ok(draft.definition_id.clone());
        }
        let generation = versions.iter().map(|version| version.definition_version).max().unwrap_or(0);
        let created = definitions
            .create_definition_draft(
                CreateDefinitionDraftRequest {
                    document_type,
                    name: spec.name.clone(),
                    draft_source: DraftSource::Empty,
                    idempotency_key: format!("demo-approval-{}-draft-after-{generation}", spec.document_type),
                },
                actor,
            )
            .await?;
        Ok(created.definition_id)
    }

    /// 所有指定审批人须具有同一启用角色授予的读取与决定资格。
    async fn validate_approval_accounts(&self, accounts: &PreparedAccounts) -> Result<()> {
        let auth = workflow_auth(self.db.clone(), self.rbac.clone());
        for spec in &spec::foundation_spec().approvals {
            approval_nodes(spec, accounts)?;
            for node in &spec.nodes {
                let account = spec::foundation_spec()
                    .accounts
                    .iter()
                    .find(|row| row.key == node.assignee)
                    .ok_or_else(|| Error::ValidationError(format!("未定义审批岗位 {}", node.assignee)))?;
                let actor = self
                    .role_actor(&account.account)
                    .await?
                    .ok_or_else(|| Error::Forbidden(format!("审批账号 {} 不可用", account.account)))?;
                if !approval_participant_permissions_with_executor(&auth, &actor, &mut NoTransaction).await? {
                    return Err(Error::Forbidden(format!(
                        "审批账号 {} 缺少同一启用角色的审批读取与决定权限",
                        account.account
                    )));
                }
            }
        }
        Ok(())
    }

    /// 装配正式定义服务与当前类型管理可见边界。
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

/// 按真实账号 ID 验证岗位分离后构造有序节点。
fn approval_nodes(spec: &ApprovalSpec, accounts: &PreparedAccounts) -> Result<Vec<DefinitionNodeRequest>> {
    let submitter = accounts
        .by_key
        .get(&spec.submitter)
        .ok_or_else(|| Error::NotFound(format!("提交人 {} 尚未建号", spec.submitter)))?;
    spec.nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let assignee = accounts
                .by_key
                .get(&node.assignee)
                .cloned()
                .ok_or_else(|| Error::NotFound(format!("审批人 {} 尚未建号", node.assignee)))?;
            if &assignee == submitter {
                return Err(Error::ValidationError(format!(
                    "{} 提交人不得审批自己的单据",
                    spec.document_type
                )));
            }
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

/// 名称、顺序与真实账号均一致才视为匹配。
fn nodes_match(detail: &DefinitionDetailView, spec: &ApprovalSpec, accounts: &PreparedAccounts) -> bool {
    let mut live = detail.nodes.clone();
    live.sort_by_key(|node| node.display_order);
    live.len() == spec.nodes.len()
        && live.iter().zip(&spec.nodes).all(|(live, expected)| {
            live.node_name == expected.name
                && accounts.by_key.get(&expected.assignee).is_some_and(|id| id == &live.assignee_user_id)
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn approval_nodes_resolve_real_accounts_and_reject_alias_collision() {
        let spec = &spec::foundation_spec().approvals[0];
        let mut accounts = PreparedAccounts {
            by_key: HashMap::from([
                ("sales".into(), "seller-id".into()),
                ("procurement".into(), "buyer-id".into()),
            ]),
            by_login: HashMap::new(),
        };
        let nodes = approval_nodes(spec, &accounts).unwrap();
        assert_eq!(nodes[0].assignee_user_id, "buyer-id");
        assert_eq!(nodes[0].display_order, 1);
        assert!(nodes[0].node_id.is_none());
        accounts.by_key.insert("procurement".into(), "seller-id".into());
        assert!(approval_nodes(spec, &accounts).is_err());
        accounts.by_key.remove("sales");
        assert!(approval_nodes(spec, &accounts).is_err());
    }
}
