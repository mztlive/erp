//! 从销售单追加费用采购单：同一事务内建单并提交审批（§7.5.1）。
//!
//! 费用采购单不经供给分配、不选择供给、不计入供给覆盖；采购类型为费用、无需履约。
//! 建单沿用采购单的单据登记、审批绑定、提交冻结和命令回执，失败时整单回滚。

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditLog};
use erp_core::ids::{PurchaseOrderId, PurchaseOrderSubmissionId, SalesOrderId, SalesOrderRevisionId};
use erp_identity::SharedRbacService;
use erp_procurement::dto::purchase_order::{
    CREATE_EXPENSE_ACTION, CreateExpensePurchaseRequest, CreatePurchaseOrderResult,
};
use erp_procurement::entity::purchase_order::{
    BasisScope, CreationReceipt, FulfillmentResponsibility, PurchaseCommandReceipt,
    PurchaseCommandReceiptIdentity, PurchaseOrder, PurchaseOrderData, PurchaseOrderSubmission,
    PurchaseOrderSubmissionLine, PurchaseType,
};
use erp_procurement::repository::PurchaseCommandExt;
use erp_procurement::service::purchase_order::creation_basis::build_draft_submission;
use erp_procurement::service::purchase_order::expense::build_expense_draft_lines;
use erp_read_models::purchase_center::repository::supplier_names::current_legal_names_by_account_ids;
use erp_read_models::sales_center::access::SalesAccess;
use erp_sales::entity::sales_order::SalesOrder;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::PurchaseOrderProcess;
use super::adapters::payment_term::parse;
use super::authorization::{PurchaseOrderAuthorization, ensure_purchase_order_actor_account};
use super::create_submit::{CreatedDraftBundle, submit_created_draft};
use super::creation_basis::create::{PreparedDraftWrite, replay_creation, write_prepared_draft};
use super::creation_basis::supplier::CreationBasisSupplierAdapter;
use super::payment_subject::load_payment_subject;
use super::sourcing_create::sequence::PurchaseCreationEventSequence;
use crate::adapters::purchase_access;
use crate::audit::{persist_log, recover_command};
use crate::business_ownership::required_business_org;
use crate::finance_posting::extra_cost::sales_line::{ensure_attributable_line, ensure_extra_cost_order};
use crate::{Error, Result};

const CREATE_PERMISSION: &str = "purchase_order:create";
const EXPENSE_RECEIPT_PREFIX: &str = "purchase-order-expense-command-";

/// 事务内的费用采购建单命令。
struct ExpenseCommand {
    sales_order_id: String,
    req: CreateExpensePurchaseRequest,
    identity: PurchaseCommandReceiptIdentity,
    fingerprint: String,
    actor: AuditActor,
}

impl PurchaseOrderProcess {
    /// 从已生效的实物与服务销售单追加一张费用采购单，并立即提交审批。
    ///
    /// # 参数
    /// * `sales_order_id` - 来源销售单
    /// * `req` - 收款供应商、付款条件、费用行与幂等键
    /// * `actor` - 已通过 `purchase_order:create` 鉴权的操作人
    ///
    /// # 返回
    /// 返回已提交审批的采购单；同一幂等键同一载荷重复提交时返回原结果。
    ///
    /// # 错误
    /// 输入非法、销售单不可见或不可追加费用、归属明细无效、供应商不存在或缺少商务结算版本、
    /// 付款条件非法、幂等键载荷冲突、审批绑定或写入失败时返回错误。
    pub async fn create_expense_purchase(
        &self,
        sales_order_id: &str,
        req: CreateExpensePurchaseRequest,
        actor: &AuditActor,
    ) -> Result<CreatePurchaseOrderResult> {
        req.validate()?;
        let fingerprint = req.request_fingerprint(sales_order_id)?;
        let identity = PurchaseCommandReceipt::<CreationReceipt>::identity(
            EXPENSE_RECEIPT_PREFIX,
            actor.id(),
            CREATE_EXPENSE_ACTION,
            Some(sales_order_id),
            &req.idempotency_key,
        )?;
        let authorization = self.authorize_actor_permission(actor, CREATE_PERMISSION).await?;
        if let Some(result) =
            replay_creation(&self.db, &identity, &fingerprint, actor, &mut NoTransaction).await?
        {
            return Ok(result);
        }
        let command = ExpenseCommand {
            sales_order_id: sales_order_id.to_string(),
            req,
            identity: identity.clone(),
            fingerprint: fingerprint.clone(),
            actor: actor.clone(),
        };
        match self.expense_transaction(command, authorization).await {
            Ok(result) => Ok(result),
            Err(error) => recover_command(
                error,
                replay_creation(&self.db, &identity, &fingerprint, actor, &mut NoTransaction).await,
            ),
        }
    }

