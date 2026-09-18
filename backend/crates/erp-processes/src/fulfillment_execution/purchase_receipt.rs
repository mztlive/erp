use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_fulfillment::dto::{CreatePurchaseReceiptRequest, PurchaseReceiptView, UpdatePurchaseReceiptRequest};
use erp_fulfillment::entity::fulfillment::{PurchaseReceipt, PurchaseReceiptLine};
use erp_identity::SharedRbacService;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::service::approval::binding::{
    BindPublishedDefinitionCommand, BindingDecision, binding_decision,
};
use erp_workflow::service::approval::business_adapter::{BindingRevalidationContext, adapter_spec_of};
use erp_workflow::service::approval::policy::{DocumentApprovalPolicy, policy_of};
use erp_workflow::service::document_registry::{new_registered_document, persist_registered_document};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::FulfillmentProcess;
use crate::{Error, Result};
impl FulfillmentProcess {
    /// 创建采购入库单（草稿，跨集合：表头 + 行 + 审计）。
    ///
    /// 同一事务注册 `BusinessDocument` 并调用统一绑定端口。采购收货为
    /// `NO_APPROVAL`：返回空绑定，不查询已发布定义，不启动审批实例，
    /// 不创建审批任务。行的质量结果由服务端按合格/到货关系派生。
    ///
    /// # 参数
    /// * `req` - 创建请求（表头 + 行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建入库单的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 单号重复（唯一索引透出）
    /// * `RepositoryError` - 数据库写入失败
    #[tracing::instrument(
        name = "fulfillment.purchase_receipt_create",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "purchase_receipt_create")
    )]
    pub async fn create_purchase_receipt(
        &self,
        req: CreatePurchaseReceiptRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReceiptView> {
        let (receipt, lines) = erp_fulfillment::service::FulfillmentService::prepare_purchase_receipt(req)?;
        persist_created_purchase_receipt(
            &self.db,
            &self.rbac,
            std::sync::Arc::clone(&self.object_read),
            receipt.clone(),
            lines,
            actor.clone(),
        )
        .await?;
        Ok(receipt.into())
    }
    /// 更新采购入库单（仅草稿；乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 入库单主键
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后入库单的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 入库单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    /// * `ValidationError` - 请求体校验失败
    #[tracing::instrument(
        name = "fulfillment.purchase_receipt_update",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "purchase_receipt_update")
    )]
    pub async fn update_purchase_receipt(
        &self,
        id: &str,
        req: UpdatePurchaseReceiptRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReceiptView> {
        let mut receipt = self.domain().prepare_purchase_receipt_update(id, req).await?;
        let audit =
            actor.clone().resource_log("purchase_receipt.update", "purchase_receipt", id.to_string())?;
        let db = self.db.clone();
        let actor_id = actor.id().to_string();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    erp_fulfillment::service::FulfillmentService::new(db.clone())
                        .persist_purchase_receipt(&mut receipt, executor)
                        .await?;
                    super::task::record_fulfillment_activity(
                        &db,
                        super::task::FulfillmentTaskObject::PurchaseReceipt(&receipt),
                        &actor_id,
                        executor,
                    )
                    .await?;
                    db.audit_logs().create(&audit, executor).await?;
                    Ok::<PurchaseReceipt, crate::Error>(receipt)
                })
            })
            .await?;
        Ok(updated.into())
    }
}
/// 采购收货创建必须跳过绑定：政策只能是 `NO_APPROVAL`。
///
/// # 返回
/// 返回 `SkipNoApproval`。
///
/// # 错误
/// 政策缺失或误登记为必须审批时返回部署不变量错误。
fn purchase_receipt_create_binding_decision() -> Result<BindingDecision> {
    let policy = policy_of(DocumentType::PurchaseReceipt)?;
    match &policy {
        DocumentApprovalPolicy::NoApproval(no_approval) => {
            if no_approval.document_type != DocumentType::PurchaseReceipt {
                return Err(Error::Internal("采购收货政策类型不匹配".to_string()));
            }
            Ok(binding_decision(policy.requirement()))
        },
        DocumentApprovalPolicy::ProcessRequired(_) => {
            Err(Error::Internal("采购收货必须是 NO_APPROVAL，不得绑定流程".to_string()))
        },
    }
}

