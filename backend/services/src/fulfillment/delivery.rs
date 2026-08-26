use std::collections::HashMap;

use database::{
    AccessControlExt, DocumentRegistryExt, Executor, FulfillmentExt, NoTransaction, Transactional,
};
use entities::document_registry::{BusinessDocument, DocumentType};
use entities::fulfillment::{Delivery, DeliveryData, DeliveryLine, DeliveryLineData, DeliveryType};
use entities::ids::{DeliveryId, DeliveryLineId};
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
use super::{
    CreateDeliveryRequest, DeliveryDetailView, DeliveryLineInput, DeliveryLineView, DeliveryListParams,
    DeliveryView, FulfillmentService, PageView, UpdateDeliveryRequest,
};

/// 发货单列表筛选条件类型。
type DeliveryFilter = <mongodb::Database as FulfillmentExt>::DeliveryFilter;

impl FulfillmentService {
    // ------------------------------------------------------------------- delivery

    /// 分页查询发货单列表（W01 履约任务作业面）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sales_order_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.delivery_list",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "delivery_list")
    )]
    pub async fn delivery_list(&self, params: &DeliveryListParams) -> Result<PageView<DeliveryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = DeliveryFilter {
            sales_order_id: query.sales_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .deliveries()
            .search_deliveries(&filter, &mut NoTransaction)
            .await?;
        let direct_ids: Vec<DeliveryId> = page
            .items
            .iter()
            .filter(|row| row.delivery_type == DeliveryType::SupplierDirect)
            .map(|row| DeliveryId::new(row.id.clone()))
            .collect();
        let direct_po_ids = load_direct_po_ids(&self.db, &direct_ids).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| DeliveryView {
                id: row.id.clone(),
                delivery_no: row.delivery_no,
                delivery_type: row.delivery_type,
                sales_order_id: row.sales_order_id.to_string(),
                purchase_order_id: direct_po_ids.get(&row.id).cloned().flatten(),
                warehouse_id: row.warehouse_id.map(|id| id.to_string()),
                status: row.status,
                carrier: row.carrier,
                tracking_no: row.tracking_no,
                shipped_at: row.shipped_at.map(|instant| instant.unix_secs()),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询发货单详情（表头 + 行）。
    ///
    /// # 参数
    /// * `id` - 发货单主键
    ///
    /// # 返回
    /// 返回发货单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 发货单不存在
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.delivery_detail",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "delivery_detail")
    )]
    pub async fn delivery_detail(&self, id: &str) -> Result<DeliveryDetailView> {
        let delivery = self
            .db
            .deliveries()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("发货单不存在".to_string()))?;
        let lines = self
            .db
            .fulfillment()
            .delivery_lines_by_delivery_ids(&[delivery.base.id.clone().into()], &mut NoTransaction)
            .await?;
        Ok(DeliveryDetailView {
            delivery: delivery.clone().into(),
            lines: lines.into_iter().map(Into::into).collect(),
        })
    }

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
        req.validate()?;
        let id = DeliveryId::new(next_id());
        let delivery = Delivery::new(
            id.clone(),
            DeliveryData {
                delivery_no: req.delivery_no,
                delivery_type: req.delivery_type,
                sales_order_id: req.sales_order_id,
                purchase_order_id: req.purchase_order_id,
                warehouse_id: req.warehouse_id,
                carrier: req.carrier,
                tracking_no: req.tracking_no,
                address_snapshot_encrypted: None,
                address_snapshot_fingerprint: None,
            },
        )?;
        let lines = build_delivery_lines(&id, delivery.delivery_type, &req.lines)?;
        persist_created_delivery(&self.db, &self.rbac, delivery.clone(), lines, actor.clone()).await?;
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
        req.validate()?;
        let mut delivery = self
            .db
            .deliveries()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("发货单不存在".to_string()))?;
        if delivery.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        delivery.update(entities::fulfillment::DeliveryUpdate {
            carrier: req.carrier,
            tracking_no: req.tracking_no,
        })?;
        let audit = actor
            .clone()
            .resource_log("delivery.update", "delivery", id.to_string())?;
        let db = self.db.clone();
        let actor_id = actor.id().to_string();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.deliveries().update(&mut delivery, session).await?;
                    super::task::record_fulfillment_activity(
                        &db,
                        super::task::FulfillmentTaskObject::Delivery(&delivery),
                        &actor_id,
                        session,
                    )
                    .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Delivery, crate::errors::Error>(delivery)
                })
            })
            .await?;
        Ok(updated.into())
    }
}

// ------------------------------------------------------------------ private helpers

/// 批量取供应商直发货单的采购来源（P2 投影行未含 `purchase_order_id`）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `delivery_ids` - 供应商直发的主键集合（仓发无需查询）
///
/// # 返回
/// 返回「发货单 → 采购来源」映射。
///
/// # 错误
/// 批量查询失败时返回 `RepositoryError`。
async fn load_direct_po_ids(
    db: &Database,
    delivery_ids: &[DeliveryId],
) -> Result<HashMap<String, Option<String>>> {
    let deliveries = db
        .fulfillment()
        .list_deliveries_by_ids(delivery_ids, &mut NoTransaction)
        .await?;
    Ok(deliveries
        .into_iter()
        .map(|delivery| {
            (
                delivery.base.id,
                delivery.purchase_order_id.map(|id| id.to_string()),
            )
        })
        .collect())
}