    /// 在授权快照事务中执行建单与提交。
    async fn expense_transaction(
        &self,
        command: ExpenseCommand,
        authorization: PurchaseOrderAuthorization,
    ) -> Result<CreatePurchaseOrderResult> {
        let PurchaseOrderAuthorization { rbac, policy_revision } = authorization;
        let db = self.db.clone();
        let binding_rbac = rbac.clone();
        let object_read = std::sync::Arc::clone(&self.object_read);
        rbac.run_authorized_policy_transaction(policy_revision, move |executor| {
            Box::pin(async move {
                apply_expense_create(&db, &binding_rbac, object_read.as_ref(), &command, executor).await
            })
        })
        .await
    }
}

/// 事务内重放检查、资格校验、建单、提交与回执。
async fn apply_expense_create(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    command: &ExpenseCommand,
    executor: &mut dyn Executor,
) -> Result<CreatePurchaseOrderResult> {
    if let Some(result) =
        replay_creation(db, &command.identity, &command.fingerprint, &command.actor, executor).await?
    {
        return Ok(result);
    }
    ensure_purchase_order_actor_account(db, &command.actor, executor).await?;
    let sales_order = load_expense_source(db, rbac, command, executor).await?;
    let (order, submission, lines) = prepare_expense_draft(db, rbac, &sales_order, command, executor).await?;
    let write =
        PreparedDraftWrite { order: &order, submission: &submission, lines: &lines, actor: &command.actor };
    let document = write_prepared_draft(db, rbac, object_read, &write, executor).await?;
    let purchase_order_id = order.base.id.clone();
    let bundle = CreatedDraftBundle {
        order,
        draft: submission,
        draft_lines: lines,
        document,
        audit_command_id: command.identity.receipt_id().to_string(),
        audit_event_sequence: PurchaseCreationEventSequence::standalone(),
    };
    let submitted = submit_created_draft(
        db,
        &sales_order,
        bundle,
        &command.actor,
        &command.req.idempotency_key,
        executor,
    )
    .await?;
    let receipt = CreationReceipt {
        purchase_order_id,
        purchase_no: submitted.purchase_no,
        lock_version: submitted.lock_version,
    };
    write_expense_receipt(db, command, receipt, executor).await
}

/// 读取操作人可见、已生效的实物与服务销售单，并校验每条归属明细。
async fn load_expense_source(
    db: &Database,
    rbac: &SharedRbacService,
    command: &ExpenseCommand,
    executor: &mut dyn Executor,
) -> Result<SalesOrder> {
    let order = SalesAccess::new(db.clone(), rbac.clone())
        .require_object(&command.actor, "detail", &command.sales_order_id, &[], executor)
        .await?;
    ensure_extra_cost_order(&order)?;
    for line in &command.req.lines {
        ensure_attributable_line(db, &order, line.sales_order_line_id.as_ref(), executor).await?;
    }
    Ok(order)
}

