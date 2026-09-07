//! 发货创建、更新与无审批注册的跨域事务编排。

use super::FulfillmentProcess;
use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_fulfillment::dto::{CreateDeliveryRequest, DeliveryView, UpdateDeliveryRequest};
use erp_fulfillment::entity::fulfillment::{Delivery, DeliveryLine};
use erp_identity::SharedRbacService;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::service::approval::binding::{
    binding_decision, BindPublishedDefinitionCommand, BindingDecision,
};
use erp_workflow::service::approval::business_adapter::{adapter_spec_of, BindingRevalidationContext};
use erp_workflow::service::approval::policy::{policy_of, DocumentApprovalPolicy};
use erp_workflow::service::document_registry::new_registered_document;
use erp_workflow::DocumentRegistryExt;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use services::{Error, Result};

impl FulfillmentProcess {
    /// 创建发货单（草稿，跨集合：表头 + 行 + 审计）。
    ///
    /// 同一事务注册 `BusinessDocument` 并调用统一绑定端口。发货为
    /// `NO_APPROVAL`：返回空绑定，不查询已发布定义，不启动审批实例，
    /// 不创建审批任务。仓发/直发的表头与行归属由实体按发货类型校验。
    ///
    /// # 参数
    /// * `req` - 创建请求（表头 + 行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建发货单的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 单号重复（唯一索引透出）
    /// * `RepositoryError` - 数据库写入失败
    #[tracing::instrument(
        name = "fulfillment.delivery_create",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "delivery_create")
    )]
    pub async fn create_delivery(
        &self,
        req: CreateDeliveryRequest,
        actor: &AuditActor,
    ) -> Result<DeliveryView> {
        let (delivery, lines) = self.domain().prepare_delivery(req)?;
        persist_created_delivery(
            &self.db,
            &self.rbac,
            std::sync::Arc::clone(&self.object_read),
            delivery.clone(),
            lines,
            actor.clone(),
        )
        .await?;
        Ok(delivery.into())
    }

    /// 更新发货单（仅草稿；乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 发货单主键
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后发货单的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 发货单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    #[tracing::instrument(
        name = "fulfillment.delivery_update",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "delivery_update")
    )]
    pub async fn update_delivery(
        &self,
        id: &str,
        req: UpdateDeliveryRequest,
        actor: &AuditActor,
    ) -> Result<DeliveryView> {
        let mut delivery = self.domain().prepare_delivery_update(id, req).await?;
        let audit = actor
            .clone()
            .resource_log("delivery.update", "delivery", id.to_string())?;
        let db = self.db.clone();
        let actor_id = actor.id().to_string();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    erp_fulfillment::service::FulfillmentService::new(db.clone())
                        .persist_delivery(&mut delivery, session)
                        .await?;
                    super::task::record_fulfillment_activity(
                        &db,
                        super::task::FulfillmentTaskObject::Delivery(&delivery),
                        &actor_id,
                        session,
                    )
                    .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Delivery, services::Error>(delivery)
                })
            })
            .await?;
        Ok(updated.into())
    }
}

/// 发货创建必须跳过绑定：政策只能是 `NO_APPROVAL`。
///
/// # 返回
/// 返回 `SkipNoApproval`。
///
/// # 错误
/// 政策缺失或误登记为必须审批时返回部署不变量错误。
fn delivery_create_binding_decision() -> Result<BindingDecision> {
    let policy = policy_of(DocumentType::Delivery)?;
    match &policy {
        DocumentApprovalPolicy::NoApproval(no_approval) => {
            if no_approval.document_type != DocumentType::Delivery {
                return Err(Error::Internal("发货政策类型不匹配".to_string()));
            }
            Ok(binding_decision(policy.requirement()))
        }
        DocumentApprovalPolicy::ProcessRequired(_) => Err(Error::Internal(
            "发货必须是 NO_APPROVAL，不得绑定流程".to_string(),
        )),
    }
}

/// 确认发货创建路径不得查询发布定义。
///
/// # 错误
/// 绑定决定不是跳过时返回错误。
fn ensure_delivery_skips_approval_binding() -> Result<BindingDecision> {
    let decision = delivery_create_binding_decision()?;
    if decision != BindingDecision::SkipNoApproval {
        return Err(Error::Internal("发货创建必须跳过审批绑定".to_string()));
    }
    Ok(decision)
}

