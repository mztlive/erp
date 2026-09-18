//! 线下服务履约确认：写入现场事实与图片凭证后由草稿迁到已确认。

use std::collections::HashSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::ids::{FileAssetId, ServiceFulfillmentId};
use erp_fulfillment::dto::{ConfirmServiceFulfillmentRequest, ServiceFulfillmentView};
use erp_fulfillment::entity::fulfillment::{
    ServiceEvidencePolicy, ServiceFulfillment, ServiceFulfillmentConfirmation,
};
use erp_fulfillment::service::FulfillmentService;
use erp_fulfillment::service::service_fulfillment_confirm::service_confirmation_from_request;
use erp_procurement::repository::PurchaseOrderExt;
use erp_support::{EmptyPendingAttachments, FileAssetExt, PendingAttachmentBatch};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::FulfillmentProcess;
use super::purchase_context::{ensure_allocation_valid, ensure_po_fulfillable, ensure_prepay_gate};
use super::service_crypto::{ServiceCryptoAdapter, evidence_metadata};
use crate::{Error, Result};

impl FulfillmentProcess {
    /// 确认服务履约（草稿 → 已确认；§8.1.5 + §6.7 跨集合事务）。
    ///
    /// 不携带新上传对象的内部兼容入口；HTTP 确认使用
    /// [`Self::confirm_service_fulfillment_with_assets`]。
    ///
    /// # 参数
    /// * `id` - 记录主键
    /// * `req` - 现场事实、乐观锁版本与图片凭证
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回确认后的记录视图。
    ///
    /// # 错误
    /// 校验失败、状态冲突、门槛未满足或事务提交结果未知时返回错误。
    pub async fn confirm_service_fulfillment(
        &self,
        id: &str,
        req: ConfirmServiceFulfillmentRequest,
        actor: &AuditActor,
    ) -> Result<ServiceFulfillmentView> {
        self.confirm_service_fulfillment_with_assets(id, req, Arc::new(EmptyPendingAttachments), actor).await
    }

    /// 确认服务履约，同时登记本次上传的现场图片凭证。
    ///
    /// 门槛、采购销售分配、凭证资产、履约记录、任务完成和审计位于同一事务。
    ///
    /// # 参数
    /// * `id` - 记录主键
    /// * `req` - 现场事实、乐观锁版本与图片凭证
    /// * `asset_requests` - 本次 multipart 待登记文件
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回确认后的记录视图。
    ///
    /// # 错误
    /// * `ValidationError` - 地点、时间、说明、数量或图片不合法
    /// * `NotFound` - 记录/采购单/分配/凭证不存在
    /// * `ConflictError` - 状态不允许确认或版本已变化
    /// * `BusinessLogicError` - 门槛未满足或分配无效
    /// * `OutcomeUnknown` - 提交结果无法确认
    #[tracing::instrument(
        name = "fulfillment.service_fulfillment_confirm",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "service_fulfillment_confirm")
    )]
    pub async fn confirm_service_fulfillment_with_assets(
        &self,
        id: &str,
        mut req: ConfirmServiceFulfillmentRequest,
        pending_assets: Arc<dyn PendingAttachmentBatch>,
        actor: &AuditActor,
    ) -> Result<ServiceFulfillmentView> {
        req.validate()?;
        let evidence_attachment_id = resolve_service_evidence_id(&mut req, &pending_assets)?;
        let confirmation = service_confirmation_from_request(
            &req,
            evidence_attachment_id,
            &self.fingerprint_key,
            &ServiceCryptoAdapter(self.sensitive_data.as_ref()),
        )?;
        persist_confirmed_service_fulfillment(
            &self.db,
            ServiceFulfillmentId::new(id.to_string()),
            req.version,
            confirmation,
            pending_assets,
            actor.clone(),
        )
        .await
    }
}

