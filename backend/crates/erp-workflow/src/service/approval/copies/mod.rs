//! 审批抄送：配置资格、事件收件和独立只读授权。
mod read;
mod runtime;
use application_core::AuditActor;
use bpm::graph::DefinitionGraph;
use entity_core::BaseModel;
use mongodb::Database;
use persistence_core::Executor;
pub use read::{CopyDetail, CopyHistoryPage, CopyReadRequest};
pub use runtime::{persist_copies, persist_started_copy};

use crate::entity::approval_copy::{ApprovalCopyConfig, ApprovalCopyRule};
use crate::error::{Error, Result};
use crate::ports::WorkflowAuthorizationPort;
use crate::repository::approval_copy::ApprovalCopyExt;
use crate::service::approval::approval_action_roles_with_executor;

pub const COPY_READ_PERMISSION: &str = "approval_copy:read";

/// 重验当前账号的抄送读取资格；无需审批决定权限。
/// # 参数
/// 认证主体、授权端口、原执行器。
/// # 返回
/// 有效内部账号且有读取权限时成功。
/// # 错误
/// 停用、无权限或基础设施失败时拒绝。
pub async fn require_reader(
    auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    ex: &mut dyn Executor,
) -> Result<()> {
    let account = auth.load_account(actor.id(), ex).await?;
    if !account.is_some_and(|a| a.kind == actor.kind() && a.is_active_backoffice())
        || approval_action_roles_with_executor(auth, actor, COPY_READ_PERMISSION, ex).await?.is_empty()
    {
        return Err(Error::Forbidden("当前账号无抄送读取权限".into()));
    }
    Ok(())
}

/// 发布及保存时验证所有接收人；账号或资格失效不允许发布。
/// # 参数
/// 授权端口、规则、原执行器。
/// # 返回
/// 全员有效时成功。
/// # 错误
/// 人员或权限不满足时返回校验错误；基础设施错误透传。
pub async fn validate_recipients(
    auth: &impl WorkflowAuthorizationPort,
    rules: &[ApprovalCopyRule],
    ex: &mut dyn Executor,
) -> Result<()> {
    let ids = rules
        .iter()
        .flat_map(|r| r.recipient_user_ids.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>();
    let accounts = auth.load_accounts(&ids.iter().cloned().collect::<Vec<_>>(), ex).await?;
    for id in ids {
        let account = accounts
            .iter()
            .find(|a| a.id == id && a.is_active_backoffice())
            .ok_or_else(|| Error::ValidationError("抄送人不存在或已停用，请重新选择".into()))?;
        let actor = AuditActor::new(account.id.clone(), account.login_account.clone(), account.kind);
        if approval_action_roles_with_executor(auth, &actor, COPY_READ_PERMISSION, ex).await?.is_empty() {
            return Err(Error::ValidationError(format!("{}不具备抄送读取权限", account.display_name)));
        }
    }
    Ok(())
}

/// 在草稿图的 CAS 事务中保存完整抄送配置。
/// # 参数
/// 数据库、规则、定义图和事务。
/// # 返回
/// 无。
/// # 错误
/// 校验或数据库错误。
pub async fn save_config(
    db: &Database,
    graph: &DefinitionGraph,
    auth: &impl WorkflowAuthorizationPort,
    rules: Vec<ApprovalCopyRule>,
    ex: &mut dyn Executor,
) -> Result<()> {
    let rules = ApprovalCopyRule::prepare(rules, graph.nodes.len())?;
    let ids = rules.iter().flat_map(|r| r.recipient_user_ids.iter().cloned()).collect::<Vec<_>>();
    let recipient_names =
        auth.load_accounts(&ids, ex).await?.into_iter().map(|a| (a.id, a.display_name)).collect();
    db.approval_copies()
        .save_config(
            &ApprovalCopyConfig {
                base: BaseModel::new(graph.definition.base.id.clone()),
                rules,
                recipient_names,
            },
            ex,
        )
        .await?;
    Ok(())
}
