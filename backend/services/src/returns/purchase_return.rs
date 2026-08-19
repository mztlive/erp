use super::dto::{
    CreatePurchaseReturnOrderRequest, PageView, PurchaseReturnOrderListParams, PurchaseReturnOrderView,
    SortDir,
};
use super::ReturnsService;
use crate::approval::binding::{
    bind_published_definition_on_document_create, binding_decision, BindPublishedDefinitionCommand,
    BindingDecision,
};
use crate::approval::business_adapter::{adapter_spec_of, BindingRevalidationContext};
use crate::approval::policy::{policy_of, DocumentApprovalPolicy};
use crate::audit::AuditActor;
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use database::{AccessControlExt, Executor, NoTransaction, ReturnsExt, Transactional};
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::{BusinessDocument, DocumentType};
use entities::ids::{PurchaseReturnLineId, PurchaseReturnOrderId};
use entities::returns::{
    PurchaseReturnLine, PurchaseReturnLineData, PurchaseReturnOrder, PurchaseReturnOrderData,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

/// 采购退货单列表筛选条件类型。
type PurchaseReturnOrderFilter = <mongodb::Database as ReturnsExt>::PurchaseReturnOrderFilter;

impl ReturnsService {
    // -----------------------------------------------------------------------
    // 采购退货单
    // -----------------------------------------------------------------------

    /// 分页查询采购退货单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    pub async fn purchase_return_order_list(
        &self,
        params: &PurchaseReturnOrderListParams,
    ) -> Result<PageView<PurchaseReturnOrderView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = PurchaseReturnOrderFilter {
            purchase_return_no: query.purchase_return_no,
            purchase_order_id: query.purchase_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .purchase_return_orders()
            .search_purchase_return_orders(&filter, &mut NoTransaction)
            .await?;
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            views.push(self.purchase_return_order_view(row.id).await?);
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询采购退货单详情（退货单 + 明细行）。
    ///
    /// # 参数
    /// * `id` - 退货单 ID
    ///
    /// # 返回
    /// 返回完整退货单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退货单不存在
    pub async fn purchase_return_order_detail(&self, id: &str) -> Result<PurchaseReturnOrderView> {
        self.purchase_return_order_view(id.to_string()).await
    }

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
        let order_id = PurchaseReturnOrderId::new(next_id());
        let order = PurchaseReturnOrder::new(
            order_id.clone(),
            PurchaseReturnOrderData {
                purchase_return_no: req.purchase_return_no,
                purchase_order_id: req.purchase_order_id,
                sales_return_case_id: req.sales_return_case_id,
                return_mode: req.return_mode,
            },
            actor.id(),
        )?;
        let line = PurchaseReturnLine::new(
            PurchaseReturnLineId::new(next_id()),
            PurchaseReturnLineData {
                purchase_return_order_id: order_id.clone(),
                purchase_order_revision_line_id: req.lines[0].purchase_order_revision_line_id.clone(),
                return_quantity: req.lines[0].return_quantity,
                warehouse_id: req.lines[0].warehouse_id.clone(),
            },
        )?;
        persist_created_purchase_return_order(&self.db, &self.rbac, order, line, actor.clone()).await?;
        self.purchase_return_order_detail(&order_id).await
    }

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配采购退货单视图。
    ///
    /// # 参数
    /// * `id` - 退货单 ID
    ///
    /// # 返回
    /// 返回完整退货单视图。
    ///
    /// # 错误
    /// * `NotFound` - 退货单不存在
    async fn purchase_return_order_view(&self, id: String) -> Result<PurchaseReturnOrderView> {
        let order = self
            .db
            .purchase_return_orders()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购退货单不存在".to_string()))?;
        let lines = self
            .db
            .purchase_return_lines()
            .find_lines_by_orders(&[order.base.id.clone().into()], &mut NoTransaction)
            .await?
            .into_iter()
            .map(|line| crate::returns::dto::PurchaseReturnLineView {
                id: line.base.id.clone(),
                purchase_order_revision_line_id: line.purchase_order_revision_line_id.to_string(),
                return_quantity: line.return_quantity,
                warehouse_id: line.warehouse_id.map(|id| id.to_string()),
            })
            .collect();
        Ok(PurchaseReturnOrderView {
            id: order.base.id.clone(),
            purchase_return_no: order.purchase_return_no,
            purchase_order_id: order.purchase_order_id.to_string(),
            sales_return_case_id: order.sales_return_case_id.map(|id| id.to_string()),
            return_mode: order.return_mode,
            status: order.stable.status(),
            version: order.base.version,
            created_at: order.base.created_at,
            lines,
        })
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
        }
        DocumentApprovalPolicy::ProcessRequired(_) => Err(Error::Internal(
            "采购退货必须是 NO_APPROVAL，不得绑定流程".to_string(),
        )),
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
        return Err(Error::ValidationError(
            "采购退货缺少原采购单，无法构造绑定上下文".to_string(),
        ));
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
        context: BindingRevalidationContext {
            organization_id: purchase_return_binding_organization_id(order)?,
            creator_id: creator_id.to_string(),
        },
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
        return Err(Error::Internal(
            "采购退货为 NO_APPROVAL，不得写入审批绑定".to_string(),
        ));
    }
    if document.approval_binding.is_some() {
        return Err(Error::Internal("采购退货注册行不得预置审批绑定".to_string()));
    }
    if document.document_type != DocumentType::PurchaseReturnOrder {
        return Err(Error::Internal(
            "采购退货创建只能注册 PurchaseReturnOrder 单据".to_string(),
        ));
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
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = ensure_purchase_return_skips_approval_binding()?;
    ensure_purchase_return_has_no_adapter()?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, executor).await?;
    apply_purchase_return_create_binding(&mut document, binding)?;
    persist_registered_document(db, &document, executor).await
}

