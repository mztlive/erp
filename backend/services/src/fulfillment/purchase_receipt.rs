use database::{AccessControlExt, Executor, FulfillmentExt, NoTransaction, Transactional};
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::{BusinessDocument, DocumentType};
use entities::fulfillment::{
    PurchaseReceipt, PurchaseReceiptData, PurchaseReceiptLine, PurchaseReceiptLineBatch,
};
use entities::ids::PurchaseReceiptId;
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
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

use super::dto::SortDir;
use super::purchase_receipt_lines::receipt_line_specs;
use super::{
    CreatePurchaseReceiptRequest, FulfillmentService, PageView, PurchaseReceiptDetailView,
    PurchaseReceiptLineView, PurchaseReceiptListParams, PurchaseReceiptView, UpdatePurchaseReceiptRequest,
};

/// 采购入库单列表筛选条件类型（经 `FulfillmentExt` 关联类型跨 crate 可达）。
type PurchaseReceiptFilter = <mongodb::Database as FulfillmentExt>::PurchaseReceiptFilter;

impl FulfillmentService {
    /// 分页查询采购入库单列表（W01 履约任务作业面）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`purchase_order_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.purchase_receipt_list",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "purchase_receipt_list"
        )
    )]
    pub async fn purchase_receipt_list(
        &self,
        params: &PurchaseReceiptListParams,
    ) -> Result<PageView<PurchaseReceiptView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = PurchaseReceiptFilter {
            purchase_order_id: query.purchase_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .purchase_receipts()
            .search_purchase_receipts(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| PurchaseReceiptView {
                id: row.id,
                receipt_no: row.receipt_no,
                purchase_order_id: row.purchase_order_id.to_string(),
                warehouse_id: row.warehouse_id.to_string(),
                status: row.status,
                posted_at: row.posted_at.map(|instant| instant.unix_secs()),
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

    /// 查询采购入库单详情（表头 + 行）。
    ///
    /// # 参数
    /// * `id` - 入库单主键
    ///
    /// # 返回
    /// 返回入库单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 入库单不存在
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.purchase_receipt_detail",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "purchase_receipt_detail"
        )
    )]
    pub async fn purchase_receipt_detail(&self, id: &str) -> Result<PurchaseReceiptDetailView> {
        let receipt = self
            .db
            .purchase_receipts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购入库单不存在".to_string()))?;
        let lines = self
            .db
            .fulfillment()
            .receipt_lines_by_receipt_ids(&[receipt.base.id.clone().into()], &mut NoTransaction)
            .await?;
        Ok(PurchaseReceiptDetailView {
            receipt: receipt.into(),
            lines: lines.into_iter().map(Into::into).collect(),
        })
    }

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
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "purchase_receipt_create"
        )
    )]
    pub async fn create_purchase_receipt(
        &self,
        req: CreatePurchaseReceiptRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReceiptView> {
        req.validate()?;
        let id = PurchaseReceiptId::new(next_id());
        let receipt = PurchaseReceipt::new(
            id.clone(),
            PurchaseReceiptData {
                receipt_no: req.receipt_no,
                purchase_order_id: req.purchase_order_id,
                warehouse_id: req.warehouse_id,
            },
        )?;
        let lines = PurchaseReceiptLineBatch::build(id.clone(), receipt_line_specs(&req.lines))
            .map_err(Error::Logic)?;
        persist_created_purchase_receipt(&self.db, &self.rbac, receipt.clone(), lines, actor.clone()).await?;
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
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "purchase_receipt_update"
        )
    )]
    pub async fn update_purchase_receipt(
        &self,
        id: &str,
        req: UpdatePurchaseReceiptRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReceiptView> {
        req.validate()?;
        let mut receipt = self
            .db
            .purchase_receipts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购入库单不存在".to_string()))?;
        if receipt.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        if req
            .warehouse_id
            .as_ref()
            .is_some_and(|warehouse_id| warehouse_id != &receipt.warehouse_id)
        {
            return Err(Error::ValidationError(
                "采购入库单的目标仓库已冻结，不能在任务生成后变更".to_string(),
            ));
        }
        receipt.update(entities::fulfillment::PurchaseReceiptUpdate {
            warehouse_id: req.warehouse_id.or(Some(receipt.warehouse_id.clone())),
        })?;
        let audit =
            actor
                .clone()
                .resource_log("purchase_receipt.update", "purchase_receipt", id.to_string())?;
        let db = self.db.clone();
        let actor_id = actor.id().to_string();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_receipts().update(&mut receipt, session).await?;
                    super::task::record_fulfillment_activity(
                        &db,
                        super::task::FulfillmentTaskObject::PurchaseReceipt(&receipt),
                        &actor_id,
                        session,
                    )
                    .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<PurchaseReceipt, crate::errors::Error>(receipt)
                })
            })
            .await?;
        Ok(updated.into())
    }
}

