use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditLog};
use erp_core::ids::{PurchaseOrderId, PurchaseOrderSubmissionId, SalesOrderId, WarehouseId};
use erp_identity::SharedRbacService;
use erp_procurement::dto::purchase_order::{
    CREATE_ACTION, CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderResult,
};
use erp_procurement::entity::purchase_order::{
    BasisGroup, CreationBasisFacts, CreationReceipt, FulfillmentResponsibility, PurchaseCommandReceipt,
    PurchaseCommandReceiptError, PurchaseCommandReceiptIdentity, PurchaseOrder, PurchaseOrderData,
    PurchaseOrderSubmission, PurchaseOrderSubmissionLine, RequestedLine, basis_id_for,
};
use erp_procurement::repository::{PurchaseCommandExt, PurchaseOrderExt};
use erp_procurement::service::purchase_order::creation_basis::{
    SelectedLine, build_draft_submission, build_submission_line, compute_selected_lines,
    ensure_request_scope, find_requested_group, parse_basis_sales_order_id,
};
use erp_read_models::purchase_center::repository::{
    basis_groups_and_facts, basis_groups_for_order, load_effective_sales_order, sales_order_basis_fact,
};
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::SalesOrderExt;
use erp_warehouse::{WarehouseExt, WarehouseFulfillmentOperation};
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::ports::OrderTaskSource;
use erp_workflow::service::approval::binding::{BindPublishedDefinitionCommand, attach_published_binding};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::document_registry::new_registered_document;
use erp_workflow::{ApprovalObjectReadPort, DocumentRegistryExt};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::super::PurchaseOrderProcess;
use super::super::adapter::{purchase_order_object_readable, purchase_order_responsible_org_id};
use super::super::adapters::payment_term::parse;
use super::super::authorization::{PurchaseOrderAuthorization, ensure_purchase_order_actor_account};
use super::super::create_submit::{CreatedDraftBundle, submit_created_draft};
use super::super::procurement_task_sync::{
    load_owned_open_procurement_task, sync_procurement_tasks_for_sales_order,
};
use super::super::sourcing_create::sequence::PurchaseCreationEventSequence;
use super::supplier::CreationBasisSupplierAdapter;
use super::{procurement_quantity_changed, validate_requested_quantities};
use crate::adapters::purchase_access;
use crate::audit::{persist_log, recover_command};
use crate::business_ownership::required_business_org;
use crate::{Error, Result};

const CREATE_PERMISSION: &str = "purchase_order:create";
const CREATE_RECEIPT_PREFIX: &str = "purchase-order-create-command-";

/// 事务内采购创建命令上下文。
pub struct CreateBasisCommand<'a> {
    /// 来源销售单。
    pub sales_order_id: &'a SalesOrderId,
    /// 原始创建请求。
    pub req: &'a CreatePurchaseOrderFromBasisRequest,
    /// 已规范化逐行数量。
    pub requested_lines: &'a [RequestedLine],
    /// 稳定命令收据 ID。
    pub receipt_identity: &'a PurchaseCommandReceiptIdentity,
    /// 整批审计关联；独立建单时为本命令，选源子建单时为原批次命令。
    pub audit_command_id: &'a str,
    /// 业务首写前已校验的提交与创建事件序号。
    pub(crate) audit_event_sequence: PurchaseCreationEventSequence,
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

/// 完成事务内资格和快照构造、尚未登记的采购草稿聚合。
struct PreparedBasisDraft {
    order: PurchaseOrder,
    submission: PurchaseOrderSubmission,
    lines: Vec<PurchaseOrderSubmissionLine>,
}

