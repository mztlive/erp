use erp_audit::AuditExt;
use erp_core::ids::{PurchaseOrderId, PurchaseOrderSubmissionId, SalesOrderId, WarehouseId};
use erp_procurement::entity::purchase_order::{
    basis_id_for, BasisGroup, CreationBasisFacts, FulfillmentResponsibility, LegacyReceiptIdScheme,
    PurchaseCommandReceipt, PurchaseCommandReceiptError, PurchaseOrder, PurchaseOrderData,
    PurchaseOrderSubmission, PurchaseOrderSubmissionLine, RequestedLine,
};
use erp_sales::entity::sales_order::SalesOrder;
use erp_warehouse::WarehouseExt;
use erp_warehouse::WarehouseFulfillmentOperation;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::DocumentRegistryExt;
use id_generator::next_id;
use mongodb::ClientSession;
use persistence_core::{Executor, NoTransaction};
use serde::{Deserialize, Serialize};
use validator::Validate;
use {erp_procurement::repository::PurchaseOrderExt, erp_sales::repository::SalesOrderExt};

use super::super::adapter::{purchase_order_object_readable, purchase_order_responsible_org_id};
use super::super::authorization::{ensure_purchase_order_actor_account, PurchaseOrderAuthorization};
use super::super::create_submit::submit_created_draft_in_session;
use super::super::procurement_task_sync::{
    load_owned_open_procurement_task, sync_procurement_tasks_for_sales_order,
};
use erp_procurement::dto::purchase_order::{
    CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderResult, CREATE_ACTION,
};

use super::super::PurchaseOrderProcess;
use super::{procurement_quantity_changed, validate_requested_quantities};
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_identity::SharedRbacService;
use erp_procurement::service::purchase_order::creation_basis::{
    build_draft_submission, build_submission_line, compute_selected_lines, ensure_request_scope,
    find_requested_group, parse_basis_sales_order_id, SelectedLine,
};
use erp_read_models::purchase_center::repository::{
    basis_groups_and_facts, basis_groups_for_order, load_effective_sales_order, sales_order_basis_fact,
};
use erp_workflow::service::approval::binding::{attach_published_binding, BindPublishedDefinitionCommand};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::document_registry::new_registered_document;
use services::{Error, Result};

const CREATE_PERMISSION: &str = "purchase_order:create";
const CREATE_RECEIPT_PREFIX: &str = "purchase-order-create-command-";

/// 幂等命令收据载荷。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CreationReceipt {
    /// 采购单主键。
    purchase_order_id: String,
    /// 采购单号。
    purchase_no: String,
    /// 创建完成时乐观锁版本。
    lock_version: u64,
}

/// 事务内采购创建命令上下文。
pub struct CreateBasisCommand<'a> {
    /// 来源销售单。
    pub sales_order_id: &'a SalesOrderId,
    /// 原始创建请求。
    pub req: &'a CreatePurchaseOrderFromBasisRequest,
    /// 已规范化逐行数量。
    pub requested_lines: &'a [RequestedLine],
    /// 稳定命令收据 ID。
    pub audit_id: &'a str,
    /// 命令载荷指纹。
    pub request_fingerprint: &'a str,
    /// 审计操作人。
    pub actor: &'a AuditActor,
}

/// 待一次性持久化的采购草稿聚合。
struct PreparedDraftWrite<'a> {
    /// 来源销售单。
    sales_order: &'a SalesOrder,
    /// 新采购单。
    order: &'a PurchaseOrder,
    /// 当前草稿提交。
    submission: &'a PurchaseOrderSubmission,
    /// 当前草稿提交行。
    lines: &'a [PurchaseOrderSubmissionLine],
    /// 审计操作人。
    actor: &'a AuditActor,
}