/// 确认采购收货创建路径不得查询发布定义。
///
/// # 错误
/// 绑定决定不是跳过时返回错误。
fn ensure_purchase_receipt_skips_approval_binding() -> Result<BindingDecision> {
    let decision = purchase_receipt_create_binding_decision()?;
    if decision != BindingDecision::SkipNoApproval {
        return Err(Error::Internal("采购收货创建必须跳过审批绑定".to_string()));
    }
    Ok(decision)
}

/// 采购收货不得注册空审批适配器。
///
/// # 错误
/// 适配器登记存在时返回部署不变量错误。
fn ensure_purchase_receipt_has_no_adapter() -> Result<()> {
    if adapter_spec_of(DocumentType::PurchaseReceipt).is_ok() {
        return Err(Error::Internal("采购收货不得注册审批适配器".to_string()));
    }
    Ok(())
}

/// 入库仓作为绑定上下文组织，不得用空串补位。
///
/// # 参数
/// * `receipt` - 待登记采购收货单
///
/// # 返回
/// 返回非空入库仓标识。
///
/// # 错误
/// 入库仓为空时返回校验错误。
fn purchase_receipt_binding_organization_id(receipt: &PurchaseReceipt) -> Result<String> {
    let org = receipt.warehouse_id.to_string();
    if org.trim().is_empty() {
        return Err(Error::ValidationError("采购收货缺少入库仓，无法构造绑定上下文".to_string()));
    }
    Ok(org)
}

/// 构造采购收货创建绑定命令。客户端不得提交定义 ID。
///
/// # 参数
/// * `receipt` - 待登记采购收货单
/// * `creator_id` - 创建人
///
/// # 错误
/// 入库仓为空时返回校验错误。
fn purchase_receipt_bind_command(
    receipt: &PurchaseReceipt,
    creator_id: &str,
) -> Result<BindPublishedDefinitionCommand> {
    Ok(BindPublishedDefinitionCommand {
        document_type: DocumentType::PurchaseReceipt,
        business_object_id: receipt.base.id.clone(),
        business_object_version: receipt.base.version,
        context: BindingRevalidationContext::new(
            purchase_receipt_binding_organization_id(receipt)?,
            creator_id.to_string(),
        ),
    })
}

/// 将绑定端口返回值落实为采购收货注册行：空绑定保持未绑定。
///
/// # 参数
/// * `document` - 采购收货注册行
/// * `binding` - 统一绑定端口返回值
///
/// # 返回
/// 固定返回 `None`。
///
/// # 错误
/// 端口返回绑定或注册行已预置绑定时返回错误。
fn apply_purchase_receipt_create_binding(
    document: &mut BusinessDocument,
    binding: Option<ApprovalDefinitionBinding>,
) -> Result<Option<ApprovalDefinitionBinding>> {
    if binding.is_some() {
        return Err(Error::Internal("采购收货为 NO_APPROVAL，不得写入审批绑定".to_string()));
    }
    if document.approval_binding.is_some() {
        return Err(Error::Internal("采购收货注册行不得预置审批绑定".to_string()));
    }
    if document.document_type != DocumentType::PurchaseReceipt {
        return Err(Error::Internal("采购收货创建只能注册 PurchaseReceipt 单据".to_string()));
    }
    Ok(None)
}

/// 在调用方事务内登记采购收货单据并证明空绑定。
///
/// 必须先确认政策跳过，再调用统一绑定端口；不得查询发布定义后假装成功。
///
/// # 错误
/// 政策非无审批、端口返回绑定或写入失败时返回错误。
async fn persist_unbound_purchase_receipt_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = ensure_purchase_receipt_skips_approval_binding()?;
    ensure_purchase_receipt_has_no_adapter()?;
    let binding = crate::adapters::workflow::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        bind_command,
        actor,
        executor,
    )
    .await?;
    apply_purchase_receipt_create_binding(&mut document, binding)?;
    persist_registered_document(db, &document, executor).await.map_err(crate::Error::from)
}

/// 为已构造采购收货登记 `BusinessDocument` 并调用统一绑定端口。
///
/// # 错误
/// 绑定端口或注册写入失败时返回错误。
async fn register_created_purchase_receipt_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    receipt: &PurchaseReceipt,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let bind_command = purchase_receipt_bind_command(receipt, actor.id())?;
    let document =
        new_registered_document(&receipt.base.id, DocumentType::PurchaseReceipt, receipt.receipt_no.clone())
            .map_err(crate::Error::from)?;
    persist_unbound_purchase_receipt_document(db, rbac, object_read, document, &bind_command, actor, executor)
        .await
}

