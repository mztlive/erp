//! 按原蓝票一次开具红票并红冲分配。

use application_core::{AuditActor, CommandReceipt};
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{InvoiceId, PayableAccountId, ReceivableAccountId, SalesOrderId};
use erp_core::money::Amount;
use erp_finance::dto::receivable::{CommitRedInvoiceRequest, InvoiceView};
use erp_finance::entity::payable::{AllocationAction as PayableAllocationAction, PurchaseInvoiceAllocation};
use erp_finance::entity::receivable::{
    AllocationAction, Invoice, InvoiceData, InvoiceDirection, InvoiceKind, RedInvoiceAllocationPlan,
    SalesInvoiceAllocation,
};
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_finance::service::receivable::ReceivableService;
use erp_finance::service::receivable::red_invoice_plan::{
    purchase_red_invoice_allocation_plan, purchase_red_invoice_for_account, sales_red_invoice_allocation_plan,
};
use erp_finance::service::receivable::red_invoice_posting::persist_red_invoice;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::receivable;
use erp_procure_to_pay::procure_to_pay::return_finance::sync_return_accounts;
use erp_read_models::finance::receivable::snapshot::zero_amount;
use erp_read_models::sales_money_progress::SalesMoneyProgressReader;
use erp_sales::service::sales_order::progress::update_sales_order_money_progress;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use sha2::{Digest, Sha256};
use validator::Validate;

use super::invoice::register_created_invoice_document;
use super::{ReceivableProcess, invoice_task};
use crate::{Error, FinancePostingPorts, Result};

impl ReceivableProcess {
    /// 按原蓝票一次开具红票并红冲（§8.3-3 事务不变量）。
    ///
    /// 服务端在同一事务内读取原票的有效分配、计算本次反向行、创建红票、冲减应收或应付子账进度并写审计。客户端不得提交分配 ID、净额或税额。部分红冲时原蓝票保持已登记；全部剩余金额红冲后才置为已红冲。
    ///
    /// `id` 为原蓝票 ID；`req` 为红票业务意图与幂等键。返回新建红票视图。`NotFound`：原蓝票或有效分配不存在。
    ///
    /// `ConflictError`：红票号码重复。`BusinessLogicError`：红冲累计超过原分配或超额红冲。
    ///
    /// 领域计划只计算金额；ID 生成、事务、写入、任务同步和审计继续由 Service 持有。
    pub async fn issue_red_invoice(
        &self,
        id: &str,
        req: CommitRedInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<InvoiceView> {
        req.validate()?;
        let prepared = RedInvoiceCommand::prepare(actor, id, &req)?;

        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let id = apply_red_invoice(&db, &rbac, &prepared, executor).await?;
                    red_invoice_result(&db, &rbac.authorization, &prepared.actor, &id, executor).await
                })
            })
            .await
    }
}

// 在原事务中重验红冲资格并读取新建或幂等命中的红票视图。
//
// db 为发票数据库，authorization 为共享授权服务，actor 为本次操作人，id 为实际红票身份，executor 为原事务执行器。
//
// 返回同一授权快照内的完整发票及分配视图。原事务失效、资格不足或发票读取失败时拒绝交付结果。
async fn red_invoice_result(
    db: &Database,
    authorization: &AuthorizationService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<InvoiceView> {
    authorization.require(actor, &receivable::INVOICE_RED_ISSUE, AccessTarget::Operation, executor).await?;
    Ok(ReceivableService::new(db.clone()).invoice_detail_with_executor(id, executor).await?)
}

/// 红票命令的事务外快照：落事务前的全部确定性输入。
struct RedInvoiceCommand {
    /// 原蓝票 ID。
    original_id: String,
    /// 红票号码。
    red_no: String,
    /// 本次红冲含税金额。
    requested_amount: Option<Amount>,
    requested_account: Option<String>,
    receipt: CommandReceipt,
    /// 红冲业务原因。
    reason: String,
    /// 操作人快照。
    actor: AuditActor,
    /// 操作人 ID。
    actor_id: String,
}

impl RedInvoiceCommand {
    // 组装命令快照（纯内存，不访问数据库）。
    //
    // `id` 为原蓝票 ID；`req` 为红票业务意图与幂等键。返回事务外快照。
    fn prepare(actor: &AuditActor, id: &str, req: &CommitRedInvoiceRequest) -> Result<Self> {
        let actor_owned = actor.clone();
        let digest = hex::encode(Sha256::digest(
            format!("{}|{}|{}", actor.id(), id, req.idempotency_key.trim()).as_bytes(),
        ));
        Ok(Self {
            original_id: id.to_string(),
            red_no: red_number(req, &digest),
            requested_amount: req.amount,
            requested_account: req.payable_account_id.clone(),
            receipt: CommandReceipt::from_payload(
                "red-invoice-",
                actor.id(),
                "invoice.red_issue",
                "invoice",
                &req.idempotency_key,
                &(id, req),
            )?,
            reason: req.reason.trim().to_string(),
            actor_id: actor.id().to_string(),
            actor: actor_owned,
        })
    }
}

// 计算红票号码（纯内存）。
//
// `req` 为红票业务意图与幂等键；`digest` 为幂等键摘要。返回显式号码或按幂等键生成的稳定号码。
fn red_number(req: &CommitRedInvoiceRequest, digest: &str) -> String {
    req.invoice_no
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("HT-{}", &digest[..12]))
}

