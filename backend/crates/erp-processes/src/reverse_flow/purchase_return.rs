//! PurchaseReturnOrder 无审批登记、业务创建与审计的原子流程。
use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_identity::SharedRbacService;
use erp_read_models::returns_center::ReturnsReadService;
use erp_read_models::returns_center::dto::PurchaseReturnOrderView;
use erp_returns::dto::CreatePurchaseReturnOrderRequest;
use erp_returns::entity::returns::{PurchaseReturnLine, PurchaseReturnOrder};
use erp_returns::service::purchase_return::{
    build_purchase_return_order_and_line, persist_purchase_return_order_with_line,
};
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
use validator::Validate;

use super::ReturnsProcess;
use crate::{Error, Result};

impl ReturnsProcess {
    /// 建立采购退货单与明细行（跨集合事务写入）。
    ///
    /// 同一事务注册 `BusinessDocument` 并调用统一绑定端口。采购退货为
    /// `NO_APPROVAL`：返回空绑定，不查询已发布定义，不启动审批实例，
    /// 不创建审批任务。`purchase_return_no` 全局唯一（唯一索引）构成幂等去重。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建退货单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 采购退货单号重复
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_purchase_return_order(
        &self,
        req: CreatePurchaseReturnOrderRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReturnOrderView> {
        req.validate()?;
        let (order_id, order, line) = build_purchase_return_order_and_line(req, actor.id())?;
        persist_created_purchase_return_order(
            &self.db,
            &self.rbac,
            std::sync::Arc::clone(&self.object_read),
            order,
            line,
            actor.clone(),
        )
        .await?;
        ReturnsReadService::new(self.db.clone())
            .with_purchase_scope(crate::adapters::MongoPurchaseDataScope::shared(
                self.db.clone(),
                self.rbac.clone(),
            ))
            .purchase_return_order_detail(&order_id, actor)
            .await
            .map_err(crate::Error::from)
    }
}

/// 采购退货创建必须跳过绑定：政策只能是 `NO_APPROVAL`。
///
/// # 返回
/// 返回 `SkipNoApproval`。
///
/// # 错误
/// 政策缺失或误登记为必须审批时返回部署不变量错误。
fn purchase_return_create_binding_decision() -> Result<BindingDecision> {
    let policy = policy_of(DocumentType::PurchaseReturnOrder)?;
    match &policy {
        DocumentApprovalPolicy::NoApproval(no_approval) => {
            if no_approval.document_type != DocumentType::PurchaseReturnOrder {
                return Err(Error::Internal("采购退货政策类型不匹配".to_string()));
            }
            Ok(binding_decision(policy.requirement()))
        },
        DocumentApprovalPolicy::ProcessRequired(_) => {
            Err(Error::Internal("采购退货必须是 NO_APPROVAL，不得绑定流程".to_string()))
        },
    }
}

/// 确认采购退货创建路径不得查询发布定义。
///
/// # 错误
/// 绑定决定不是跳过时返回错误。
fn ensure_purchase_return_skips_approval_binding() -> Result<BindingDecision> {
    let decision = purchase_return_create_binding_decision()?;
    if decision != BindingDecision::SkipNoApproval {
        return Err(Error::Internal("采购退货创建必须跳过审批绑定".to_string()));
    }
    Ok(decision)
}

/// 采购退货不得注册空审批适配器。
///
/// # 错误
/// 适配器登记存在时返回部署不变量错误。
fn ensure_purchase_return_has_no_adapter() -> Result<()> {
    if adapter_spec_of(DocumentType::PurchaseReturnOrder).is_ok() {
        return Err(Error::Internal("采购退货不得注册审批适配器".to_string()));
    }
    Ok(())
}

/// 原采购单作为绑定上下文组织，不得用空串补位。
///
/// # 参数
/// * `order` - 待登记采购退货单
///
/// # 返回
/// 返回非空采购单标识。
///
/// # 错误
/// 采购单为空时返回校验错误。
fn purchase_return_binding_organization_id(order: &PurchaseReturnOrder) -> Result<String> {
    let org = order.purchase_order_id.to_string();
    if org.trim().is_empty() {
        return Err(Error::ValidationError("采购退货缺少原采购单，无法构造绑定上下文".to_string()));
    }
    Ok(org)
}

/// 构造采购退货创建绑定命令。客户端不得提交定义 ID。
///
/// # 参数
/// * `order` - 待登记采购退货单
/// * `creator_id` - 创建人
///
/// # 错误
/// 原采购单为空时返回校验错误。
fn purchase_return_bind_command(
    order: &PurchaseReturnOrder,
    creator_id: &str,
) -> Result<BindPublishedDefinitionCommand> {
    Ok(BindPublishedDefinitionCommand {
        document_type: DocumentType::PurchaseReturnOrder,
        business_object_id: order.base.id.clone(),
        business_object_version: order.base.version,
        context: BindingRevalidationContext::new(
            purchase_return_binding_organization_id(order)?,
            creator_id.to_string(),
        ),
    })
}

/// 将绑定端口返回值落实为采购退货注册行：空绑定保持未绑定。
///
/// # 参数
/// * `document` - 采购退货注册行
/// * `binding` - 统一绑定端口返回值
///
/// # 返回
/// 固定返回 `None`。
///
/// # 错误
/// 端口返回绑定或注册行已预置绑定时返回错误。
fn apply_purchase_return_create_binding(
    document: &mut BusinessDocument,
    binding: Option<ApprovalDefinitionBinding>,
) -> Result<Option<ApprovalDefinitionBinding>> {
    if binding.is_some() {
        return Err(Error::Internal("采购退货为 NO_APPROVAL，不得写入审批绑定".to_string()));
    }
    if document.approval_binding.is_some() {
        return Err(Error::Internal("采购退货注册行不得预置审批绑定".to_string()));
    }
    if document.document_type != DocumentType::PurchaseReturnOrder {
        return Err(Error::Internal("采购退货创建只能注册 PurchaseReturnOrder 单据".to_string()));
    }
    Ok(None)
}

/// 在调用方事务内登记采购退货单据并证明空绑定。
///
/// 必须先确认政策跳过，再调用统一绑定端口；不得查询发布定义后假装成功。
///
/// # 错误
/// 政策非无审批、端口返回绑定或写入失败时返回错误。
async fn persist_unbound_purchase_return_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = ensure_purchase_return_skips_approval_binding()?;
    ensure_purchase_return_has_no_adapter()?;
    let binding = crate::adapters::workflow::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        bind_command,
        actor,
        executor,
    )
    .await?;
    apply_purchase_return_create_binding(&mut document, binding)?;
    persist_registered_document(db, &document, executor).await.map_err(crate::Error::from)
}

