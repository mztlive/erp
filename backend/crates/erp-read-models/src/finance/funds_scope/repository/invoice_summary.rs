//! 发票和分配的轻量金额流；保持 Rust 定点数归并和空值合同。

use std::collections::HashMap;

use erp_core::money::Amount;
use erp_finance::entity::payable::{AllocationAction, PayableSourceType};
use erp_finance::repository::{InvoiceSummaryLink, InvoiceSummaryRepository};
use mongodb::Database;
use persistence_core::Executor;
use serde::Deserialize;

use super::super::payable_source::source_key;
use super::super::rows::{FundsSummaryView, build_summary, zero_amount};
use super::super::whole_amount;
use super::invoice::InvoiceVersion;
use crate::{Error, Result};

/// 首拍版本游标顺带读取的必要票面资格，不包括分配金额数组。
#[derive(Debug, Deserialize)]
pub(super) struct InvoiceSummaryHeader {
    pub whole: bool,
    pub gross_amount: Amount,
    pub sales_sources: Vec<InvoiceSummarySource>,
    pub purchase_sources: Vec<InvoiceSummarySource>,
    pub all_sales_sources: Vec<InvoiceSummarySource>,
    pub all_purchase_sources: Vec<InvoiceSummarySource>,
}

/// 最终匹配来源的负责人；复核拍不传输此汇总材料。
#[derive(Debug, Deserialize)]
pub(super) struct InvoiceSummarySource {
    pub key: String,
    pub owner: Option<String>,
}

/// 最终匹配单据的票面金额及授权份额，不含完整单据或其他分配字段。
#[derive(Debug, Deserialize)]
pub(super) struct InvoiceSummaryRow {
    pub whole: bool,
    pub gross_amount: Amount,
    pub shares: Vec<InvoiceShare>,
    #[serde(default)]
    pub sales_owners: HashMap<String, String>,
    #[serde(default)]
    pub purchase_owners: HashMap<String, String>,
}

/// 份额沿真实正反动作记账，负责人来自真实来源。
#[derive(Debug, Deserialize)]
pub(super) struct InvoiceShare {
    pub id: String,
    pub order: String,
    pub owner: Option<String>,
    pub allocated_gross_amount: Amount,
    pub allocation_action: AllocationAction,
    #[serde(default)]
    pub purchase: bool,
    #[serde(default)]
    pub source_type: Option<PayableSourceType>,
}

impl InvoiceShare {
    /// 将已登记分配的正反动作转换为原定点方向金额。
    ///
    /// # 参数
    /// 当前原始正反动作与含税金额。
    /// # 返回
    /// 沿原动作方向转换的定点金额。
    /// # 错误
    /// 金额越界保留原金额类型行为。
    pub(super) fn signed(&self) -> Amount {
        match self.allocation_action {
            AllocationAction::Apply => self.allocated_gross_amount,
            AllocationAction::Reverse => zero_amount().checked_sub(self.allocated_gross_amount),
        }
    }
}

/// 使用同快照内原最终父 ID 集合的两方向 find，保留旧分配流和来源事实。
///
/// # 参数
/// 数据库、最终完整版本与首拍汇总资格、原执行器。
/// # 返回
/// 保持原父票及两方向分配流的汇总行。
/// # 错误
/// 查询、解码或首拍汇总资格缺失时拒绝。
pub(super) async fn summary_rows(
    db: &Database,
    versions: &[InvoiceVersion],
    executor: &mut dyn Executor,
) -> Result<Vec<InvoiceSummaryRow>> {
    let ids = versions.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
    let repository = InvoiceSummaryRepository::new(db);
    let sales = repository.sales_links(&ids, executor).await?;
    let purchase = repository.purchase_links(&ids, executor).await?;
    assemble_summary_rows(versions, sales, purchase)
}