/// 为已构造采购退货登记 `BusinessDocument` 并调用统一绑定端口。
///
/// # 错误
/// 绑定端口或注册写入失败时返回错误。
async fn register_created_purchase_return_document(
    db: &Database,
    rbac: &SharedRbacService,
    order: &PurchaseReturnOrder,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let bind_command = purchase_return_bind_command(order, actor.id())?;
    let document = new_registered_document(
        &order.base.id,
        DocumentType::PurchaseReturnOrder,
        order.purchase_return_no.clone(),
    )?;
    persist_unbound_purchase_return_document(db, rbac, document, &bind_command, actor, executor).await
}

/// 在创建事务内写入采购退货草稿并登记无绑定单据。
///
/// # 错误
/// 绑定、注册或退货单写入失败时返回错误，调用方必须视作整体回滚。
async fn persist_created_purchase_return_order(
    db: &Database,
    rbac: &SharedRbacService,
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
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                register_created_purchase_return_document(&db, &rbac, &order, &actor, session).await?;
                db.returns()
                    .create_purchase_return_with_line(&order, &line, session)
                    .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

#[cfg(test)]
mod purchase_return_no_approval_tests {
    use super::{
        apply_purchase_return_create_binding, ensure_purchase_return_has_no_adapter,
        ensure_purchase_return_skips_approval_binding, policy_of, purchase_return_bind_command,
        purchase_return_create_binding_decision, BindingDecision, DocumentApprovalPolicy, DocumentType,
        PurchaseReturnOrder, PurchaseReturnOrderData,
    };
    use crate::approval::binding::binding_from_published;
    use crate::document_registry::new_registered_document;
    use bpm::ids::ApprovalProcessDefinitionId;
    use bpm::ProcessKind;
    use entities::common::time::Instant;
    use entities::ids::{PurchaseOrderId, PurchaseReturnOrderId};
    use entities::returns::ReturnMode;

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

        let forged = binding_from_published(
            ApprovalProcessDefinitionId::new("def-1"),
            1,
            Instant::from_unix_secs(10),
        )
        .expect("测试绑定");
        assert!(apply_purchase_return_create_binding(&mut document, Some(forged)).is_err());
    }

    /// 创建路径调用统一绑定端口，不查询发布定义、不启动实例、不建任务。
    #[test]
    fn create_does_not_query_definition_or_start_instance() {
        let production = include_str!("purchase_return.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("persist_created_purchase_return_order"));
        assert!(production.contains("register_created_purchase_return_document"));
        assert!(production.contains("persist_unbound_purchase_return_document"));
        assert!(production.contains("bind_published_definition_on_document_create"));
        assert!(production.contains("DocumentType::PurchaseReturnOrder"));
        assert!(production.contains("new_registered_document"));
        assert!(production.contains("ensure_purchase_return_skips_approval_binding"));
        assert!(production.contains("ensure_purchase_return_has_no_adapter"));
        assert!(!production.contains("pub async fn submit_purchase_return"));
        assert!(!production.contains("start_purchase_return_approval"));
        assert!(!production.contains("PurchaseReturnOrderAdapter"));
        assert!(!production.contains("load_published_graph"));
        let create = production
            .split("pub async fn create_purchase_return_order")
            .nth(1)
            .and_then(|rest| rest.split("async fn purchase_return_order_view").next())
            .expect("create_purchase_return_order 生产片段");
        assert!(create.contains("persist_created_purchase_return_order"));
        assert!(!create.contains("prepare_start"));
        assert!(!create.contains("attach_published_binding"));
        assert!(!create.contains("WorkItem"));
        assert!(!create.contains("start_approval"));
    }
}
