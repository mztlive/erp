//! 采购单下载及受控外部邮件发送；持久化发件占用后才调用外部服务。
mod paper;
mod transport;
use std::sync::Arc;

use application_core::AuditActor;
use config::PurchaseMailConfig;
use entity_core::BaseModel;
use erp_audit::execution::recover_command;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::purchase_order;
use erp_procurement::PurchaseDataScopePort;
use erp_procurement::entity::purchase_email::{PurchaseEmail, PurchaseEmailStatus, SendPurchaseEmail};
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::purchase_email::{purchase_emails, recent};
use erp_read_models::purchase_center::PurchaseOrderReadService;
use erp_read_models::purchase_center::dto::PurchaseOrderCenterView;
use mongodb::Database;
pub use paper::PurchasePaper;
use persistence_core::{Executor, NoTransaction, Transactional};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// 发件结果仅包含状态和服务商流水，不回传邮件正文或凭据。
#[derive(Debug, Serialize)]
pub struct PurchaseEmailResult {
    pub status: PurchaseEmailStatus,
    pub message_id: Option<String>,
}

/// 已渲染采购版本、原请求与确定性发件占用，供首次写入及只读查证共用。
struct EmailIntent {
    actor: AuditActor,
    request: SendPurchaseEmail,
    rendered_version: u64,
    rendered_revision_id: Option<String>,
    record: PurchaseEmail,
}

/// 只有首次确定提交的发件占用可以调用外部传输。
enum EmailReservation {
    Created(Box<PurchaseEmail>),
    Replayed(PurchaseEmailResult),
}

/// 下载当前持久化采购内容，沿用采购详情授权。
/// # 参数
/// `db`、`rbac`、`scope` 为应用能力，`id` 为采购单，`actor` 为当前操作人。
/// # 返回
/// 独立可打印采购文件。
/// # 错误
/// 不可见、无详情资格或读取失败时拒绝。
pub async fn download(
    db: Database,
    rbac: Arc<AuthorizationService>,
    scope: Arc<dyn PurchaseDataScopePort>,
    id: &str,
    actor: &AuditActor,
) -> Result<PurchasePaper> {
    Ok(paper::render(&read_order(&db, rbac, scope, id, actor).await?))
}

/// 发出已生效采购版本邮件；同键永久去重，结果未知时只返回原状态。
/// # 参数
/// `config` 为发件配置，`req` 为期望版本、邮箱和重试键，其余为应用及身份上下文。
/// # 返回
/// 发件服务受理、明确失败或结果未知的状态。
/// # 错误
/// 未配置、无发送权限、范围拒绝、版本变化或同键异载荷时拒绝。
pub async fn send(
    db: Database,
    authorization: Arc<AuthorizationService>,
    scope: Arc<dyn PurchaseDataScopePort>,
    config: Option<&PurchaseMailConfig>,
    id: &str,
    req: SendPurchaseEmail,
    actor: &AuditActor,
) -> Result<PurchaseEmailResult> {
    let config =
        config.ok_or_else(|| Error::ValidationError("尚未配置采购邮件服务，请联系系统管理员".into()))?;
    let recipient = req.recipient()?;
    let order = read_order(&db, authorization.clone(), scope, id, actor).await?;
    let key = digest(&(actor.id(), id, &req.idempotency_key))?;
    let fingerprint = digest(&(id, req.expected_version, &recipient))?;
    let paper = paper::render(&order);
    let command = Arc::new(EmailIntent {
        actor: actor.clone(),
        request: req,
        rendered_version: order.version,
        rendered_revision_id: order
            .current_revision_id
            .clone()
            .filter(|_| order.content_source == "REVISION"),
        record: prepare_record(&order, actor, key, fingerprint, recipient, paper.html),
    });
    let mut record = match reserve_email(&db, authorization, command).await? {
        EmailReservation::Created(record) => *record,
        EmailReservation::Replayed(result) => return Ok(result),
    };
    let result = transport::deliver(config, &record, &paper.filename, &order.purchase_no).await;
    record.status = result.status;
    record.provider_message_id = result.message_id.clone();
    purchase_emails(&db).update(&mut record, &mut NoTransaction).await?;
    Ok(result)
}

