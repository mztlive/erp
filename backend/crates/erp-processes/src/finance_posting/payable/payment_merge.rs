//! 工作台一键合并付款的候选任务查询。

use std::collections::HashMap;
use std::str::FromStr;

use erp_core::ids::{PayableAccountId, SupplierAccountId, WorkItemId};
use erp_core::money::Amount;
use erp_finance::dto::payment_merge::{
    PaymentMergeCandidateItemView, PaymentMergeCandidatesParams, PaymentMergeCandidatesView,
};
use erp_finance::entity::payable::{PayableAccount, PayableSourceType};
use erp_finance::repository::PayableExt;
use erp_party::PartyExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_supplier::SupplierExt;
use erp_workflow::entity::work_item::{
    matches_supplier_payment_identity, WorkItem, WorkItemStatus, MAX_PAYMENT_EXECUTION_MERGE,
};
use erp_workflow::WorkItemExt;
use persistence_core::NoTransaction;
use validator::Validate;

use super::mapping::{payment_recipient_view, resolve_optional_payment_recipient_for_read};
use super::PayableService;
use crate::{Error, Result};
use application_core::AuditActor;

impl PayableService {
    /// 查询当前付款任务可合并的同供应商开放任务。
    ///
    /// # 参数
    /// * `params` - 当前工作台打开的付款执行任务
    /// * `actor` - 已认证出纳
    ///
    /// # 返回
    /// 返回当前任务及同一供应商下该出纳的其它开放付款任务。
    ///
    /// # 错误
    /// 任务不存在、不是当前责任人、不是开放付款执行任务或主数据损坏时失败关闭。
    ///
    /// # 关键业务约束
    /// 合并候选只包含采购应付；跨供应商任务不得出现。
    pub async fn payment_merge_candidates(
        &self,
        params: PaymentMergeCandidatesParams,
        actor: &AuditActor,
    ) -> Result<PaymentMergeCandidatesView> {
        params.validate()?;
        let mut executor = NoTransaction;
        let (anchor_task, anchor_account) =
            load_merge_anchor(&self.db, params.work_item_id.trim(), actor, &mut executor).await?;
        assemble_merge_candidates(&self.db, &anchor_task, &anchor_account, actor, &mut executor).await
    }
}