// 在调用方事务内执行红票开具的全部写入与扇出。
//
// 收据幂等由外层根命令持有；本函数只做事务内读取、校验与写入。
//
// `rbac` 为审批绑定授权源；`command` 为事务外命令快照。返回新建或幂等命中的红票 ID。原票缺失、资格不符、号码冲突或写入失败时返回错误。
async fn apply_red_invoice(
    db: &Database,
    rbac: &FinancePostingPorts,
    command: &RedInvoiceCommand,
    executor: &mut dyn Executor,
) -> Result<String> {
    rbac.authorization
        .require(&command.actor, &receivable::INVOICE_RED_ISSUE, AccessTarget::Operation, executor)
        .await?;

    let receipts = FinanceCommandReceiptService::new(db.clone());
    if let Some(id) = receipts.committed_resource_id(&command.receipt, executor).await? {
        return Ok(id);
    }

    let original = db
        .invoices()
        .find_by_id(&command.original_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原蓝票不存在".to_string()))?;
    if !original.is_registered() || original.invoice_kind != InvoiceKind::Blue {
        return Err(Error::BusinessLogicError("只有已登记的蓝票可以被红冲".to_string()));
    }

    let allocation_plan = load_red_invoice_plan(
        db,
        &original,
        command.requested_amount,
        command.requested_account.as_deref(),
        executor,
    )
    .await?;
    let (red_gross, red_net, red_tax) = allocation_plan.totals();
    if let Some(existing) =
        find_red_idempotent(db, &original, &command.red_no, red_gross, red_net, red_tax, executor).await?
    {
        if command.requested_account.is_some() {
            return Err(Error::ConflictError("红票号码已登记，请核对原登记记录".into()));
        }
        return Ok(existing);
    }

    let red_mut =
        build_red_invoice(&original, &command.red_no, red_gross, red_net, red_tax, &command.actor_id)?;
    let mut original_mut = original;
    register_created_invoice_document(db, rbac, &red_mut, &command.actor, executor).await?;
    let sales_ids =
        persist_red_invoice(db, &red_mut, &mut original_mut, &allocation_plan, &command.actor_id, executor)
            .await?;

    let invoices = RedPostingInvoices { red: &red_mut, original: &original_mut };
    complete_red_posting(db, rbac, command, invoices, &allocation_plan, &sales_ids, executor).await?;
    Ok(red_mut.base.id.clone())
}

// 加载红票反向分配计划（同一事务内批量读取）。
//
// `original` 为原蓝票；`requested_amount` 为本次红冲含税金额。返回校验通过的反向分配计划。分配缺失或超额红冲时返回错误。
async fn load_red_invoice_plan(
    db: &Database,
    original: &Invoice,
    requested_amount: Option<Amount>,
    requested_account: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<RedInvoiceAllocationPlan> {
    match original.invoice_direction {
        InvoiceDirection::Sales => {
            if requested_account.is_some() {
                return Err(Error::ValidationError("销项红票不能指定采购应付".into()));
            }
            let blue = db
                .sales_invoice_allocations()
                .find_allocations_by_invoices(&[InvoiceId::new(original.base.id.clone())], executor)
                .await?;
            let account_ids = apply_account_ids(&blue);
            let related =
                db.sales_invoice_allocations().find_allocations_by_accounts(&account_ids, executor).await?;
            Ok(sales_red_invoice_allocation_plan(&blue, &related, requested_amount)?)
        },
        InvoiceDirection::Purchase => {
            let blue = db
                .purchase_invoice_allocations()
                .find_allocations_by_invoices(&[InvoiceId::new(original.base.id.clone())], executor)
                .await?;
            let account_ids = purchase_apply_account_ids(&blue);
            let related = db
                .purchase_invoice_allocations()
                .find_allocations_by_accounts(&account_ids, executor)
                .await?;
            Ok(if let Some(account) = requested_account {
                purchase_red_invoice_for_account(&blue, &related, requested_amount, account)?
            } else {
                purchase_red_invoice_allocation_plan(&blue, &related, requested_amount)?
            })
        },
    }
}

// 收集销项有效分配的目标子账（纯内存）。
//
// `blue` 为原蓝票有效分配。返回 `Apply` 行的目标子账 ID。
fn apply_account_ids(blue: &[SalesInvoiceAllocation]) -> Vec<ReceivableAccountId> {
    blue.iter()
        .filter(|line| line.allocation_action == AllocationAction::Apply)
        .map(|line| line.receivable_account_id.clone())
        .collect()
}

// 收集购项有效分配的目标子账（纯内存）。
//
// `blue` 为原蓝票有效分配。返回 `Apply` 行的目标子账 ID。
fn purchase_apply_account_ids(blue: &[PurchaseInvoiceAllocation]) -> Vec<PayableAccountId> {
    blue.iter()
        .filter(|line| line.allocation_action == PayableAllocationAction::Apply)
        .map(|line| line.payable_account_id.clone())
        .collect()
}

// 按红票号码查找幂等命中（纯读取，无写入）。
//
// `original` 为原蓝票；`red_no` 为红票号码；`red_gross` 为红冲含税金额；`red_net` 为红冲净额；`red_tax` 为红冲税额；沿用原事务的执行器。
//
// 同票同金额返回红票 ID；号码冲突由调用方映射为冲突错误。号码重复提交时返回冲突错误。
async fn find_red_idempotent(
    db: &Database,
    original: &Invoice,
    red_no: &str,
    red_gross: Amount,
    red_net: Amount,
    red_tax: Amount,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let Some(existing) = db
        .invoices()
        .find_by_direction_and_normalized_no(original.invoice_direction, &red_no.to_uppercase(), executor)
        .await?
    else {
        return Ok(None);
    };
    if existing.invoice_kind == InvoiceKind::Red
        && existing.original_invoice_id.as_ref() == Some(&InvoiceId::new(original.base.id.clone()))
        && existing.gross_amount == red_gross
        && existing.net_amount == Some(red_net)
        && existing.tax_amount == Some(red_tax)
    {
        return Ok(Some(existing.base.id));
    }
    Err(Error::ConflictError("红票号码已登记，请勿重复提交".to_string()))
}

// 构造已登记的红票实体（纯内存）。
//
// `original` 为原蓝票；`red_no` 为红票号码；`red_gross` 为红冲含税金额；`red_net` 为红冲净额；`red_tax` 为红冲税额。
//
// 返回已登记的红票实体。实体构造失败时返回错误。
fn build_red_invoice(
    original: &Invoice,
    red_no: &str,
    red_gross: Amount,
    red_net: Amount,
    red_tax: Amount,
    actor_id: &str,
) -> Result<Invoice> {
    let mut red_mut = Invoice::new(
        InvoiceId::new(next_id()),
        InvoiceData {
            invoice_direction: original.invoice_direction,
            invoice_kind: InvoiceKind::Red,
            party_id: original.party_id.clone(),
            invoice_code: original.invoice_code.clone(),
            invoice_no: red_no.to_string(),
            invoice_date: BusinessDate::today(),
            gross_amount: red_gross,
            net_amount: red_net,
            tax_amount: red_tax,
            rounding_adjustment_amount: zero_amount(),
            rounding_reason: None,
            original_invoice_id: Some(original.base.id.clone().into()),
        },
        actor_id,
    )?;
    red_mut.mark_registered(actor_id)?;
    Ok(red_mut)
}

// 红冲后任务同步与销售进度扇出（分片顺序短路）。
//
// 去重分组已在上游批量查询完成；同一事务执行器不可并发共享，按片顺序推进即分组后的可靠形态，保持原失败短路语义。
//
// `account_ids` 为受影响的应收子账；`actor_id` 为审计操作人。任一任务同步或进度刷新失败时返回错误并停止后续分片。
async fn apply_red_invoice_fanout(
    db: &Database,
    account_ids: &[String],
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut sorted = account_ids.to_vec();
    sorted.sort();
    sorted.dedup();
    for chunk in sorted.chunks(super::invoice_posting::FANOUT_CHUNK) {
        for account_id in chunk {
            invoice_task::sync_sales_invoice_task(
                db,
                &ReceivableAccountId::new(account_id.clone()),
                invoice_task::SalesInvoiceTaskChange::RedInvoiceIssued,
                executor,
            )
            .await?;
        }
    }

    let accounts = db.receivable_accounts().find_accounts_by_ids(&sorted, executor).await?;
    let mut order_ids = accounts.iter().map(|a| a.sales_order_id.to_string()).collect::<Vec<_>>();
    order_ids.sort();
    order_ids.dedup();
    refresh_progress_shards(db, &order_ids, actor_id, executor).await
}

// 分片刷新销售单资金进度，首错短路。
//
// `order_ids` 为去重后的销售单；`actor_id` 为审计操作人；沿用原事务的执行器。任一销售单刷新失败时返回错误并停止后续分片。
async fn refresh_progress_shards(
    db: &Database,
    order_ids: &[String],
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    for chunk in order_ids.chunks(super::invoice_posting::FANOUT_CHUNK) {
        for order_id in chunk {
            update_sales_order_money_progress(
                db,
                &SalesMoneyProgressReader::new(db.clone()),
                executor,
                &SalesOrderId::new(order_id.clone()),
                actor_id.to_string(),
                None,
            )
            .await?;
        }
    }
    Ok(())
}

/// 已完成财务分配写入的红票和原蓝票，供同事务后续审计及任务同步使用。
struct RedPostingInvoices<'a> {
    red: &'a Invoice,
    original: &'a Invoice,
}

async fn complete_red_posting(
    db: &Database,
    rbac: &FinancePostingPorts,
    command: &RedInvoiceCommand,
    invoices: RedPostingInvoices<'_>,
    allocation_plan: &RedInvoiceAllocationPlan,
    sales_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<()> {
    let RedPostingInvoices { red: red_mut, original: original_mut } = invoices;
    let audit = command.actor.clone().resource_log_with_message(
        "invoice.red_issue",
        "invoice",
        red_mut.base.id.clone(),
        Some(command.reason.clone()),
    )?;
    persist_log(db, &audit, executor).await?;

    if original_mut.invoice_direction == InvoiceDirection::Sales {
        apply_red_invoice_fanout(db, sales_ids, &command.actor_id, executor).await?;
    } else {
        sync_return_accounts(
            db,
            rbac.tasks.as_ref(),
            allocation_plan.lines().iter().map(|line| line.account_id.clone()).collect(),
            executor,
        )
        .await?;
    }

    FinanceCommandReceiptService::new(db.clone())
        .save_resource(&command.receipt, red_mut.base.id.clone(), audit.base.id.clone(), executor)
        .await?;
    Ok(())
}

#[cfg(test)]
mod result_tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    // 原事务或红冲资格不可用时，必须在读取发票及分配前拒绝。
    #[tokio::test]
    async fn red_invoice_results_require_snapshot_and_current_command_qualification() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("red_invoice_results");
        let authorization = unavailable_authorization();
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);

        let error = red_invoice_result(&db, &authorization, &actor, "red", &mut ReadExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::ValidationError(message)
            if message == "写入授权必须使用快照事务"));

        let error = red_invoice_result(&db, &authorization, &actor, "red", &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message)
            if message == "测试授权设施不可用"));
    }
}