/// 先销项后进项、按父票原顺序消费各方向原 find 流，禁止金额重新排序。
fn assemble_summary_rows(
    versions: &[InvoiceVersion],
    sales: Vec<InvoiceSummaryLink>,
    purchase: Vec<InvoiceSummaryLink>,
) -> Result<Vec<InvoiceSummaryRow>> {
    let headers = versions
        .iter()
        .map(|row| {
            let header = row.summary.as_ref().ok_or_else(|| Error::Internal("发票汇总资格缺失".into()))?;
            Ok((row.id.as_str(), header))
        })
        .collect::<Result<HashMap<_, _>>>()?;
    let mut grouped: HashMap<String, Vec<InvoiceShare>> = HashMap::new();
    append_summary_links(sales, false, &headers, &mut grouped);
    append_summary_links(purchase, true, &headers, &mut grouped);
    Ok(versions
        .iter()
        .map(|row| {
            let header = headers[row.id.as_str()];
            InvoiceSummaryRow {
                whole: header.whole,
                gross_amount: header.gross_amount,
                shares: grouped.remove(&row.id).unwrap_or_default(),
                sales_owners: summary_owners(&header.all_sales_sources, false),
                purchase_owners: summary_owners(&header.all_purchase_sources, true),
            }
        })
        .collect())
}

/// 所有存在来源均按原事实口径提供负责人，采购 map 最后覆盖销售 map。
fn summary_owners(sources: &[InvoiceSummarySource], purchase: bool) -> HashMap<String, String> {
    sources
        .iter()
        .filter_map(|source| {
            let owner = source.owner.as_ref()?;
            if purchase
                && !source.key.starts_with("supplier_settlement_statement:")
                && owner.trim().is_empty()
            {
                return None;
            }
            Some((source.key.clone(), owner.clone()))
        })
        .collect()
}

/// 单次映射只筛选匹配份额，不归并动作或金额，不读取第二份跨域来源。
fn append_summary_links(
    links: Vec<InvoiceSummaryLink>,
    purchase: bool,
    headers: &HashMap<&str, &InvoiceSummaryHeader>,
    grouped: &mut HashMap<String, Vec<InvoiceShare>>,
) {
    let matched = headers
        .iter()
        .map(|(id, header)| {
            let sources = if purchase { &header.purchase_sources } else { &header.sales_sources };
            (*id, sources.iter().map(|source| (source.key.as_str(), source)).collect::<HashMap<_, _>>())
        })
        .collect::<HashMap<_, _>>();
    for link in links {
        let Some(sources) = matched.get(link.parent_id.as_str()) else { continue };
        let Some(id) = &link.source_document_id else { continue };
        let key = if purchase {
            let Some(kind) = link.source_type else { continue };
            source_key(kind, id)
        } else {
            id.clone()
        };
        let Some(source) = sources.get(key.as_str()) else { continue };
        let owner = source.owner.clone().filter(|owner| {
            link.source_type != Some(PayableSourceType::PurchaseOrder) || !owner.trim().is_empty()
        });
        grouped.entry(link.parent_id).or_default().push(InvoiceShare {
            id: link.id,
            order: key,
            owner,
            allocated_gross_amount: link.amount,
            allocation_action: link.action,
            purchase,
            source_type: link.source_type,
        });
    }
}

