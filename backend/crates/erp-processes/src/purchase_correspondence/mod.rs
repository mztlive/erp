//! 采购单下载及受控外部邮件发送；持久化发件占用后才调用外部服务。
mod paper;
mod transport;
use application_core::AuditActor;
use config::PurchaseMailConfig;
use entity_core::BaseModel;
use erp_identity::{Permission, SharedRbacService, subject};
use erp_procurement::entity::purchase_email::{PurchaseEmail, PurchaseEmailStatus, SendPurchaseEmail};
use erp_procurement::repository::purchase_email::{purchase_emails, recent};
use erp_read_models::purchase_center::PurchaseOrderReadService;
use erp_read_models::purchase_center::dto::PurchaseOrderCenterView;
use mongodb::Database;
pub use paper::PurchasePaper;
use persistence_core::NoTransaction;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::adapters::MongoPurchaseDataScope;
use crate::{Error, Result};

/// 发件结果仅包含状态和服务商流水，不回传邮件正文或凭据。
#[derive(Debug, Serialize)]
pub struct PurchaseEmailResult {
    pub status: PurchaseEmailStatus,
    pub message_id: Option<String>,
}

/// 下载当前持久化采购内容，沿用采购详情授权。
/// # 参数
/// `db`、`rbac` 为应用能力，`id` 为采购单，`actor` 为当前操作人。
/// # 返回
/// 独立可打印采购文件。
/// # 错误
/// 不可见、无详情资格或读取失败时拒绝。
pub async fn download(
    db: Database,
    rbac: SharedRbacService,
    id: &str,
    actor: &AuditActor,
) -> Result<PurchasePaper> {
    Ok(paper::render(&read_order(&db, &rbac, id, actor).await?))
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
    rbac: SharedRbacService,
    config: Option<&PurchaseMailConfig>,
    id: &str,
    req: SendPurchaseEmail,
    actor: &AuditActor,
) -> Result<PurchaseEmailResult> {
    let config =
        config.ok_or_else(|| Error::ValidationError("尚未配置采购邮件服务，请联系系统管理员".into()))?;
    let permission = Permission::parse("purchase_order:send_email")?;
    if !rbac.enforce(&subject(actor.kind(), actor.id()), &permission).await? {
        return Err(Error::Forbidden("当前账号没有发送采购邮件的权限".into()));
    }
    let recipient = req.recipient()?;
    let order = read_order(&db, &rbac, id, actor).await?;
    let key = digest(&(actor.id(), id, &req.idempotency_key))?;
    let fingerprint = digest(&(id, req.expected_version, &recipient))?;
    let repo = purchase_emails(&db);
    if let Some(existing) = repo.find_by_id(&key, &mut NoTransaction).await? {
        return replay(&existing, &fingerprint);
    }
    req.validate_source(
        order.status,
        order.version,
        order.content_source == "REVISION" && order.current_revision_id.is_some(),
    )?;
    let paper = paper::render(&order);
    let mut record = prepare_record(&order, actor, key, fingerprint, recipient, paper.html);
    // 先占用确定性主键；并发或崩溃后不重复发信。记录本身保存完整可审计发件内容。
    if let Err(error) = repo.create(&record, &mut NoTransaction).await {
        if let Some(existing) = repo.find_by_id(&record.base.id, &mut NoTransaction).await? {
            return replay(&existing, &record.fingerprint);
        }
        return Err(error.into());
    }
    let result = transport::deliver(config, &record, &paper.filename, &order.purchase_no).await;
    record.status = result.status;
    record.provider_message_id = result.message_id.clone();
    repo.update(&mut record, &mut NoTransaction).await?;
    Ok(result)
}
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
    rbac: &SharedRbacService,
    id: &str,
    actor: &AuditActor,
) -> Result<PurchaseOrderCenterView> {
    Ok(PurchaseOrderReadService::with_scope(
        db.clone(),
        MongoPurchaseDataScope::shared(db.clone(), rbac.clone()),
    )
    .purchase_order_detail(id, Some(actor))
    .await?)
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
/// `db`、`rbac` 为应用能力，`id` 为采购单，`actor` 为认证人。
/// # 返回
/// 已授权的最近发送记录。
/// # 错误
/// 不可见、无详情权限或读取失败时拒绝。
pub async fn history(
    db: Database,
    rbac: SharedRbacService,
    id: &str,
    actor: &AuditActor,
) -> Result<Vec<PurchaseEmailHistory>> {
    read_order(&db, &rbac, id, actor).await?;
    Ok(recent(&db, id, &mut NoTransaction)
        .await?
        .into_iter()
        .map(|row| PurchaseEmailHistory {
            id: row.base.id,
            recipient: row.recipient,
            status: row.status,
            created_at: row.base.created_at,
        })
        .collect())
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
    }
}
