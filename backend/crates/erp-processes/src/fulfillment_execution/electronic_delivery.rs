use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_fulfillment::dto::{CreateElectronicDeliveryRequest, ElectronicDeliveryView};
use erp_fulfillment::entity::fulfillment::ElectronicDelivery;
use erp_fulfillment::service::FulfillmentService;
use erp_fulfillment::service::electronic_delivery_crypto::electronic_delivery_draft_from_request;
use erp_identity::SharedRbacService;
use erp_workflow::DocumentRegistryExt;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::binding::{
    BindPublishedDefinitionCommand, BindingDecision, binding_decision,
};
use erp_workflow::service::approval::business_adapter::{BindingRevalidationContext, adapter_spec_of};
use erp_workflow::service::approval::policy::{DocumentApprovalPolicy, policy_of};
use erp_workflow::service::document_registry::new_registered_document;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::FulfillmentProcess;
use crate::{Error, Result};

impl FulfillmentProcess {
    /// 创建电子交付记录（草稿）。
    ///
    /// 同一事务注册 `BusinessDocument` 并调用统一绑定端口。电子交付为
    /// `NO_APPROVAL`：返回空绑定，不查询已发布定义，不启动审批实例，
    /// 不创建审批任务。交付对象快照以不透明值传入，服务端用指纹密钥
    /// 计算查询指纹后落库；快照的字段级加密由边界完成。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建记录的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 履约记录号重复（唯一索引透出）
    /// * `RepositoryError` - 数据库写入失败
    #[tracing::instrument(
        name = "fulfillment.electronic_delivery_create",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "electronic_delivery_create")
    )]
    pub async fn create_electronic_delivery(
        &self,
        req: CreateElectronicDeliveryRequest,
        actor: &AuditActor,
    ) -> Result<ElectronicDeliveryView> {
        req.validate()?;
        let record = electronic_delivery_draft_from_request(req, actor.id(), &self.fingerprint_key)?;
        persist_created_electronic_delivery(
            &self.db,
            &self.rbac,
            std::sync::Arc::clone(&self.object_read),
            record.clone(),
            actor.clone(),
        )
        .await?;
        Ok(record.into())
    }
}

/// 电子交付创建必须跳过绑定：政策只能是 `NO_APPROVAL`。
///
/// # 返回
/// 返回 `SkipNoApproval`。
///
/// # 错误
/// 政策缺失或误登记为必须审批时返回部署不变量错误。
fn electronic_delivery_create_binding_decision() -> Result<BindingDecision> {
    let policy = policy_of(DocumentType::ElectronicDelivery)?;
    match &policy {
        DocumentApprovalPolicy::NoApproval(no_approval) => {
            if no_approval.document_type != DocumentType::ElectronicDelivery {
                return Err(Error::Internal("电子交付政策类型不匹配".to_string()));
            }
            Ok(binding_decision(policy.requirement()))
        },
        DocumentApprovalPolicy::ProcessRequired(_) => {
            Err(Error::Internal("电子交付必须是 NO_APPROVAL，不得绑定流程".to_string()))
        },
    }
}

/// 确认电子交付创建路径不得查询发布定义。
///
/// # 错误
/// 绑定决定不是跳过时返回错误。
fn ensure_electronic_delivery_skips_approval_binding() -> Result<BindingDecision> {
    let decision = electronic_delivery_create_binding_decision()?;
    if decision != BindingDecision::SkipNoApproval {
        return Err(Error::Internal("电子交付创建必须跳过审批绑定".to_string()));
    }
    Ok(decision)
}

/// 电子交付不得注册空审批适配器。
///
/// # 错误
/// 适配器登记存在时返回部署不变量错误。
fn ensure_electronic_delivery_has_no_adapter() -> Result<()> {
    if adapter_spec_of(DocumentType::ElectronicDelivery).is_ok() {
        return Err(Error::Internal("电子交付不得注册审批适配器".to_string()));
    }
    Ok(())
}