/// 在事务内写入现场事实、凭证资产并确认服务履约。
///
/// # 参数
/// * `db` - 数据库实例
/// * `record_id` - 服务履约主键
/// * `expected_version` - 调用方看到的乐观锁版本
/// * `confirmation` - 已规范化的确认现场事实
/// * `pending_assets` - 本次待登记图片凭证
/// * `actor` - 已通过鉴权的审计操作人
///
/// # 返回
/// 返回确认后的记录视图。
///
/// # 错误
/// 记录不存在、版本冲突、门槛失败或事务提交失败时返回错误。
async fn persist_confirmed_service_fulfillment(
    db: &Database,
    record_id: ServiceFulfillmentId,
    expected_version: u64,
    confirmation: ServiceFulfillmentConfirmation,
    pending_assets: Arc<dyn PendingAttachmentBatch>,
    actor: AuditActor,
) -> Result<ServiceFulfillmentView> {
    let db = db.clone();
    let client = db.client().clone();
    let confirmed = client
        .with_transaction(move |executor| {
            Box::pin(async move {
                confirm_service_fulfillment_apply(
                    &db,
                    &record_id,
                    expected_version,
                    confirmation,
                    &pending_assets,
                    &actor,
                    executor,
                )
                .await
            })
        })
        .await?;
    Ok(confirmed.into())
}

/// 在调用方事务内完成门槛校验、凭证登记与状态迁移。
///
/// # 参数
/// * `db` - 数据库实例
/// * `record_id` - 服务履约主键
/// * `expected_version` - 调用方看到的乐观锁版本
/// * `confirmation` - 已规范化的确认现场事实
/// * `pending_assets` - 本次待登记图片凭证
/// * `actor` - 已通过鉴权的审计操作人
/// * `session` - 事务会话
///
/// # 返回
/// 返回确认后的实体。
///
/// # 错误
/// 记录不存在、版本冲突、门槛失败或凭证不合法时返回错误。
async fn confirm_service_fulfillment_apply(
    db: &Database,
    record_id: &ServiceFulfillmentId,
    expected_version: u64,
    confirmation: ServiceFulfillmentConfirmation,
    pending_assets: &dyn PendingAttachmentBatch,
    actor: &AuditActor,
    session: &mut dyn Executor,
) -> Result<ServiceFulfillment> {
    execute_confirmation(
        &MongoServiceConfirmation { db, record_id, expected_version, confirmation, pending_assets, actor },
        session,
    )
    .await
}

/// 把确认命令中的临时凭证引用替换为本批次正式资产 ID。
///
/// # 参数
/// * `req` - 确认命令
/// * `pending_assets` - 本次待登记图片凭证
///
/// # 返回
/// 返回正式凭证主键。
///
/// # 错误
/// 引用了未上传文件或存在未被引用的上传文件时返回校验错误。
fn resolve_service_evidence_id(
    req: &mut ConfirmServiceFulfillmentRequest,
    pending_assets: &dyn PendingAttachmentBatch,
) -> Result<FileAssetId> {
    let mut used = HashSet::new();
    pending_assets.resolve_id(&mut req.evidence_attachment_id, &mut used)?;
    pending_assets.ensure_all_used(&used)?;
    Ok(req.evidence_attachment_id.clone())
}