/// 发货不得注册空审批适配器。
///
/// # 错误
/// 适配器登记存在时返回部署不变量错误。
fn ensure_delivery_has_no_adapter() -> Result<()> {
    if adapter_spec_of(DocumentType::Delivery).is_ok() {
        return Err(Error::Internal("发货不得注册审批适配器".to_string()));
    }
    Ok(())
}

/// 构造发货创建绑定命令。客户端不得提交定义 ID。
///
/// # 参数
/// * `delivery` - 待登记发货单
/// * `creator_id` - 创建人
///
/// # 错误
/// 销售单为空时返回校验错误。
fn delivery_bind_command(delivery: &Delivery, creator_id: &str) -> Result<BindPublishedDefinitionCommand> {
    Ok(BindPublishedDefinitionCommand {
        document_type: DocumentType::Delivery,
        business_object_id: delivery.base.id.clone(),
        business_object_version: delivery.base.version,
        context: BindingRevalidationContext {
            organization_id: delivery
                .registration_context_id()
                .map_err(|error| Error::ValidationError(error.to_string()))?
                .to_string(),
            creator_id: creator_id.to_string(),
        },
    })
}

/// 在调用方事务内登记发货单据并证明空绑定。
///
/// 必须先确认政策跳过，再调用统一绑定端口；不得查询发布定义后假装成功。
///
/// # 错误
/// 政策非无审批、端口返回绑定或写入失败时返回错误。
async fn persist_unbound_delivery_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = ensure_delivery_skips_approval_binding()?;
    ensure_delivery_has_no_adapter()?;
    let binding = services::workflow_compose::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        bind_command,
        actor,
        executor,
    )
    .await?;
    document
        .ensure_no_approval_registration(DocumentType::Delivery, binding.as_ref())
        .map_err(|error| Error::Internal(error.to_string()))?;
    db.business_documents()
        .register_no_approval_document(&document, executor)
        .await?;
    Ok(())
}

/// 为已构造发货登记 `BusinessDocument` 并调用统一绑定端口。
///
/// # 错误
/// 绑定端口或注册写入失败时返回错误。
async fn register_created_delivery_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    delivery: &Delivery,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let bind_command = delivery_bind_command(delivery, actor.id())?;
    let document = new_registered_document(
        &delivery.base.id,
        DocumentType::Delivery,
        delivery.delivery_no.clone(),
    )
    .map_err(services::Error::from)?;
    persist_unbound_delivery_document(db, rbac, object_read, document, &bind_command, actor, executor).await
}

/// 在创建事务内写入发货草稿并登记无绑定单据。
///
/// # 错误
/// 绑定、注册或发货单写入失败时返回错误，调用方必须视作整体回滚。
async fn persist_created_delivery(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    delivery: Delivery,
    lines: Vec<DeliveryLine>,
    actor: AuditActor,
) -> Result<()> {
    let audit = actor
        .clone()
        .resource_log("delivery.create", "delivery", delivery.base.id.clone())?;
    let db = db.clone();
    let rbac = rbac.clone();
    let object_read = object_read.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                register_created_delivery_document(
                    &db,
                    &rbac,
                    object_read.as_ref(),
                    &delivery,
                    &actor,
                    session,
                )
                .await?;
                erp_fulfillment::service::FulfillmentService::new(db.clone())
                    .persist_created_delivery(&delivery, &lines, session)
                    .await?;
                super::task::ensure_fulfillment_task(
                    &db,
                    super::task::FulfillmentTaskObject::Delivery(&delivery),
                    session,
                )
                .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), services::Error>(())
            })
        })
        .await
}

#[cfg(test)]
mod delivery_no_approval_tests {
    use super::{
        delivery_bind_command, delivery_create_binding_decision, ensure_delivery_has_no_adapter,
        ensure_delivery_skips_approval_binding, policy_of, BindingDecision, Delivery, DocumentApprovalPolicy,
        DocumentType,
    };
    use bpm::ids::ApprovalProcessDefinitionId;
    use bpm::ProcessKind;
    use erp_core::common::time::Instant;
    use erp_core::ids::{DeliveryId, SalesOrderId, WarehouseId};
    use erp_fulfillment::entity::fulfillment::{DeliveryData, DeliveryType};
    use erp_workflow::service::approval::binding::binding_from_published;
    use erp_workflow::service::document_registry::new_registered_document;

