//! 同一应收快照的核销净额与当前页分录事实。

use std::collections::{BTreeSet, HashMap};

use erp_core::ids::{ReceivableAccountId, ReceivableEntryId};
use erp_core::money::Amount;
use erp_finance::dto::receivable::ReceivableEntryView;
use erp_finance::entity::receivable::{ReceiptAllocation, ReceivableEntry};
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use persistence_core::Executor;

use super::super::authorization::FundsAccess;
use super::super::receivable_display::entry_view;
use crate::Result;

/// 每个子账的净核销额；只有当前页保留分录视图以限制内存占用。
#[derive(Default)]
pub(super) struct ReceivableFacts {
    shares: HashMap<String, Amount>,
    entries: HashMap<String, Vec<ReceivableEntryView>>,
}

impl ReceivableFacts {
    /// 合并当前页之外的子账事实；调用方保证每个子账只装载一批。
    pub(super) fn extend(&mut self, other: Self) {
        self.shares.extend(other.shares);
        self.entries.extend(other.entries);
    }

    /// 返回该子账净额；无分录或无核销分配均为零。
    pub(super) fn share(&self, account_id: &str) -> Amount {
        self.shares.get(account_id).copied().unwrap_or_else(Amount::zero)
    }

    /// 取走当前页分录，沿用按来源序号稳定排序的展示口径。
    pub(super) fn take_entries(&mut self, account_id: &str) -> Vec<ReceivableEntryView> {
        let mut entries = self.entries.remove(account_id).unwrap_or_default();
        entries.sort_by_key(|entry| entry.source_sequence);
        entries
    }

    /// 单子账原查询结果按原返回顺序逐笔折叠，不按动作或分录重组运算。
    fn append_allocations(&mut self, account_id: &str, allocations: Vec<ReceiptAllocation>) {
        let mut net = Amount::zero();
        for allocation in allocations {
            net = allocation.allocation_action.apply_to_net(net, allocation.allocated_amount);
        }
        self.shares.insert(account_id.to_string(), net);
    }

    /// 当前页的批量分录按仓储返回顺序保留，稳定排序仅在生成展示时执行。
    fn append_entries(&mut self, entries: Vec<ReceivableEntry>, display_ids: &BTreeSet<String>) {
        for entry in entries {
            if display_ids.contains(entry.receivable_account_id.as_ref()) {
                self.entries
                    .entry(entry.receivable_account_id.to_string())
                    .or_default()
                    .push(entry_view(entry));
            }
        }
    }
}

/// 逐子账收集完整分录集合，保留批量返回流的子账内顺序，不扩大核销查询边界。
fn entry_ids_by_account(entries: &[ReceivableEntry]) -> HashMap<String, Vec<ReceivableEntryId>> {
    let mut ids: HashMap<String, Vec<ReceivableEntryId>> = HashMap::new();
    for entry in entries {
        ids.entry(entry.receivable_account_id.to_string())
            .or_default()
            .push(ReceivableEntryId::new(entry.base.id.clone()));
    }
    ids
}

