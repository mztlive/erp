//! 定义期审批人候选：只做静态账号过滤，不读具体单据 DataScopeFact。

use application_core::AuditActor;
use persistence_core::NoTransaction;
use serde::{Deserialize, Serialize};

use super::approval_participant_permissions_with_executor;
use super::definition::ApprovalDefinitionService;
use super::execution::runtime_service::RuntimeAssigneeCandidate;
use super::policy::require_process_required;
use crate::entity::document_registry::DocumentType;
use crate::error::Result;
use crate::ports::{WorkflowAccountFact, WorkflowAuthorizationPort};

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

impl<A: WorkflowAuthorizationPort> ApprovalDefinitionService<A> {
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
        let policy = require_process_required(query.document_type)?;
        self.ensure_definition_admin(actor, &policy).await?;
        let accounts = self
            .auth
            .list_active_approval_candidates(query.search.as_deref(), query.limit, &mut NoTransaction)
            .await?;
        let mut items = Vec::new();
        for account in accounts {
            let eligible = eligible_candidate(&self.auth, &account, query.copies).await?;
            if eligible {
                items.push(RuntimeAssigneeCandidate { user_id: account.id, name: account.display_name });
            }
        }
        Ok(DefinitionAssigneePage { items })
    }
}

/// 按用途校验候选账号；抄送不要求审批或角色权限。
/// # 参数
/// 授权端口、候选账号和是否用于抄送。
/// # 返回
/// 有效内部抄送人直接通过，审批人继续校验审批权限。
/// # 错误
/// 审批权限读取失败时拒绝。
pub(super) async fn eligible_candidate(
    auth: &impl WorkflowAuthorizationPort,
    account: &WorkflowAccountFact,
    copies: bool,
) -> Result<bool> {
    if !account.is_active_backoffice() {
        return Ok(false);
    }
    if copies {
        return Ok(true);
    }
    let candidate = AuditActor::new(account.id.clone(), account.login_account.clone(), account.kind);
    approval_participant_permissions_with_executor(auth, &candidate, &mut NoTransaction).await
}
