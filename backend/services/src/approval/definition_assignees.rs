//! 定义期审批人候选：只做静态账号过滤，不读具体单据 DataScope。

use database::{AccessControlExt, NoTransaction};
use entities::document_registry::DocumentType;
use serde::{Deserialize, Serialize};

use super::definition::ApprovalDefinitionService;
use super::execution::runtime_query::definition_assignee_matches;
use super::execution::runtime_service::RuntimeAssigneeCandidate;
use super::policy::require_process_required;
use crate::audit::AuditActor;
use crate::errors::Result;

/// 定义期候选人查询。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionAssigneeQuery {
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
    /// * `query` - 类型与检索
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
            .db()
            .accounts()
            .find_many(mongodb::bson::doc! { "status": "active", "kind": "admin" }, &mut NoTransaction)
            .await?;
        let search = query.search.unwrap_or_default();
        let items = accounts
            .into_iter()
            .filter(|account| definition_assignee_matches(&account.name, account.secret.account(), &search))
            .take(query.limit as usize)
            .map(|account| RuntimeAssigneeCandidate {
                user_id: account.base.id,
                name: account.name,
            })
            .collect();
        Ok(DefinitionAssigneePage { items })
    }
}
