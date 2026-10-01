//! 多来源款项的窄版本、份额汇总及数据库分页公共合同。

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use erp_core::money::Amount;
use erp_finance::entity::payable::PayableSourceType;
use erp_finance::entity::receivable::AllocationAction;
use erp_finance::repository::FinancialSummaryLink;
use mongodb::bson::{Bson, Document, doc};
use serde::Deserialize;

use super::super::rows::{FundsSummaryView, build_summary, zero_amount};
use super::super::whole_amount;
use crate::{Error, Result};

/// 最终匹配集合的有界规模，额外一条只用于拒绝不完整结果。
pub(in crate::finance::funds_scope) const FLOW_LIMIT: u64 = 10_000;

/// 同一快照的数据库页、完整版本及窄份额摘要。
#[derive(Deserialize)]
pub(in crate::finance::funds_scope) struct FlowPage<T> {
    pub items: Vec<T>,
    #[serde(default)]
    pub versions: Vec<FlowVersion>,
    pub total: Vec<FlowCount>,
    #[serde(default)]
    pub summary: Vec<FlowSummary>,
}

/// 同条件下数据库计数，空集合的 count 分支为空。
#[derive(Deserialize)]
pub(in crate::finance::funds_scope) struct FlowCount {
    pub count: u64,
}

/// 主表及全部匹配来源版本；不包含金额、分配视图或名称。
#[derive(Debug, Deserialize)]
pub(in crate::finance::funds_scope) struct FlowVersion {
    pub id: String,
    pub version: u64,
    pub sources: Vec<SourceVersion>,
}

/// 按来源 ID 排序去重后的责任版本。
#[derive(Debug, Deserialize)]
pub(in crate::finance::funds_scope) struct SourceVersion {
    pub id: String,
    pub version: u64,
}

/// 首拍逐行投影责任版本与主表金额，不把所有来源或核销流合入一个 BSON。
#[derive(Deserialize)]
pub(in crate::finance::funds_scope) struct FlowHeader {
    #[serde(flatten)]
    pub version: FlowVersion,
    pub amount: Amount,
    pub whole: bool,
    pub owners: Vec<SourceOwner>,
}

/// 同拍来源责任人，复用授权关联结果而不另读跨域事实。
#[derive(Deserialize)]
pub(in crate::finance::funds_scope) struct SourceOwner {
    pub id: String,
    pub owner: Option<String>,
}

/// 汇总只装载权威金额和授权份额，完整分配 DTO 留在当前页。
#[derive(Deserialize)]
pub(in crate::finance::funds_scope) struct FlowSummary {
    pub amount: Amount,
    pub whole: bool,
    pub links: Vec<SummaryLink>,
}

/// 份额流保留原动作与读取次序，不按正反动作重组金额。
#[derive(Deserialize)]
pub(in crate::finance::funds_scope) struct SummaryLink {
    pub id: String,
    pub order: Option<String>,
    pub owner: Option<String>,
    pub amount: Amount,
    pub action: String,
}

impl<T> FlowPage<T> {
    /// 拒绝超过完整版本上限的结果，保留数据库真实总数。
    pub fn count(&self) -> Result<u64> {
        let total = self.total.first().map_or(0, |row| row.count);
        if total >= FLOW_LIMIT {
            return Err(Error::ValidationError("资金查询超过上限，请收窄业务条件".into()));
        }
        Ok(total)
    }
}

/// 版本绑定全部匹配主表及来源，保持原候选排序和来源 ID 顺序。
pub(in crate::finance::funds_scope) fn version(scope: &str, rows: &[FlowVersion]) -> String {
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    scope.hash(&mut fingerprint);
    for row in rows {
        row.id.hash(&mut fingerprint);
        row.version.hash(&mut fingerprint);
        let sources = row
            .sources
            .iter()
            .map(|source| (&source.id, source.version))
            .collect::<std::collections::BTreeMap<_, _>>();
        for source in sources.values() {
            source.hash(&mut fingerprint);
        }
    }
    format!("{:x}", fingerprint.finish())
}

