use std::collections::HashMap;

use database::{
    AccessControlExt, DocumentRegistryExt, Executor, FulfillmentExt, NoTransaction, PurchaseOrderExt,
    Transactional,
};
use entities::common::source::SourceType;
use entities::common::time::Instant;
use entities::document_registry::{BusinessDocument, DocumentType};
use entities::fulfillment::{ElectronicDelivery, ElectronicDeliveryData};
use entities::ids::ElectronicDeliveryId;
use id_generator::next_id;
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
use super::purchase_context::{ensure_allocation_valid, ensure_po_fulfillable, ensure_prepay_gate};
use super::{
    CreateElectronicDeliveryRequest, ElectronicDeliveryListParams, ElectronicDeliveryView,
    FulfillmentService, PageView,
};

/// 电子交付记录列表筛选条件类型。
type ElectronicDeliveryFilter = <mongodb::Database as FulfillmentExt>::ElectronicDeliveryFilter;

impl FulfillmentService {
    // ----------------------------------------------------------- electronic_delivery

    /// 分页查询电子交付记录列表（W01 履约任务作业面）。
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
        name = "fulfillment.electronic_delivery_list",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "electronic_delivery_list"
        )
    )]
    pub async fn electronic_delivery_list(
        &self,
        params: &ElectronicDeliveryListParams,
    ) -> Result<PageView<ElectronicDeliveryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ElectronicDeliveryFilter {
            sales_order_line_id: query.sales_order_line_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .electronic_deliveries()
            .search_electronic_deliveries(&filter, &mut NoTransaction)
            .await?;
        let page_ids: Vec<ElectronicDeliveryId> = page
            .items
            .iter()
            .map(|row| ElectronicDeliveryId::new(row.id.clone()))
            .collect();
        let allocation_ids = load_electronic_allocation_ids(&self.db, &page_ids).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ElectronicDeliveryView {
                id: row.id.clone(),
                fulfillment_no: row.fulfillment_no,
                sales_order_line_id: row.sales_order_line_id.to_string(),
                purchase_order_id: row.purchase_order_id.to_string(),
                purchase_line_sales_allocation_id: allocation_ids.get(&row.id).cloned().unwrap_or_default(),
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

    /// 按主键查询电子交付记录。
    ///
    /// W01 履约任务以工作项冻结的业务对象主键精确读取，不经列表分页扫描。
    ///
    /// # 参数
    /// * `id` - 电子交付记录主键
    ///
    /// # 返回
    /// 返回电子交付记录视图。
    ///
    /// # 错误
    /// * `NotFound` - 电子交付记录不存在
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.electronic_delivery_detail",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "electronic_delivery_detail"
        )
    )]
    pub async fn electronic_delivery_detail(&self, id: &str) -> Result<ElectronicDeliveryView> {
        let record = self
            .db
            .electronic_deliveries()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("电子交付记录不存在".to_string()))?;
        Ok(record.into())
    }

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
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "electronic_delivery_create"
        )
    )]
    pub async fn create_electronic_delivery(
        &self,
        req: CreateElectronicDeliveryRequest,
        actor: &AuditActor,
    ) -> Result<ElectronicDeliveryView> {
        req.validate()?;
        let record = electronic_delivery_from_request(req, actor, &self.fingerprint_key)?;
        persist_created_electronic_delivery(&self.db, &self.rbac, record.clone(), actor.clone()).await?;
        Ok(record.into())
    }

    /// 确认电子交付（草稿 → 已确认；§8.1.5 + §6.7 跨集合事务）。
    ///
    /// 在同一事务内：校验采购单可履约与 `PREPAY` 门槛、校验采购销售分配的
    /// 有效性（采购行归属当前生效版本、销售行归属本明细）、迁移记录状态、写
    /// 审计。重复确认由状态守卫（仅草稿）防护。
    ///
    /// # 参数
    /// * `id` - 记录主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回确认后的记录视图。
    ///
    /// # 错误
    /// * `NotFound` - 记录/采购单/分配不存在
    /// * `ConflictError` - 状态不允许确认或重复确认
    /// * `BusinessLogicError` - 门槛未满足或分配无效
    /// * `OutcomeUnknown` - 提交结果无法确认
    #[tracing::instrument(
        name = "fulfillment.electronic_delivery_confirm",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "electronic_delivery_confirm"
        )
    )]
    pub async fn confirm_electronic_delivery(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<ElectronicDeliveryView> {
        let record_id = ElectronicDeliveryId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let confirmed = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut record = db
                        .electronic_deliveries()
                        .find_by_id(record_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("电子交付记录不存在".to_string()))?;
                    record
                        .ensure_confirmable()
                        .map_err(|error| Error::ConflictError(error.to_string()))?;
                    let po = db
                        .purchase_orders()
                        .find_by_id(record.purchase_order_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("来源采购单不存在".to_string()))?;
                    ensure_po_fulfillable(&po)?;
                    ensure_prepay_gate(&db, session, &po).await?;
                    ensure_allocation_valid(
                        &db,
                        session,
                        &po,
                        &record.purchase_line_sales_allocation_id,
                        &record.sales_order_line_id,
                    )
                    .await?;
                    record.confirm()?;
                    db.electronic_deliveries().update(&mut record, session).await?;
                    super::task::complete_fulfillment_task(
                        &db,
                        super::task::FulfillmentTaskObject::ElectronicDelivery(&record),
                        actor.id(),
                        session,
                    )
                    .await?;
                    super::customer_acceptance_task::ensure_customer_acceptance_task(
                        &db,
                        &po.sales_order_id,
                        super::customer_acceptance_task::CustomerAcceptanceTaskReason::DeliveryAvailable,
                        session,
                    )
                    .await?;
                    let audit = actor.resource_log(
                        "electronic_delivery.confirm",
                        "electronic_delivery",
                        record_id.to_string(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ElectronicDelivery, crate::errors::Error>(record)
                })
            })
            .await?;
        Ok(confirmed.into())
    }
}