/// 原命令入口向授权事务传递的完整输入。
struct BasisTransactionInput<'a> {
    /// 原请求，克隆时机保持在事务准备阶段。
    req: &'a CreatePurchaseOrderFromBasisRequest,
    /// 入口已经规范化的采购行。
    requested_lines: Vec<RequestedLine>,
    /// 已解析的来源销售单。
    sales_order_id: SalesOrderId,
    /// 原命令稳定收据身份。
    receipt_identity: &'a PurchaseCommandReceiptIdentity,
    /// 原规范化请求指纹。
    request_fingerprint: &'a str,
    /// 已鉴权操作人。
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
        )?;
        let authorization = self.authorize_actor_permission(actor, CREATE_PERMISSION).await?;
        if let Some(result) =
            replay_creation(&self.db, &receipt_identity, &request_fingerprint, actor, &mut NoTransaction)
                .await?
        {
            return Ok(result);
        }
        let sales_order_id = parse_basis_sales_order_id(&req.basis_id)?;
        let transaction_result = self
            .basis_transaction(
                BasisTransactionInput {
                    req: &req,
                    requested_lines,
                    sales_order_id,
                    receipt_identity: &receipt_identity,
                    request_fingerprint: &request_fingerprint,
                    actor,
                },
                authorization,
            )
            .await;
        match transaction_result {
            Ok(result) => Ok(result),
            Err(error) => recover_command(
                error,
                replay_creation(&self.db, &receipt_identity, &request_fingerprint, actor, &mut NoTransaction)
                    .await,
            ),
        }
    }

    /// 保留授权事务的原克隆、账号复验和实际业务写入顺序。
    async fn basis_transaction(
        &self,
        input: BasisTransactionInput<'_>,
        authorization: PurchaseOrderAuthorization,
    ) -> Result<CreatePurchaseOrderResult> {
        let PurchaseOrderAuthorization { rbac, policy_revision } = authorization;
        let BasisTransactionInput {
            req,
            requested_lines,
            sales_order_id,
            receipt_identity,
            request_fingerprint,
            actor,
        } = input;
        let db = self.db.clone();
        let binding_rbac = rbac.clone();
        let object_read = std::sync::Arc::clone(&self.object_read);
        let transaction_actor = actor.clone();
        let transaction_req = req.clone();
        let transaction_fingerprint = request_fingerprint.to_string();
        let transaction_receipt_identity = receipt_identity.clone();
        rbac.run_authorized_policy_transaction(policy_revision, move |executor| {
            Box::pin(async move {
                ensure_purchase_order_actor_account(&db, &transaction_actor, executor).await?;
                let command = CreateBasisCommand {
                    sales_order_id: &sales_order_id,
                    req: &transaction_req,
                    requested_lines: &requested_lines,
                    receipt_identity: &transaction_receipt_identity,
                    audit_command_id: transaction_receipt_identity.receipt_id(),
                    audit_event_sequence: PurchaseCreationEventSequence::standalone(),
                    request_fingerprint: &transaction_fingerprint,
                    actor: &transaction_actor,
                };
                create_from_basis_apply(&db, &binding_rbac, object_read.as_ref(), &command, executor).await
            })
        })
        .await
    }
}

