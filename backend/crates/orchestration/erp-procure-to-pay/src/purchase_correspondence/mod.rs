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

/// 尚未读取业务内容的原请求与确定性命令身份，供首次写入及只读查证共用。
struct EmailIntent {
    actor: AuditActor,
    request: SendPurchaseEmail,
    purchase_order_id: String,
    key: String,
    fingerprint: String,
    recipient: String,
}

/// 原事务读取、校验和渲染后的发件内容；提交完成后才可调用外部服务。
struct PreparedEmail {
    record: PurchaseEmail,
    filename: String,
    purchase_no: String,
}

/// 只有首次确定提交的发件占用可以调用外部传输。
enum EmailReservation {
    Created(Box<PreparedEmail>),
    Replayed(PurchaseEmailResult),
}

/// 下载当前持久化采购内容，沿用采购详情授权。
///
/// `db`、`rbac`、`scope` 为应用能力，`id` 为采购单，`actor` 为当前操作人。独立可打印采购文件。不可见、无详情资格或读取失败时拒绝。
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
///
/// `config` 为发件配置，`req` 为期望版本、邮箱和重试键，其余为应用及身份上下文。发件服务受理、明确失败或结果未知的状态。
///
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
    let key = digest(&(actor.id(), id, &req.idempotency_key))?;
    let fingerprint = digest(&(id, req.expected_version, &recipient))?;

    let command = Arc::new(EmailIntent {
        actor: actor.clone(),
        request: req,
        purchase_order_id: id.to_owned(),
        key,
        fingerprint,
        recipient,
    });
    let mut email = match reserve_email(&db, authorization, scope, command).await? {
        EmailReservation::Created(email) => *email,
        EmailReservation::Replayed(result) => return Ok(result),
    };

    let result = transport::deliver(config, &email.record, &email.filename, &email.purchase_no).await;
    email.record.status = result.status;
    email.record.provider_message_id = result.message_id.clone();
    purchase_emails(&db).update(&mut email.record, &mut NoTransaction).await?;
    Ok(result)
}

// 原发件事务内占用唯一记录；只有可能已提交的错误才进入只读查证。
async fn reserve_email(
    db: &Database,
    authorization: Arc<AuthorizationService>,
    scope: Arc<dyn PurchaseDataScopePort>,
    command: Arc<EmailIntent>,
) -> Result<EmailReservation> {
    let transaction_db = db.clone();
    let source = PurchaseOrderReadService::new(db.clone(), authorization.clone(), scope);
    let transaction_command = command.clone();
    let result = db
        .client()
        .with_transaction(move |executor| {
            Box::pin(
                async move { create_intent(&transaction_db, &source, &transaction_command, executor).await },
            )
        })
        .await;

    match result {
        Err(error) if error.command_may_have_committed() => {
            recover_command(error, recover_intent(db, authorization, command).await)
        },
        other => other,
    }
}

// 授权、真实采购版本核验与唯一占用使用同一执行器。
async fn create_intent(
    db: &Database,
    source: &PurchaseOrderReadService,
    command: &EmailIntent,
    executor: &mut dyn Executor,
) -> Result<EmailReservation> {
    let order =
        source.purchase_order_email_source(&command.purchase_order_id, &command.actor, executor).await?;
    let repo = purchase_emails(db);
    if let Some(existing) = repo.find_by_id(&command.key, executor).await? {
        return Ok(EmailReservation::Replayed(replay_intent(command, &existing)?));
    }

    let email = prepare_email(command, &order)?;
    repo.create(&email.record, executor).await.map_err(|error| match error {
        error @ persistence_core::Error::DuplicateKey(_) => Error::ReceiptDuplicate(error),
        other => other.into(),
    })?;
    Ok(EmailReservation::Created(Box::new(email)))
}

// 固定发件入口只检查发送资格和实际采购对象。
async fn require_email(
    authorization: &AuthorizationService,
    command: &EmailIntent,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization
        .require(
            &command.actor,
            &purchase_order::CORRESPONDENCE_SEND_PURCHASE_EMAIL,
            AccessTarget::record(&command.purchase_order_id)?,
            executor,
        )
        .await?;
    Ok(())
}

// 只渲染原发送事务内读取的已生效采购版本。
fn prepare_email(command: &EmailIntent, order: &PurchaseOrderCenterView) -> Result<PreparedEmail> {
    command.request.validate_source(
        order.status,
        order.version,
        order.content_source == "REVISION" && order.current_revision_id.is_some(),
    )?;

    let paper = paper::render(order);
    Ok(PreparedEmail {
        record: prepare_record(order, command, paper.html),
        filename: paper.filename,
        purchase_no: order.purchase_no.clone(),
    })
}

