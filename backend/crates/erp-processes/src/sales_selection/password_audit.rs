//! 选品访问密码维护与安全审计共用同一事务。

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::{BusinessEventContent, BusinessEventContext, BusinessEventResult, registered_action};
use erp_sales::dto::sales_selection::{SalesSelectionBookletView, SalesSelectionPasswordRequest};
use erp_sales::entity::sales_selection::SelectionRequestFingerprint;
use erp_sales::service::sales_selection::{SalesSelectionService, SelectionAccess};
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::audit::{AuditedCommand, AuditedWrite, run_audited_event};

const ACTION: &str = "sales_selection_booklet.set_access_password";
const RESOURCE: &str = "sales_selection_booklet";

/// 执行已授权范围内的密码维护并登记一次安全成功事件。
///
/// # 参数
/// `db` 为数据库，`access` 为范围访问器，`id` 为册，`req` 为命令，`hash` 为慢哈希，`fingerprint` 为完整请求的带密钥指纹，`actor` 为已认证人。
///
/// # 返回
/// 返回更新详情；重放沿领域收据恢复且不追加成功审计。
///
/// # 错误
/// 授权、版本、业务或审计失败时拒绝且原子回滚。
pub(super) async fn update_password(
    db: Database,
    access: SelectionAccess,
    id: String,
    req: SalesSelectionPasswordRequest,
    hash: String,
    fingerprint: SelectionRequestFingerprint,
    actor: AuditActor,
) -> Result<SalesSelectionBookletView> {
    let context = password_context(&actor, &id)?;
    let command = PasswordCommand { db: db.clone(), access, id, req, hash, fingerprint, actor };
    run_audited_event(&db, context, command).await
}

struct PasswordCommand {
    db: Database,
    access: SelectionAccess,
    id: String,
    req: SalesSelectionPasswordRequest,
    hash: String,
    fingerprint: SelectionRequestFingerprint,
    actor: AuditActor,
}

#[async_trait]
impl AuditedCommand for PasswordCommand {
    type Output = SalesSelectionBookletView;

    async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<Self::Output>> {
        self.access.require_booklet(&self.actor, "maintain", &self.id, executor).await?;
        let (result, fresh) = SalesSelectionService::new(self.db.clone())
            .set_access_password(
                &self.id,
                self.req.clone(),
                self.hash.clone(),
                self.fingerprint.clone(),
                self.actor.id(),
                executor,
            )
            .await?;
        if !fresh {
            return Ok(AuditedWrite::Replayed(result));
        }
        Ok(AuditedWrite::Fresh { content: password_content(result.id.clone()), result })
    }
}

fn password_context(actor: &AuditActor, id: &str) -> Result<BusinessEventContext> {
    Ok(BusinessEventContext::new(actor.clone(), registered_action(ACTION, RESOURCE)?)?
        .with_target(Some(id.to_string()), None)?)
}

fn password_content(id: String) -> BusinessEventContent {
    BusinessEventContent {
        target_id: id,
        target_number: None,
        result: BusinessEventResult::Succeeded,
        field_changes: Vec::new(),
        facts: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use erp_audit::{AuditFact, AuditValue};
    use erp_core::AccountKind;

    use super::*;

    #[test]
    fn password_event_uses_registered_action_and_projects_only_resource_identity() {
        let actor = AuditActor::new("sales".into(), "sales-account".into(), AccountKind::Admin);
        let context = password_context(&actor, "book").unwrap();
        let log = context.log(password_content("book".into())).unwrap();
        let event = log.structured_event.unwrap();
        assert_eq!(event.action_code, ACTION);
        assert_eq!(event.resource_id, "book");
        assert_eq!(event.action_label, "维护选品册访问密码");
        assert!(event.resource_number_snapshot.is_none());
        assert!(event.facts.is_empty() && event.field_changes.is_empty());
        assert!(
            context
                .log(BusinessEventContent {
                    facts: vec![AuditFact { field: "access_password".into(), value: AuditValue::Changed }],
                    ..password_content("book".into())
                })
                .is_err()
        );
    }
}