impl PurchaseOrderProcess {
    /// 依据精确拆分维度和逐行本次数量创建一张采购单并提交审批。
    ///
    /// # 参数
    /// * `req` - 精确依据、逐行数量与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回已提交审批的采购单；同一幂等键与同一载荷重复提交时返回原结果。
    ///
    /// # 错误
    /// 操作账号不可登录或缺少采购创建权限、依据失效、数量非正或超过事务内最新
    /// 剩余/可供量、幂等键载荷冲突、并发冲突、审批绑定、启动审批或仓储写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 操作人授权版本通过 policy CAS 与提交绑定；事务内再以销售单 CAS guard 串行化并重算剩余量。
    /// 创建成功即进入审批中，不得留下可编辑草稿。
    pub async fn create_from_basis(
        &self,
        req: CreatePurchaseOrderFromBasisRequest,
        actor: &AuditActor,
    ) -> Result<CreatePurchaseOrderResult> {
        req.validate()?;
        let requested_lines = req.normalized_lines()?;
        let request_fingerprint = req.request_fingerprint(&requested_lines);
        let receipt_identity = PurchaseCommandReceipt::<CreationReceipt>::identity(
            CREATE_RECEIPT_PREFIX,
            actor.id(),
            CREATE_ACTION,
            None,
            &req.idempotency_key,
            LegacyReceiptIdScheme::None,
        )?;
        let audit_id = receipt_identity.receipt_id().to_string();
        let PurchaseOrderAuthorization {
            rbac,
            policy_revision,
        } = self.authorize_actor_permission(actor, CREATE_PERMISSION).await?;
        if let Some(result) = replay_creation(
            &self.db,
            &audit_id,
            &request_fingerprint,
            actor,
            &mut NoTransaction,
        )
        .await?
        {
            return Ok(result);
        }
        let sales_order_id = parse_basis_sales_order_id(&req.basis_id)?;
        let db = self.db.clone();
        let binding_rbac = rbac.clone();
        let object_read = std::sync::Arc::clone(&self.object_read);
        let transaction_actor = actor.clone();
        let transaction_req = req.clone();
        let transaction_fingerprint = request_fingerprint.clone();
        let transaction_audit_id = audit_id.clone();
        let transaction_result = rbac
            .run_authorized_policy_transaction(policy_revision, move |session| {
                Box::pin(async move {
                    ensure_purchase_order_actor_account(&db, &transaction_actor, session).await?;
                    let command = CreateBasisCommand {
                        sales_order_id: &sales_order_id,
                        req: &transaction_req,
                        requested_lines: &requested_lines,
                        audit_id: &transaction_audit_id,
                        request_fingerprint: &transaction_fingerprint,
                        actor: &transaction_actor,
                    };
                    create_from_basis_in_transaction(
                        &db,
                        &binding_rbac,
                        object_read.as_ref(),
                        &command,
                        session,
                    )
                    .await
                })
            })
            .await;
        match transaction_result {
            Ok(result) => Ok(result),
            Err(error) => replay_creation(
                &self.db,
                &audit_id,
                &request_fingerprint,
                actor,
                &mut NoTransaction,
            )
            .await?
            .ok_or(error),
        }
    }
}

/// 在 MongoDB 事务内串行化、重算并写入一张采购单后立即提交审批。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 审批绑定授权源
/// * `command` - 来源销售单、请求、幂等收据与审计操作人
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 返回本次创建或事务内命中的幂等结果。
///
/// # 错误
/// 依据、数量、并发 guard、审批绑定或持久化失败时返回错误。
///
/// # 关键业务约束
/// guard CAS 成功后必须再次按采购当前指针计算剩余量，并复用同一事务事实。
async fn create_from_basis_in_transaction(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    command: &CreateBasisCommand<'_>,
    session: &mut ClientSession,
) -> Result<CreatePurchaseOrderResult> {
    if let Some(result) = replay_creation(
        db,
        command.audit_id,
        command.request_fingerprint,
        command.actor,
        session,
    )
    .await?
    {
        return Ok(result);
    }
    let task = load_owned_open_procurement_task(
        db,
        &command.req.work_item_id,
        command.sales_order_id,
        command.actor.id(),
        session,
    )
    .await?;
    let mut order = load_effective_sales_order(db, command.sales_order_id, session).await?;
    let groups = basis_groups_for_order(db, &order, task.responsibility_scope_ids(), session).await?;
    let selected = find_requested_group(
        &sales_order_basis_fact(&order),
        &groups,
        &command.req.basis_id,
        &command.req.work_item_id,
    )?
    .clone();
    ensure_request_scope(command.req, &selected.scope)?;
    order.advance_procurement_guard(command.actor.id())?;
    db.sales_orders().update(&mut order, session).await?;
    let (latest_groups, latest_facts) =
        basis_groups_and_facts(db, &order, task.responsibility_scope_ids(), session).await?;
    let latest = latest_groups
        .into_iter()
        .find(|group| group.scope == selected.scope)
        .ok_or_else(procurement_quantity_changed)?;
    let selected_lines = validate_requested_quantities(command.requested_lines, &latest)?;
    let input = VerifiedBasisInput {
        sales_order: &order,
        group: &latest,
        selected_lines: &selected_lines,
        facts: &latest_facts,
    };
    persist_basis_draft(db, rbac, object_read, &input, command, session).await
}

