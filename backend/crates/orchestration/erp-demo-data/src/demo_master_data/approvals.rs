//! 发布规格中的审批链。已发布且节点一致时跳过；节点不一致时说明原因，不覆盖。

use std::slice;

use application_core::AuditActor;
use erp_workflow::ports::WorkflowAuthorizationPort;
use erp_workflow::service::approval::approval_participant_permissions_with_executor;
use erp_workflow::service::approval::business_adapter::ensure_separation_of_duties;
use erp_workflow::service::approval::definition::ApprovalDefinitionService;
use erp_workflow::service::approval::definition_dto::{
    CreateDefinitionDraftRequest, DefinitionConfigurationStatus, DefinitionDetailView, DefinitionNodeRequest,
    DraftSource, PublishDefinitionRequest, ReplaceDefinitionNodesRequest,
};
use erp_workflow::service::approval::policy::require_process_required;
use persistence_core::ReadExecutor;

use super::accounts::PreparedAccounts;
use super::spec::{self, ApprovalSpec};
use super::{DemoFoundationReport, DemoMasterDataProcess};
use crate::{Error, Result};

impl<A: WorkflowAuthorizationPort> DemoMasterDataProcess<A> {
    // 先校验演示审批人资格，再用正式工作流入口发布缺失定义。已有发布版只核对并报告差异，不覆盖人工配置；权限、定义操作或发布失败时停止。
    pub(super) async fn ensure_approvals(
        &self,
        actor: &AuditActor,
        accounts: &PreparedAccounts,
        report: &mut DemoFoundationReport,
    ) -> Result<()> {
        self.validate_approval_accounts(accounts).await?;

        let definitions = ApprovalDefinitionService::with_audit(
            self.db.clone(),
            self.identity.clone(),
            self.authorization.clone(),
            self.ports.workflow_audit.clone(),
        );
        let catalog = definitions.definition_catalog(actor).await?;

        for spec in &spec::foundation_spec().approvals {
            let document_type = spec::document_type(&spec.document_type)?;
            let published = catalog.iter().any(|item| {
                item.document_type == document_type
                    && item.configuration_status == DefinitionConfigurationStatus::Published
            });
            if published {
                self.note_published(&definitions, actor, spec, accounts, report).await?;
                continue;
            }
            self.publish_approval(&definitions, actor, spec, accounts).await?;
            report.approvals_published += 1;
        }
        Ok(())
    }

    // 对照演示节点记录现有发布版一致或不一致，让基础准备能阻断不匹配的审批链，而不改写已发布定义。
    async fn note_published(
        &self,
        definitions: &ApprovalDefinitionService,
        actor: &AuditActor,
        spec: &ApprovalSpec,
        accounts: &PreparedAccounts,
        report: &mut DemoFoundationReport,
    ) -> Result<()> {
        let detail = self.published_detail(definitions, actor, spec).await?;
        if detail.is_some_and(|detail| nodes_match(&detail, spec, accounts)) {
            report.approvals_existing += 1;
            return Ok(());
        }
        report.approvals_mismatched += 1;
        report.notices.push(format!("审批流程「{}」的已发布节点与演示岗位不一致，未改已发布流程", spec.name));
        Ok(())
    }

    // 只读取指定单据的当前已发布定义详情，避免拿并存草稿判断实际生效的审批链。
    async fn published_detail(
        &self,
        definitions: &ApprovalDefinitionService,
        actor: &AuditActor,
        spec: &ApprovalSpec,
    ) -> Result<Option<DefinitionDetailView>> {
        let document_type = spec::document_type(&spec.document_type)?;
        let versions = definitions.definition_versions(document_type, actor).await?;
        let Some(published) = versions.into_iter().find(|version| version.status == "PUBLISHED") else {
            return Ok(None);
        };
        Ok(Some(definitions.definition_detail(&published.definition_id, actor).await?))
    }