    fn draft_delivery() -> Delivery {
        Delivery::new(
            DeliveryId::new("dv-1"),
            DeliveryData {
                delivery_no: "DV-1".into(),
                delivery_type: DeliveryType::WarehouseShip,
                sales_order_id: SalesOrderId::new("so-1"),
                purchase_order_id: None,
                warehouse_id: Some(WarehouseId::new("wh-1")),
                carrier: None,
                tracking_no: None,
                address_snapshot_encrypted: None,
                address_snapshot_fingerprint: None,
            },
        )
        .expect("草稿必须可构造")
    }

    /// 政策仅含 document_type、approval_requirement、process_kind，不得注册空 Adapter。
    #[test]
    fn delivery_policy_is_no_approval_identity_only() {
        let policy = policy_of(DocumentType::Delivery).expect("发货政策必须存在");
        let DocumentApprovalPolicy::NoApproval(no_approval) = &policy else {
            panic!("发货必须是 NO_APPROVAL");
        };
        assert_eq!(no_approval.document_type, DocumentType::Delivery);
        assert_eq!(no_approval.process_kind, ProcessKind::Delivery);
        assert_eq!(
            delivery_create_binding_decision().expect("绑定决定"),
            BindingDecision::SkipNoApproval
        );
        assert_eq!(
            ensure_delivery_skips_approval_binding().expect("必须跳过"),
            BindingDecision::SkipNoApproval
        );
        ensure_delivery_has_no_adapter().expect("不得注册空适配器");
    }

    /// 创建必须注册 BusinessDocument，绑定端口返回空，禁止写入绑定。
    #[test]
    fn create_registers_document_and_returns_empty_binding() {
        let delivery = draft_delivery();
        let command = delivery_bind_command(&delivery, "admin-1").expect("绑定命令");
        assert_eq!(command.document_type, DocumentType::Delivery);
        assert_eq!(command.business_object_id, delivery.base.id);
        assert_eq!(command.context.organization_id, "so-1");

        let document = new_registered_document(
            &delivery.base.id,
            DocumentType::Delivery,
            delivery.delivery_no.clone(),
        )
        .expect("可注册");
        assert!(document.approval_binding.is_none());
        document
            .ensure_no_approval_registration(DocumentType::Delivery, None)
            .expect("空绑定");
        assert!(document.approval_binding.is_none());

        let forged = binding_from_published(
            ApprovalProcessDefinitionId::new("def-1"),
            1,
            Instant::from_unix_secs(10),
        )
        .expect("测试绑定");
        assert!(document
            .ensure_no_approval_registration(DocumentType::Delivery, Some(&forged))
            .is_err());
    }

    /// 创建路径调用统一绑定端口，不查询发布定义、不启动实例、不建任务。
    #[test]
    fn create_does_not_query_definition_or_start_instance() {
        let production = include_str!("delivery.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("persist_created_delivery"));
        assert!(production.contains("register_created_delivery_document"));
        assert!(production.contains("persist_unbound_delivery_document"));
        assert!(production.contains("bind_published_definition_on_document_create"));
        assert!(production.contains("DocumentType::Delivery"));
        assert!(production.contains("new_registered_document"));
        assert!(production.contains("ensure_delivery_skips_approval_binding"));
        assert!(production.contains("ensure_delivery_has_no_adapter"));
        assert!(!production.contains("pub async fn submit_delivery"));
        assert!(!production.contains("start_delivery_approval"));
        assert!(!production.contains("DeliveryAdapter"));
        assert!(!production.contains("load_published_graph"));
        let create = production
            .split("pub async fn create_delivery")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn update_delivery").next())
            .expect("create_delivery 生产片段");
        assert!(create.contains("persist_created_delivery"));
        assert!(!create.contains("prepare_start"));
        assert!(!create.contains("attach_published_binding"));
        assert!(!create.contains("WorkItem"));
        assert!(!create.contains("start_approval"));
    }
}