/// guard 重算后的事务内创建输入：已完成 CAS 的销售单、最新依据范围、
/// 校验通过的本次采购行与批量加载的供给及供应商结算事实。
///
/// # 参数
/// * `sales_order` - 已完成 guard CAS 的销售单
/// * `group` - guard 后重算得到的最新依据范围
/// * `selected_lines` - 事务内校验通过的本次采购行
/// * `facts` - guard 后重算时批量加载的供给与供应商结算事实
pub struct VerifiedBasisInput<'a> {
    /// 已完成 guard CAS 的销售单。
    pub sales_order: &'a SalesOrder,
    /// guard 后重算得到的最新依据范围。
    pub group: &'a BasisGroup,
    /// 事务内校验通过的本次采购行。
    pub selected_lines: &'a [SelectedLine],
    /// guard 后重算时批量加载的供给与供应商结算事实。
    pub facts: &'a CreationBasisFacts,
}

/// 在事务内持久化一张精确依据采购单并提交审批。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 审批绑定授权源
/// * `input` - guard 重算后的事务内创建输入（销售单、依据范围、本次行与事实）
/// * `command` - 原始请求、命令收据与审计操作人
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 返回已提交审批的创建结果。
///
/// # 错误
/// 实体构造、审批绑定、启动审批或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// `creation_basis_id` 唯一，且本函数只创建一个采购聚合；命令收据记录提交后正式号。
/// 供应商名称快照只从同一事务内批量加载的事实读取，不得再次逐段查询。
pub async fn persist_basis_draft(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    input: &VerifiedBasisInput<'_>,
    command: &CreateBasisCommand<'_>,
    session: &mut ClientSession,
) -> Result<CreatePurchaseOrderResult> {
    let sales_order = input.sales_order;
    let group = input.group;
    let selected_lines = input.selected_lines;
    let facts = input.facts;
    let target_warehouse_id = resolve_target_warehouse(
        db,
        rbac,
        group.scope.fulfillment_responsibility,
        command.req.target_warehouse_id.as_deref(),
        session,
    )
    .await?;
    ensure_initial_purchase_order_owner(
        db,
        rbac,
        group.scope.fulfillment_responsibility,
        command.actor.id(),
        session,
    )
    .await?;
    let creation_basis_id = basis_id_for(
        &sales_order_basis_fact(sales_order),
        group,
        &command.req.work_item_id,
        target_warehouse_id.as_ref(),
    );
    let order_id = PurchaseOrderId::new(next_id());
    let mut order = PurchaseOrder::new(
        order_id.clone(),
        PurchaseOrderData {
            purchase_no: String::new(),
            sales_order_id: SalesOrderId::new(sales_order.base.id.clone()),
            sales_order_revision_id: group.revision.base.id.clone().into(),
            creation_basis_id,
            supplier_id: group.scope.supplier_id.clone(),
            purchase_type: group.scope.purchase_type,
            payment_term_code: group.scope.payment_term_code.clone(),
            fulfillment_responsibility: group.scope.fulfillment_responsibility,
            owner_user_id: command.actor.id().to_string(),
            target_warehouse_id,
        },
        command.actor.id(),
        super::super::adapters::payment_term::parse,
    )?;
    let supplier_name = facts
        .supplier_names
        .get(&group.scope.supplier_id.to_string())
        .cloned()
        .unwrap_or_else(|| group.scope.supplier_id.to_string());
    let computed = compute_selected_lines(selected_lines, group.scope.fulfillment_responsibility);
    let submission = build_draft_submission(
        &super::supplier::CreationBasisSupplierAdapter::new(db.clone()),
        &order_id,
        &group.scope,
        &supplier_name,
        computed.totals,
        session,
    )
    .await?;
    let submission_id = PurchaseOrderSubmissionId::new(submission.base.id.clone());
    let mut submission_lines = Vec::with_capacity(computed.lines.len());
    for (index, line) in computed.lines.iter().enumerate() {
        submission_lines.push(build_submission_line(&submission_id, (index + 1) as u32, line)?);
    }
    order.attach_draft_submission(submission.base.id.clone().into())?;
    let write = PreparedDraftWrite {
        sales_order,
        order: &order,
        submission: &submission,
        lines: &submission_lines,
        actor: command.actor,
    };
    write_prepared_draft(db, rbac, object_read, &write, session).await?;
    let submitted = submit_created_draft_in_session(
        db,
        sales_order,
        &order.base.id,
        command.actor,
        command.req.idempotency_key.as_str(),
        session,
    )
    .await?;
    write_creation_receipt(
        db,
        command,
        &order.base.id,
        submitted.purchase_no,
        submitted.lock_version,
        session,
    )
    .await
}