/// 批量取电子交付记录的采购销售分配（P2 投影行未含该字段）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `record_ids` - 记录主键集合
///
/// # 返回
/// 返回「记录 → 采购销售分配」映射。
///
/// # 错误
/// 批量查询失败时返回 `RepositoryError`。
async fn load_electronic_allocation_ids(
    db: &Database,
    record_ids: &[ElectronicDeliveryId],
) -> Result<HashMap<String, String>> {
    let records = db
        .fulfillment()
        .list_electronic_deliveries_by_ids(record_ids, &mut NoTransaction)
        .await?;
    Ok(records
        .into_iter()
        .map(|record| {
            (
                record.base.id,
                record.purchase_line_sales_allocation_id.to_string(),
            )
        })
        .collect())
}

impl From<ElectronicDelivery> for ElectronicDeliveryView {
    /// 从电子交付记录实体构造视图。
    fn from(record: ElectronicDelivery) -> Self {
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

/// 由创建请求构造电子交付草稿。
///
/// 交付对象快照以不透明值传入，服务端用指纹密钥计算查询指纹后落库。
///
/// # 参数
/// * `req` - 已通过校验的创建请求
/// * `actor` - 已通过鉴权的审计操作人
/// * `fingerprint_key` - 查询指纹密钥
///
/// # 返回
/// 返回新建草稿实体。
///
/// # 错误
/// 实体规范化失败时返回校验错误。
fn electronic_delivery_from_request(
    req: CreateElectronicDeliveryRequest,
    actor: &AuditActor,
    fingerprint_key: &[u8],
) -> Result<ElectronicDelivery> {
    let occurred_at = Instant::from_unix_secs(req.occurred_at);
    let recorded_at = Instant::now();
    Ok(ElectronicDelivery::new(
        ElectronicDeliveryId::new(next_id()),
        ElectronicDeliveryData {
            fulfillment_no: req.fulfillment_no,
            sales_order_line_id: req.sales_order_line_id,
            purchase_order_id: req.purchase_order_id,
            purchase_line_sales_allocation_id: req.purchase_line_sales_allocation_id,
            recipient_snapshot: req.recipient_snapshot.clone(),
            recipient_snapshot_fingerprint: ElectronicDelivery::recipient_snapshot_fingerprint(
                &req.recipient_snapshot,
                fingerprint_key,
            ),
            quantity: req.quantity,
            result: req.result,
            evidence_attachment_id: req.evidence_attachment_id,
            fact_no: next_id(),
            occurred_at,
            recorded_at,
            recorded_by: actor.id().to_string(),
            source_type: SourceType::Erp,
            source_reference: None,
            reason_code: None,
            reason_text: None,
        },
    )?)
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
        }
        DocumentApprovalPolicy::ProcessRequired(_) => Err(Error::Internal(
            "电子交付必须是 NO_APPROVAL，不得绑定流程".to_string(),
        )),
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
        context: BindingRevalidationContext {
            organization_id: record
                .registration_context_id()
                .map_err(|error| Error::ValidationError(error.to_string()))?
                .to_string(),
            creator_id: creator_id.to_string(),
        },
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
    document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = ensure_electronic_delivery_skips_approval_binding()?;
    ensure_electronic_delivery_has_no_adapter()?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, executor).await?;
    document
        .ensure_no_approval_registration(DocumentType::ElectronicDelivery, binding.as_ref())
        .map_err(|error| Error::Internal(error.to_string()))?;
    db.business_documents()
        .register_no_approval_document(&document, executor)
        .await?;
    Ok(())
}