/// 读取并校验当前付款任务与绑定应付。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `work_item_id` - 当前付款执行任务
/// * `actor` - 当前出纳
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回开放且归属当前出纳的付款任务与采购应付。
///
/// # 错误
/// 任务缺失、版本以外的身份不匹配或应付不是采购来源时失败关闭。
async fn load_merge_anchor(
    db: &mongodb::Database,
    work_item_id: &str,
    actor: &AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<(WorkItem, PayableAccount)> {
    let task = db
        .work_items()
        .find_by_id(work_item_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商付款执行任务不存在".to_string()))?;
    if task.status != WorkItemStatus::Open {
        return Err(Error::BusinessLogicError(
            "当前付款任务已结束，无法合并付款".to_string(),
        ));
    }
    if !task.is_owned_by(actor.id()) {
        return Err(Error::Forbidden(
            "当前账号不是开放付款任务的当前责任人".to_string(),
        ));
    }
    let account = db
        .payable_accounts()
        .find_by_id(&task.business_object_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("付款任务关联的应付往来子账不存在".to_string()))?;
    if !matches_supplier_payment_identity(&task, &account.base.id)
        || account.source_type != PayableSourceType::PurchaseOrder
    {
        return Err(Error::BusinessLogicError(
            "付款任务责任身份与应付子账不一致，请联系管理员修复后重试".to_string(),
        ));
    }
    Ok((task, account))
}

/// 装载同供应商开放付款任务并装配候选视图。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `anchor_task` - 当前付款任务
/// * `anchor_account` - 当前任务绑定应付
/// * `actor` - 当前出纳
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回按当前任务优先、到期日和采购单号排序的候选。
///
/// # 错误
/// 候选超过 50 条、金额合计溢出或仓储读取失败时返回错误。
async fn assemble_merge_candidates(
    db: &mongodb::Database,
    anchor_task: &WorkItem,
    anchor_account: &PayableAccount,
    actor: &AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<PaymentMergeCandidatesView> {
    let mut accounts = db
        .payable_accounts()
        .find_unsettled_purchase_accounts_by_supplier(&anchor_account.supplier_id, executor)
        .await?;
    if !accounts
        .iter()
        .any(|account| account.base.id == anchor_account.base.id)
    {
        accounts.push(anchor_account.clone());
    }
    let payable_ids: Vec<String> = accounts.iter().map(|account| account.base.id.clone()).collect();
    let tasks = db
        .work_items()
        .list_open_payment_execution_by_payables_and_owner(&payable_ids, actor.id(), executor)
        .await?;
    if tasks.len() > MAX_PAYMENT_EXECUTION_MERGE {
        return Err(Error::BusinessLogicError(
            "一次合并付款最多包含 50 条任务".to_string(),
        ));
    }
    let account_by_id: HashMap<&str, &PayableAccount> = accounts
        .iter()
        .map(|account| (account.base.id.as_str(), account))
        .collect();
    let mut items = collect_candidate_items(anchor_task, &tasks, &account_by_id)?;
    fill_candidate_details(db, &mut items, &accounts, executor).await?;
    sort_candidate_items(&mut items, &anchor_task.base.id);
    let recipient_account =
        resolve_optional_payment_recipient_for_read(db, &anchor_account.supplier_id, executor).await?;
    Ok(PaymentMergeCandidatesView {
        anchor_work_item_id: WorkItemId::new(anchor_task.base.id.clone()),
        supplier_id: anchor_account.supplier_id.clone(),
        supplier_name: load_supplier_name(db, &anchor_account.supplier_id, executor).await?,
        payment_recipient: recipient_account.as_ref().map(payment_recipient_view),
        open_total: sum_open_totals(items.iter().map(|item| item.open_total))?,
        items,
    })
}

/// 为候选行补齐采购单号与到期日。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `items` - 已投影候选
/// * `accounts` - 候选应付
/// * `executor` - 调用方执行器
///
/// # 返回
/// 成功时就地写入展示字段。
///
/// # 错误
/// 仓储读取失败时返回错误。
async fn fill_candidate_details(
    db: &mongodb::Database,
    items: &mut [PaymentMergeCandidateItemView],
    accounts: &[PayableAccount],
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let source_nos = purchase_source_nos(db, accounts, executor).await?;
    for item in items.iter_mut() {
        item.source_document_no = source_nos.get(item.source_document_id.as_str()).cloned();
    }
    attach_due_dates(db, items, executor).await
}

/// 把已授权开放任务投影为合并候选行。
///
/// # 参数
/// * `anchor_task` - 当前付款任务
/// * `tasks` - 同供应商、当前出纳的开放付款任务
/// * `account_by_id` - 应付子账索引
///
/// # 返回
/// 只保留身份匹配采购应付的任务行。
///
/// # 错误
/// 当前任务没有对应应付时失败关闭。
fn collect_candidate_items(
    anchor_task: &WorkItem,
    tasks: &[WorkItem],
    account_by_id: &HashMap<&str, &PayableAccount>,
) -> Result<Vec<PaymentMergeCandidateItemView>> {
    let mut items = Vec::with_capacity(tasks.len());
    for task in tasks {
        let Some(account) = account_by_id.get(task.business_object_id.as_str()) else {
            continue;
        };
        if !matches_supplier_payment_identity(task, &account.base.id) {
            continue;
        }
        items.push(candidate_item(task, account, task.base.id == anchor_task.base.id));
    }
    if !items.iter().any(|item| item.is_anchor) {
        return Err(Error::BusinessLogicError(
            "当前付款任务不在可合并范围内，请刷新工作台后重试".to_string(),
        ));
    }
    Ok(items)
}

/// 构造单条合并候选展示行。
///
/// # 参数
/// * `task` - 开放付款执行任务
/// * `account` - 绑定应付
/// * `is_anchor` - 是否为当前工作台任务
///
/// # 返回
/// 返回不含来源单号和到期日的基础行，这两项由后续批量查询补齐。
///
/// # 错误
/// 无。
fn candidate_item(
    task: &WorkItem,
    account: &PayableAccount,
    is_anchor: bool,
) -> PaymentMergeCandidateItemView {
    PaymentMergeCandidateItemView {
        work_item_id: WorkItemId::new(task.base.id.clone()),
        task_version: task.base.version.to_string(),
        payable_account_id: PayableAccountId::new(account.base.id.clone()),
        subject_version: account.base.version.to_string(),
        source_document_id: account.source_document_id.clone(),
        source_document_no: None,
        open_total: account.open_total,
        due_date: None,
        is_anchor,
    }
}

/// 当前任务排在最前，其余按到期日、采购单号和任务 ID 稳定排序。
///
/// # 参数
/// * `items` - 待排序候选
/// * `anchor_work_item_id` - 当前任务 ID
///
/// # 返回
/// 无；就地排序。
///
/// # 错误
/// 无。
fn sort_candidate_items(items: &mut [PaymentMergeCandidateItemView], anchor_work_item_id: &str) {
    items.sort_by(|left, right| {
        match (
            left.work_item_id.as_ref() == anchor_work_item_id,
            right.work_item_id.as_ref() == anchor_work_item_id,
        ) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => left
                .due_date
                .cmp(&right.due_date)
                .then(left.source_document_no.cmp(&right.source_document_no))
                .then(left.work_item_id.as_ref().cmp(right.work_item_id.as_ref())),
        }
    });
}

/// 合计候选未付金额。
///
/// # 参数
/// * `amounts` - 各任务开放余额
///
/// # 返回
/// 返回精确到分的合计。
///
/// # 错误
/// 合计溢出金额标度时返回错误。
fn sum_open_totals(amounts: impl IntoIterator<Item = Amount>) -> Result<Amount> {
    let mut total = Amount::from_str("0.00").map_err(Error::Logic)?;
    for amount in amounts {
        total = Amount::try_from(total.to_decimal() + amount.to_decimal()).map_err(Error::Logic)?;
    }
    Ok(total)
}

/// 批量读取采购单号。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `accounts` - 候选应付
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回采购单 ID 到业务单号的映射；缺失单号不上表。
///
/// # 错误
/// 仓储读取失败时返回错误。
async fn purchase_source_nos(
    db: &mongodb::Database,
    accounts: &[PayableAccount],
    executor: &mut dyn persistence_core::Executor,
) -> Result<HashMap<String, String>> {
    let purchase_ids: Vec<String> = accounts
        .iter()
        .filter(|account| account.source_type == PayableSourceType::PurchaseOrder)
        .map(|account| account.source_document_id.clone())
        .collect();
    db.purchase_order()
        .purchase_nos_by_ids(&purchase_ids, executor)
        .await
        .map_err(Into::into)
}

/// 为候选行补齐最早到期日。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `items` - 已投影候选
/// * `executor` - 调用方执行器
///
/// # 返回
/// 成功时就地写入到期日。
///
/// # 错误
/// 仓储聚合失败时返回错误。
async fn attach_due_dates(
    db: &mongodb::Database,
    items: &mut [PaymentMergeCandidateItemView],
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let account_ids: Vec<PayableAccountId> =
        items.iter().map(|item| item.payable_account_id.clone()).collect();
    let due_dates = db
        .payable_entries()
        .minimum_due_dates_by_accounts(&account_ids, executor)
        .await?;
    for item in items {
        item.due_date = due_dates.get(item.payable_account_id.as_ref()).copied();
    }
    Ok(())
}

/// 读取供应商当前展示名称。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `supplier_id` - 往来供应商
/// * `executor` - 调用方执行器
///
/// # 返回
/// 主数据或主体修订缺失时返回空，不得回退供应商 ID。
///
/// # 错误
/// 仓储读取失败时返回错误。
async fn load_supplier_name(
    db: &mongodb::Database,
    supplier_id: &SupplierAccountId,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<String>> {
    let Some(supplier) = db
        .supplier_accounts()
        .find_by_id(supplier_id.as_ref(), executor)
        .await?
    else {
        return Ok(None);
    };
    let Some(party) = db
        .parties()
        .find_by_id(supplier.party_id.as_ref(), executor)
        .await?
    else {
        return Ok(None);
    };
    let Some(revision_id) = party.stable.current_revision_id.clone() else {
        return Ok(None);
    };
    let Some(revision) = db.party_revisions().find_by_id(&revision_id, executor).await? else {
        return Ok(None);
    };
    let name = revision.legal_name.trim();
    if name.is_empty() {
        Ok(None)
    } else {
        Ok(Some(name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::sum_open_totals;
    use erp_core::money::Amount;
    use std::str::FromStr;

    /// 多条开放余额必须精确加总到分。
    #[test]
    fn merge_candidate_totals_sum_to_the_cent() {
        let total = sum_open_totals([
            Amount::from_str("10.10").unwrap(),
            Amount::from_str("20.20").unwrap(),
        ])
        .unwrap();
        assert_eq!(total, Amount::from_str("30.30").unwrap());
    }
}
