//! 定义期审批人候选：只做静态账号过滤，不读具体单据 DataScopeFact。

use application_core::AuditActor;
use erp_permissions::internal;
use persistence_core::{Executor, Transactional};
use serde::{Deserialize, Serialize};

use super::approval_participant_permissions_with_executor;
use super::definition::{ApprovalDefinitionService, require_definition_access};
use super::execution::runtime_service::RuntimeAssigneeCandidate;
use super::policy::require_process_required;
use crate::entity::document_registry::DocumentType;
use crate::error::Result;

/// 定义期候选人查询。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionAssigneeQuery {
    /// 抄送候选包含所有有效内部账号，不要求角色权限。
    pub copies: bool,
    /// 单据类型。
    pub document_type: DocumentType,
    /// 姓名或账号检索。
    pub search: Option<String>,
    /// 页大小。
    pub limit: u32,
}

/// 定义期候选人页。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefinitionAssigneePage {
    /// 候选人。
    pub items: Vec<RuntimeAssigneeCandidate>,
}

impl ApprovalDefinitionService {
    /// 定义期静态过滤可选审批人。
    ///
    /// 只按后台有效账号与检索串过滤，不复用运行期单据上下文。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `query` - 类型、检索串与页大小
    ///
    /// # 返回
    /// 返回 Repository 已按有效后台账号范围过滤的候选人页。
    ///
    /// # 错误
    /// 无定义管理权或类型不适用时返回错误。
    pub async fn eligible_assignees(
        &self,
        actor: &AuditActor,
        query: DefinitionAssigneeQuery,
    ) -> Result<DefinitionAssigneePage> {
        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.assignee_list(&actor, query, executor).await })
            })
            .await
    }

    async fn assignee_list(
        &self,
        actor: &AuditActor,
        query: DefinitionAssigneeQuery,
        executor: &mut dyn Executor,
    ) -> Result<DefinitionAssigneePage> {
        let policy = require_process_required(query.document_type)?;
        require_definition_access(
            &self.authorization,
            actor,
            &internal::APPROVAL_DEFINITION_ASSIGNEES,
            &policy,
            executor,
        )
        .await?;

        let accounts = self
            .identity
            .directory()
            .active_backoffice(query.search.as_deref(), query.limit, executor)
            .await?;

        let mut items = Vec::new();
        for account in accounts {
            if !query.copies {
                let candidate =
                    AuditActor::new(account.id.clone(), account.login_account.clone(), account.kind);
                if !approval_participant_permissions_with_executor(
                    &self.authorization,
                    &self.db,
                    &self.identity,
                    &candidate,
                    executor,
                )
                .await?
                {
                    continue;
                }
            }
            items.push(RuntimeAssigneeCandidate { user_id: account.id, name: account.display_name });
        }
        Ok(DefinitionAssigneePage { items })
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::testing::SnapshotExecutor;

    use super::super::definition::test_support::unavailable_service;
    use super::*;
    use crate::error::Error;

    #[tokio::test]
    async fn candidates_reject_before_reading_accounts() {
        let (service, actor) = unavailable_service().await;
        let query = DefinitionAssigneeQuery {
            copies: false,
            document_type: DocumentType::SalesOrder,
            search: None,
            limit: 20,
        };
        let error = service.assignee_list(&actor, query, &mut SnapshotExecutor::default()).await.unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
