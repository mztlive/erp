//! 供应商往来只读展示字段装配：供应商名称、来源业务单号、付款核销目标单据。
//!
//! 内部主键不得作为业务单号或名称回填；主数据或来源单据缺失时字段为空。

use std::collections::{HashMap, HashSet};

use database::{NoTransaction, PayableExt};
use entities::ids::{PayableAccountId, PayableEntryId};
use entities::payable::{PayableAccount, PayableEntry, PayableSourceType};
use mongodb::Database;

use super::dto::{PaymentAllocationView, SupplierPaymentView};
use super::{resolve_source_document_no, resolve_supplier_display, PayableService};
use crate::errors::Result;

/// 付款核销目标的可读来源。
#[derive(Clone)]
struct AllocationSource {
    /// 应付子账主键。
    payable_account_id: String,
    /// 应付来源类型。
    source_type: PayableSourceType,
    /// 来源单据内部身份。
    source_document_id: String,
    /// 来源业务单号；缺失时为空。
    source_document_no: Option<String>,
}

impl PayableService {
    /// 为付款视图补全供应商展示名与核销目标单据。
    ///
    /// # 参数
    /// * `view` - 尚未补全展示字段的付款视图
    ///
    /// # 返回
    /// 返回带供应商名称与核销来源单号的付款视图。
    ///
    /// # 错误
    /// 仓储读取失败时返回错误；主数据或来源单据缺失不视为错误。
    pub(super) async fn enrich_supplier_payment_view(
        &self,
        mut view: SupplierPaymentView,
    ) -> Result<SupplierPaymentView> {
        let (supplier_no, supplier_name) = resolve_supplier_display(&self.db, &view.supplier_id).await?;
        view.supplier_no = supplier_no;
        view.supplier_name = supplier_name;
        view.allocations = enrich_payment_allocation_views(&self.db, view.allocations).await?;
        Ok(view)
    }
}

/// 为付款核销分配补全应付子账与来源业务单号。
///
/// # 参数
/// * `db` - 数据库实例
/// * `views` - 仅含分录主键的分配视图
///
/// # 返回
/// 返回补全后的分配视图；找不到对应分录或子账的行保持展示字段为空。
///
/// # 错误
/// 仓储读取失败时返回错误。
async fn enrich_payment_allocation_views(
    db: &Database,
    views: Vec<PaymentAllocationView>,
) -> Result<Vec<PaymentAllocationView>> {
    let sources = load_allocation_sources(db, &views).await?;
    Ok(views
        .into_iter()
        .map(|mut view| {
            let key = view.payable_entry_id.clone();
            apply_allocation_source(&mut view, sources.get(&key));
            view
        })
        .collect())
}

/// 把已解析的来源展示字段写入分配视图。
///
/// # 参数
/// * `view` - 待写入的分配视图
/// * `source` - 分录对应的应付来源；缺失时不清空既有空值
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
fn apply_allocation_source(view: &mut PaymentAllocationView, source: Option<&AllocationSource>) {
    let Some(source) = source else {
        return;
    };
    view.payable_account_id = Some(source.payable_account_id.clone());
    view.source_type = Some(source.source_type);
    view.source_document_id = Some(source.source_document_id.clone());
    view.source_document_no = source.source_document_no.clone();
}

/// 按分配行批量加载应付分录与子账来源。
///
/// # 参数
/// * `db` - 数据库实例
/// * `views` - 付款核销分配视图
///
/// # 返回
/// 返回以应付分录 ID 为键的来源映射。
///
/// # 错误
/// 仓储读取失败时返回错误。
async fn load_allocation_sources(
    db: &Database,
    views: &[PaymentAllocationView],
) -> Result<HashMap<String, AllocationSource>> {
    let entry_ids = allocation_entry_ids(views);
    let entries = db
        .payable_entries()
        .find_entries_by_ids(&entry_ids, &mut NoTransaction)
        .await?;
    let accounts = load_accounts_for_entries(db, &entries).await?;
    let mut document_nos = HashMap::new();
    collect_source_document_nos(db, accounts.values(), &mut document_nos).await?;
    Ok(map_allocation_sources(&entries, &accounts, &document_nos))
}

/// 收集分配视图中的应付分录主键。
///
/// # 参数
/// * `views` - 付款核销分配视图
///
/// # 返回
/// 返回去重后的分录 ID 列表。
///
/// # 错误
/// 无。
fn allocation_entry_ids(views: &[PaymentAllocationView]) -> Vec<PayableEntryId> {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for view in views {
        let id = view.payable_entry_id.trim();
        if id.is_empty() || !seen.insert(id.to_string()) {
            continue;
        }
        ids.push(PayableEntryId::new(id));
    }
    ids
}

/// 按分录批量加载应付子账。
///
/// # 参数
/// * `db` - 数据库实例
/// * `entries` - 已加载的应付分录
///
/// # 返回
/// 返回以子账 ID 为键的子账映射。
///
/// # 错误
/// 仓储读取失败时返回错误。
async fn load_accounts_for_entries(
    db: &Database,
    entries: &[PayableEntry],
) -> Result<HashMap<String, PayableAccount>> {
    let mut seen = HashSet::new();
    let mut account_ids = Vec::new();
    for entry in entries {
        let id = entry.payable_account_id.to_string();
        if seen.insert(id.clone()) {
            account_ids.push(PayableAccountId::new(id));
        }
    }
    let accounts = db
        .payable_accounts()
        .find_accounts_by_ids(&account_ids, &mut NoTransaction)
        .await?;
    Ok(accounts
        .into_iter()
        .map(|account| (account.base.id.clone(), account))
        .collect())
}

/// 解析子账来源业务单号并写入缓存。
///
/// # 参数
/// * `db` - 数据库实例
/// * `accounts` - 待解析的应付子账
/// * `document_nos` - 以子账 ID 为键的单号缓存
///
/// # 返回
/// 无。
///
/// # 错误
/// 仓储读取失败时返回错误。
async fn collect_source_document_nos<'a, I>(
    db: &Database,
    accounts: I,
    document_nos: &mut HashMap<String, Option<String>>,
) -> Result<()>
where
    I: IntoIterator<Item = &'a PayableAccount>,
{
    for account in accounts {
        if document_nos.contains_key(&account.base.id) {
            continue;
        }
        let document_no = resolve_source_document_no(db, account).await?;
        document_nos.insert(account.base.id.clone(), document_no);
    }
    Ok(())
}

/// 把分录、子账与业务单号装配为核销来源映射。
///
/// # 参数
/// * `entries` - 应付分录
/// * `accounts` - 应付子账
/// * `document_nos` - 子账来源业务单号
///
/// # 返回
/// 返回以分录 ID 为键的来源映射；找不到子账的分录被跳过。
///
/// # 错误
/// 无。
fn map_allocation_sources(
    entries: &[PayableEntry],
    accounts: &HashMap<String, PayableAccount>,
    document_nos: &HashMap<String, Option<String>>,
) -> HashMap<String, AllocationSource> {
    let mut sources = HashMap::new();
    for entry in entries {
        let account_id = entry.payable_account_id.to_string();
        let Some(account) = accounts.get(&account_id) else {
            continue;
        };
        sources.insert(
            entry.base.id.clone(),
            AllocationSource {
                payable_account_id: account_id,
                source_type: account.source_type,
                source_document_id: entry.source_document_id.clone(),
                source_document_no: document_nos.get(&account.base.id).cloned().flatten(),
            },
        );
    }
    sources
}
