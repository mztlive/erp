use erp_audit::AuditExt;
use erp_core::common::time::Instant;
use erp_core::ids::{StockAdjustmentId, StockAdjustmentLineId};
use erp_inventory::InventoryExt;
use erp_inventory::{
    AdjustmentReasonType, StockAdjustment, StockAdjustmentData, StockAdjustmentLine, StockAdjustmentLineData,
};
use erp_warehouse::WarehouseExt;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Transactional;
use validator::Validate;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_identity::SharedRbacService;
use erp_workflow::service::approval::binding::{attach_published_binding, BindPublishedDefinitionCommand};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::document_registry::{new_registered_document, persist_registered_document};
use services::{Error, Result};

use super::adapter::document_approval_view_with_history;
use super::approval_prepare as start_approval;
use super::InventoryAdjustmentService;
use crate::adapters::authorize_inventory;
use erp_inventory::{
    CreateStockAdjustmentRequest, DocumentApprovalHistoryPageView, StockAdjustmentDetailView,
    StockAdjustmentLineInput, SubmitStockAdjustmentApprovalTokenView,
};

impl InventoryAdjustmentService {
    /// 创建库存调整单（草稿，跨集合：表头 + 明细 + 绑定 + 审计）。
    ///
    /// 同一事务内注册 `BusinessDocument` 并调用统一绑定端口。无已发布定义
    /// 时整体失败关闭，不得留下以后补流程的单据。
    ///
    /// # 参数
    /// * `req` - 创建请求（表头 + 明细）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建调整单的完整详情视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 调整单号重复或流程未配置
    /// * `RepositoryError` - 数据库写入失败
    #[tracing::instrument(
        name = "inventory.stock_adjustment_create",
        skip_all,
        fields(
            layer = "service",
            domain = "inventory",
            operation = "stock_adjustment_create"
        )
    )]
    pub async fn create_stock_adjustment(
        &self,
        req: CreateStockAdjustmentRequest,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentDetailView> {
        req.validate()?;
        let balance_id = req.balance_id.clone();
        let expected_balance_version = req.expected_balance_version;
        let id = StockAdjustmentId::new(next_id());
        let adjustment = StockAdjustment::new(
            id.clone(),
            StockAdjustmentData {
                adjustment_no: req.adjustment_no,
                warehouse_id: req.warehouse_id,
                reason_type: req.reason_type,
                prepared_by: actor.id().to_string(),
                note: req.note,
                occurred_at: req.occurred_at.map(Instant::from_unix_secs),
            },
            actor.id(),
        )?;
        let lines = build_adjustment_lines(&id, adjustment.reason_type, &req.lines)?;
        let audit =
            actor
                .clone()
                .resource_log("stock_adjustment.create", "stock_adjustment", id.to_string())?;
        let document = new_registered_document(
            &id,
            DocumentType::StockAdjustment,
            adjustment.adjustment_no.clone(),
        )
        .map_err(services::Error::from)?;
        let bind_command = BindPublishedDefinitionCommand {
            document_type: DocumentType::StockAdjustment,
            business_object_id: id.to_string(),
            business_object_version: adjustment.base.version,
            context: BindingRevalidationContext {
                organization_id: adjustment.warehouse_id.to_string(),
                creator_id: actor.id().to_string(),
            },
        };
        persist_created_adjustment(
            &self.db,
            &self.rbac,
            std::sync::Arc::clone(&self.object_read),
            CreatedAdjustmentPersist {
                adjustment: adjustment.clone(),
                lines,
                document,
                bind_command,
                audit,
                actor: actor.clone(),
                balance_id,
                expected_balance_version,
            },
        )
        .await
    }
}