/// 沿用金额逐笔折叠与人员归组，未授权份额永不进入汇总。
pub(in crate::finance::funds_scope) fn summary(
    rows: &[FlowSummary],
    version: &str,
) -> Result<FundsSummaryView> {
    let mut triples = Vec::new();
    let mut owners = HashMap::new();
    let mut whole = true;
    let mut total = zero_amount();
    for row in rows {
        whole &= row.whole;
        total = total.checked_add(row.amount);
        for link in &row.links {
            let signed = signed(link)?;
            triples.push((link.id.clone(), signed, link.order.clone()));
            if let (Some(order), Some(owner)) = (&link.order, &link.owner) {
                owners.insert(order.clone(), owner.clone());
            }
        }
    }
    build_summary(&triples, &owners, whole_amount(whole, total), version, !whole)
}

/// 固定动作转换复用原正负方向；持久化非法动作整体拒绝。
fn signed(link: &SummaryLink) -> Result<Amount> {
    match link.action.as_str() {
        "apply" => Ok(link.amount),
        "reverse" => Ok(zero_amount().checked_sub(link.amount)),
        _ => Err(Error::Internal("核销分配动作非法".into())),
    }
}

/// 来源版本按身份去重并稳定排序，与 Rust BTreeSet 的旧口径一致。
pub(super) fn source_versions() -> Bson {
    doc! { "$sortArray": { "input": { "$setUnion": [{ "$map": {
        "input": { "$filter": { "input": "$scope_links", "as": "link", "cond": "$$link.matched" } },
        "as": "link", "in": { "id": "$$link.order", "version": "$$link.source_version" }
    } }, []] }, "sortBy": { "id": 1 } } }
    .into()
}

/// 完整责任版本投影不包含金额、分配视图和附件。
pub(super) fn version_projection() -> Document {
    doc! { "_id": 0, "id": 1, "version": 1, "sources": source_versions() }
}

/// 首拍只保留逐行主表金额及已经匹配的来源责任人。
pub(super) fn header_stages(sort: Document) -> Vec<Document> {
    let mut projection = version_projection();
    projection.insert("amount", 1);
    projection.insert("whole", "$scope_whole");
    projection.insert(
        "owners",
        doc! { "$map": {
            "input": { "$filter": { "input": "$scope_links", "as": "link", "cond": "$$link.matched" } },
            "as": "link", "in": { "id": "$$link.order", "owner": "$$link.owner" }
        } },
    );
    vec![doc! { "$sort": sort }, doc! { "$limit": 10_000_i64 }, doc! { "$project": projection }]
}

/// 页面分支只包含当前页与完整计数；金额和责任版本使用独立游标。
pub(super) fn facet(page: u64, size: u32, projection: Document, sort: Document) -> Result<Document> {
    Ok(super::page_only_facet(page, size, sort, projection))
}

/// 原父单集合 find 流按父单归组；保留父单内顺序及实际来源类型。
pub(in crate::finance::funds_scope) fn summaries(
    headers: Vec<FlowHeader>,
    links: Vec<FinancialSummaryLink>,
) -> (Vec<FlowVersion>, Vec<FlowSummary>) {
    let mut grouped: HashMap<String, Vec<FinancialSummaryLink>> = HashMap::new();
    for link in links {
        grouped.entry(link.parent_id.clone()).or_default().push(link);
    }
    let mut versions = Vec::with_capacity(headers.len());
    let mut summary = Vec::with_capacity(headers.len());
    for header in headers {
        let owners =
            header.owners.into_iter().map(|source| (source.id, source.owner)).collect::<HashMap<_, _>>();
        let links = grouped
            .remove(&header.version.id)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|link| summary_link(link, &owners))
            .collect();
        summary.push(FlowSummary { amount: header.amount, whole: header.whole, links });
        versions.push(header.version);
    }
    (versions, summary)
}