impl From<PurchaseReceipt> for PurchaseReceiptView {
    /// 从入库单实体构造视图。
    fn from(receipt: PurchaseReceipt) -> Self {
        Self {
            id: receipt.base.id,
            receipt_no: receipt.receipt_no,
            purchase_order_id: receipt.purchase_order_id.to_string(),
            warehouse_id: receipt.warehouse_id.to_string(),
            status: receipt.status,
            posted_at: receipt.posted_at.map(|instant| instant.unix_secs()),
            version: receipt.base.version,
            created_at: receipt.base.created_at,
        }
    }
}

impl From<PurchaseReceiptLine> for PurchaseReceiptLineView {
    /// 从入库行实体构造视图。
    fn from(line: PurchaseReceiptLine) -> Self {
        Self {
            id: line.base.id,
            line_no: line.line_no,
            purchase_order_revision_line_id: line.purchase_order_revision_line_id.to_string(),
            received_quantity: line.received_quantity,
            qualified_quantity: line.qualified_quantity,
            rejected_quantity: line.rejected_quantity,
            quality_result: line.quality_result,
        }
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
        }
        DocumentApprovalPolicy::ProcessRequired(_) => Err(Error::Internal(
            "采购收货必须是 NO_APPROVAL，不得绑定流程".to_string(),
        )),
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
        return Err(Error::ValidationError(
            "采购收货缺少入库仓，无法构造绑定上下文".to_string(),
        ));
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
        context: BindingRevalidationContext {
            organization_id: purchase_receipt_binding_organization_id(receipt)?,
            creator_id: creator_id.to_string(),
        },
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
        return Err(Error::Internal(
            "采购收货为 NO_APPROVAL，不得写入审批绑定".to_string(),
        ));
    }
    if document.approval_binding.is_some() {
        return Err(Error::Internal("采购收货注册行不得预置审批绑定".to_string()));
    }
    if document.document_type != DocumentType::PurchaseReceipt {
        return Err(Error::Internal(
            "采购收货创建只能注册 PurchaseReceipt 单据".to_string(),
        ));
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
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = ensure_purchase_receipt_skips_approval_binding()?;
    ensure_purchase_receipt_has_no_adapter()?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, executor).await?;
    apply_purchase_receipt_create_binding(&mut document, binding)?;
    persist_registered_document(db, &document, executor).await
}

/// 为已构造采购收货登记 `BusinessDocument` 并调用统一绑定端口。
///
/// # 错误
/// 绑定端口或注册写入失败时返回错误。
async fn register_created_purchase_receipt_document(
    db: &Database,
    rbac: &SharedRbacService,
    receipt: &PurchaseReceipt,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let bind_command = purchase_receipt_bind_command(receipt, actor.id())?;
    let document = new_registered_document(
        &receipt.base.id,
        DocumentType::PurchaseReceipt,
        receipt.receipt_no.clone(),
    )?;
    persist_unbound_purchase_receipt_document(db, rbac, document, &bind_command, actor, executor).await
}

