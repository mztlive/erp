use database::{
    AccessControlExt, DocumentRegistryExt, Executor, FulfillmentExt, NoTransaction, Transactional,
};
use entities::document_registry::{BusinessDocument, DocumentType};
use entities::fulfillment::ServiceFulfillment;
use mongodb::Database;
use validator::Validate;

use crate::approval::binding::{
    bind_published_definition_on_document_create, binding_decision, BindPublishedDefinitionCommand,
    BindingDecision,
};
use crate::approval::business_adapter::{adapter_spec_of, BindingRevalidationContext};
use crate::approval::policy::{policy_of, DocumentApprovalPolicy};
use crate::audit::AuditActor;
use crate::document_registry::new_registered_document;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

use super::dto::SortDir;
use super::service_fulfillment_crypto::service_fulfillment_draft_from_request;
use super::{
    CreateServiceFulfillmentRequest, FulfillmentService, PageView, ServiceFulfillmentListParams,
    ServiceFulfillmentView,
};

/// 线下服务履约记录列表筛选条件类型。
type ServiceFulfillmentFilter = <mongodb::Database as FulfillmentExt>::ServiceFulfillmentFilter;

impl FulfillmentService {
    // ---------------------------------------------------------- service_fulfillment

    /// 分页查询线下服务履约记录列表（W01 履约任务作业面）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sales_order_line_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.service_fulfillment_list",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "service_fulfillment_list"
        )
    )]
    pub async fn service_fulfillment_list(
        &self,
        params: &ServiceFulfillmentListParams,
    ) -> Result<PageView<ServiceFulfillmentView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ServiceFulfillmentFilter {
            sales_order_line_id: query.sales_order_line_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .service_fulfillments()
            .search_service_fulfillments(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ServiceFulfillmentView {
                id: row.id.clone(),
                fulfillment_no: row.fulfillment_no,
                sales_order_line_id: row.sales_order_line_id.to_string(),
                purchase_order_id: row.purchase_order_id.to_string(),
                purchase_line_sales_allocation_id: row.purchase_line_sales_allocation_id.to_string(),
                quantity: row.quantity,
                result: row.result,
                status: row.status,
                occurred_at: row.occurred_at.unix_secs(),
                recorded_at: row.recorded_at.unix_secs(),
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 按主键查询线下服务履约记录。
    ///
    /// W01 履约任务以工作项冻结的业务对象主键精确读取，不经列表分页扫描。
    ///
    /// # 参数
    /// * `id` - 服务履约记录主键
    ///
    /// # 返回
    /// 返回服务履约记录视图。
    ///
    /// # 错误
    /// * `NotFound` - 服务履约记录不存在
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.service_fulfillment_detail",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "service_fulfillment_detail"
        )
    )]
    pub async fn service_fulfillment_detail(&self, id: &str) -> Result<ServiceFulfillmentView> {
        let record = self
            .db
            .service_fulfillments()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("服务履约记录不存在".to_string()))?;
        Ok(record.into())
    }

    /// 创建线下服务履约记录（草稿）。
    ///
    /// 同一事务注册 `BusinessDocument` 并调用统一绑定端口。服务履约为
    /// `NO_APPROVAL`：返回空绑定，不查询已发布定义，不启动审批实例，
    /// 不创建审批任务。服务地点与交付对象快照以不透明值传入，服务端
    /// 计算查询指纹后落库。
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
        name = "fulfillment.service_fulfillment_create",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "service_fulfillment_create"
        )
    )]
    pub async fn create_service_fulfillment(
        &self,
        req: CreateServiceFulfillmentRequest,
        actor: &AuditActor,
    ) -> Result<ServiceFulfillmentView> {
        req.validate()?;
        let record = service_fulfillment_draft_from_request(req, actor, &self.fingerprint_key)?;
        persist_created_service_fulfillment(&self.db, &self.rbac, record.clone(), actor.clone()).await?;
        Ok(record.into())
    }
}

