//! 回执恢复只读取原成功结果，禁止重建命令或重放未知提交。

use application_core::{AuditActor, CommandIdentity};
use erp_permissions::sales_inquiry as sales_inquiry_permissions;
use erp_sales::entity::sales_inquiry::{InquiryCommandReceipt, InquiryCommandResult};
use persistence_core::{Executor, Transactional};

use super::SalesInquiryProcess;
use super::dto::InquiryMutationView;
use super::execution::ReplaySource;
use crate::{Error, Result};

impl SalesInquiryProcess {
    /// 当询价办理响应丢失时，当前账号按原动作和幂等键查本人成功回执。只有创建命令恢复时可以不传询价 ID。
    /// 同一恢复快照内核对回执身份和当前读取资格，命中后返回原冻结结果，未命中返回 `None`，不执行原写命令。
    /// 回执损坏、操作人或询价不匹配、读取资格失效时拒绝。
    pub async fn recover(
        &self,
        actor: &AuditActor,
        id: &str,
        action: &str,
        command_key: &str,
    ) -> Result<Option<InquiryMutationView>> {
        let key = command_key.trim();
        if key.is_empty() || key.chars().count() > 256 {
            return Err(Error::ValidationError("操作号不能为空且不得超过 256 字符".into()));
        }
        let identity = CommandIdentity::new(
            "sales-inquiry-command:",
            [actor.id(), action, "sales_inquiry", key].into_iter().map(str::to_string),
        )?;

        let this = self.clone();
        let actor = actor.clone();
        let action = action.to_string();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    recover(&this, &actor, &id, &action, identity.current_id(), executor).await
                })
            })
            .await
    }
}

// 回执读取、当前入口授权及冻结结果交付均留在原恢复快照。
pub(super) async fn recover(
    source: &impl ReplaySource,
    actor: &AuditActor,
    id: &str,
    action: &str,
    command_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<InquiryMutationView>> {
    let Some(receipt) = source.receipt(command_id, executor).await? else { return Ok(None) };
    let view = recovery_result(&receipt, actor, action, id)?;

    if is_procurement_action(action) {
        source.procurement(actor, &sales_inquiry_permissions::RECOVERY_PROCUREMENT_RECOVER, executor).await?;
    } else {
        source
            .sales(actor, &sales_inquiry_permissions::RECOVERY_SALES_RECOVER, &view.inquiry_id, executor)
            .await?;
    }
    Ok(Some(view))
}

// 验证冻结回执与恢复请求一致，创建请求允许从回执取得真实对象身份。
fn recovery_result(
    receipt: &InquiryCommandReceipt,
    actor: &AuditActor,
    action: &str,
    id: &str,
) -> Result<InquiryMutationView> {
    receipt.validate()?;
    if receipt.base.is_deleted() || receipt.actor_id != actor.id() || receipt.action != action {
        return Err(Error::ConflictError("无法确认原办理记录与本次询价操作相符，请先核对询价进度".into()));
    }

    let InquiryCommandResult::Mutation {
        inquiry_id,
        version,
        requirement_version,
        feedback_round,
        status,
        task_ids,
    } = &receipt.result
    else {
        return Err(Error::Internal("询价命令回执缺少冻结结果".into()));
    };
    if id != inquiry_id && !(action == "create" && id.is_empty()) {
        return Err(Error::ConflictError("原办理记录不属于当前询价，请核对所选询价单".into()));
    }

    Ok(InquiryMutationView {
        inquiry_id: inquiry_id.clone(),
        version: *version,
        requirement_version: *requirement_version,
        feedback_round: *feedback_round,
        status: *status,
        task_ids: task_ids.clone(),
    })
}

// 区分询价回执应走的采购恢复授权；历史接手动作仍可只读查证，但不再开放对应写入口。
fn is_procurement_action(action: &str) -> bool {
    matches!(
        action,
        "renew"
            | "claim"
            | "supplement"
            | "save_feedback"
            | "publish"
            | "reassign"
            | "internal_quote"
            | "import_quotes"
            | "fee_basis"
    )
}