/// 为已构造电子交付登记 `BusinessDocument` 并调用统一绑定端口。
///
/// # 错误
/// 绑定端口或注册写入失败时返回错误。
async fn register_created_electronic_delivery_document(
    db: &Database,
    rbac: &SharedRbacService,
    record: &ElectronicDelivery,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let bind_command = electronic_delivery_bind_command(record, actor.id())?;
    let document = new_registered_document(
        &record.base.id,
        DocumentType::ElectronicDelivery,
        record.fulfillment_no.clone(),
    )?;
    persist_unbound_electronic_delivery_document(db, rbac, document, &bind_command, actor, executor).await
}

/// 在创建事务内写入电子交付草稿并登记无绑定单据。
///
/// # 错误
/// 绑定、注册或电子交付写入失败时返回错误，调用方必须视作整体回滚。
async fn persist_created_electronic_delivery(
    db: &Database,
    rbac: &SharedRbacService,
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
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                register_created_electronic_delivery_document(&db, &rbac, &record, &actor, session).await?;
                db.electronic_deliveries().create(&record, session).await?;
                super::task::ensure_fulfillment_task(
                    &db,
                    super::task::FulfillmentTaskObject::ElectronicDelivery(&record),
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
mod electronic_delivery_no_approval_tests {
    use super::{
        electronic_delivery_bind_command, electronic_delivery_create_binding_decision,
        ensure_electronic_delivery_has_no_adapter, ensure_electronic_delivery_skips_approval_binding,
        policy_of, BindingDecision, DocumentApprovalPolicy, DocumentType, ElectronicDelivery,
        ElectronicDeliveryData,
    };
    use crate::approval::binding::binding_from_published;
    use crate::document_registry::new_registered_document;
    use bpm::ids::ApprovalProcessDefinitionId;
    use bpm::ProcessKind;
    use entities::common::source::SourceType;
    use entities::common::time::Instant;
    use entities::fulfillment::FulfillmentResult;
    use entities::ids::{
        ElectronicDeliveryId, PurchaseLineSalesAllocationId, PurchaseOrderId, SalesOrderLineId,
    };
    use entities::money::Quantity;
    use std::str::FromStr;

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
        document
            .ensure_no_approval_registration(DocumentType::ElectronicDelivery, None)
            .expect("空绑定");
        assert!(document.approval_binding.is_none());

        let forged = binding_from_published(
            ApprovalProcessDefinitionId::new("def-1"),
            1,
            Instant::from_unix_secs(10),
        )
        .expect("测试绑定");
        assert!(document
            .ensure_no_approval_registration(DocumentType::ElectronicDelivery, Some(&forged))
            .is_err());
    }

    /// 创建路径调用统一绑定端口，不查询发布定义、不启动实例、不建任务。
    #[test]
    fn create_does_not_query_definition_or_start_instance() {
        let production = include_str!("electronic_delivery.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("persist_created_electronic_delivery"));
        assert!(production.contains("register_created_electronic_delivery_document"));
        assert!(production.contains("persist_unbound_electronic_delivery_document"));
        assert!(production.contains("bind_published_definition_on_document_create"));
        assert!(production.contains("DocumentType::ElectronicDelivery"));
        assert!(production.contains("new_registered_document"));
        assert!(production.contains("ensure_electronic_delivery_skips_approval_binding"));
        assert!(production.contains("ensure_electronic_delivery_has_no_adapter"));
        assert!(!production.contains("pub async fn submit_electronic_delivery"));
        assert!(!production.contains("start_electronic_delivery_approval"));
        assert!(!production.contains("ElectronicDeliveryAdapter"));
        assert!(!production.contains("load_published_graph"));
        let create = production
            .split("pub async fn create_electronic_delivery")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn confirm_electronic_delivery").next())
            .expect("create_electronic_delivery 生产片段");
        assert!(create.contains("persist_created_electronic_delivery"));
        assert!(!create.contains("prepare_start"));
        assert!(!create.contains("attach_published_binding"));
        assert!(!create.contains("WorkItem"));
        assert!(!create.contains("start_approval"));
    }
}