impl From<ServiceFulfillment> for ServiceFulfillmentView {
    /// 从服务履约记录实体构造视图。
    fn from(record: ServiceFulfillment) -> Self {
        Self {
            id: record.base.id,
            fulfillment_no: record.fulfillment_no,
            sales_order_line_id: record.sales_order_line_id.to_string(),
            purchase_order_id: record.purchase_order_id.to_string(),
            purchase_line_sales_allocation_id: record.purchase_line_sales_allocation_id.to_string(),
            quantity: record.quantity,
            result: record.result,
            status: record.status,
            occurred_at: record.fact.occurred_at.unix_secs(),
            recorded_at: record.fact.recorded_at.unix_secs(),
            version: record.base.version,
        }
    }
}

/// 服务履约创建必须跳过绑定：政策只能是 `NO_APPROVAL`。
///
/// # 返回
/// 返回 `SkipNoApproval`。
///
/// # 错误
/// 政策缺失或误登记为必须审批时返回部署不变量错误。
fn service_fulfillment_create_binding_decision() -> Result<BindingDecision> {
    let policy = policy_of(DocumentType::ServiceFulfillment)?;
    match &policy {
        DocumentApprovalPolicy::NoApproval(no_approval) => {
            if no_approval.document_type != DocumentType::ServiceFulfillment {
                return Err(Error::Internal("服务履约政策类型不匹配".to_string()));
            }
            Ok(binding_decision(policy.requirement()))
        }
        DocumentApprovalPolicy::ProcessRequired(_) => Err(Error::Internal(
            "服务履约必须是 NO_APPROVAL，不得绑定流程".to_string(),
        )),
    }
}

/// 确认服务履约创建路径不得查询发布定义。
///
/// # 错误
/// 绑定决定不是跳过时返回错误。
fn ensure_service_fulfillment_skips_approval_binding() -> Result<BindingDecision> {
    let decision = service_fulfillment_create_binding_decision()?;
    if decision != BindingDecision::SkipNoApproval {
        return Err(Error::Internal("服务履约创建必须跳过审批绑定".to_string()));
    }
    Ok(decision)
}

/// 服务履约不得注册空审批适配器。
///
/// # 错误
/// 适配器登记存在时返回部署不变量错误。
fn ensure_service_fulfillment_has_no_adapter() -> Result<()> {
    if adapter_spec_of(DocumentType::ServiceFulfillment).is_ok() {
        return Err(Error::Internal("服务履约不得注册审批适配器".to_string()));
    }
    Ok(())
}

/// 构造服务履约创建绑定命令。客户端不得提交定义 ID。
///
/// # 参数
/// * `record` - 待登记服务履约
/// * `creator_id` - 创建人
///
/// # 错误
/// 销售明细为空时返回校验错误。
fn service_fulfillment_bind_command(
    record: &ServiceFulfillment,
    creator_id: &str,
) -> Result<BindPublishedDefinitionCommand> {
    Ok(BindPublishedDefinitionCommand {
        document_type: DocumentType::ServiceFulfillment,
        business_object_id: record.base.id.clone(),
        business_object_version: record.base.version,
        context: BindingRevalidationContext {
            organization_id: record
                .registration_context_id()
                .map_err(|error| Error::ValidationError(error.to_string()))?
                .to_string(),
            creator_id: creator_id.to_string(),
        },
    })
}

/// 在调用方事务内登记服务履约单据并证明空绑定。
///
/// 必须先确认政策跳过，再调用统一绑定端口；不得查询发布定义后假装成功。
///
/// # 错误
/// 政策非无审批、端口返回绑定或写入失败时返回错误。
async fn persist_unbound_service_fulfillment_document(
    db: &Database,
    rbac: &SharedRbacService,
    document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = ensure_service_fulfillment_skips_approval_binding()?;
    ensure_service_fulfillment_has_no_adapter()?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, executor).await?;
    document
        .ensure_no_approval_registration(DocumentType::ServiceFulfillment, binding.as_ref())
        .map_err(|error| Error::Internal(error.to_string()))?;
    db.business_documents()
        .register_no_approval_document(&document, executor)
        .await?;
    Ok(())
}