/// 构造电子交付创建绑定命令。客户端不得提交定义 ID。
///
/// # 参数
/// * `record` - 待登记电子交付
/// * `creator_id` - 创建人
///
/// # 错误
/// 销售明细为空时返回校验错误。
fn electronic_delivery_bind_command(
    record: &ElectronicDelivery,
    creator_id: &str,
) -> Result<BindPublishedDefinitionCommand> {
    Ok(BindPublishedDefinitionCommand {
        document_type: DocumentType::ElectronicDelivery,
        business_object_id: record.base.id.clone(),
        business_object_version: record.base.version,
        context: BindingRevalidationContext::new(
            record
                .registration_context_id()
                .map_err(|error| Error::ValidationError(error.to_string()))?
                .to_string(),
            creator_id.to_string(),
        ),
    })
}

/// 在调用方事务内登记电子交付单据并证明空绑定。
///
/// 必须先确认政策跳过，再调用统一绑定端口；不得查询发布定义后假装成功。
///
/// # 错误
/// 政策非无审批、端口返回绑定或写入失败时返回错误。
async fn persist_unbound_electronic_delivery_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = ensure_electronic_delivery_skips_approval_binding()?;
    ensure_electronic_delivery_has_no_adapter()?;
    let binding = crate::adapters::workflow::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        bind_command,
        actor,
        executor,
    )
    .await?;
    document
        .ensure_no_approval_registration(DocumentType::ElectronicDelivery, binding.as_ref())
        .map_err(|error| Error::Internal(error.to_string()))?;
    db.business_documents().register_no_approval_document(&document, executor).await?;
    Ok(())
}

/// 为已构造电子交付登记 `BusinessDocument` 并调用统一绑定端口。
///
/// # 错误
/// 绑定端口或注册写入失败时返回错误。
async fn register_created_electronic_delivery_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    record: &ElectronicDelivery,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let bind_command = electronic_delivery_bind_command(record, actor.id())?;
    let document = new_registered_document(
        &record.base.id,
        DocumentType::ElectronicDelivery,
        record.fulfillment_no.clone(),
    )
    .map_err(crate::Error::from)?;
    persist_unbound_electronic_delivery_document(
        db,
        rbac,
        object_read,
        document,
        &bind_command,
        actor,
        executor,
    )
    .await
}

/// 在创建事务内写入电子交付草稿并登记无绑定单据。
///
/// # 错误
/// 绑定、注册或电子交付写入失败时返回错误，调用方必须视作整体回滚。
async fn persist_created_electronic_delivery(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    record: ElectronicDelivery,
    actor: AuditActor,
) -> Result<()> {
    let audit = actor.clone().resource_log(
        "electronic_delivery.create",
        "electronic_delivery",
        record.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let object_read = object_read.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |executor| {
            Box::pin(async move {
                register_created_electronic_delivery_document(
                    &db,
                    &rbac,
                    object_read.as_ref(),
                    &record,
                    &actor,
                    executor,
                )
                .await?;
                FulfillmentService::new(db.clone())
                    .persist_created_electronic_delivery(&record, executor)
                    .await?;
                super::task::ensure_fulfillment_task(
                    &db,
                    super::task::FulfillmentTaskObject::ElectronicDelivery(&record),
                    executor,
                )
                .await?;
                db.audit_logs().create(&audit, executor).await?;
                Ok::<(), crate::Error>(())
            })
        })
        .await
}

#[cfg(test)]
mod electronic_delivery_no_approval_tests {
    use std::str::FromStr;

    use bpm::ProcessKind;
    use bpm::ids::ApprovalProcessDefinitionId;
    use erp_core::common::source::SourceType;
    use erp_core::common::time::Instant;
    use erp_core::ids::{
        ElectronicDeliveryId, PurchaseLineSalesAllocationId, PurchaseOrderId, SalesOrderLineId,
    };
    use erp_core::money::Quantity;
    use erp_fulfillment::entity::fulfillment::{ElectronicDeliveryData, FulfillmentResult};
    use erp_workflow::service::approval::binding::binding_from_published;
    use erp_workflow::service::document_registry::new_registered_document;