/// 构造费用采购单主表、草稿提交与费用行，并证明对象范围。
async fn prepare_expense_draft(
    db: &Database,
    rbac: &SharedRbacService,
    sales_order: &SalesOrder,
    command: &ExpenseCommand,
    executor: &mut dyn Executor,
) -> Result<(PurchaseOrder, PurchaseOrderSubmission, Vec<PurchaseOrderSubmissionLine>)> {
    let mut order = new_expense_order(db, sales_order, command, executor).await?;
    let order_id = PurchaseOrderId::new(order.base.id.clone());
    let totals =
        build_expense_draft_lines(&PurchaseOrderSubmissionId::new("pending"), &command.req.lines)?.totals;
    let scope = BasisScope {
        supplier_id: command.req.supplier_id.clone(),
        purchase_type: PurchaseType::Expense,
        payment_term_code: order.payment_term_code.clone(),
        fulfillment_responsibility: FulfillmentResponsibility::NotApplicable,
    };
    let supplier_name = supplier_name(db, &command.req, executor).await?;
    let payment_subject =
        load_payment_subject(db, &order.sales_order_id, &order.sales_order_revision_id, executor).await?;
    let adapter = CreationBasisSupplierAdapter::new(db.clone());
    let submission = build_draft_submission(
        &adapter,
        &order_id,
        &scope,
        &supplier_name,
        payment_subject,
        totals,
        executor,
    )
    .await?;
    let submission_id = PurchaseOrderSubmissionId::new(submission.base.id.clone());
    let lines = build_expense_draft_lines(&submission_id, &command.req.lines)?.lines;
    order.attach_draft_submission(submission_id)?;
    purchase_access(db.clone(), rbac.clone())
        .ensure_create_and_submit(&command.actor, &order, executor)
        .await?;
    Ok((order, submission, lines))
}

/// 按操作人主属组织构造费用采购单主表。
async fn new_expense_order(
    db: &Database,
    sales_order: &SalesOrder,
    command: &ExpenseCommand,
    executor: &mut dyn Executor,
) -> Result<PurchaseOrder> {
    let revision_id = sales_order
        .stable
        .current_revision_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前生效版本".into()))?;
    let id = next_id();
    PurchaseOrder::new(
        PurchaseOrderId::new(id.clone()),
        PurchaseOrderData {
            business_org_unit_id: required_business_org(db, command.actor.id(), executor).await?,
            purchase_no: String::new(),
            sales_order_id: SalesOrderId::new(sales_order.base.id.clone()),
            sales_order_revision_id: SalesOrderRevisionId::new(revision_id),
            creation_basis_id: format!("expense:{id}"),
            supplier_id: command.req.supplier_id.clone(),
            purchase_type: PurchaseType::Expense,
            payment_term_code: command.req.payment_term_code.trim().to_string(),
            fulfillment_responsibility: FulfillmentResponsibility::NotApplicable,
            owner_user_id: command.actor.id().to_string(),
            target_warehouse_id: None,
        },
        command.actor.id(),
        parse,
    )
    .map_err(Into::into)
}

/// 读取收款供应商当前法定名称作为快照。
async fn supplier_name(
    db: &Database,
    req: &CreateExpensePurchaseRequest,
    executor: &mut dyn Executor,
) -> Result<String> {
    let names =
        current_legal_names_by_account_ids(db, std::slice::from_ref(&req.supplier_id), executor).await?;
    names
        .get(req.supplier_id.as_ref())
        .cloned()
        .ok_or_else(|| Error::ValidationError("收款供应商不存在，请先建立供应商档案".into()))
}

/// 写入可回放的建单回执与创建审计。
async fn write_expense_receipt(
    db: &Database,
    command: &ExpenseCommand,
    receipt: CreationReceipt,
    executor: &mut dyn Executor,
) -> Result<CreatePurchaseOrderResult> {
    let audit = expense_audit(command, &receipt)?;
    let record = PurchaseCommandReceipt::new(
        &command.identity,
        &command.fingerprint,
        receipt.clone(),
        audit.base.id.clone(),
    )?;
    db.purchase_command_receipts::<CreationReceipt>().create(&record, executor).await?;
    persist_log(db, &audit, executor).await?;
    Ok(receipt.into_result(false))
}

fn expense_audit(command: &ExpenseCommand, receipt: &CreationReceipt) -> Result<AuditLog> {
    Ok(command
        .actor
        .clone()
        .resource_log_with_id(
            next_id(),
            CREATE_EXPENSE_ACTION,
            "purchase_order",
            receipt.purchase_order_id.clone(),
            None,
        )?
        .with_command_id(Some(command.identity.receipt_id().to_string()))?
        .with_resource_number(Some(receipt.purchase_no.clone()))?
        .with_event_sequence(PurchaseCreationEventSequence::standalone().created())?)
}