/// 构建发货行实体集合（行号从 1 递增）。
///
/// # 参数
/// * `delivery_id` - 发货单主键
/// * `delivery_type` - 发货类型（决定行级归属校验）
/// * `inputs` - 行输入
///
/// # 返回
/// 返回行实体集合。
///
/// # 错误
/// 行归属与发货类型不一致或数量非正时返回错误（实体构造）。
fn build_delivery_lines(
    delivery_id: &DeliveryId,
    delivery_type: DeliveryType,
    inputs: &[DeliveryLineInput],
) -> Result<Vec<DeliveryLine>> {
    let mut lines = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        lines.push(
            DeliveryLine::new(
                DeliveryLineId::new(next_id()),
                DeliveryLineData {
                    delivery_id: delivery_id.clone(),
                    line_no: index as u32 + 1,
                    sales_order_line_id: input.sales_order_line_id.clone(),
                    quantity: input.quantity,
                    stock_reservation_id: input.stock_reservation_id.clone(),
                    purchase_line_sales_allocation_id: input.purchase_line_sales_allocation_id.clone(),
                },
                delivery_type,
            )
            .map_err(Error::Logic)?,
        );
    }
    Ok(lines)
}

impl From<Delivery> for DeliveryView {
    /// 从发货单实体构造视图。
    fn from(delivery: Delivery) -> Self {
        Self {
            id: delivery.base.id,
            delivery_no: delivery.delivery_no,
            delivery_type: delivery.delivery_type,
            sales_order_id: delivery.sales_order_id.to_string(),
            purchase_order_id: delivery.purchase_order_id.map(|id| id.to_string()),
            warehouse_id: delivery.warehouse_id.map(|id| id.to_string()),
            status: delivery.status,
            carrier: delivery.carrier,
            tracking_no: delivery.tracking_no,
            shipped_at: delivery.shipped_at.map(|instant| instant.unix_secs()),
            version: delivery.base.version,
            created_at: delivery.base.created_at,
        }
    }
}

impl From<DeliveryLine> for DeliveryLineView {
    /// 从发货行实体构造视图。
    fn from(line: DeliveryLine) -> Self {
        Self {
            id: line.base.id,
            line_no: line.line_no,
            sales_order_line_id: line.sales_order_line_id.to_string(),
            quantity: line.quantity,
            stock_reservation_id: line.stock_reservation_id.map(|id| id.to_string()),
            purchase_line_sales_allocation_id: line
                .purchase_line_sales_allocation_id
                .map(|id| id.to_string()),
        }
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
    document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = ensure_delivery_skips_approval_binding()?;
    ensure_delivery_has_no_adapter()?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, executor).await?;
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
    delivery: &Delivery,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let bind_command = delivery_bind_command(delivery, actor.id())?;
    let document = new_registered_document(
        &delivery.base.id,
        DocumentType::Delivery,
        delivery.delivery_no.clone(),
    )?;
    persist_unbound_delivery_document(db, rbac, document, &bind_command, actor, executor).await
}

/// 在创建事务内写入发货草稿并登记无绑定单据。
///
/// # 错误
/// 绑定、注册或发货单写入失败时返回错误，调用方必须视作整体回滚。
async fn persist_created_delivery(
    db: &Database,
    rbac: &SharedRbacService,
    delivery: Delivery,
    lines: Vec<DeliveryLine>,
    actor: AuditActor,
) -> Result<()> {
    let audit = actor
        .clone()
        .resource_log("delivery.create", "delivery", delivery.base.id.clone())?;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                register_created_delivery_document(&db, &rbac, &delivery, &actor, session).await?;
                db.fulfillment()
                    .create_delivery_with_lines(&delivery, &lines, session)
                    .await?;
                super::task::ensure_fulfillment_task(
                    &db,
                    super::task::FulfillmentTaskObject::Delivery(&delivery),
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
mod tests {
    use super::build_delivery_lines;
    use crate::fulfillment::DeliveryLineInput;
    use entities::fulfillment::DeliveryType;
    use entities::ids::{DeliveryId, PurchaseLineSalesAllocationId, SalesOrderLineId, StockReservationId};
    use entities::money::Quantity;
    use std::str::FromStr;

    #[test]
    fn delivery_lines_enforce_type_ownership() {
        let ok = build_delivery_lines(
            &DeliveryId::new("d-1"),
            DeliveryType::WarehouseShip,
            &[DeliveryLineInput {
                sales_order_line_id: SalesOrderLineId::new("so-line-1"),
                quantity: Quantity::from_str("2").unwrap(),
                stock_reservation_id: Some(StockReservationId::new("rsv-1")),
                purchase_line_sales_allocation_id: None,
            }],
        )
        .unwrap();
        assert_eq!(ok.len(), 1);
        let wrong = build_delivery_lines(
            &DeliveryId::new("d-2"),
            DeliveryType::WarehouseShip,
            &[DeliveryLineInput {
                sales_order_line_id: SalesOrderLineId::new("so-line-1"),
                quantity: Quantity::from_str("2").unwrap(),
                stock_reservation_id: None,
                purchase_line_sales_allocation_id: Some(PurchaseLineSalesAllocationId::new("a-1")),
            }],
        );
        assert!(wrong.is_err(), "仓发不得携带直发分配");
    }
}

#[cfg(test)]
mod delivery_no_approval_tests {
    use super::{
        delivery_bind_command, delivery_create_binding_decision, ensure_delivery_has_no_adapter,
        ensure_delivery_skips_approval_binding, policy_of, BindingDecision, Delivery, DeliveryData,
        DeliveryType, DocumentApprovalPolicy, DocumentType,
    };
    use crate::approval::binding::binding_from_published;
    use crate::document_registry::new_registered_document;
    use bpm::ids::ApprovalProcessDefinitionId;
    use bpm::ProcessKind;
    use entities::common::time::Instant;
    use entities::ids::{DeliveryId, SalesOrderId, WarehouseId};

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