/// 只归并数据库最终匹配的金额流，整单合计仅在全部行完整可读时返回。
///
/// # 参数
/// 最终匹配票的必要金额事实与完整范围版本。
/// # 返回
/// 按原顺序归并的份额分区和整票金额资格。
/// # 错误
/// 汇总资格错误时拒绝；金额越界保留原金额类型行为。
pub(super) fn invoice_summary(rows: &[InvoiceSummaryRow], version: &str) -> Result<FundsSummaryView> {
    let mut triples = Vec::new();
    let mut owners = HashMap::new();
    let mut purchase_owners = HashMap::new();
    let mut whole_sum = zero_amount();
    let mut all_whole = true;
    for row in rows {
        all_whole &= row.whole;
        if row.whole {
            whole_sum = whole_sum.checked_add(row.gross_amount);
        }
        owners.extend(row.sales_owners.iter().map(|(key, owner)| (key.clone(), owner.clone())));
        purchase_owners.extend(row.purchase_owners.iter().map(|(key, owner)| (key.clone(), owner.clone())));
        for share in &row.shares {
            triples.push((share.id.clone(), share.signed(), Some(share.order.clone())));
            if let Some(owner) = &share.owner {
                if share.purchase {
                    purchase_owners.insert(share.order.clone(), owner.clone());
                } else {
                    owners.insert(share.order.clone(), owner.clone());
                }
            }
        }
    }
    owners.extend(purchase_owners);
    build_summary(&triples, &owners, whole_amount(all_whole, whole_sum), version, !all_whole)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 独立生产装配保留父票排序、两方向原 find 流和未匹配来源的 owner 覆盖。
    #[test]
    fn assembly_keeps_parent_and_allocation_flow_with_all_source_owner_precedence() {
        let versions: Vec<InvoiceVersion> = serde_json::from_value(serde_json::json!([
            {"id":"first","version":1,"sales_versions":[1],"purchase_versions":[],"summary":{
                "whole":false,"gross_amount":"100","sales_sources":[{"key":"shared","owner":"seller"}],
                "purchase_sources":[],"all_sales_sources":[{"key":"shared","owner":"seller"}],
                "all_purchase_sources":[{"key":"shared","owner":"buyer"}]}},
            {"id":"second","version":1,"sales_versions":[1],"purchase_versions":[1],"summary":{
                "whole":true,"gross_amount":"200","sales_sources":[{"key":"sales","owner":"seller"}],
                "purchase_sources":[{"key":"purchase","owner":"buyer"}],
                "all_sales_sources":[{"key":"sales","owner":"seller"}],
                "all_purchase_sources":[{"key":"purchase","owner":"buyer"}]}}
        ]))
        .unwrap();
        let sales = vec![
            summary_link("b", "second", "sales", None, "10", AllocationAction::Apply),
            summary_link("a", "first", "shared", None, "50", AllocationAction::Apply),
            summary_link("c", "second", "sales", None, "2", AllocationAction::Reverse),
        ];
        let purchase = vec![
            summary_link(
                "denied",
                "first",
                "shared",
                Some(PayableSourceType::PurchaseOrder),
                "70",
                AllocationAction::Apply,
            ),
            summary_link(
                "d",
                "second",
                "purchase",
                Some(PayableSourceType::PurchaseOrder),
                "3",
                AllocationAction::Apply,
            ),
        ];
        let rows = assemble_summary_rows(&versions, sales, purchase).unwrap();
        assert_eq!(rows[0].shares.iter().map(|share| share.id.as_str()).collect::<Vec<_>>(), ["a"]);
        assert_eq!(rows[1].shares.iter().map(|share| share.id.as_str()).collect::<Vec<_>>(), ["b", "c", "d"]);
        let summary = invoice_summary(&rows, "v").unwrap();
        assert_eq!(
            summary.grouped.iter().find(|group| group.owner_user_id == "buyer").unwrap().visible_share,
            "53".parse().unwrap()
        );
        assert_eq!(
            summary.grouped.iter().find(|group| group.owner_user_id == "seller").unwrap().visible_share,
            "8".parse().unwrap()
        );
        assert_eq!(summary.whole_total, None);
    }

    /// 测试只构造事实，不复制装配或汇总算法。
    fn summary_link(
        id: &str,
        parent: &str,
        source: &str,
        source_type: Option<PayableSourceType>,
        amount: &str,
        action: AllocationAction,
    ) -> InvoiceSummaryLink {
        InvoiceSummaryLink {
            id: id.into(),
            parent_id: parent.into(),
            source_document_id: Some(source.into()),
            source_type,
            amount: amount.parse().unwrap(),
            action,
        }
    }

    /// 二拍纯版本不能冒充首拍票面资格，也不能静默生成零合计。
    #[test]
    fn assembly_rejects_missing_summary_header() {
        let versions: Vec<InvoiceVersion> = serde_json::from_value(serde_json::json!([
            {"id":"one","version":1,"sales_versions":[],"purchase_versions":[]}
        ]))
        .unwrap();
        assert!(matches!(assemble_summary_rows(&versions, vec![], vec![]), Err(Error::Internal(message))
            if message == "发票汇总资格缺失"));
    }

    /// 原正反动作先后关系保留中间溢出，不能在数据库先净额归并。
    #[test]
    #[should_panic(expected = "Addition overflowed")]
    fn summary_keeps_intermediate_overflow_in_original_flow() {
        let rows: Vec<InvoiceSummaryRow> = serde_json::from_value(serde_json::json!([
            {"whole":false,"gross_amount":"0","shares":[
                {"id":"max","order":"sales","owner":"seller","allocated_gross_amount":"79228162514264337593543950335","allocation_action":"apply"},
                {"id":"add","order":"sales","owner":"seller","allocated_gross_amount":"1","allocation_action":"apply"},
                {"id":"reverse","order":"sales","owner":"seller","allocated_gross_amount":"1","allocation_action":"reverse"}
            ]}
        ])).unwrap();
        let _ = invoice_summary(&rows, "v").unwrap();
    }

    /// 安全顺序 MAX、-1、+1 必须完整成功，独立于预期溢出测试验证。
    #[test]
    fn summary_keeps_safe_reverse_before_apply_near_maximum() {
        let rows: Vec<InvoiceSummaryRow> = serde_json::from_value(serde_json::json!([
            {"whole":false,"gross_amount":"0","shares":[
                {"id":"max","order":"sales","owner":"seller","allocated_gross_amount":"79228162514264337593543950335","allocation_action":"apply"},
                {"id":"reverse","order":"sales","owner":"seller","allocated_gross_amount":"1","allocation_action":"reverse"},
                {"id":"add","order":"sales","owner":"seller","allocated_gross_amount":"1","allocation_action":"apply"}
            ]}
        ])).unwrap();
        let summary = invoice_summary(&rows, "v").unwrap();
        assert_eq!(summary.grouped.len(), 1);
        assert_eq!(summary.grouped[0].owner_user_id, "seller");
        assert_eq!(summary.grouped[0].visible_share, "79228162514264337593543950335".parse().unwrap());
        assert_eq!(summary.unassigned, Amount::zero());
        assert_eq!(summary.whole_total, None);
        assert!(summary.permission_limited);
    }

    /// 解码生产聚合金额投影，校验冲正、结算无负责人及部分票面隐藏。
    #[test]
    fn summary_preserves_reverse_unassigned_and_partial_nulls() {
        let rows: Vec<InvoiceSummaryRow> = serde_json::from_value(serde_json::json!([
            {"whole":false,"gross_amount":"100","shares":[
                {"id":"a","order":"sales","owner":"one","allocated_gross_amount":"60","allocation_action":"apply"},
                {"id":"b","order":"sales","owner":"one","allocated_gross_amount":"10","allocation_action":"reverse"},
                {"id":"c","order":"supplier_settlement_statement:s","owner":null,"allocated_gross_amount":"7","allocation_action":"apply"}
            ]}
        ])).unwrap();
        let summary = invoice_summary(&rows, "scope").unwrap();
        assert_eq!(summary.grouped.len(), 1);
        assert_eq!(summary.grouped[0].owner_user_id, "one");
        assert_eq!(summary.grouped[0].visible_share, "50".parse().unwrap());
        assert_eq!(summary.unassigned, "7".parse().unwrap());
        assert_eq!(summary.whole_total, None);
        assert!(summary.permission_limited);
    }

    /// 真正零分配的整票保留票面合计，空匹配集合保留零合计。
    #[test]
    fn empty_allocations_keep_whole_invoice_and_empty_summary() {
        let rows = vec![InvoiceSummaryRow {
            whole: true,
            gross_amount: "120".parse().unwrap(),
            shares: vec![],
            sales_owners: HashMap::new(),
            purchase_owners: HashMap::new(),
        }];
        let summary = invoice_summary(&rows, "v").unwrap();
        assert_eq!(summary.whole_total, Some("120".parse().unwrap()));
        assert_eq!(summary.unassigned, Amount::zero());
        assert!(!summary.permission_limited);
        assert_eq!(invoice_summary(&[], "v").unwrap().whole_total, Some(Amount::zero()));
    }
}