/// 库存调整单创建事务的单据、绑定与审计载荷。
///
/// # 用途
/// 将调整单、明细、单据注册、绑定命令与审计打包后一次写入。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 绑定失败必须回滚业务实体，不得留下以后补流程的单据。
struct CreatedAdjustmentPersist {
    /// 已构造的调整单。
    adjustment: StockAdjustment,
    /// 调整明细。
    lines: Vec<StockAdjustmentLine>,
    /// 待登记单据。
    document: BusinessDocument,
    /// 发布定义绑定命令。
    bind_command: BindPublishedDefinitionCommand,
    /// 已构造审计。
    audit: erp_audit::AuditLog,
    /// 审计操作人。
    actor: AuditActor,
    /// 用户发起时所依据的库存余额行。
    balance_id: String,
    /// 用户发起时看到的库存余额版本。
    expected_balance_version: u64,
}

/// 在创建事务内写入调整单、绑定发布定义并登记单据。
///
/// 绑定失败必须回滚业务实体，不得留下以后补流程的单据。
///
/// # 用途
/// 在同一事务内写入调整单、绑定与审计。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `persist` - 调整单、单据绑定与审计
///
/// # 返回
/// 写入成功时返回 `Ok(())`。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误。
///
/// # 关键业务约束
/// 绑定失败必须整体回滚。
async fn persist_created_adjustment(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    persist: CreatedAdjustmentPersist,
) -> Result<StockAdjustmentDetailView> {
    let CreatedAdjustmentPersist {
        adjustment,
        lines,
        mut document,
        bind_command,
        audit,
        actor,
        balance_id,
        expected_balance_version,
    } = persist;
    let db = db.clone();
    let rbac = rbac.clone();
    let object_read = object_read.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                let authorization = authorize_inventory(&db, &rbac, &actor, session).await?;
                if !authorization.actor_is_active()
                    || !authorization.can_create(adjustment.warehouse_id.as_ref())
                {
                    return Err(Error::Forbidden("无权在该仓库创建库存调整单".to_string()));
                }
                db.warehouse()
                    .warehouse(adjustment.warehouse_id.as_ref(), session)
                    .await?
                    .ok_or_else(|| Error::NotFound("仓库不存在".to_string()))?;
                let balance = db
                    .inventory()
                    .stock_balance(&balance_id, session)
                    .await?
                    .ok_or_else(|| Error::NotFound("库存余额不存在".to_string()))?;
                if balance.warehouse_id != adjustment.warehouse_id {
                    return Err(Error::NotFound("库存余额不存在".to_string()));
                }
                if !balance.matches_version(expected_balance_version) {
                    return Err(Error::ConflictError("库存余额已变化，请刷新后重试".to_string()));
                }
                if !balance.matches_adjustment_dimensions(&adjustment, &lines) {
                    return Err(Error::ValidationError("库存余额与调整单维度不一致".to_string()));
                }
                let can_submit =
                    match start_approval::ensure_stock_adjustment_submit_authorized_with_executor(
                        &db,
                        &rbac,
                        &adjustment,
                        &actor,
                        session,
                    )
                    .await
                    {
                        Ok(()) => true,
                        Err(Error::Forbidden(_)) => false,
                        Err(error) => return Err(error),
                    };
                db.inventory()
                    .create_stock_adjustment_with_lines(&adjustment, &lines, session)
                    .await?;
                let binding = persist_bound_document(
                    &db,
                    &rbac,
                    object_read.as_ref(),
                    &mut document,
                    &bind_command,
                    &actor,
                    session,
                )
                .await?;
                db.audit_logs().create(&audit, session).await?;
                created_adjustment_detail(adjustment, lines, binding, can_submit)
            })
        })
        .await
}