impl FundsAccess {
    /// 本次事务批量装载分录，核销沿用单子账查询边界；页行与汇总共享事实。
    ///
    /// # 参数
    /// * `account_ids` - 已授权、已筛选的全部子账
    /// * `display_ids` - 需要返回分录的当前页子账
    /// * `executor` - 本次快照事务，不跨快照复用
    ///
    /// # 返回
    /// 返回全部子账净额和当前页分录；分录每批最多 500 个子账，核销逐子账读取。
    ///
    /// # 错误
    /// 读取失败时返回仓储错误；金额溢出语义沿用原逐笔折叠。
    pub(super) async fn receivable_facts(
        &self,
        account_ids: &[String],
        display_ids: &BTreeSet<String>,
        executor: &mut dyn Executor,
    ) -> Result<ReceivableFacts> {
        let mut facts = ReceivableFacts::default();
        for chunk in account_ids.chunks(500) {
            let ids = chunk.iter().map(ReceivableAccountId::new).collect::<Vec<_>>();
            let entries = self.db.receivable_entries().find_entries_by_accounts(&ids, executor).await?;
            let mut ids = entry_ids_by_account(&entries);
            for account_id in chunk {
                let keys = ids.remove(account_id).unwrap_or_default();
                let allocations =
                    self.db.receipt_allocations().find_allocations_by_entries(&keys, executor).await?;
                facts.append_allocations(account_id, allocations);
            }
            facts.append_entries(entries, display_ids);
        }
        Ok(facts)
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::stable::StableBase;
    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{CustomerAccountId, CustomerReceiptId, PartyId, ReceiptAllocationId, SalesOrderId};
    use erp_finance::entity::receivable::{
        AllocationAction, EntryDirection, ReceivableAccountStatus, ReceivableEntryType,
    };
    use erp_finance::repository::receivable::ReceivableAccountRow;
    use erp_sales::entity::sales_order::{BusinessType, OriginSystem, SalesOrder, SalesOrderData};

    use super::super::list::{receivable_page_rows, receivable_summary};
    use super::*;

    /// 构造来自仓储的有效分录事实。
    fn entry(id: &str, account: &str, sequence: u32) -> ReceivableEntry {
        ReceivableEntry {
            base: BaseModel { id: id.into(), ..BaseModel::fake() },
            receivable_account_id: ReceivableAccountId::new(account),
            entry_type: ReceivableEntryType::Original,
            direction: EntryDirection::Increase,
            amount: "100".parse().unwrap(),
            due_date: BusinessDate::from_ymd(2026, 10, 1).unwrap(),
            source_fact_type: "sales_order".into(),
            source_document_id: "so-1".into(),
            source_revision_id: "rev-1".into(),
            source_sequence: sequence,
            posted_at: Instant::from_unix_secs(1),
        }
    }

    /// 构造正向或反向分配，不改变存储金额符号。
    fn allocation(entry: &str, action: AllocationAction, amount: &str) -> ReceiptAllocation {
        ReceiptAllocation {
            base: BaseModel::fake(),
            customer_receipt_id: CustomerReceiptId::new("r-1"),
            receivable_entry_id: ReceivableEntryId::new(entry),
            allocation_seq: 1,
            allocation_action: action,
            allocated_amount: amount.parse().unwrap(),
            allocated_at: Instant::from_unix_secs(1),
            reverses_allocation_id: (action == AllocationAction::Reverse)
                .then(|| ReceiptAllocationId::new("original")),
        }
    }

    /// 构造实际分页组装所需的仓储投影行。
    fn account(id: &str, order: &str, gross: &str) -> ReceivableAccountRow {
        let zero = Amount::zero();
        ReceivableAccountRow {
            id: id.into(),
            stable: StableBase::new(ReceivableAccountStatus::Open, "register"),
            sales_order_id: order.into(),
            account_seq: 1,
            customer_id: "customer".into(),
            counterparty_party_id: "party".into(),
            gross_total: gross.parse().unwrap(),
            settled_total: zero,
            open_total: gross.parse().unwrap(),
            invoiceable_total: gross.parse().unwrap(),
            invoiced_total: zero,
            open_invoiceable_total: gross.parse().unwrap(),
            version: 7,
            created_at: 1,
        }
    }

    /// 构造已通过授权的真实销售来源实体。
    fn order(id: &str, owner: &str) -> SalesOrder {
        SalesOrder::new(
            SalesOrderId::new(id),
            SalesOrderData {
                sales_owner_user_id: owner.into(),
                business_org_unit_id: "org".into(),
                order_no: format!("SO-{id}"),
                business_type: BusinessType::GoodsService,
                origin_system: OriginSystem::Erp,
                source_identity_id: None,
                customer_id: CustomerAccountId::new("customer"),
                contract_id: None,
                settlement_party_id: PartyId::new("party"),
                source_status_code: None,
            },
            owner,
        )
        .unwrap()
    }

    /// 页行取走分录后，汇总仍使用同一净额并覆盖全部匹配行。
    #[test]
    fn batched_receivable_facts_page_and_summary_share_net_amounts_without_reloading() {
        let rows = vec![account("a", "so-a", "100"), account("b", "so-b", "60")];
        let orders =
            HashMap::from([("so-a".into(), order("so-a", "owner")), ("so-b".into(), order("so-b", "owner"))]);
        let mut facts = ReceivableFacts::default();
        facts.append_allocations(
            "a",
            vec![
                allocation("a-1", AllocationAction::Apply, "70.01"),
                allocation("a-1", AllocationAction::Reverse, "20"),
            ],
        );
        facts.append_entries(vec![entry("a-1", "a", 1)], &BTreeSet::from(["a".into()]));
        let page = receivable_page_rows(&rows[..1], &orders, &mut facts);
        let mut rest = ReceivableFacts::default();
        rest.append_allocations("b", vec![allocation("b-1", AllocationAction::Apply, "20.02")]);
        facts.extend(rest);
        let summary = receivable_summary(&rows, &orders, &facts, "v").unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].id, "a");
        assert_eq!(page[0].sales_order_no, "SO-so-a");
        assert_eq!(page[0].sales_owner_user_id.as_deref(), Some("owner"));
        assert_eq!(page[0].visible_settled_share, "50.01".parse().unwrap());
        assert_eq!(page[0].gross_total, Some("100".parse().unwrap()));
        assert_eq!(page[0].entries.len(), 1);
        assert_eq!(summary.grouped[0].visible_share, "70.03".parse().unwrap());
        assert_eq!(summary.whole_total, Some("160".parse().unwrap()));
        assert_eq!(summary.scope_version, "v");
        assert_eq!(summary.unassigned, Amount::zero());
    }

    /// 单子账核销查询输入只含其完整分录，批量分录读取不合并核销边界。
    #[test]
    fn batched_receivable_facts_keep_complete_single_account_allocation_query_keys() {
        let entries =
            vec![entry("a-2", "a", 2), entry("b-1", "b", 1), entry("a-1", "a", 1), entry("b-2", "b", 2)];
        let mut keys = entry_ids_by_account(&entries);
        let a = keys.remove("a").unwrap();
        let b = keys.remove("b").unwrap();
        assert_eq!(a.iter().map(AsRef::as_ref).collect::<Vec<&str>>(), ["a-2", "a-1"]);
        assert_eq!(b.iter().map(AsRef::as_ref).collect::<Vec<&str>>(), ["b-1", "b-2"]);
        assert!(!keys.contains_key("missing"));
        assert!(entry_ids_by_account(&[]).is_empty());
    }

    /// 单子账净额与当前页分录共享事实，同来源序号保持仓储顺序。
    #[test]
    fn batched_receivable_facts_group_shares_and_keep_page_entries() {
        let mut facts = ReceivableFacts::default();
        facts.append_allocations(
            "a",
            vec![
                allocation("a-1", AllocationAction::Apply, "60.12"),
                allocation("a-2", AllocationAction::Apply, "0.01"),
                allocation("a-1", AllocationAction::Reverse, "10.11"),
            ],
        );
        facts.append_allocations("b", vec![allocation("b-1", AllocationAction::Apply, "40")]);
        facts.append_entries(
            vec![
                entry("a-tie-first", "a", 2),
                entry("b-1", "b", 1),
                entry("a-1", "a", 1),
                entry("a-tie-second", "a", 2),
            ],
            &BTreeSet::from(["a".into()]),
        );
        assert_eq!(facts.share("a"), "50.02".parse().unwrap());
        assert_eq!(facts.share("b"), "40".parse().unwrap());
        let entries = facts.take_entries("a");
        assert_eq!(
            entries.iter().map(|entry| entry.id.as_str()).collect::<Vec<_>>(),
            ["a-1", "a-tie-first", "a-tie-second"]
        );
        assert_eq!(entries[0].amount, "100".parse().unwrap());
        assert_eq!(entries[0].offset_total, Amount::zero());
        assert!(facts.take_entries("b").is_empty());
        assert_eq!(facts.share("a"), "50.02".parse().unwrap());
    }

    /// 空事实、缺失子账和完全反向仍返回零额。
    #[test]
    fn batched_receivable_facts_empty_missing_and_full_reverse_are_zero() {
        let mut facts = ReceivableFacts::default();
        facts.append_allocations("empty", Vec::new());
        facts.append_entries(Vec::new(), &BTreeSet::new());
        assert_eq!(facts.share("empty"), Amount::zero());
        facts.append_allocations(
            "a",
            vec![
                allocation("a-1", AllocationAction::Apply, "9.99"),
                allocation("a-1", AllocationAction::Reverse, "9.99"),
            ],
        );
        assert_eq!(facts.share("a"), Amount::zero());
        assert_eq!(facts.share("missing"), Amount::zero());
        assert!(facts.entries.is_empty());
    }

    /// 单子账核销保留原查询返回顺序，极值边界不按正反动作重组。
    #[test]
    fn batched_receivable_facts_preserve_single_account_order_near_amount_limit() {
        let max = "79228162514264337593543950335";
        let mut facts = ReceivableFacts::default();
        facts.append_allocations(
            "a",
            vec![
                allocation("a-1", AllocationAction::Apply, max),
                allocation("a-1", AllocationAction::Reverse, "1"),
                allocation("a-1", AllocationAction::Apply, "1"),
            ],
        );
        facts.append_allocations(
            "b",
            vec![
                allocation("b-1", AllocationAction::Apply, max),
                allocation("b-1", AllocationAction::Reverse, "1"),
            ],
        );
        assert_eq!(facts.share("a"), max.parse().unwrap());
        assert_eq!(facts.share("b"), max.parse::<Amount>().unwrap().checked_sub("1".parse().unwrap()));
    }

    /// 中间值溢出保持原逐笔运算的失败语义。
    #[test]
    #[should_panic]
    fn batched_receivable_facts_keep_existing_overflow_behavior() {
        let mut facts = ReceivableFacts::default();
        facts.append_allocations(
            "a",
            vec![
                allocation("a-1", AllocationAction::Apply, "79228162514264337593543950335"),
                allocation("a-1", AllocationAction::Apply, "1"),
                allocation("a-1", AllocationAction::Reverse, "1"),
            ],
        );
    }
}