/// 在创建事务内写入采购收货草稿并登记无绑定单据。
///
/// # 错误
/// 绑定、注册或入库单写入失败时返回错误，调用方必须视作整体回滚。
async fn persist_created_purchase_receipt(
    db: &Database,
    rbac: &SharedRbacService,
    receipt: PurchaseReceipt,
    lines: Vec<PurchaseReceiptLine>,
    actor: AuditActor,
) -> Result<()> {
    let audit = actor.clone().resource_log(
        "purchase_receipt.create",
        "purchase_receipt",
        receipt.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                register_created_purchase_receipt_document(&db, &rbac, &receipt, &actor, session).await?;
                db.fulfillment()
                    .create_purchase_receipt_with_lines(&receipt, &lines, session)
                    .await?;
                super::task::ensure_fulfillment_task(
                    &db,
                    super::task::FulfillmentTaskObject::PurchaseReceipt(&receipt),
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
    use super::receipt_line_specs;
    use crate::fulfillment::PurchaseReceiptLineInput;
    use entities::fulfillment::{PurchaseReceiptLineBatch, PurchaseReceiptLineData, QualityResult};
    use entities::ids::{PurchaseOrderRevisionLineId, PurchaseReceiptId};
    use entities::money::Quantity;
    use std::str::FromStr;

    fn passed_line() -> PurchaseReceiptLineInput {
        PurchaseReceiptLineInput {
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("porl-1"),
            received_quantity: Quantity::from_str("10").unwrap(),
            qualified_quantity: Quantity::from_str("10").unwrap(),
            rejected_quantity: Quantity::from_str("0").unwrap(),
        }
    }

    #[test]
    fn quality_result_is_derived_from_quantities() {
        let passed = passed_line();
        assert_eq!(
            QualityResult::from_quantities(passed.qualified_quantity, passed.rejected_quantity),
            QualityResult::Passed
        );
        let rejected = PurchaseReceiptLineInput {
            qualified_quantity: Quantity::from_str("0").unwrap(),
            rejected_quantity: Quantity::from_str("10").unwrap(),
            ..passed_line()
        };
        assert_eq!(
            QualityResult::from_quantities(rejected.qualified_quantity, rejected.rejected_quantity),
            QualityResult::Rejected
        );
        let partial = PurchaseReceiptLineInput {
            qualified_quantity: Quantity::from_str("9").unwrap(),
            rejected_quantity: Quantity::from_str("1").unwrap(),
            ..passed_line()
        };
        assert_eq!(
            QualityResult::from_quantities(partial.qualified_quantity, partial.rejected_quantity),
            QualityResult::Partial
        );
    }

    #[test]
    fn receipt_lines_are_built_with_incrementing_line_no_and_validation() {
        let lines = PurchaseReceiptLineBatch::build(
            PurchaseReceiptId::new("r-1"),
            receipt_line_specs(&[
                passed_line(),
                PurchaseReceiptLineInput {
                    purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("porl-2"),
                    received_quantity: Quantity::from_str("5").unwrap(),
                    qualified_quantity: Quantity::from_str("5").unwrap(),
                    rejected_quantity: Quantity::from_str("0").unwrap(),
                },
            ]),
        )
        .unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].line_no, 1);
        assert_eq!(lines[1].line_no, 2);
        let over_sum = PurchaseReceiptLineInput {
            qualified_quantity: Quantity::from_str("9.5").unwrap(),
            rejected_quantity: Quantity::from_str("1").unwrap(),
            ..passed_line()
        };
        assert!(PurchaseReceiptLineBatch::build(
            PurchaseReceiptId::new("r-2"),
            receipt_line_specs(&[over_sum])
        )
        .is_err());
        let _ = PurchaseReceiptLineData {
            purchase_receipt_id: PurchaseReceiptId::new("r-1"),
            line_no: 1,
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("porl-1"),
            received_quantity: Quantity::from_str("10").unwrap(),
            qualified_quantity: Quantity::from_str("9").unwrap(),
            rejected_quantity: Quantity::from_str("1").unwrap(),
            quality_result: QualityResult::Partial,
        };
    }

    /// 创建路径经实体批量工厂派生质量：旧 Service helper 已删除。
    #[test]
    fn receipt_create_uses_entity_batch_factory() {
        let production = include_str!("purchase_receipt.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(
            !production.contains("fn build_receipt_lines"),
            "旧 helper 必须删除"
        );
        assert!(
            production.contains("PurchaseReceiptLineBatch::build"),
            "创建路径必须调用实体工厂"
        );
        assert!(
            !production.contains("QualityResult::from_quantities"),
            "质量派生不得留在 Service"
        );
    }
}

#[cfg(test)]
mod purchase_receipt_no_approval_tests {
    use super::{
        apply_purchase_receipt_create_binding, ensure_purchase_receipt_has_no_adapter,
        ensure_purchase_receipt_skips_approval_binding, policy_of, purchase_receipt_bind_command,
        purchase_receipt_create_binding_decision, BindingDecision, DocumentApprovalPolicy, DocumentType,
        PurchaseReceipt, PurchaseReceiptData,
    };
    use crate::approval::binding::binding_from_published;
    use crate::document_registry::new_registered_document;
    use bpm::ids::ApprovalProcessDefinitionId;
    use bpm::ProcessKind;
    use entities::common::time::Instant;
    use entities::ids::{PurchaseOrderId, PurchaseReceiptId, WarehouseId};

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

        let forged = binding_from_published(
            ApprovalProcessDefinitionId::new("def-1"),
            1,
            Instant::from_unix_secs(10),
        )
        .expect("测试绑定");
        assert!(apply_purchase_receipt_create_binding(&mut document, Some(forged)).is_err());
    }

    /// 创建路径调用统一绑定端口，不查询发布定义、不启动实例、不建任务。
    #[test]
    fn create_does_not_query_definition_or_start_instance() {
        let production = include_str!("purchase_receipt.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("persist_created_purchase_receipt"));
        assert!(production.contains("register_created_purchase_receipt_document"));
        assert!(production.contains("persist_unbound_purchase_receipt_document"));
        assert!(production.contains("bind_published_definition_on_document_create"));
        assert!(production.contains("DocumentType::PurchaseReceipt"));
        assert!(production.contains("new_registered_document"));
        assert!(production.contains("ensure_purchase_receipt_skips_approval_binding"));
        assert!(production.contains("ensure_purchase_receipt_has_no_adapter"));
        assert!(!production.contains("pub async fn submit_purchase_receipt"));
        assert!(!production.contains("start_purchase_receipt_approval"));
        assert!(!production.contains("PurchaseReceiptAdapter"));
        assert!(!production.contains("load_published_graph"));
        let create = production
            .split("pub async fn create_purchase_receipt")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn update_purchase_receipt").next())
            .expect("create_purchase_receipt 生产片段");
        assert!(create.contains("persist_created_purchase_receipt"));
        assert!(!create.contains("prepare_start"));
        assert!(!create.contains("attach_published_binding"));
        assert!(!create.contains("WorkItem"));
        assert!(!create.contains("start_approval"));
    }
}