/// 原发件事务内占用唯一记录；只有可能已提交的错误才进入只读查证。
async fn reserve_email(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    command: Arc<EmailIntent>,
) -> Result<EmailReservation> {
    let transaction_db = db.clone();
    let transaction_authorization = authorization.clone();
    let transaction_command = command.clone();
    let result = db
        .client()
        .with_transaction(move |executor| {
            Box::pin(async move {
                create_intent(&transaction_db, &transaction_authorization, &transaction_command, executor)
                    .await
            })
        })
        .await;
    match result {
        Err(error) if error.command_may_have_committed() => {
            recover_command(error, recover_intent(db, authorization, command).await)
        },
        other => other,
    }
}

/// 授权、真实采购版本核验与唯一占用使用同一执行器。
async fn create_intent(
    db: &Database,
    authorization: &AuthorizationService,
    command: &EmailIntent,
    executor: &mut dyn Executor,
) -> Result<EmailReservation> {
    require_email(authorization, command, executor).await?;
    let repo = purchase_emails(db);
    if let Some(existing) = repo.find_by_id(&command.record.base.id, executor).await? {
        return Ok(EmailReservation::Replayed(replay_intent(command, &existing)?));
    }
    let current = db
        .purchase_orders()
        .find_by_id(&command.record.purchase_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购单不存在".into()))?;
    ensure_rendered_current(command, &current)?;
    repo.create(&command.record, executor).await.map_err(|error| match error {
        error @ persistence_core::Error::DuplicateKey(_) => Error::ReceiptDuplicate(error),
        other => other.into(),
    })?;
    Ok(EmailReservation::Created(Box::new(command.record.clone())))
}

/// 固定发件入口一次检查发送资格、独立详情资格和实际采购对象。
async fn require_email(
    authorization: &AuthorizationService,
    command: &EmailIntent,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization
        .require(
            &command.actor,
            &purchase_order::CORRESPONDENCE_SEND_PURCHASE_EMAIL,
            AccessTarget::record(&command.record.purchase_order_id)?,
            executor,
        )
        .await?;
    Ok(())
}

/// 已渲染正文只能占用仍为同一版本及生效修订的采购单。
fn ensure_rendered_current(command: &EmailIntent, current: &PurchaseOrder) -> Result<()> {
    if current.base.version != command.rendered_version
        || current.stable.current_revision_id != command.rendered_revision_id
    {
        return Err(Error::ConflictError("采购单已变化，请刷新后重新确认".into()));
    }
    command.request.validate_source(
        current.stable.status,
        current.base.version,
        command.rendered_revision_id.is_some(),
    )?;
    Ok(())
}

/// 并发占用或提交未知后只授权并读取原键，不写记录、不再次发送。
async fn recover_intent(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    command: Arc<EmailIntent>,
) -> Result<Option<EmailReservation>> {
    let transaction_db = db.clone();
    db.client()
        .with_transaction(move |executor| {
            Box::pin(async move {
                require_email(&authorization, &command, executor).await?;
                purchase_emails(&transaction_db)
                    .find_by_id(&command.record.base.id, executor)
                    .await?
                    .map(|record| replay_intent(&command, &record).map(EmailReservation::Replayed))
                    .transpose()
            })
        })
        .await
}

/// 只接受同一操作人、采购单和确定性主键的原发送结果。
fn replay_intent(command: &EmailIntent, record: &PurchaseEmail) -> Result<PurchaseEmailResult> {
    if record.base.id != command.record.base.id
        || record.actor_id != command.record.actor_id
        || record.purchase_order_id != command.record.purchase_order_id
    {
        return Err(Error::ConflictError("发件记录与原命令不匹配".into()));
    }
    replay(record, &command.record.fingerprint)
}

/// 保存当前已渲染生效修订的审计内容。
fn prepare_record(
    order: &PurchaseOrderCenterView,
    actor: &AuditActor,
    key: String,
    fingerprint: String,
    recipient: String,
    html: String,
) -> PurchaseEmail {
    PurchaseEmail {
        base: BaseModel::new(key),
        purchase_order_id: order.id.clone(),
        revision_id: order.current_revision_id.clone().unwrap_or_default(),
        actor_id: actor.id().into(),
        fingerprint,
        recipient,
        html,
        status: PurchaseEmailStatus::Sending,
        provider_message_id: None,
    }
}
async fn read_order(
    db: &Database,
    rbac: Arc<AuthorizationService>,
    scope: Arc<dyn PurchaseDataScopePort>,
    id: &str,
    actor: &AuditActor,
) -> Result<PurchaseOrderCenterView> {
    Ok(PurchaseOrderReadService::new(db.clone(), rbac, scope).purchase_order_detail(id, Some(actor)).await?)
}
fn digest(value: &impl Serialize) -> Result<String> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(value).map_err(|_| Error::Internal("发件命令编码失败".into()))?,
    )))
}
fn replay(record: &PurchaseEmail, fingerprint: &str) -> Result<PurchaseEmailResult> {
    record.validate_replay(fingerprint)?;
    Ok(PurchaseEmailResult { status: record.status, message_id: record.provider_message_id.clone() })
}