/// 在创建事务内写入采购收货草稿并登记无绑定单据。
///
/// # 错误
/// 绑定、注册或入库单写入失败时返回错误，调用方必须视作整体回滚。
async fn persist_created_purchase_receipt(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    receipt: PurchaseReceipt,
    lines: Vec<PurchaseReceiptLine>,
    actor: AuditActor,
) -> Result<()> {
    let audit =
        actor.clone().resource_log("purchase_receipt.create", "purchase_receipt", receipt.base.id.clone())?;
    let db = db.clone();
    let rbac = rbac.clone();
    let object_read = object_read.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |executor| {
            Box::pin(async move {
                register_created_purchase_receipt_document(
                    &db,
                    &rbac,
                    object_read.as_ref(),
                    &receipt,
                    &actor,
                    executor,
                )
                .await?;
                erp_fulfillment::service::FulfillmentService::new(db.clone())
                    .persist_created_purchase_receipt(&receipt, &lines, executor)
                    .await?;
                super::task::ensure_fulfillment_task(
                    &db,
                    super::task::FulfillmentTaskObject::PurchaseReceipt(&receipt),
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
mod purchase_receipt_no_approval_tests {
    use bpm::ProcessKind;
    use bpm::ids::ApprovalProcessDefinitionId;
    use erp_core::common::time::Instant;
    use erp_core::ids::{PurchaseOrderId, PurchaseReceiptId, WarehouseId};
    use erp_fulfillment::entity::fulfillment::PurchaseReceiptData;
    use erp_workflow::service::approval::binding::binding_from_published;
    use erp_workflow::service::document_registry::new_registered_document;

    use super::{
        BindingDecision, DocumentApprovalPolicy, DocumentType, PurchaseReceipt,
        apply_purchase_receipt_create_binding, ensure_purchase_receipt_has_no_adapter,
        ensure_purchase_receipt_skips_approval_binding, policy_of, purchase_receipt_bind_command,
        purchase_receipt_create_binding_decision,
    };

    fn draft_receipt() -> PurchaseReceipt {
        PurchaseReceipt::new(
            PurchaseReceiptId::new("pr-1"),
            PurchaseReceiptData {
                receipt_no: "PR-1".into(),
                purchase_order_id: PurchaseOrderId::new("po-1"),
                warehouse_id: WarehouseId::new("wh-1"),
            },
        )
        .expect("草稿必须可构造")
    }

    /// 政策仅含 document_type、approval_requirement、process_kind，不得注册空 Adapter。
    #[test]
    fn purchase_receipt_policy_is_no_approval_identity_only() {
        let policy = policy_of(DocumentType::PurchaseReceipt).expect("采购收货政策必须存在");
        let DocumentApprovalPolicy::NoApproval(no_approval) = &policy else {
            panic!("采购收货必须是 NO_APPROVAL");
        };
        assert_eq!(no_approval.document_type, DocumentType::PurchaseReceipt);
        assert_eq!(no_approval.process_kind, ProcessKind::PurchaseReceipt);
        assert_eq!(
            purchase_receipt_create_binding_decision().expect("绑定决定"),
            BindingDecision::SkipNoApproval
        );
        assert_eq!(
            ensure_purchase_receipt_skips_approval_binding().expect("必须跳过"),
            BindingDecision::SkipNoApproval
        );
        ensure_purchase_receipt_has_no_adapter().expect("不得注册空适配器");
    }

    /// 创建必须注册 BusinessDocument，绑定端口返回空，禁止写入绑定。
    #[test]
    fn create_registers_document_and_returns_empty_binding() {
        let receipt = draft_receipt();
        let command = purchase_receipt_bind_command(&receipt, "admin-1").expect("绑定命令");
        assert_eq!(command.document_type, DocumentType::PurchaseReceipt);
        assert_eq!(command.business_object_id, receipt.base.id);
        assert_eq!(command.context.organization_id, "wh-1");

        let mut document = new_registered_document(
            &receipt.base.id,
            DocumentType::PurchaseReceipt,
            receipt.receipt_no.clone(),
        )
        .expect("可注册");
        assert!(document.approval_binding.is_none());
        let empty = apply_purchase_receipt_create_binding(&mut document, None).expect("空绑定");
        assert!(empty.is_none());
        assert!(document.approval_binding.is_none());

        let forged =
            binding_from_published(ApprovalProcessDefinitionId::new("def-1"), 1, Instant::from_unix_secs(10))
                .expect("测试绑定");
        assert!(apply_purchase_receipt_create_binding(&mut document, Some(forged)).is_err());
    }
}
