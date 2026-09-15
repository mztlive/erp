//! 仅对已裁剪的事实排序分页，不使用隐藏金额或其他单据时间决定顺序。
use std::collections::HashMap;

use application_core::PageView;
use erp_finance::dto::cost::{
    CostAllocationListQuery, CostAllocationView, PageParams, ScopedCostEntryView, SortDir,
};

use super::{Error, Result, page_offset};

/// 成本列表金额排序使用可见分配份额，稳定次序以成本身份收尾。
pub(super) fn costs(mut rows: Vec<ScopedCostEntryView>, paging: PageParams) -> PageView<ScopedCostEntryView> {
    rows.sort_by(|a, b| {
        let order = match paging.sort_by {
            "gross_amount" => a.scope_gross_amount.cmp(&b.scope_gross_amount),
            "net_amount" => a.scope_net_amount.cmp(&b.scope_net_amount),
            "occurred_at" => a.occurred_at.cmp(&b.occurred_at),
            _ => a.created_at.cmp(&b.created_at),
        };
        order.then_with(|| a.id.cmp(&b.id))
    });
    if matches!(paging.sort_dir, SortDir::Desc) {
        rows.reverse();
    }
    page(rows, paging)
}

/// 先对授权分配应用业务条件，再用分配本身的持久化时间排序和分页。
pub(super) fn allocations(
    rows: Vec<CostAllocationView>,
    created: &HashMap<String, u64>,
    query: &CostAllocationListQuery,
) -> Result<PageView<CostAllocationView>> {
    let mut rows = rows
        .into_iter()
        .filter(|line| {
            query.sales_order_id.as_ref().is_none_or(|id| line.sales_order_id.as_deref() == Some(id.as_ref()))
        })
        .map(|line| {
            let time = created
                .get(&line.id)
                .copied()
                .ok_or_else(|| Error::Internal("成本分配缺少持久化创建时间".into()))?;
            Ok((time, line))
        })
        .collect::<Result<Vec<_>>>()?;
    rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
    if matches!(query.paging.sort_dir, SortDir::Desc) {
        rows.reverse();
    }
    Ok(page(rows.into_iter().map(|(_, row)| row).collect(), query.paging))
}

/// 总数取完整授权且匹配条件的集合，分页不得影响汇总或溢出页码。
fn page<T>(rows: Vec<T>, paging: PageParams) -> PageView<T> {
    let total = rows.len() as i64;
    PageView {
        items: rows
            .into_iter()
            .skip(page_offset(paging.page, paging.page_size))
            .take(paging.page_size as usize)
            .collect(),
        total,
        page: paging.page,
        page_size: paging.page_size,
    }
}

#[cfg(test)]
mod tests {
    use erp_core::money::Amount;

    use super::*;
    fn line(id: &str, order: &str) -> CostAllocationView {
        CostAllocationView {
            id: id.into(),
            cost_entry_id: "older-cost".into(),
            sales_order_id: Some(order.into()),
            sales_order_line_id: None,
            allocated_gross_amount: Amount::zero(),
            allocated_net_amount: Amount::zero(),
            rounding_residual_flag: false,
        }
    }
    #[test]
    fn allocation_time_and_id_order_before_pagination_and_total_uses_filtered_facts() {
        let rows = vec![line("b", "sales-a"), line("a", "sales-a"), line("c", "sales-b")];
        let times = HashMap::from([("a".into(), 20), ("b".into(), 10), ("c".into(), 5)]);
        let query = CostAllocationListQuery {
            sales_order_id: Some(erp_core::ids::SalesOrderId::new("sales-a")),
            cost_entry_id: None,
            paging: PageParams { page: 2, page_size: 1, sort_by: "created_at", sort_dir: SortDir::Asc },
        };
        let result = allocations(rows.clone(), &times, &query).unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.items[0].id, "a");
        let mut tie = times;
        tie.insert("b".into(), 20);
        let result = allocations(rows.clone(), &tie, &query).unwrap();
        assert_eq!(result.items[0].id, "b");
        tie.remove("b");
        assert!(allocations(rows, &tie, &query).is_err());
    }
}