/// 来源身份保持采购与结算区分，只有同拍已匹配来源可以进入汇总。
fn summary_link(link: FinancialSummaryLink, owners: &HashMap<String, Option<String>>) -> Option<SummaryLink> {
    let order = link.source_document_id.map(|id| match link.source_type {
        Some(PayableSourceType::SupplierSettlement) => format!("supplier_settlement_statement:{id}"),
        _ => id,
    });
    let owner = match &order {
        Some(order) => owners.get(order)?.clone(),
        None => None,
    };
    Some(SummaryLink {
        id: link.id,
        order,
        owner,
        amount: link.amount,
        action: match link.action {
            AllocationAction::Apply => "apply",
            AllocationAction::Reverse => "reverse",
        }
        .into(),
    })
}

/// 重新授权只返回完整窄版本，不装配第二份页面及金额。
pub(super) fn recheck_stages(sort: Document) -> Vec<Document> {
    vec![doc! { "$sort": sort }, doc! { "$limit": 10_000_i64 }, doc! { "$project": version_projection() }]
}

/// 核销单据可见性沿用完整来源存在、匹配份额和整账资格的交集。
pub(super) fn visibility(ledger: bool, require_source: bool) -> Vec<Document> {
    let whole = doc! { "$and": [ledger, { "$allElementsTrue": [{ "$map": {
        "input": "$scope_links", "as": "link", "in": "$$link.matched"
    } }] }] };
    let matched = doc! { "$anyElementTrue": [{ "$map": { "input": "$scope_links", "as": "link", "in": "$$link.matched" } }] };
    let visible = if require_source {
        matched.clone()
    } else {
        doc! { "$or": [whole.clone(), matched.clone()] }
    };
    vec![
        doc! { "$set": { "scope_whole": whole } },
        doc! { "$match": { "$expr": { "$and": [
            { "$allElementsTrue": [{ "$map": { "input": "$scope_links", "as": "link", "in": "$$link.exists" } }] }, visible
        ] } } },
    ]
}

/// 对关联身份执行拥有领域的明确过滤，不在仓储推断登录权限。
pub(super) fn lookup(
    collection: &str,
    field: &str,
    target: &str,
    output: &str,
    extra: Document,
    projection: Document,
) -> Document {
    doc! { "$lookup": { "from": collection, "let": { "linked": format!("${field}") }, "pipeline": [
        { "$match": { "$and": [{ "deleted_at": 0_i64, "$expr": { "$eq": [format!("${target}"), "$$linked"] } }, extra] } },
        { "$project": projection }
    ], "as": output } }
}

/// 取关联结果首值；缺失引用保持 null，不能当成零分配。
pub(super) fn first(path: &str) -> Bson {
    doc! { "$ifNull": [{ "$arrayElemAt": [format!("${path}"), 0] }, Bson::Null] }.into()
}