// 并发占用或提交未知后只授权并读取原键，不写记录、不再次发送。
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
                    .find_by_id(&command.key, executor)
                    .await?
                    .map(|record| replay_intent(&command, &record).map(EmailReservation::Replayed))
                    .transpose()
            })
        })
        .await
}

// 只接受同一操作人、采购单和确定性主键的原发送结果。
fn replay_intent(command: &EmailIntent, record: &PurchaseEmail) -> Result<PurchaseEmailResult> {
    if record.base.id != command.key
        || record.actor_id != command.actor.id()
        || record.purchase_order_id != command.purchase_order_id
    {
        return Err(Error::ConflictError("已有采购邮件记录与本次发送内容不符，请先核对发件记录".into()));
    }
    replay(record, &command.fingerprint)
}

// 保存当前已渲染生效修订的审计内容。
fn prepare_record(order: &PurchaseOrderCenterView, command: &EmailIntent, html: String) -> PurchaseEmail {
    PurchaseEmail {
        base: BaseModel::new(command.key.clone()),
        purchase_order_id: order.id.clone(),
        revision_id: order.current_revision_id.clone().unwrap_or_default(),
        actor_id: command.actor.id().into(),
        fingerprint: command.fingerprint.clone(),
        recipient: command.recipient.clone(),
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
///
/// `db`、`authorization` 为应用能力，`id` 为采购单，`actor` 为认证人。已授权的最近发送记录。不可见、无详情权限或读取失败时拒绝。
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
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_identity::testing::unavailable_authorization;
    use erp_permissions::Permission;
    use erp_procurement::dto::purchase_order::TotalsView;
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, ProgressStatus, PurchaseOrderStatus, PurchaseReviewStatus, PurchaseType,
    };
    use erp_procurement::{Error as ProcurementError, PurchaseResolvedScope, Result as ProcurementResult};
    use erp_read_models::purchase_center::dto::{DocumentApprovalHistoryPageView, DocumentApprovalView};
    use mongodb::Client;
    use persistence_core::ExecutionBoundary;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    fn command() -> EmailIntent {
        EmailIntent {
            actor: AuditActor::new("actor-1".into(), "buyer".into(), AccountKind::Admin),
            request: SendPurchaseEmail {
                expected_version: 1,
                recipient: "a@example.com".into(),
                idempotency_key: "key".into(),
            },
            purchase_order_id: "purchase-1".into(),
            key: "email-1".into(),
            fingerprint: "same".into(),
            recipient: "a@example.com".into(),
        }
    }

    fn order() -> PurchaseOrderCenterView {
        PurchaseOrderCenterView {
            id: "purchase-1".into(),
            purchase_no: "PO-1".into(),
            status: PurchaseOrderStatus::Effective,
            review_status: PurchaseReviewStatus::Approved,
            version: 1,
            sales_order_id: "sales-1".into(),
            sales_order_no: "SO-1".into(),
            supplier_id: "supplier-1".into(),
            supplier_name: "供应商".into(),
            purchase_type: PurchaseType::Physical,
            payment_term_code: "NET-30".into(),
            fulfillment_responsibility: FulfillmentResponsibility::SupplierDirect,
            owner_user_id: "actor-1".into(),
            owner_name: "采购负责人".into(),
            target_warehouse_id: None,
            target_warehouse_name: None,
            payment_progress: ProgressStatus::None,
            invoice_progress: ProgressStatus::None,
            fulfillment_progress: ProgressStatus::None,
            current_submission_id: None,
            current_revision_id: Some("rev-1".into()),
            revision_no: Some(1),
            content_source: "REVISION".into(),
            lines: Vec::new(),
            source_sales_order: None,
            source_update: None,
            delivery: None,
            totals: TotalsView { gross: "113.00".into(), net: "100.00".into(), tax: "13.00".into() },
            allocations: Vec::new(),
            api_executions: Vec::new(),
            fulfillment_quantities: Vec::new(),
            api_execution_funds: None,
            changes: Vec::new(),
            payable_summary: None,
            approval: DocumentApprovalView {
                requirement: "NO_APPROVAL".into(),
                definition: None,
                instance: None,
                recent_history: Vec::new(),
                history_page: DocumentApprovalHistoryPageView::default(),
                allowed_actions: Vec::new(),
            },
            created_at: 1,
        }
    }

    #[test]
    fn confirmed_source_is_rendered_into_the_deterministic_reservation() {
        let command = command();
        let email = prepare_email(&command, &order()).unwrap();

        assert_eq!(email.record.base.id, command.key);
        assert_eq!(email.record.purchase_order_id, command.purchase_order_id);
        assert_eq!(email.record.revision_id, "rev-1");
        assert_eq!(email.record.actor_id, "actor-1");
        assert_eq!(email.record.fingerprint, command.fingerprint);
        assert_eq!(email.record.recipient, command.recipient);
        assert_eq!(email.record.status, PurchaseEmailStatus::Sending);

        assert!(email.record.html.contains("供应商"));
        assert!(email.record.html.contains("113.00"));
        assert_eq!(email.filename, "采购单-PO-1.html");
        assert_eq!(email.purchase_no, "PO-1");
    }

    #[test]
    fn stale_or_incomplete_current_content_cannot_create_a_reservation() {
        let command = command();
        let mut stale = order();
        stale.version = 2;
        let error = prepare_email(&command, &stale).err().unwrap();
        assert_eq!(error.to_string(), "采购单已变化，请刷新后重新确认");
        assert!(matches!(error, Error::Logic(_)));

        for incomplete in ["SUBMISSION", "DRAFT"] {
            let mut current = order();
            current.content_source = incomplete.into();
            let error = prepare_email(&command, &current).err().unwrap();
            assert_eq!(error.to_string(), "采购单审批生效后才能发送给供应商");
        }

        let mut missing = order();
        missing.current_revision_id = None;
        let error = prepare_email(&command, &missing).err().unwrap();
        assert_eq!(error.to_string(), "采购单审批生效后才能发送给供应商");
        assert!(matches!(error, Error::Logic(_)));
    }

    struct DeniedSource {
        boundary: ExecutionBoundary,
        entries: Mutex<Vec<Permission>>,
        unavailable: bool,
    }

    #[async_trait]
    impl PurchaseDataScopePort for DeniedSource {
        async fn require_record_entry(
            &self,
            actor: &AuditActor,
            entry: &Permission,
            id: &str,
            executor: &mut dyn Executor,
        ) -> ProcurementResult<()> {
            assert_eq!(actor.id(), "actor-1");
            assert_eq!(id, "purchase-1");
            assert!(self.boundary.is_current(executor));
            self.entries.lock().unwrap().push(entry.clone());
            if self.unavailable {
                Err(ProcurementError::Internal("发送授权设施不可用".into()))
            } else {
                Err(ProcurementError::Forbidden("采购对象越界".into()))
            }
        }

        async fn resolve(
            &self,
            _: &AuditActor,
            _: &Permission,
            _: &mut dyn Executor,
        ) -> ProcurementResult<PurchaseResolvedScope> {
            panic!("发送不得编译详情范围查询")
        }
        async fn expand_org_units(
            &self,
            _: &[String],
            _: bool,
            _: &mut dyn Executor,
        ) -> ProcurementResult<BTreeSet<String>> {
            panic!("发送不得自行展开组织")
        }
        async fn org_member_ids(
            &self,
            _: &BTreeSet<String>,
            _: Instant,
            _: &mut dyn Executor,
        ) -> ProcurementResult<Vec<String>> {
            panic!("发送不得自行读取成员")
        }
        async fn own_org(
            &self,
            _: &str,
            _: Instant,
            _: &mut dyn Executor,
        ) -> ProcurementResult<Option<String>> {
            panic!("发送不得自行读取主属组织")
        }
    }

    #[tokio::test]
    async fn send_authorization_uses_the_original_executor_and_stops_before_business_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("purchase_email_authorization_unit");

        for unavailable in [false, true] {
            let mut executor = SnapshotExecutor::default();
            let scope = Arc::new(DeniedSource {
                boundary: executor.boundary().unwrap(),
                entries: Mutex::default(),
                unavailable,
            });
            let source =
                PurchaseOrderReadService::new(db.clone(), unavailable_authorization(), scope.clone());

            let error = create_intent(&db, &source, &command(), &mut executor).await.err().unwrap();
            if unavailable {
                assert!(matches!(error, Error::Internal(message) if message == "发送授权设施不可用"));
            } else {
                assert!(matches!(error, Error::NotFound(message) if message == "采购单不存在或无权操作"));
            }
            assert_eq!(*scope.entries.lock().unwrap(), [purchase_order::CORRESPONDENCE_SEND_PURCHASE_EMAIL]);
        }
    }

    #[test]
    fn retries_return_the_recorded_result_and_reject_payload_changes() {
        let mut row = PurchaseEmail {
            base: BaseModel { id: "email-1".into(), ..BaseModel::fake() },
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
        let command = command();
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