/// 在确认事务内校验正式或本批次待登记的现场图片凭证。
///
/// 本批次待登记资产在事务前已按 [`ServiceEvidencePolicy`] 完成元数据校验，
/// 此处只识别其属于本批次；正式资产按同一策略校验持久化元数据。
///
/// # 参数
/// * `db` - 数据库实例
/// * `asset_id` - 正式凭证主键
/// * `pending_assets` - 本次待登记图片凭证
/// * `session` - 事务会话
///
/// # 返回
/// 凭证可用时返回 `Ok(())`。
///
/// # 错误
/// 凭证不存在、已销毁或元数据不合法时返回错误。
pub(super) async fn ensure_service_evidence_asset(
    db: &Database,
    asset_id: &FileAssetId,
    pending_assets: &dyn PendingAttachmentBatch,
    session: &mut dyn Executor,
) -> Result<()> {
    if pending_assets.contains_id(asset_id) {
        return Ok(());
    }
    let asset = db
        .file_assets()
        .find_by_id(asset_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::NotFound("现场图片凭证不存在".to_string()))?;
    let (sensitivity, retention) = evidence_metadata(asset.sensitivity_class, asset.retention_class);
    ServiceEvidencePolicy::validate(&asset.content_type, sensitivity, retention, asset.destroyed_at.is_some())
        .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 真实确认步骤边界；替身测试与 Mongo 适配器共同执行同一顺序。
#[async_trait::async_trait]
trait ServiceConfirmationPort: Send + Sync {
    type Record: Send + Sync;
    type Purchase: Send + Sync;

    async fn load(&self, executor: &mut dyn Executor) -> Result<Self::Record>;
    async fn purchase(&self, record: &Self::Record, executor: &mut dyn Executor) -> Result<Self::Purchase>;
    async fn allocation(
        &self,
        record: &Self::Record,
        purchase: &Self::Purchase,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn evidence(&self, executor: &mut dyn Executor) -> Result<()>;
    async fn pending(&self, executor: &mut dyn Executor) -> Result<()>;
    async fn confirm(&self, record: &mut Self::Record, executor: &mut dyn Executor) -> Result<()>;
    async fn task(&self, record: &Self::Record, executor: &mut dyn Executor) -> Result<()>;
    fn is_acceptance_eligible(&self, record: &Self::Record) -> bool;
    async fn acceptance(&self, purchase: &Self::Purchase, executor: &mut dyn Executor) -> Result<()>;
    async fn audit(&self, executor: &mut dyn Executor) -> Result<()>;
}

async fn execute_confirmation<P: ServiceConfirmationPort>(
    port: &P,
    executor: &mut dyn Executor,
) -> Result<P::Record> {
    let mut record = port.load(executor).await?;
    let purchase = port.purchase(&record, executor).await?;
    port.allocation(&record, &purchase, executor).await?;
    port.evidence(executor).await?;
    port.pending(executor).await?;
    port.confirm(&mut record, executor).await?;
    port.task(&record, executor).await?;
    if port.is_acceptance_eligible(&record) {
        port.acceptance(&purchase, executor).await?;
    }
    port.audit(executor).await?;
    Ok(record)
}

struct MongoServiceConfirmation<'a> {
    db: &'a Database,
    record_id: &'a ServiceFulfillmentId,
    expected_version: u64,
    confirmation: ServiceFulfillmentConfirmation,
    pending_assets: &'a dyn PendingAttachmentBatch,
    actor: &'a AuditActor,
}

#[async_trait::async_trait]
impl ServiceConfirmationPort for MongoServiceConfirmation<'_> {
    type Record = ServiceFulfillment;
    type Purchase = erp_procurement::entity::purchase_order::PurchaseOrder;

    async fn load(&self, executor: &mut dyn Executor) -> Result<Self::Record> {
        Ok(FulfillmentService::new(self.db.clone())
            .prepare_service_confirmation(self.record_id, self.expected_version, executor)
            .await?)
    }

    async fn purchase(&self, record: &Self::Record, executor: &mut dyn Executor) -> Result<Self::Purchase> {
        let po = self
            .db
            .purchase_orders()
            .find_by_id(record.purchase_order_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("来源采购单不存在".to_string()))?;
        ensure_po_fulfillable(&po)?;
        ensure_prepay_gate(self.db, executor, &po).await?;
        Ok(po)
    }

    async fn allocation(
        &self,
        record: &Self::Record,
        po: &Self::Purchase,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        ensure_allocation_valid(
            self.db,
            executor,
            po,
            &record.purchase_line_sales_allocation_id,
            &record.sales_order_line_id,
        )
        .await
    }

    async fn evidence(&self, executor: &mut dyn Executor) -> Result<()> {
        ensure_service_evidence_asset(
            self.db,
            &self.confirmation.evidence_attachment_id,
            self.pending_assets,
            executor,
        )
        .await
    }

    async fn pending(&self, executor: &mut dyn Executor) -> Result<()> {
        self.pending_assets.persist(self.db, executor).await?;
        Ok(())
    }

    async fn confirm(&self, record: &mut Self::Record, executor: &mut dyn Executor) -> Result<()> {
        FulfillmentService::new(self.db.clone())
            .persist_service_confirmation(record, self.confirmation.clone(), executor)
            .await?;
        Ok(())
    }

    async fn task(&self, record: &Self::Record, executor: &mut dyn Executor) -> Result<()> {
        super::task::complete_fulfillment_task(
            self.db,
            super::task::FulfillmentTaskObject::ServiceFulfillment(record),
            self.actor.id(),
            executor,
        )
        .await
    }

    fn is_acceptance_eligible(&self, record: &Self::Record) -> bool {
        record.is_acceptance_eligible()
    }

    async fn acceptance(&self, po: &Self::Purchase, executor: &mut dyn Executor) -> Result<()> {
        super::customer_acceptance::task::ensure_customer_acceptance_task(
            self.db,
            &po.sales_order_id,
            super::customer_acceptance::task::CustomerAcceptanceTaskReason::DeliveryAvailable,
            executor,
        )
        .await?;
        Ok(())
    }

    async fn audit(&self, executor: &mut dyn Executor) -> Result<()> {
        let audit = self.actor.clone().resource_log(
            "service_fulfillment.confirm",
            "service_fulfillment",
            self.record_id.to_string(),
        )?;
        self.db.audit_logs().create(&audit, executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::FileAssetId;
    use erp_core::money::Quantity;
    use erp_fulfillment::entity::fulfillment::{FulfillmentResult, ServiceFulfillment};
    use erp_party::SensitiveDataCodec;

    use super::{ConfirmServiceFulfillmentRequest, ServiceCryptoAdapter, service_confirmation_from_request};

    /// 确认边界只持久化密文，并以同一份规范化明文计算查询指纹。
    #[test]
    fn confirmation_encrypts_normalized_service_location() {
        let sensitive_data = SensitiveDataCodec::from_secret(b"test-secret-that-is-at-least-32-bytes");
        let fingerprint_key = b"test-service-location-fingerprint";
        let request = ConfirmServiceFulfillmentRequest {
            version: 1,
            result: FulfillmentResult::Success,
            completion_note: "上门安装完成".to_string(),
            service_location: "  客户现场  ".to_string(),
            service_started_at: 1_700_000_000,
            service_ended_at: 1_700_003_600,
            quantity: Quantity::from_str("1").unwrap(),
            evidence_attachment_id: FileAssetId::new("file-1"),
        };

        let confirmation = service_confirmation_from_request(
            &request,
            FileAssetId::new("file-1"),
            fingerprint_key,
            &ServiceCryptoAdapter(&sensitive_data),
        )
        .unwrap();

        assert_ne!(confirmation.service_location_encrypted, "客户现场");
        assert_eq!(sensitive_data.decrypt(&confirmation.service_location_encrypted).unwrap(), "客户现场");
        assert_eq!(
            confirmation.service_location_fingerprint,
            ServiceFulfillment::service_location_fingerprint("客户现场", fingerprint_key)
        );
    }
}

#[cfg(test)]
mod confirmation_order_tests {
    use std::sync::Mutex;

    use persistence_core::Executor;

    use super::{ServiceConfirmationPort, execute_confirmation};
    use crate::{Error, Result};

    struct TestExecutor {
        visits: usize,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            self.visits += 1;
            None
        }
    }
    struct RecordingPort {
        identity: usize,
        eligible: bool,
        fail: Option<&'static str>,
        calls: Mutex<Vec<&'static str>>,
    }
    impl RecordingPort {
        fn record(&self, name: &'static str, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.identity);
            executor.session();
            self.calls.lock().unwrap().push(name);
            if self.fail == Some(name) {
                return Err(Error::ConflictError(format!("failed {name}")));
            }
            Ok(())
        }
    }
    #[async_trait::async_trait]
    impl ServiceConfirmationPort for RecordingPort {
        type Record = bool;
        type Purchase = ();
        async fn load(&self, e: &mut dyn Executor) -> Result<bool> {
            self.record("draft_version", e)?;
            Ok(false)
        }
        async fn purchase(&self, _: &bool, e: &mut dyn Executor) -> Result<()> {
            self.record("purchase_prepay", e)
        }
        async fn allocation(&self, _: &bool, _: &(), e: &mut dyn Executor) -> Result<()> {
            self.record("allocation", e)
        }
        async fn evidence(&self, e: &mut dyn Executor) -> Result<()> {
            self.record("evidence", e)
        }
        async fn pending(&self, e: &mut dyn Executor) -> Result<()> {
            self.record("pending", e)
        }
        async fn confirm(&self, record: &mut bool, e: &mut dyn Executor) -> Result<()> {
            self.record("confirm", e)?;
            *record = self.eligible;
            Ok(())
        }
        async fn task(&self, _: &bool, e: &mut dyn Executor) -> Result<()> {
            self.record("task", e)
        }
        fn is_acceptance_eligible(&self, record: &bool) -> bool {
            *record
        }
        async fn acceptance(&self, _: &(), e: &mut dyn Executor) -> Result<()> {
            self.record("acceptance", e)
        }
        async fn audit(&self, e: &mut dyn Executor) -> Result<()> {
            self.record("audit", e)
        }
    }
    const STEPS: &[&str] = &[
        "draft_version",
        "purchase_prepay",
        "allocation",
        "evidence",
        "pending",
        "confirm",
        "task",
        "acceptance",
        "audit",
    ];

    #[tokio::test]
    async fn confirmation_keeps_evidence_before_pending_and_same_executor() {
        let mut executor = TestExecutor { visits: 0 };
        let port = RecordingPort {
            identity: (&mut executor as *mut TestExecutor) as usize,
            eligible: true,
            fail: None,
            calls: Mutex::new(Vec::new()),
        };
        assert!(execute_confirmation(&port, &mut executor).await.unwrap());
        assert_eq!(*port.calls.lock().unwrap(), STEPS);
        assert_eq!(executor.visits, STEPS.len());
    }

    #[tokio::test]
    async fn confirmation_failure_stops_every_later_step_with_original_error() {
        for (index, step) in STEPS.iter().enumerate() {
            let mut executor = TestExecutor { visits: 0 };
            let port = RecordingPort {
                identity: (&mut executor as *mut TestExecutor) as usize,
                eligible: true,
                fail: Some(step),
                calls: Mutex::new(Vec::new()),
            };
            let error = execute_confirmation(&port, &mut executor).await.unwrap_err();
            assert!(matches!(error, Error::ConflictError(message) if message == format!("failed {step}")));
            assert_eq!(*port.calls.lock().unwrap(), STEPS[..=index]);
            assert_eq!(executor.visits, index + 1);
        }
    }

    #[tokio::test]
    async fn unsuccessful_service_does_not_create_acceptance_task() {
        let mut executor = TestExecutor { visits: 0 };
        let port = RecordingPort {
            identity: (&mut executor as *mut TestExecutor) as usize,
            eligible: false,
            fail: None,
            calls: Mutex::new(Vec::new()),
        };
        assert!(!execute_confirmation(&port, &mut executor).await.unwrap());
        assert_eq!(
            *port.calls.lock().unwrap(),
            [
                "draft_version",
                "purchase_prepay",
                "allocation",
                "evidence",
                "pending",
                "confirm",
                "task",
                "audit"
            ]
        );
    }
}