/// 采购详情可读的发件记录，不返回正文及服务凭据。
#[derive(Serialize)]
pub struct PurchaseEmailHistory {
    pub id: String,
    pub recipient: String,
    pub status: PurchaseEmailStatus,
    pub created_at: u64,
}
/// 读取本采购单最近 20 次邮件发送结果。
/// # 参数
/// `db`、`authorization` 为应用能力，`id` 为采购单，`actor` 为认证人。
/// # 返回
/// 已授权的最近发送记录。
/// # 错误
/// 不可见、无详情权限或读取失败时拒绝。
pub async fn history(
    db: Database,
    authorization: Arc<AuthorizationService>,
    id: &str,
    actor: &AuditActor,
) -> Result<Vec<PurchaseEmailHistory>> {
    let transaction_db = db.clone();
    let id = id.to_string();
    let actor = actor.clone();
    db.client()
        .with_transaction(move |executor| {
            Box::pin(async move {
                authorization
                    .require(&actor, &purchase_order::DETAIL, AccessTarget::record(&id)?, executor)
                    .await?;
                Ok(recent(&transaction_db, &id, executor)
                    .await?
                    .into_iter()
                    .map(|row| PurchaseEmailHistory {
                        id: row.base.id,
                        recipient: row.recipient,
                        status: row.status,
                        created_at: row.base.created_at,
                    })
                    .collect())
            })
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retries_return_the_recorded_result_and_reject_payload_changes() {
        let mut row = PurchaseEmail {
            base: BaseModel::fake(),
            purchase_order_id: "purchase-1".into(),
            revision_id: "rev-1".into(),
            actor_id: "actor-1".into(),
            fingerprint: "same".into(),
            recipient: "a@example.com".into(),
            html: "paper".into(),
            status: PurchaseEmailStatus::Sending,
            provider_message_id: None,
        };
        for status in [
            PurchaseEmailStatus::Sending,
            PurchaseEmailStatus::Accepted,
            PurchaseEmailStatus::Failed,
            PurchaseEmailStatus::Unknown,
        ] {
            row.status = status;
            assert_eq!(replay(&row, "same").unwrap().status, status);
            assert!(replay(&row, "changed").is_err());
        }
        assert_ne!(digest(&("a", "bc")).unwrap(), digest(&("ab", "c")).unwrap());
        let command = EmailIntent {
            actor: AuditActor::new("actor-1".into(), "buyer".into(), erp_core::AccountKind::Admin),
            request: SendPurchaseEmail {
                expected_version: 1,
                recipient: row.recipient.clone(),
                idempotency_key: "key".into(),
            },
            rendered_version: 1,
            rendered_revision_id: Some(row.revision_id.clone()),
            record: row.clone(),
        };
        assert_eq!(replay_intent(&command, &row).unwrap().status, PurchaseEmailStatus::Unknown);
        let mut foreign_actor = row.clone();
        foreign_actor.actor_id.push_str("-other");
        assert!(replay_intent(&command, &foreign_actor).is_err());
        let mut foreign_order = row.clone();
        foreign_order.purchase_order_id.push_str("-other");
        assert!(replay_intent(&command, &foreign_order).is_err());
        let mut foreign_key = row;
        foreign_key.base.id.push_str("-other");
        assert!(replay_intent(&command, &foreign_key).is_err());
    }
}
