//! 回执恢复只读取原成功结果，禁止重建命令或重放未知提交。

use application_core::{AuditActor, CommandIdentity};
use erp_sales::entity::sales_inquiry::{InquiryCommandReceipt, InquiryCommandResult};
use erp_sales::service::sales_inquiry::SalesInquiryService;
use persistence_core::{NoTransaction, Transactional};

use super::SalesInquiryProcess;
use super::dto::InquiryMutationView;
use crate::{Error, Result};

impl SalesInquiryProcess {
    /// 按原动作与幂等键恢复本人成功命令的固定结果。
    ///
    /// # 参数
    /// `actor` 为当前账号，`id` 为询价（创建响应丢失时允许为空），
    /// `action` 与 `command_key` 必须沿用原命令。
    /// # 返回
    /// 成功回执存在且当前读取资格有效时返回原结果，尚未查到返回空。
    /// # 错误
    /// 身份不匹配、请求询价与结果不符、回执损坏或当前资格失效时拒绝。
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
        let Some(receipt) = SalesInquiryService::new(self.db.clone())
            .receipt(identity.current_id(), &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        let view = recovery_result(&receipt, actor, action, id)?;
        let this = self.clone();
        let actor = actor.clone();
        let action = action.to_string();
        let inquiry_id = view.inquiry_id.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    if is_procurement_action(&action) {
                        this.access().ensure_procurement_permission(&actor, "recover", executor).await?;
                    } else {
                        this.access().require_sales(&actor, "recover", &inquiry_id, executor).await?;
                    }
                    Ok::<(), Error>(())
                })
            })
            .await?;
        Ok(Some(view))
    }
}

fn recovery_result(
    receipt: &InquiryCommandReceipt,
    actor: &AuditActor,
    action: &str,
    id: &str,
) -> Result<InquiryMutationView> {
    if receipt.base.is_deleted() || receipt.actor_id != actor.id() || receipt.action != action {
        return Err(Error::ConflictError("询价命令回执身份不一致".into()));
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
        return Err(Error::ConflictError("询价命令回执不属于当前询价".into()));
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

// 历史接手回执仍可只读核对，不再提供对应写命令。
fn is_procurement_action(action: &str) -> bool {
    matches!(
        action,
        "claim"
            | "supplement"
            | "save_feedback"
            | "publish"
            | "reassign"
            | "internal_quote"
            | "import_quotes"
            | "fee_basis"
    )
}