    // 将草稿配置为演示审批链并走正式发布命令；用定义 ID 和锁版本派生命令键，防止重试或退役后重建回放旧定义。
    async fn publish_approval(
        &self,
        definitions: &ApprovalDefinitionService,
        actor: &AuditActor,
        spec: &ApprovalSpec,
        accounts: &PreparedAccounts,
    ) -> Result<()> {
        let definition_id = self.draft_id(definitions, actor, spec).await?;
        let detail = definitions.definition_detail(&definition_id, actor).await?;

        let replaced = definitions
            .replace_definition_nodes(
                ReplaceDefinitionNodesRequest {
                    copy_rules: None,
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

    // 优先复用仍活动的草稿；新建命令键绑定下一个业务版本，避免退役后的新草稿命中旧回执。
    async fn draft_id(
        &self,
        definitions: &ApprovalDefinitionService,
        actor: &AuditActor,
        spec: &ApprovalSpec,
    ) -> Result<String> {
        let document_type = spec::document_type(&spec.document_type)?;
        let versions = definitions.definition_versions(document_type, actor).await?;
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

    // 逐个检查演示审批人是否由同一启用角色同时取得读取和决定资格，避免发布后出现不能办理的节点。
    async fn validate_approval_accounts(&self, accounts: &PreparedAccounts) -> Result<()> {
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

                if !approval_participant_permissions_with_executor(
                    &self.authorization,
                    &self.db,
                    &self.identity,
                    &actor,
                    &mut ReadExecutor::default(),
                )
                .await?
                {
                    return Err(Error::Forbidden(format!(
                        "审批账号 {} 缺少同一启用角色的审批读取与决定权限",
                        account.account
                    )));
                }
            }
        }
        Ok(())
    }
}

// 用真实账号 ID 验证该单据的岗位分离规则，再按演示顺序构造节点请求，防止不同岗位别名指向同一人时漏验。
fn approval_nodes(spec: &ApprovalSpec, accounts: &PreparedAccounts) -> Result<Vec<DefinitionNodeRequest>> {
    let submitter = accounts
        .by_key
        .get(&spec.submitter)
        .ok_or_else(|| Error::NotFound(format!("提交人 {} 尚未建号", spec.submitter)))?;
    let policy = require_process_required(spec::document_type(&spec.document_type)?)?;

    spec.nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let assignee = accounts
                .by_key
                .get(&node.assignee)
                .cloned()
                .ok_or_else(|| Error::NotFound(format!("审批人 {} 尚未建号", node.assignee)))?;
            ensure_separation_of_duties(
                policy.separation_of_duties_policy,
                submitter,
                slice::from_ref(&assignee),
            )?;

            Ok(DefinitionNodeRequest {
                assignee_source: Default::default(),
                additional_assignee_user_ids: Vec::new(),
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

// 比较已发布节点的名称、顺序及真实审批人，只有三者一致才把生效流程视为已按演示配置。
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
    fn approval_nodes_resolve_accounts_and_allow_sales_alias_collision() {
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
        assert_eq!(approval_nodes(spec, &accounts).unwrap()[0].assignee_user_id, "seller-id");

        accounts.by_key.remove("sales");
        assert!(approval_nodes(spec, &accounts).is_err());
    }

    #[test]
    fn approval_nodes_check_resolved_account_identity_by_policy() {
        let accounts = PreparedAccounts {
            by_key: HashMap::from([
                ("submitter".into(), "leader".into()),
                ("approver".into(), "leader".into()),
            ]),
            by_login: HashMap::new(),
        };
        for (kind, allowed) in [
            ("sales_order", true),
            ("voucher_sales_order", true),
            ("sales_change_order", true),
            ("purchase_order", false),
            ("customer_receipt", false),
        ] {
            let spec = ApprovalSpec {
                document_type: kind.into(),
                name: "审批".into(),
                submitter: "submitter".into(),
                nodes: vec![spec::ApprovalNodeSpec {
                    name: "领导审批".into(), assignee: "approver".into()
                }],
            };
            let result = approval_nodes(&spec, &accounts);
            if allowed {
                let nodes = result.unwrap();
                assert_eq!(nodes.len(), 1);
                assert_eq!(nodes[0].assignee_user_id, "leader");
            } else {
                assert!(result.is_err(), "{kind}");
            }
        }
    }
}