/// 在 MongoDB 事务内串行化、重算并写入一张采购单后立即提交审批。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 审批绑定授权源
/// * `command` - 来源销售单、请求、幂等收据与审计操作人
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回本次创建或事务内命中的幂等结果。
///
/// # 错误
/// 依据、数量、并发 guard、审批绑定或持久化失败时返回错误。
///
/// # 关键业务约束
/// guard CAS 成功后必须再次按采购当前指针计算剩余量，并复用同一事务事实。
async fn create_from_basis_apply(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    command: &CreateBasisCommand<'_>,
    executor: &mut dyn Executor,
) -> Result<CreatePurchaseOrderResult> {
    if let Some(result) =
        replay_creation(db, command.receipt_identity, command.request_fingerprint, command.actor, executor)
            .await?
    {
        return Ok(result);
    }
    let task = load_owned_open_procurement_task(
        db,
        &command.req.work_item_id,
        command.sales_order_id,
        command.actor.id(),
        executor,
    )
    .await?;
    let mut order = load_effective_sales_order(db, command.sales_order_id, executor).await?;
    let groups = basis_groups_for_order(db, &order, task.responsibility_scope_ids(), executor).await?;
    let selected = find_requested_group(
        &sales_order_basis_fact(&order),
        &groups,
        &command.req.basis_id,
        &command.req.work_item_id,
    )?
    .clone();
    ensure_request_scope(command.req, &selected.scope)?;
    order.advance_procurement_guard(command.actor.id())?;
    db.sales_orders().update(&mut order, executor).await?;
    let (latest_groups, latest_facts) =
        basis_groups_and_facts(db, &order, task.responsibility_scope_ids(), executor).await?;
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
    persist_basis_draft(db, rbac, object_read, &input, command, executor).await
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
/// * `object_read` - 创建并提交时证明采购对象范围的读取端口
/// * `input` - guard 重算后的事务内创建输入（销售单、依据范围、本次行与事实）
/// * `command` - 原始请求、命令收据与审计操作人
/// * `executor` - 数据访问执行器
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
/// 创建并提交必须在同一事务证明采购对象范围，不得由创建人审计字段兜底。
pub async fn persist_basis_draft(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn ApprovalObjectReadPort,
    input: &VerifiedBasisInput<'_>,
    command: &CreateBasisCommand<'_>,
    executor: &mut dyn Executor,
) -> Result<CreatePurchaseOrderResult> {
    let PreparedBasisDraft { order, submission, lines } =
        prepare_basis_draft(db, rbac, input, command, executor).await?;
    let write = PreparedDraftWrite {
        sales_order: input.sales_order,
        order: &order,
        submission: &submission,
        lines: &lines,
        actor: command.actor,
    };
    let document = write_prepared_draft(db, rbac, object_read, &write, executor).await?;
    let purchase_order_id = order.base.id.clone();
    let bundle = CreatedDraftBundle {
        order,
        draft: submission,
        draft_lines: lines,
        document,
        audit_command_id: command.audit_command_id.to_string(),
        audit_event_sequence: command.audit_event_sequence,
    };
    let submitted = submit_created_draft(
        db,
        input.sales_order,
        bundle,
        command.actor,
        command.req.idempotency_key.as_str(),
        executor,
    )
    .await?;
    write_creation_receipt(
        db,
        command,
        &purchase_order_id,
        submitted.purchase_no,
        submitted.lock_version,
        executor,
    )
    .await
}

/// 按原顺序校验目标仓库、初始责任人和对象范围，再构造完整草稿事实。
async fn prepare_basis_draft(
    db: &Database,
    rbac: &SharedRbacService,
    input: &VerifiedBasisInput<'_>,
    command: &CreateBasisCommand<'_>,
    executor: &mut dyn Executor,
) -> Result<PreparedBasisDraft> {
    let group = input.group;
    let target_warehouse_id = resolve_target_warehouse(
        db,
        rbac,
        group.scope.fulfillment_responsibility,
        command.req.target_warehouse_id.as_deref(),
        executor,
    )
    .await?;
    ensure_initial_purchase_order_owner(
        db,
        rbac,
        group.scope.fulfillment_responsibility,
        command.actor.id(),
        executor,
    )
    .await?;
    let mut order = new_basis_order(db, input, command, target_warehouse_id, executor).await?;
    let (submission, lines) = prepare_basis_submission(db, &order, input, executor).await?;
    order.attach_draft_submission(submission.base.id.clone().into())?;
    purchase_access(db.clone(), rbac.clone())
        .ensure_create_and_submit(command.actor, &order, executor)
        .await?;
    Ok(PreparedBasisDraft { order, submission, lines })
}

/// 从同阶段依据和当前主属组织构造采购主表，沿用领域的唯一规范化与状态规则。
async fn new_basis_order(
    db: &Database,
    input: &VerifiedBasisInput<'_>,
    command: &CreateBasisCommand<'_>,
    target_warehouse_id: Option<WarehouseId>,
    executor: &mut dyn Executor,
) -> Result<PurchaseOrder> {
    let sales_order = input.sales_order;
    let group = input.group;
    let creation_basis_id = basis_id_for(
        &sales_order_basis_fact(sales_order),
        group,
        &command.req.work_item_id,
        target_warehouse_id.as_ref(),
    );
    let business_org_unit_id = required_business_org(db, command.actor.id(), executor).await?;
    PurchaseOrder::new(
        PurchaseOrderId::new(next_id()),
        PurchaseOrderData {
            business_org_unit_id,
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
        parse,
    )
    .map_err(Into::into)
}

/// 按原销售行顺序冻结金额与供应商商务指针，复用 guard 后批量事实。
async fn prepare_basis_submission(
    db: &Database,
    order: &PurchaseOrder,
    input: &VerifiedBasisInput<'_>,
    executor: &mut dyn Executor,
) -> Result<(PurchaseOrderSubmission, Vec<PurchaseOrderSubmissionLine>)> {
    let group = input.group;
    let facts = input.facts;
    let supplier_name = facts
        .supplier_names
        .get(&group.scope.supplier_id.to_string())
        .cloned()
        .unwrap_or_else(|| group.scope.supplier_id.to_string());
    let computed = compute_selected_lines(input.selected_lines, group.scope.fulfillment_responsibility);
    let submission = build_draft_submission(
        &CreationBasisSupplierAdapter::from_facts(db.clone(), &group.scope.supplier_id, facts),
        &PurchaseOrderId::new(order.base.id.clone()),
        &group.scope,
        &supplier_name,
        computed.totals,
        executor,
    )
    .await?;
    let submission_id = PurchaseOrderSubmissionId::new(submission.base.id.clone());
    let mut submission_lines = Vec::with_capacity(computed.lines.len());
    for (index, line) in computed.lines.iter().enumerate() {
        submission_lines.push(build_submission_line(&submission_id, (index + 1) as u32, line)?);
    }
    Ok((submission, submission_lines))
}

/// 写入采购草稿聚合、单据注册和审批绑定。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 审批绑定授权源
/// * `write` - 来源销售单、采购聚合与审计操作人
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回使用同一执行器成功登记的业务注册行，供随后的冻结步骤复用。
///
/// # 错误
/// 审批绑定、单据注册或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 本函数只写入草稿聚合；正式号与审批启动由随后的提交步骤完成。
async fn write_prepared_draft(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn ApprovalObjectReadPort,
    write: &PreparedDraftWrite<'_>,
    executor: &mut dyn Executor,
) -> Result<BusinessDocument> {
    let organization_id = purchase_order_responsible_org_id(write.sales_order)?;
    let _ = purchase_order_object_readable(&organization_id, write.actor.id())?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::PurchaseOrder,
        business_object_id: write.order.base.id.clone(),
        business_object_version: write.order.base.version,
        context: BindingRevalidationContext::new(organization_id, write.actor.id().to_string())
            .with_order_source(Some(OrderTaskSource::Purchase(write.order.base.id.clone())))
            .with_business_org_unit_id(Some(write.order.business_org_unit_id.clone()))
            .with_scope_owner_user_id(Some(write.order.current_owner_user_id()?.to_string())),
    };
    let binding = crate::adapters::workflow::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        &bind_command,
        write.actor,
        executor,
    )
    .await?
    .ok_or_else(|| Error::Internal("采购单必须绑定已发布定义".to_string()))?;
    let mut document = new_registered_document(&write.order.base.id, DocumentType::PurchaseOrder, "")
        .map_err(crate::Error::from)?;
    attach_published_binding(&mut document, binding)?;
    db.purchase_orders().create(write.order, executor).await?;
    db.business_documents().create(&document, executor).await?;
    db.purchase_order_submissions().create(write.submission, executor).await?;
    db.purchase_order().create_draft_submission_lines(write.lines, executor).await?;
    sync_procurement_tasks_for_sales_order(db, &write.order.sales_order_id, executor).await?;
    Ok(document)
}

