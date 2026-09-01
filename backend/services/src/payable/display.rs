//! 供应商往来只读展示字段装配：供应商名称、来源业务单号、付款核销目标单据。
//!
//! 内部主键不得作为业务单号或名称回填；主数据或来源单据缺失时字段为空。

use std::collections::{HashMap, HashSet};

use database::{NoTransaction, PayableExt, ReturnsExt};
use entities::ids::{PayableAccountId, PayableEntryId, SupplierPaymentId};
use entities::payable::{PayableAccount, PayableEntry, PayableSourceType};
use entities::returns::PaymentReversal;
use mongodb::Database;

use super::dto::{PaymentAllocationView, SupplierPaymentReversalView, SupplierPaymentView};
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

    /// 批量把冲正记录挂到原付款视图，避免付款列表逐行查询。
    ///
    /// # 参数
    /// * `views` - 当前付款列表或单笔详情视图
    ///
    /// # 返回
    /// 成功时就地写入按创建时间倒序的冲正摘要。
    ///
    /// # 错误
    /// 冲正记录查询失败时返回错误。
    pub(super) async fn attach_supplier_payment_reversals(
        &self,
        views: &mut [SupplierPaymentView],
    ) -> Result<()> {
        let payment_ids = supplier_payment_ids(views);
        if payment_ids.is_empty() {
            return Ok(());
        }
        let reversals = self
            .db
            .payment_reversals()
            .find_reversals_by_payments(&payment_ids, &mut NoTransaction)
            .await?;
        let mut grouped = group_payment_reversals(reversals);
        for view in views {
            view.related_reversals = grouped.remove(&view.id).unwrap_or_default();
        }
        Ok(())
    }
}

/// 收集付款视图主键并保持去重，空列表不触发仓储查询。
fn supplier_payment_ids(views: &[SupplierPaymentView]) -> Vec<SupplierPaymentId> {
    let mut seen = HashSet::new();
    views
        .iter()
        .filter(|view| seen.insert(view.id.clone()))
        .map(|view| SupplierPaymentId::new(view.id.clone()))
        .collect()
}

/// 把冲正实体按原付款分组，并形成稳定的最近优先展示顺序。
fn group_payment_reversals(
    reversals: Vec<PaymentReversal>,
) -> HashMap<String, Vec<SupplierPaymentReversalView>> {
    let mut grouped = HashMap::<String, Vec<SupplierPaymentReversalView>>::new();
    for reversal in reversals {
        let payment_id = reversal.original_supplier_payment_id.to_string();
        grouped
            .entry(payment_id)
            .or_default()
            .push(payment_reversal_view(reversal));
    }
    for items in grouped.values_mut() {
        items.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.reversal_no.cmp(&left.reversal_no))
        });
    }
    grouped
}

/// 把付款冲正实体裁剪成付款列表允许公开的摘要字段。
fn payment_reversal_view(reversal: PaymentReversal) -> SupplierPaymentReversalView {
    SupplierPaymentReversalView {
        id: reversal.base.id,
        reversal_no: reversal.reversal_no,
        status: reversal.status,
        reason_text: reversal.reason_text,
        amount: reversal.amount,
        occurred_at: reversal.occurred_at,
        created_at: reversal.base.created_at,
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entities::common::time::Instant;
    use entities::ids::{PaymentReversalId, SupplierPaymentId};
    use entities::money::Amount;
    use entities::returns::{PaymentReversal, PaymentReversalData};

    use super::group_payment_reversals;

    /// 关联冲正按原付款分组，并稳定展示最近创建记录。
    #[test]
    fn payment_reversals_group_by_original_payment_and_sort_recent_first() {
        let older = reversal("reversal-1", "PCZ-1", "payment-1", 10);
        let newer = reversal("reversal-2", "PCZ-2", "payment-1", 20);
        let other = reversal("reversal-3", "PCZ-3", "payment-2", 15);

        let grouped = group_payment_reversals(vec![older, other, newer]);

        assert_eq!(grouped["payment-1"].len(), 2);
        assert_eq!(grouped["payment-1"][0].reversal_no, "PCZ-2");
        assert_eq!(grouped["payment-1"][1].reversal_no, "PCZ-1");
        assert_eq!(grouped["payment-2"][0].reversal_no, "PCZ-3");
    }

    /// 构造最小付款冲正事实供只读投影测试使用。
    fn reversal(id: &str, reversal_no: &str, payment_id: &str, created_at: u64) -> PaymentReversal {
        let mut reversal = PaymentReversal::new(
            PaymentReversalId::new(id),
            PaymentReversalData {
                reversal_no: reversal_no.to_string(),
                original_supplier_payment_id: SupplierPaymentId::new(payment_id),
                reason_code: None,
                reason_text: "付款信息有误".to_string(),
                amount: Amount::from_str("880").expect("金额合法"),
                handled_by: "cashier".to_string(),
                reviewed_by: "reviewer".to_string(),
                occurred_at: Instant::from_unix_secs(created_at as i64),
                evidence_attachment_id: None,
            },
            "creator",
        )
        .expect("冲正事实合法");
        reversal.base.created_at = created_at;
        reversal
    }
}