/// 写入采购草稿聚合、单据注册和审批绑定。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 审批绑定授权源
/// * `write` - 来源销售单、采购聚合与审计操作人
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 写入成功返回 `Ok(())`。
///
/// # 错误
/// 审批绑定、单据注册或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 本函数只写入草稿聚合；正式号与审批启动由随后的提交步骤完成。
async fn write_prepared_draft(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    write: &PreparedDraftWrite<'_>,
    session: &mut ClientSession,
) -> Result<()> {
    let organization_id = purchase_order_responsible_org_id(write.sales_order)?;
    let _ = purchase_order_object_readable(&organization_id, write.actor.id())?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::PurchaseOrder,
        business_object_id: write.order.base.id.clone(),
        business_object_version: write.order.base.version,
        context: BindingRevalidationContext {
            organization_id,
            creator_id: write.actor.id().to_string(),
        },
    };
    let binding = services::workflow_compose::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        &bind_command,
        write.actor,
        session,
    )
    .await?
    .ok_or_else(|| Error::Internal("采购单必须绑定已发布定义".to_string()))?;
    let mut document = new_registered_document(&write.order.base.id, DocumentType::PurchaseOrder, "")
        .map_err(services::Error::from)?;
    attach_published_binding(&mut document, binding)?;
    db.purchase_orders().create(write.order, session).await?;
    db.business_documents().create(&document, session).await?;
    db.purchase_order_submissions()
        .create(write.submission, session)
        .await?;
    for line in write.lines {
        db.purchase_order_submission_lines().create(line, session).await?;
    }
    sync_procurement_tasks_for_sales_order(db, &write.order.sales_order_id, session).await?;
    Ok(())
}

/// 写入提交后的采购创建命令收据。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `command` - 原始请求、收据身份与审计操作人
/// * `purchase_order_id` - 采购单主键
/// * `purchase_no` - 提交后正式号
/// * `lock_version` - 提交后乐观锁版本
/// * `session` - MongoDB 事务会话
///
/// # 返回
/// 返回可回放的创建结果。
///
/// # 错误
/// 收据序列化或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 收据必须与提交后正式号同事务落库，回放不得返回空单号。
async fn write_creation_receipt(
    db: &mongodb::Database,
    command: &CreateBasisCommand<'_>,
    purchase_order_id: &str,
    purchase_no: String,
    lock_version: u64,
    session: &mut ClientSession,
) -> Result<CreatePurchaseOrderResult> {
    let receipt = CreationReceipt {
        purchase_order_id: purchase_order_id.to_string(),
        purchase_no,
        lock_version,
    };
    let audit = command.actor.clone().resource_log_with_id(
        command.audit_id.to_string(),
        CREATE_ACTION,
        "purchase_order",
        purchase_order_id.to_string(),
        Some(
            PurchaseCommandReceipt::new(command.request_fingerprint.to_string(), receipt.clone())
                .encode_message()?,
        ),
    )?;
    db.audit_logs().create(&audit, session).await?;
    Ok(receipt.into_result(false))
}

/// 校验采购单初始责任人可以完成其责任类型对应的后续履约操作。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 与采购创建事务授权版本一致的 RBAC 服务
/// * `responsibility` - 本单履约责任
/// * `owner_user_id` - 创建后冻结为采购单责任人的账号
/// * `executor` - 当前事务执行器
///
/// # 返回
/// 入仓责任或责任人具备完整履约权限时返回成功。
///
/// # 错误
/// 责任人账号不可用、缺少对应完整履约权限，或账号与 RBAC 查询失败时返回错误。
async fn ensure_initial_purchase_order_owner(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    responsibility: FulfillmentResponsibility,
    owner_user_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(business_object_type) = responsibility.owner_fulfillment_object_type() else {
        return Ok(());
    };
    crate::fulfillment_execution::task::ensure_fulfillment_owner_eligible(
        db,
        rbac,
        owner_user_id,
        business_object_type,
        executor,
    )
    .await
    .map_err(|error| {
        contextualize_fulfillment_owner_error(
            error,
            "当前采购责任人账号不可用或缺少后续履约权限，请先调整角色后再创建采购单",
        )
    })
}