/// 排序继承原主表字段及身份尾键，方向同时作用于尾键。
pub(super) fn sort(field: &str, ascending: bool) -> Document {
    let direction = if ascending { 1 } else { -1 };
    let mut result = Document::new();
    result.insert(field, direction);
    result.insert("id", direction);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造首拍逐行轻事实，匹配来源与归属均来自同拍授权关联。
    fn header(id: &str, sources: &[(&str, Option<&str>)]) -> FlowHeader {
        FlowHeader {
            version: FlowVersion {
                id: id.into(),
                version: 1,
                sources: sources
                    .iter()
                    .map(|(id, _)| SourceVersion { id: (*id).into(), version: 2 })
                    .collect(),
            },
            amount: "1".parse().unwrap(),
            whole: false,
            owners: sources
                .iter()
                .map(|(id, owner)| SourceOwner { id: (*id).into(), owner: owner.map(str::to_owned) })
                .collect(),
        }
    }

    /// 构造拥有领域原 find 返回的必要金额，不加入排序字段。
    fn amount_link(
        id: &str,
        parent: &str,
        source: &str,
        kind: Option<PayableSourceType>,
        amount: &str,
        action: AllocationAction,
    ) -> FinancialSummaryLink {
        FinancialSummaryLink {
            id: id.into(),
            parent_id: parent.into(),
            source_document_id: Some(source.into()),
            source_type: kind,
            amount: amount.parse().unwrap(),
            action,
        }
    }

    #[test]
    fn snapshot_summary_keeps_parent_stream_extremes_and_filters_actual_typed_sources() {
        let max = "79228162514264337593543950335";
        let headers = vec![
            header("first", &[("sale", Some("owner"))]),
            header("second", &[("supplier_settlement_statement:same", None)]),
        ];
        let links = vec![
            amount_link("max", "first", "sale", None, max, AllocationAction::Apply),
            amount_link(
                "hidden-same-id",
                "second",
                "same",
                Some(PayableSourceType::PurchaseOrder),
                "999",
                AllocationAction::Apply,
            ),
            amount_link("minus", "first", "sale", None, "1", AllocationAction::Reverse),
            amount_link(
                "settlement",
                "second",
                "same",
                Some(PayableSourceType::SupplierSettlement),
                "7",
                AllocationAction::Apply,
            ),
            amount_link("plus", "first", "sale", None, "1", AllocationAction::Apply),
        ];
        let (versions, summaries) = super::summaries(headers, links);
        assert_eq!(versions.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(), ["first", "second"]);
        assert_eq!(
            summaries[0].links.iter().map(|link| link.id.as_str()).collect::<Vec<_>>(),
            ["max", "minus", "plus"]
        );
        assert_eq!(summaries[1].links.len(), 1);
        assert_eq!(summaries[1].links[0].order.as_deref(), Some("supplier_settlement_statement:same"));
        assert_eq!(summary(&summaries[..1], "v").unwrap().grouped[0].visible_share, max.parse().unwrap());
        let unassigned = summary(&summaries[1..], "v").unwrap();
        assert!(unassigned.grouped.is_empty());
        assert_eq!(unassigned.unassigned, "7".parse().unwrap());
    }

    #[test]
    fn main_page_facet_never_packs_all_versions_or_summary_links() {
        let document = facet(u64::MAX, 100, doc! { "id": 1 }, doc! { "id": 1 }).unwrap();
        let branches = document.get_document("$facet").unwrap();
        assert_eq!(branches.len(), 2);
        assert!(branches.contains_key("items"));
        assert!(branches.contains_key("total"));
        let first = header_stages(doc! { "id": 1 });
        let second = recheck_stages(doc! { "id": 1 });
        assert!(first[2].get_document("$project").unwrap().contains_key("amount"));
        assert!(!second[2].get_document("$project").unwrap().contains_key("amount"));
    }

    #[test]
    fn partial_summary_excludes_hidden_and_whole_amounts_and_keeps_reverse() {
        let rows = vec![FlowSummary {
            amount: "100".parse().unwrap(),
            whole: false,
            links: vec![
                SummaryLink {
                    id: "a".into(),
                    order: Some("sale".into()),
                    owner: Some("owner".into()),
                    amount: "60".parse().unwrap(),
                    action: "apply".into(),
                },
                SummaryLink {
                    id: "b".into(),
                    order: Some("sale".into()),
                    owner: Some("owner".into()),
                    amount: "10".parse().unwrap(),
                    action: "reverse".into(),
                },
            ],
        }];
        let view = summary(&rows, "v").unwrap();
        assert_eq!(view.grouped[0].visible_share, "50".parse().unwrap());
        assert_eq!(view.whole_total, None);
        assert!(view.permission_limited);
        assert_eq!(view.scope_version, "v");
    }

    #[test]
    fn complete_versions_cover_off_page_changes_and_empty_results() {
        let first = FlowVersion { id: "first".into(), version: 1, sources: vec![] };
        let second = FlowVersion {
            id: "second".into(),
            version: 2,
            sources: vec![SourceVersion { id: "sale".into(), version: 1 }],
        };
        let before = version("scope", &[first, second]);
        let changed = version(
            "scope",
            &[
                FlowVersion { id: "first".into(), version: 1, sources: vec![] },
                FlowVersion {
                    id: "second".into(),
                    version: 2,
                    sources: vec![SourceVersion { id: "sale".into(), version: 2 }],
                },
            ],
        );
        assert_ne!(before, changed);
        assert_ne!(version("scope", &[]), version("new-scope", &[]));
        let page = FlowPage::<()> {
            items: vec![],
            versions: vec![],
            total: vec![FlowCount { count: 10_001 }],
            summary: vec![],
        };
        assert!(matches!(page.count(), Err(Error::ValidationError(_))));
    }
}