/// 为已构造采购退货登记 `BusinessDocument` 并调用统一绑定端口。
///
/// # 错误
/// 绑定端口或注册写入失败时返回错误。
async fn register_created_purchase_return_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    order: &PurchaseReturnOrder,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let bind_command = purchase_return_bind_command(order, actor.id())?;
    let document = new_registered_document(
        &order.base.id,
        DocumentType::PurchaseReturnOrder,
        order.purchase_return_no.clone(),
    )
    .map_err(crate::Error::from)?;
    persist_unbound_purchase_return_document(db, rbac, object_read, document, &bind_command, actor, executor)
        .await
}

/// 在创建事务内写入采购退货草稿并登记无绑定单据。
///
/// # 错误
/// 绑定、注册或退货单写入失败时返回错误，调用方必须视作整体回滚。
async fn persist_created_purchase_return_order(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    order: PurchaseReturnOrder,
    line: PurchaseReturnLine,
    actor: AuditActor,
) -> Result<()> {
    let audit = actor.clone().resource_log(
        "purchase_return_order.create",
        "purchase_return_order",
        order.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let object_read = object_read.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |executor| {
            Box::pin(async move {
                let mut creation = MongoCreation {
                    db: &db,
                    rbac: &rbac,
                    object_read: object_read.as_ref(),
                    order: &order,
                    line: &line,
                    actor: &actor,
                    audit: &audit,
                };
                crate::adapters::purchase_access(db.clone(), rbac.clone())
                    .require_object(&actor, "update", order.purchase_order_id.as_ref(), &[], executor)
                    .await?;
                persist_creation(&mut creation, executor).await?;
                Ok::<(), crate::Error>(())
            })
        })
        .await
}