    use super::{
        BindingDecision, DocumentApprovalPolicy, DocumentType, ElectronicDelivery,
        electronic_delivery_bind_command, electronic_delivery_create_binding_decision,
        ensure_electronic_delivery_has_no_adapter, ensure_electronic_delivery_skips_approval_binding,
        policy_of,
    };

    fn draft_electronic_delivery() -> ElectronicDelivery {
        ElectronicDelivery::new(
            ElectronicDeliveryId::new("ed-1"),
            ElectronicDeliveryData {
                fulfillment_no: "ED-1".into(),
                sales_order_line_id: SalesOrderLineId::new("so-line-1"),
                purchase_order_id: PurchaseOrderId::new("po-1"),
                purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId::new("pla-1"),
                recipient_snapshot: "ciphertext-recipient".into(),
                recipient_snapshot_fingerprint: ElectronicDelivery::recipient_snapshot_fingerprint(
                    "recipient",
                    b"test-fingerprint-key",
                ),
                quantity: Quantity::from_str("2").expect("数量合法"),
                result: FulfillmentResult::Success,
                evidence_attachment_id: None,
                fact_no: "F-001".into(),
                occurred_at: Instant::from_unix_secs(1_700_000_000),
                recorded_at: Instant::from_unix_secs(1_700_000_100),
                recorded_by: "operator-1".into(),
                source_type: SourceType::Erp,
                source_reference: None,
                reason_code: None,
                reason_text: None,
            },
        )
        .expect("草稿必须可构造")
    }

    /// 政策仅含 document_type、approval_requirement、process_kind，不得注册空 Adapter。
    #[test]
    fn electronic_delivery_policy_is_no_approval_identity_only() {
        let policy = policy_of(DocumentType::ElectronicDelivery).expect("电子交付政策必须存在");
        let DocumentApprovalPolicy::NoApproval(no_approval) = &policy else {
            panic!("电子交付必须是 NO_APPROVAL");
        };
        assert_eq!(no_approval.document_type, DocumentType::ElectronicDelivery);
        assert_eq!(no_approval.process_kind, ProcessKind::ElectronicDelivery);
        assert_eq!(
            electronic_delivery_create_binding_decision().expect("绑定决定"),
            BindingDecision::SkipNoApproval
        );
        assert_eq!(
            ensure_electronic_delivery_skips_approval_binding().expect("必须跳过"),
            BindingDecision::SkipNoApproval
        );
        ensure_electronic_delivery_has_no_adapter().expect("不得注册空适配器");
    }

    /// 创建必须注册 BusinessDocument，绑定端口返回空，禁止写入绑定。
    #[test]
    fn create_registers_document_and_returns_empty_binding() {
        let record = draft_electronic_delivery();
        let command = electronic_delivery_bind_command(&record, "admin-1").expect("绑定命令");
        assert_eq!(command.document_type, DocumentType::ElectronicDelivery);
        assert_eq!(command.business_object_id, record.base.id);
        assert_eq!(command.context.organization_id, "so-line-1");

        let document = new_registered_document(
            &record.base.id,
            DocumentType::ElectronicDelivery,
            record.fulfillment_no.clone(),
        )
        .expect("可注册");
        assert!(document.approval_binding.is_none());
        document.ensure_no_approval_registration(DocumentType::ElectronicDelivery, None).expect("空绑定");
        assert!(document.approval_binding.is_none());

        let forged =
            binding_from_published(ApprovalProcessDefinitionId::new("def-1"), 1, Instant::from_unix_secs(10))
                .expect("测试绑定");
        assert!(
            document
                .ensure_no_approval_registration(DocumentType::ElectronicDelivery, Some(&forged))
                .is_err()
        );
    }
}