/// 写入提交后的采购创建命令收据。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `command` - 原始请求、收据身份与审计操作人
/// * `purchase_order_id` - 采购单主键
/// * `purchase_no` - 提交后正式号
/// * `lock_version` - 提交后乐观锁版本
/// * `executor` - 数据访问执行器
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
    executor: &mut dyn Executor,
) -> Result<CreatePurchaseOrderResult> {
    let receipt =
        CreationReceipt { purchase_order_id: purchase_order_id.to_string(), purchase_no, lock_version };
    let audit =
        creation_audit(command.actor, command.audit_command_id, &receipt, command.audit_event_sequence)?;
    let record = PurchaseCommandReceipt::new(
        command.receipt_identity,
        command.request_fingerprint,
        receipt.clone(),
        audit.base.id.clone(),
    )?;
    db.purchase_command_receipts::<CreationReceipt>().create(&record, executor).await?;
    persist_log(db, &audit, executor).await?;
    Ok(receipt.into_result(false))
}

/// 从首次创建结果投影随后写入的创建事件。
fn creation_audit(
    actor: &AuditActor,
    command_id: &str,
    receipt: &CreationReceipt,
    sequence: PurchaseCreationEventSequence,
) -> Result<AuditLog> {
    Ok(actor
        .clone()
        .resource_log_with_id(
            next_id(),
            CREATE_ACTION,
            "purchase_order",
            receipt.purchase_order_id.clone(),
            None,
        )?
        .with_command_id(Some(command_id.to_string()))?
        .with_resource_number(Some(receipt.purchase_no.clone()))?
        .with_event_sequence(sequence.created())?)
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
                return Err(Error::ValidationError("目标仓库已停用，请重新选择后再创建采购单".to_string()));
            }
            let handler_user_id =
                warehouse.fulfillment_handler(WarehouseFulfillmentOperation::Receipt).map_err(|_| {
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
        },
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
/// * `receipt_identity` - 稳定收据 ID
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
    receipt_identity: &PurchaseCommandReceiptIdentity,
    expected_fingerprint: &str,
    _actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<CreatePurchaseOrderResult>> {
    let Some(record) = db
        .purchase_command_receipts::<CreationReceipt>()
        .find_by_id_including_deleted(receipt_identity.receipt_id(), executor)
        .await?
    else {
        return Ok(None);
    };
    let receipt = match PurchaseCommandReceipt::<CreationReceipt>::decode(
        record,
        receipt_identity,
        expected_fingerprint,
    ) {
        Ok(receipt) => receipt,
        Err(PurchaseCommandReceiptError::IdentityMismatch | PurchaseCommandReceiptError::PayloadConflict) => {
            return Err(Error::ConflictError("幂等键已用于不同采购创建命令".to_string()));
        },
        Err(PurchaseCommandReceiptError::Corrupted(message)) => {
            return Err(Error::Internal(message));
        },
    };
    let order = db
        .purchase_orders()
        .find_by_id(&receipt.payload().purchase_order_id, executor)
        .await?
        .ok_or_else(|| Error::Internal("采购创建幂等收据引用的采购单不存在".to_string()))?;
    if order.base.id != receipt.payload().purchase_order_id {
        return Err(Error::ConflictError("采购创建幂等收据与当前采购单不一致".to_string()));
    }
    Ok(Some(receipt.into_payload().into_result(true)))
}

#[cfg(test)]
mod tests {
    use erp_audit::prepare_business_log;
    use erp_core::AccountKind;
    use erp_core::ids::{SalesOrderRevisionId, SupplierAccountId};
    use erp_procurement::dto::purchase_order::CREATE_SOURCING_ACTION;
    use erp_procurement::entity::purchase_order::PurchaseType;

    use super::super::super::create_submit::create_submit_audit;
    use super::super::super::sourcing_create::sequence::SourcingEventSequencePlan;
    use super::super::super::sourcing_create::sourcing_audit;
    use super::*;

    fn actor() -> AuditActor {
        AuditActor::new("buyer-id".into(), "buyer-account".into(), AccountKind::Admin)
            .with_actor_name_snapshot(Some("陈国平".into()))
            .unwrap()
            .with_request_id(Some("request-sourcing".into()))
            .unwrap()
    }

    fn created_order(receipt: &CreationReceipt) -> PurchaseOrder {
        PurchaseOrder::new(
            PurchaseOrderId::new(receipt.purchase_order_id.clone()),
            PurchaseOrderData {
                business_org_unit_id: "org-purchase".into(),
                purchase_no: receipt.purchase_no.clone(),
                sales_order_id: SalesOrderId::new("so-source"),
                sales_order_revision_id: SalesOrderRevisionId::new("sales-revision"),
                creation_basis_id: format!("basis-{}", receipt.purchase_order_id),
                supplier_id: SupplierAccountId::new("supplier"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "NET-30".into(),
                fulfillment_responsibility: FulfillmentResponsibility::SupplierDirect,
                owner_user_id: "buyer-id".into(),
                target_warehouse_id: None,
            },
            "buyer-id",
            parse,
        )
        .unwrap()
    }

    /// 执行生产投影工厂，按逐单提交、创建和最终批次主事件形成完整序号。
    #[test]
    fn sourcing_factories_preserve_batch_order_command_and_immutable_snapshots() {
        let actor = actor();
        let command_id = "purchase-order-sourcing-command-batch";
        let receipts = [
            CreationReceipt {
                purchase_order_id: "po-1".into(),
                purchase_no: "PO-001".into(),
                lock_version: 4,
            },
            CreationReceipt {
                purchase_order_id: "po-2".into(),
                purchase_no: "PO-002".into(),
                lock_version: 4,
            },
        ];
        let sequences = SourcingEventSequencePlan::new(receipts.len()).unwrap();
        let mut logs = Vec::new();
        for (receipt, sequence) in receipts.iter().zip(sequences.orders()) {
            let sequence = sequence.unwrap();
            logs.push(create_submit_audit(&actor, &created_order(receipt), command_id, sequence).unwrap());
            logs.push(creation_audit(&actor, command_id, receipt, sequence).unwrap());
        }
        logs.push(sourcing_audit(&actor, command_id, "so-source", "SO-001", sequences.main()).unwrap());
        let logs = logs.iter().map(|log| prepare_business_log(log).unwrap()).collect::<Vec<_>>();
        let encoded = serde_json::to_string(&logs).unwrap();
        let decoded: Vec<AuditLog> = serde_json::from_str(&encoded).unwrap();
        let events = decoded.iter().map(|log| log.structured_event.as_ref().unwrap()).collect::<Vec<_>>();
        assert_eq!(
            events.iter().map(|event| event.event_sequence.get()).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5]
        );
        assert_eq!(
            events.iter().map(|event| event.action_code.as_str()).collect::<Vec<_>>(),
            [
                "purchase_order.submit",
                CREATE_ACTION,
                "purchase_order.submit",
                CREATE_ACTION,
                CREATE_SOURCING_ACTION
            ]
        );
        assert_eq!(
            events.iter().map(|event| event.resource_id.as_str()).collect::<Vec<_>>(),
            ["po-1", "po-1", "po-2", "po-2", "so-source"]
        );
        assert_eq!(
            events.iter().map(|event| event.resource_number_snapshot.as_deref()).collect::<Vec<_>>(),
            [Some("PO-001"), Some("PO-001"), Some("PO-002"), Some("PO-002"), Some("SO-001")]
        );
        assert!(events.iter().all(|event| event.command_id.as_deref() == Some(command_id)));
        assert!(events.iter().all(|event| event.request_id.as_deref() == Some("request-sourcing")));
        assert!(events.iter().all(|event| event.actor_name_snapshot.as_deref() == Some("陈国平")));
        assert_eq!(decoded[0].base.id, "purchase-create-submit-po-1");
        assert_eq!(decoded[2].base.id, "purchase-create-submit-po-2");
        assert!(decoded.iter().all(|log| log.message.as_deref().unwrap().contains("陈国平")));
    }

    /// 独立建单复用同一生产工厂，只有提交与创建；纯库存批次只有主事件。
    #[test]
    fn standalone_and_stock_only_factories_keep_exact_event_sequences() {
        let actor = actor();
        let receipt = CreationReceipt {
            purchase_order_id: "po-only".into(),
            purchase_no: "PO-ONLY".into(),
            lock_version: 4,
        };
        let sequence = PurchaseCreationEventSequence::standalone();
        let submit =
            create_submit_audit(&actor, &created_order(&receipt), "standalone-command", sequence).unwrap();
        let created = creation_audit(&actor, "standalone-command", &receipt, sequence).unwrap();
        assert_eq!(submit.structured_event.as_ref().unwrap().event_sequence.get(), 1);
        assert_eq!(created.structured_event.as_ref().unwrap().event_sequence.get(), 2);
        assert_eq!(
            submit.structured_event.as_ref().unwrap().command_id,
            created.structured_event.as_ref().unwrap().command_id
        );
        let stock_only = SourcingEventSequencePlan::new(0).unwrap();
        let batch =
            sourcing_audit(&actor, "stock-command", "so-source", "SO-ONLY", stock_only.main()).unwrap();
        let event = batch.structured_event.unwrap();
        assert_eq!(event.event_sequence.get(), 1);
        assert_eq!(event.resource_number_snapshot.as_deref(), Some("SO-ONLY"));
    }
}