/// 无审批创建的既有写入边界；每一步复用调用方 Executor，首错停止。
#[async_trait::async_trait]
trait CreationSteps: Send {
    async fn register_document(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn persist_return(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()>;
}

async fn persist_creation(steps: &mut impl CreationSteps, executor: &mut dyn Executor) -> Result<()> {
    steps.register_document(executor).await?;
    steps.persist_return(executor).await?;
    steps.audit(executor).await
}

struct MongoCreation<'a> {
    db: &'a Database,
    rbac: &'a SharedRbacService,
    object_read: &'a dyn erp_workflow::ApprovalObjectReadPort,
    order: &'a PurchaseReturnOrder,
    line: &'a PurchaseReturnLine,
    actor: &'a AuditActor,
    audit: &'a erp_audit::AuditLog,
}

#[async_trait::async_trait]
impl CreationSteps for MongoCreation<'_> {
    async fn register_document(&mut self, executor: &mut dyn Executor) -> Result<()> {
        register_created_purchase_return_document(
            self.db,
            self.rbac,
            self.object_read,
            self.order,
            self.actor,
            executor,
        )
        .await
    }
    async fn persist_return(&mut self, executor: &mut dyn Executor) -> Result<()> {
        persist_purchase_return_order_with_line(self.db, self.order, self.line, executor).await?;
        Ok(())
    }
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
        self.db.audit_logs().create(self.audit, executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod purchase_return_no_approval_tests {
    use bpm::ProcessKind;
    use bpm::ids::ApprovalProcessDefinitionId;
    use erp_core::common::time::Instant;
    use erp_core::ids::{PurchaseOrderId, PurchaseReturnOrderId};
    use erp_returns::entity::returns::{PurchaseReturnOrderData, ReturnMode};
    use erp_workflow::service::approval::binding::binding_from_published;
    use erp_workflow::service::document_registry::new_registered_document;

    use super::{
        BindingDecision, DocumentApprovalPolicy, DocumentType, PurchaseReturnOrder,
        apply_purchase_return_create_binding, ensure_purchase_return_has_no_adapter,
        ensure_purchase_return_skips_approval_binding, policy_of, purchase_return_bind_command,
        purchase_return_create_binding_decision,
    };

    fn draft_order() -> PurchaseReturnOrder {
        PurchaseReturnOrder::new(
            PurchaseReturnOrderId::new("pro-1"),
            PurchaseReturnOrderData {
                purchase_return_no: "PR-1".into(),
                purchase_order_id: PurchaseOrderId::new("po-1"),
                sales_return_case_id: None,
                return_mode: ReturnMode::CompanyWarehouseToSupplier,
            },
            "admin-1",
        )
        .expect("草稿必须可构造")
    }

    /// 政策仅含 document_type、approval_requirement、process_kind，不得注册空 Adapter。
    #[test]
    fn purchase_return_policy_is_no_approval_identity_only() {
        let policy = policy_of(DocumentType::PurchaseReturnOrder).expect("采购退货政策必须存在");
        let DocumentApprovalPolicy::NoApproval(no_approval) = &policy else {
            panic!("采购退货必须是 NO_APPROVAL");
        };
        assert_eq!(no_approval.document_type, DocumentType::PurchaseReturnOrder);
        assert_eq!(no_approval.process_kind, ProcessKind::PurchaseReturnOrder);
        assert_eq!(
            purchase_return_create_binding_decision().expect("绑定决定"),
            BindingDecision::SkipNoApproval
        );
        assert_eq!(
            ensure_purchase_return_skips_approval_binding().expect("必须跳过"),
            BindingDecision::SkipNoApproval
        );
        ensure_purchase_return_has_no_adapter().expect("不得注册空适配器");
    }

    /// 创建必须注册 BusinessDocument，绑定端口返回空，禁止写入绑定。
    #[test]
    fn create_registers_document_and_returns_empty_binding() {
        let order = draft_order();
        let command = purchase_return_bind_command(&order, "admin-1").expect("绑定命令");
        assert_eq!(command.document_type, DocumentType::PurchaseReturnOrder);
        assert_eq!(command.business_object_id, order.base.id);
        assert_eq!(command.context.organization_id, "po-1");

        let mut document = new_registered_document(
            &order.base.id,
            DocumentType::PurchaseReturnOrder,
            order.purchase_return_no.clone(),
        )
        .expect("可注册");
        assert!(document.approval_binding.is_none());
        let empty = apply_purchase_return_create_binding(&mut document, None).expect("空绑定");
        assert!(empty.is_none());
        assert!(document.approval_binding.is_none());

        let forged =
            binding_from_published(ApprovalProcessDefinitionId::new("def-1"), 1, Instant::from_unix_secs(10))
                .expect("测试绑定");
        assert!(apply_purchase_return_create_binding(&mut document, Some(forged)).is_err());
    }
}

#[cfg(test)]
mod creation_sequence_tests {
    use persistence_core::Executor;

    use super::{CreationSteps, persist_creation};
    use crate::{Error, Result};

    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    #[derive(Default)]
    struct RecordingCreation {
        events: Vec<&'static str>,
        executor_ids: Vec<usize>,
        fail: Option<&'static str>,
    }
    impl RecordingCreation {
        fn record(&mut self, step: &'static str, executor: &mut dyn Executor) -> Result<()> {
            self.events.push(step);
            self.executor_ids.push(executor as *mut dyn Executor as *mut () as usize);
            if self.fail == Some(step) {
                return Err(Error::ConflictError(step.to_string()));
            }
            Ok(())
        }
    }
    #[async_trait::async_trait]
    impl CreationSteps for RecordingCreation {
        async fn register_document(&mut self, executor: &mut dyn Executor) -> Result<()> {
            self.record("document", executor)
        }
        async fn persist_return(&mut self, executor: &mut dyn Executor) -> Result<()> {
            self.record("return_head_and_first_line", executor)
        }
        async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
            self.record("audit", executor)
        }
    }
    #[tokio::test]
    async fn no_approval_creation_keeps_one_executor_and_write_order() {
        let mut executor = TestExecutor { _identity: 1 };
        let id = &mut executor as *mut TestExecutor as usize;
        let mut steps = RecordingCreation::default();
        persist_creation(&mut steps, &mut executor).await.unwrap();
        assert_eq!(steps.events, ["document", "return_head_and_first_line", "audit"]);
        assert_eq!(steps.executor_ids, [id, id, id]);
    }
    #[tokio::test]
    async fn no_approval_creation_preserves_first_error_and_stops_later_writes() {
        let all = ["document", "return_head_and_first_line", "audit"];
        for (at, fail) in all.iter().enumerate() {
            let mut executor = TestExecutor { _identity: 1 };
            let mut steps = RecordingCreation { fail: Some(*fail), ..Default::default() };
            let error = persist_creation(&mut steps, &mut executor).await.unwrap_err();
            assert!(matches!(error, Error::ConflictError(message) if message == *fail));
            assert_eq!(steps.events, all[..=at]);
        }
    }
}