/// 为已构造服务履约登记 `BusinessDocument` 并调用统一绑定端口。
///
/// # 错误
/// 绑定端口或注册写入失败时返回错误。
async fn register_created_service_fulfillment_document(
    db: &Database,
    rbac: &SharedRbacService,
    record: &ServiceFulfillment,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let bind_command = service_fulfillment_bind_command(record, actor.id())?;
    let document = new_registered_document(
        &record.base.id,
        DocumentType::ServiceFulfillment,
        record.fulfillment_no.clone(),
    )?;
    persist_unbound_service_fulfillment_document(db, rbac, document, &bind_command, actor, executor).await
}

/// 在创建事务内写入服务履约草稿并登记无绑定单据。
///
/// # 错误
/// 绑定、注册或服务履约写入失败时返回错误，调用方必须视作整体回滚。
async fn persist_created_service_fulfillment(
    db: &Database,
    rbac: &SharedRbacService,
    record: ServiceFulfillment,
    actor: AuditActor,
) -> Result<()> {
    let audit = actor.clone().resource_log(
        "service_fulfillment.create",
        "service_fulfillment",
        record.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                register_created_service_fulfillment_document(&db, &rbac, &record, &actor, session).await?;
                db.service_fulfillments().create(&record, session).await?;
                super::task::ensure_fulfillment_task(
                    &db,
                    super::task::FulfillmentTaskObject::ServiceFulfillment(&record),
                    session,
                )
                .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

#[cfg(test)]
mod service_fulfillment_no_approval_tests {
    use super::{
        ensure_service_fulfillment_has_no_adapter, ensure_service_fulfillment_skips_approval_binding,
        policy_of, service_fulfillment_bind_command, service_fulfillment_create_binding_decision,
        BindingDecision, DocumentApprovalPolicy, DocumentType, ServiceFulfillment,
    };
    use crate::approval::binding::binding_from_published;
    use crate::document_registry::new_registered_document;
    use bpm::ids::ApprovalProcessDefinitionId;
    use bpm::ProcessKind;
    use entities::common::source::SourceType;
    use entities::common::time::Instant;
    use entities::fulfillment::{FulfillmentResult, ServiceFulfillmentData};
    use entities::ids::{
        PurchaseLineSalesAllocationId, PurchaseOrderId, SalesOrderLineId, ServiceFulfillmentId,
    };
    use entities::money::Quantity;
    use std::str::FromStr;

    fn draft_service_fulfillment() -> ServiceFulfillment {
        ServiceFulfillment::new(
            ServiceFulfillmentId::new("sf-1"),
            ServiceFulfillmentData {
                fulfillment_no: "SF-1".into(),
                sales_order_line_id: SalesOrderLineId::new("so-line-1"),
                purchase_order_id: PurchaseOrderId::new("po-1"),
                purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId::new("pla-1"),
                recipient_snapshot: "ciphertext-recipient".into(),
                recipient_snapshot_fingerprint: ServiceFulfillment::recipient_snapshot_fingerprint(
                    "recipient",
                    b"test-fingerprint-key",
                ),
                quantity: Quantity::from_str("1").expect("数量合法"),
                result: FulfillmentResult::Success,
                evidence_attachment_id: None,
                service_location_encrypted: "ciphertext-location".into(),
                service_location_fingerprint: ServiceFulfillment::service_location_fingerprint(
                    "location",
                    b"test-fingerprint-key",
                ),
                service_started_at: Some(Instant::from_unix_secs(1_700_000_000)),
                service_ended_at: Some(Instant::from_unix_secs(1_700_003_600)),
                completion_note: Some("上门安装完成".into()),
                fact_no: "F-002".into(),
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
    fn service_fulfillment_policy_is_no_approval_identity_only() {
        let policy = policy_of(DocumentType::ServiceFulfillment).expect("服务履约政策必须存在");
        let DocumentApprovalPolicy::NoApproval(no_approval) = &policy else {
            panic!("服务履约必须是 NO_APPROVAL");
        };
        assert_eq!(no_approval.document_type, DocumentType::ServiceFulfillment);
        assert_eq!(no_approval.process_kind, ProcessKind::ServiceFulfillment);
        assert_eq!(
            service_fulfillment_create_binding_decision().expect("绑定决定"),
            BindingDecision::SkipNoApproval
        );
        assert_eq!(
            ensure_service_fulfillment_skips_approval_binding().expect("必须跳过"),
            BindingDecision::SkipNoApproval
        );
        ensure_service_fulfillment_has_no_adapter().expect("不得注册空适配器");
    }

    /// 创建必须注册 BusinessDocument，绑定端口返回空，禁止写入绑定。
    #[test]
    fn create_registers_document_and_returns_empty_binding() {
        let record = draft_service_fulfillment();
        let command = service_fulfillment_bind_command(&record, "admin-1").expect("绑定命令");
        assert_eq!(command.document_type, DocumentType::ServiceFulfillment);
        assert_eq!(command.business_object_id, record.base.id);
        assert_eq!(command.context.organization_id, "so-line-1");

        let document = new_registered_document(
            &record.base.id,
            DocumentType::ServiceFulfillment,
            record.fulfillment_no.clone(),
        )
        .expect("可注册");
        assert!(document.approval_binding.is_none());
        document
            .ensure_no_approval_registration(DocumentType::ServiceFulfillment, None)
            .expect("空绑定");
        assert!(document.approval_binding.is_none());

        let forged = binding_from_published(
            ApprovalProcessDefinitionId::new("def-1"),
            1,
            Instant::from_unix_secs(10),
        )
        .expect("测试绑定");
        assert!(document
            .ensure_no_approval_registration(DocumentType::ServiceFulfillment, Some(&forged))
            .is_err());
    }

    /// 创建路径调用统一绑定端口，不查询发布定义、不启动实例、不建任务。
    #[test]
    fn create_does_not_query_definition_or_start_instance() {
        let production = include_str!("service_fulfillment.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("persist_created_service_fulfillment"));
        assert!(production.contains("register_created_service_fulfillment_document"));
        assert!(production.contains("persist_unbound_service_fulfillment_document"));
        assert!(production.contains("bind_published_definition_on_document_create"));
        assert!(production.contains("DocumentType::ServiceFulfillment"));
        assert!(production.contains("new_registered_document"));
        assert!(production.contains("ensure_service_fulfillment_skips_approval_binding"));
        assert!(production.contains("ensure_service_fulfillment_has_no_adapter"));
        assert!(!production.contains("pub async fn submit_service_fulfillment"));
        assert!(!production.contains("start_service_fulfillment_approval"));
        assert!(!production.contains("ServiceFulfillmentAdapter"));
        assert!(!production.contains("load_published_graph"));
        let create = production
            .split("pub async fn create_service_fulfillment")
            .nth(1)
            .and_then(|rest| rest.split("impl From<ServiceFulfillment>").next())
            .expect("create_service_fulfillment 生产片段");
        assert!(create.contains("persist_created_service_fulfillment"));
        assert!(!create.contains("prepare_start"));
        assert!(!create.contains("attach_published_binding"));
        assert!(!create.contains("WorkItem"));
        assert!(!create.contains("start_approval"));
    }

    /// 创建路径经领域草稿工厂与双指纹 crypto port：旧 Service helper 已删除。
    #[test]
    fn create_uses_draft_factory_and_dual_fingerprints() {
        let production = include_str!("service_fulfillment.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(
            !production.contains("fn service_fulfillment_from_request"),
            "旧 helper 必须删除"
        );
        assert!(
            production.contains("service_fulfillment_draft_from_request"),
            "创建路径必须调用草稿编排"
        );
        assert!(
            !production.contains("SourceType::Erp"),
            "来源默认不得留在 Service"
        );
        let crypto = include_str!("service_fulfillment_crypto.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("crypto 生产代码");
        assert!(crypto.contains("ServiceFulfillmentDraft::build"));
        assert!(crypto.contains("ServiceRecipientFingerprint"));
        assert!(crypto.contains("ServiceLocationFingerprint"));
    }
}