/// 使用创建事务已持有的事实构造成功响应，避免提交后再次授权或查询失败。
fn created_adjustment_detail(
    adjustment: StockAdjustment,
    lines: Vec<StockAdjustmentLine>,
    binding: ApprovalDefinitionBinding,
    can_submit: bool,
) -> Result<StockAdjustmentDetailView> {
    let submit_command = if can_submit {
        let expected_subject_version = adjustment
            .approval_subject_version
            .checked_add(1)
            .ok_or_else(|| Error::ConflictError("库存调整审批主题版本已达上限".to_string()))?;
        Some(SubmitStockAdjustmentApprovalTokenView {
            expected_version: adjustment.base.version.to_string(),
            expected_subject_version: expected_subject_version.to_string(),
        })
    } else {
        None
    };
    let approval = document_approval_view_with_history(
        Some(&binding),
        None,
        Vec::new(),
        DocumentApprovalHistoryPageView {
            next_cursor: None,
            has_more: false,
        },
        adjustment.status,
        submit_command,
        None,
    );
    Ok(StockAdjustmentDetailView {
        adjustment: adjustment.into(),
        lines: lines.into_iter().map(Into::into).collect(),
        posted_movements: Vec::new(),
        approval,
    })
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误，调用方必须回滚。
async fn persist_bound_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    document: &mut BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<ApprovalDefinitionBinding> {
    let binding = services::workflow_compose::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        bind_command,
        actor,
        session,
    )
    .await?;
    let binding = binding.ok_or_else(|| Error::Internal("库存调整单必须绑定已发布定义".to_string()))?;
    attach_published_binding(document, binding.clone())?;
    persist_registered_document(db, document, session)
        .await
        .map_err(services::Error::from)?;
    Ok(binding)
}

/// 构建调整明细实体集合（明细主键逐条生成）。
///
/// # 参数
/// * `adjustment_id` - 调整单主键
/// * `reason_type` - 调整原因，用于校验明细方向
/// * `inputs` - 明细输入
///
/// # 返回
/// 返回明细实体集合。
///
/// # 错误
/// 明细数量非正时返回错误（实体构造）。
fn build_adjustment_lines(
    adjustment_id: &StockAdjustmentId,
    reason_type: AdjustmentReasonType,
    inputs: &[StockAdjustmentLineInput],
) -> Result<Vec<StockAdjustmentLine>> {
    let mut lines = Vec::with_capacity(inputs.len());
    for input in inputs {
        lines.push(
            StockAdjustmentLine::new_for_reason(
                StockAdjustmentLineId::new(next_id()),
                reason_type,
                StockAdjustmentLineData {
                    stock_adjustment_id: adjustment_id.clone(),
                    sku_id: input.sku_id.clone(),
                    quantity: input.quantity,
                    direction: input.direction,
                },
            )
            .map_err(Error::Logic)?,
        );
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::{build_adjustment_lines, StockAdjustmentLineInput};
    use erp_core::ids::{SkuId, StockAdjustmentId};
    use erp_core::money::Quantity;
    use erp_inventory::{AdjustmentReasonType, MovementDirection};
    use std::str::FromStr;

    #[test]
    fn adjustment_lines_are_built_with_entity_validation() {
        let lines = build_adjustment_lines(
            &StockAdjustmentId::new("adj-1"),
            AdjustmentReasonType::StockGain,
            &[StockAdjustmentLineInput {
                sku_id: SkuId::new("sku-1"),
                quantity: Quantity::from_str("2").unwrap(),
                direction: MovementDirection::Increase,
            }],
        )
        .unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].quantity, Quantity::from_str("2").unwrap());

        let invalid_quantity = build_adjustment_lines(
            &StockAdjustmentId::new("adj-2"),
            AdjustmentReasonType::StockGain,
            &[StockAdjustmentLineInput {
                sku_id: SkuId::new("sku-1"),
                quantity: Quantity::from_str("0").unwrap(),
                direction: MovementDirection::Increase,
            }],
        );
        assert!(invalid_quantity.is_err(), "调整数量必须为正数");

        let invalid_direction = build_adjustment_lines(
            &StockAdjustmentId::new("adj-3"),
            AdjustmentReasonType::StockLoss,
            &[StockAdjustmentLineInput {
                sku_id: SkuId::new("sku-1"),
                quantity: Quantity::from_str("1").unwrap(),
                direction: MovementDirection::Increase,
            }],
        );
        assert!(invalid_direction.is_err(), "盘亏明细必须为减少方向");
    }
}