/// 校验并解析采购单目标收货仓。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 与采购创建事务授权版本一致的 RBAC 服务
/// * `responsibility` - 本单履约责任
/// * `requested_id` - 客户端指定的目标仓库
/// * `executor` - 当前事务执行器
///
/// # 返回
/// 仓库履约返回存在且启用的目标仓库，其他履约返回空。
///
/// # 错误
/// 仓库履约未指定目标仓、仓库不存在或停用、入库经办人不可用或权限不足，
/// 或非仓库履约携带目标仓时返回错误。
async fn resolve_target_warehouse(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    responsibility: FulfillmentResponsibility,
    requested_id: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Option<WarehouseId>> {
    let normalized = requested_id.map(str::trim).filter(|value| !value.is_empty());
    match responsibility {
        FulfillmentResponsibility::Warehouse => {
            let id = normalized
                .map(|value| WarehouseId::new(value.to_string()))
                .ok_or_else(|| Error::ValidationError("仓库履约必须先选择目标收货仓".to_string()))?;
            let warehouse = db
                .warehouses()
                .find_by_id(&id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("目标仓库不存在，请重新选择".to_string()))?;
            if !warehouse.is_active() {
                return Err(Error::ValidationError(
                    "目标仓库已停用，请重新选择后再创建采购单".to_string(),
                ));
            }
            let handler_user_id = warehouse
                .fulfillment_handler(WarehouseFulfillmentOperation::Receipt)
                .map_err(|_| {
                    Error::ValidationError("目标仓库未配置合格入库经办人，请先完成仓库责任配置".to_string())
                })?;
            crate::fulfillment_execution::task::ensure_fulfillment_owner_eligible(
                db,
                rbac,
                handler_user_id,
                "purchase_receipt",
                executor,
            )
            .await
            .map_err(|error| {
                contextualize_fulfillment_owner_error(
                    error,
                    "目标仓库入库经办人账号不可用或权限不足，请先更新仓库责任配置",
                )
            })?;
            Ok(Some(id))
        }
        _ if normalized.is_some() => Err(Error::ValidationError("非仓库履约不能指定目标收货仓".to_string())),
        _ => Ok(None),
    }
}

/// 把责任资格失败转换为当前创建场景可执行的校验提示，同时保留基础设施错误。
fn contextualize_fulfillment_owner_error(error: Error, message: &str) -> Error {
    match error {
        Error::BusinessLogicError(_) => Error::ValidationError(message.to_string()),
        other => other,
    }
}

/// 查询并校验采购创建幂等收据。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `audit_id` - 稳定收据 ID
/// * `expected_fingerprint` - 当前命令载荷指纹
/// * `actor` - 当前操作人
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 收据不存在返回 `None`；存在且一致返回原创建结果并标记回放。
///
/// # 错误
/// 同键异载荷、收据身份不一致、收据损坏或采购单缺失时返回错误。
///
/// # 关键业务约束
/// 事务前、事务内和事务失败后均复用同一校验逻辑。
async fn replay_creation(
    db: &mongodb::Database,
    audit_id: &str,
    expected_fingerprint: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<CreatePurchaseOrderResult>> {
    let Some(audit) = db.audit_logs().find_by_id(audit_id, executor).await? else {
        return Ok(None);
    };
    let receipt = match PurchaseCommandReceipt::<CreationReceipt>::decode(
        &crate::procure_to_pay::adapters::audit::audit_receipt_fact(&audit),
        actor.id(),
        CREATE_ACTION,
        None,
        expected_fingerprint,
    ) {
        Ok(receipt) => receipt,
        Err(PurchaseCommandReceiptError::IdentityMismatch | PurchaseCommandReceiptError::PayloadConflict) => {
            return Err(Error::ConflictError("幂等键已用于不同采购创建命令".to_string()));
        }
        Err(PurchaseCommandReceiptError::Corrupted(message)) => {
            return Err(Error::Internal(message));
        }
    };
    if audit.resource_id.as_deref() != Some(receipt.payload().purchase_order_id.as_str()) {
        return Err(Error::ConflictError(
            "采购创建幂等收据与业务资源不一致".to_string(),
        ));
    }
    let order = db
        .purchase_orders()
        .find_by_id(&receipt.payload().purchase_order_id, executor)
        .await?
        .ok_or_else(|| Error::Internal("采购创建幂等收据引用的采购单不存在".to_string()))?;
    if order.base.id != receipt.payload().purchase_order_id {
        return Err(Error::ConflictError(
            "采购创建幂等收据与当前采购单不一致".to_string(),
        ));
    }
    Ok(Some(receipt.into_payload().into_result(true)))
}

impl CreationReceipt {
    /// 转换为采购创建响应。
    ///
    /// # 参数
    /// * `replayed` - 是否来自幂等收据回放
    ///
    /// # 返回
    /// 返回 API 创建结果。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 业务引用恒为原采购单 ID。
    fn into_result(self, replayed: bool) -> CreatePurchaseOrderResult {
        CreatePurchaseOrderResult {
            purchase_order_id: self.purchase_order_id.clone(),
            purchase_no: self.purchase_no,
            lock_version: self.lock_version,
            replayed,
            reference: self.purchase_order_id,
        }
    }
}
