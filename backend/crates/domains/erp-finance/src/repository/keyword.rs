//! 财务列表关键词关联。只读取财务自有集合，外域名称和单号由读取模型解析。
//! 关联包含已登记分配历史（含后续冲减）及回款待审批分配，不推定同主体全部单据相关。
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Result, insert_literal_regex_filter, mongo_ops};

use crate::repository::{PayableExt, ReceivableExt};

/// 外域提供的关键词命中事实；不包含分页、结构化筛选或权限条件。
#[derive(Debug, Clone, Default)]
pub struct FinanceKeyword {
    /// 已规范化的字面量关键词。
    pub q: String,
    /// 当前名称命中的主体。
    pub party_ids: Vec<String>,
    /// 当前名称命中的供应商。
    pub supplier_ids: Vec<String>,
    /// 单号命中的销售单。
    pub sales_order_ids: Vec<String>,
    /// 单号命中的采购单。
    pub purchase_order_ids: Vec<String>,
    /// 单号或外部账单号命中的结算单。
    pub statement_ids: Vec<String>,
}

/// 搜索返回对象类型，决定允许沿哪些财务关系匹配。
#[derive(Debug, Clone, Copy)]
pub enum FinanceSearchTarget {
    Receivable,
    Receipt,
    SalesInvoice,
    Payable,
    Payment,
    PurchaseInvoice,
}

/// 财务关键词仓储；查询中间集合仅包含 ID，不加载敏感业务文档。
pub struct FinanceKeywordRepository<'a> {
    db: &'a Database,
}

impl<'a> FinanceKeywordRepository<'a> {
    /// 构造只读仓储；不执行数据库访问。
    /// 返回尚未访问数据库的仓储。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 返回完整的关键词命中身份，后续必须与列表结构化条件取交集。
    ///
    /// 未命中返回空集合，查询失败返回错误，禁止截取部分候选充当完整结果。
    /// 返回完整命中身份；未命中时返回空列表。
    /// 任一集合查询失败时返回仓储错误。
    ///
    /// 输入约定：
    /// - `target`：搜索返回对象，决定沿哪一侧财务关系匹配。
    /// - `facts`：外域给出的关键词命中事实。
    pub async fn matching_ids(
        &self,
        target: FinanceSearchTarget,
        facts: &FinanceKeyword,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let side = Side::for_target(target);
        let source_accounts = self.ids(side.accounts, source_filter(&side, facts), "id", executor).await?;
        let fund_ids = self.ids(side.funds, literal(side.fund_number, &facts.q), "id", executor).await?;
        let invoice_ids = self
            .ids(<Database as ReceivableExt>::INVOICES, invoice_filter(&side, facts), "id", executor)
            .await?;

        let fund_accounts = self.fund_accounts(&side, &fund_ids, executor).await?;
        let invoice_accounts =
            self.related(side.invoices, "invoice_id", &invoice_ids, side.account_key, executor).await?;

        match target {
            FinanceSearchTarget::Receivable | FinanceSearchTarget::Payable => {
                let condition = account_filter(
                    &side,
                    facts,
                    &joined(&source_accounts, &joined(&fund_accounts, &invoice_accounts)),
                );
                self.ids(side.accounts, condition, "id", executor).await
            },
            FinanceSearchTarget::Receipt | FinanceSearchTarget::Payment => {
                self.funds(&side, facts, &joined(&source_accounts, &invoice_accounts), executor).await
            },
            FinanceSearchTarget::SalesInvoice | FinanceSearchTarget::PurchaseInvoice => {
                self.invoices(&side, facts, &joined(&source_accounts, &fund_accounts), executor).await
            },
        }
    }

    // 为财务关键词查找收集匹配字段的唯一值，只从未软删除的记录取数。
    async fn ids(
        &self,
        collection: &str,
        mut filter: Document,
        field: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
        let collection = self.db.collection::<Document>(collection);
        Ok(mongo_ops::distinct(&collection, field, filter, executor)
            .await?
            .into_iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect())
    }

    // 沿关联身份查询之前检查输入是否为空；无身份直接短路，不能移除过滤条件扩大范围。
    async fn related(
        &self,
        collection: &str,
        key: &str,
        ids: &[String],
        field: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.ids(collection, doc! { key: { "$in": ids } }, field, executor).await
    }

    // 收付款单需要按来源检索时，通过正式分配及待审批回款分配找到所属应收或应付账户。
    async fn fund_accounts(
        &self,
        side: &Side,
        funds: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let mut entries =
            self.related(side.allocations, side.fund_key, funds, side.entry_key, executor).await?;
        if side.sales {
            entries.extend(
                self.related(side.funds, "id", funds, "pending_allocations.receivable_entry_id", executor)
                    .await?,
            );
        }
        self.related(side.entries, "id", &entries, side.account_key, executor).await
    }

    // 为收付款检索合并自身编号、主体及关联子账命中，不将同主体的其他单据作为关联来源。
    async fn funds(
        &self,
        side: &Side,
        facts: &FinanceKeyword,
        accounts: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let entries = self.related(side.entries, side.account_key, accounts, "id", executor).await?;
        let ids = self.related(side.allocations, side.entry_key, &entries, side.fund_key, executor).await?;
        let mut clauses = vec![
            literal(side.fund_number, &facts.q),
            doc! { side.party_key: { "$in": side.parties(facts) } },
            doc! { "id": { "$in": ids } },
        ];
        if side.sales {
            clauses.push(doc! { "pending_allocations.receivable_entry_id": { "$in": entries } });
        }
        self.ids(side.funds, doc! { "$or": clauses }, "id", executor).await
    }

    // 发票检索可命中票号、主体或同子账的指定来源，但所有分支都受进销项方向限制。
    async fn invoices(
        &self,
        side: &Side,
        facts: &FinanceKeyword,
        accounts: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let ids = self.related(side.invoices, side.account_key, accounts, "invoice_id", executor).await?;
        self.ids(<Database as ReceivableExt>::INVOICES, doc! { "invoice_direction": side.direction, "$or": [
            literal("invoice_no", &facts.q), doc! { "party_id": { "$in": &facts.party_ids } }, doc! { "id": { "$in": ids } }
        ] }, "id", executor).await
    }
}

/// 财务自有两侧的集合和外键，集中列举以避免字符串散落在查询流程。
struct Side {
    sales: bool,
    direction: &'static str,
    accounts: &'static str,
    entries: &'static str,
    funds: &'static str,
    allocations: &'static str,
    invoices: &'static str,
    account_key: &'static str,
    entry_key: &'static str,
    fund_key: &'static str,
    fund_number: &'static str,
    party_key: &'static str,
}
impl Side {
    // 按待检索的财务对象选择应收或应付的集合与关联规则，避免跨方向使用来源关系。
    fn for_target(target: FinanceSearchTarget) -> Self {
        if matches!(
            target,
            FinanceSearchTarget::Receivable
                | FinanceSearchTarget::Receipt
                | FinanceSearchTarget::SalesInvoice
        ) {
            return Self {
                sales: true,
                direction: "sales",
                accounts: <Database as ReceivableExt>::RECEIVABLE_ACCOUNTS,
                entries: <Database as ReceivableExt>::RECEIVABLE_ENTRIES,
                funds: <Database as ReceivableExt>::CUSTOMER_RECEIPTS,
                allocations: <Database as ReceivableExt>::RECEIPT_ALLOCATIONS,
                invoices: <Database as ReceivableExt>::SALES_INVOICE_ALLOCATIONS,
                account_key: "receivable_account_id",
                entry_key: "receivable_entry_id",
                fund_key: "customer_receipt_id",
                fund_number: "receipt_no",
                party_key: "counterparty_party_id",
            };
        }
        Self {
            sales: false,
            direction: "purchase",
            accounts: <Database as PayableExt>::PAYABLE_ACCOUNTS,
            entries: <Database as PayableExt>::PAYABLE_ENTRIES,
            funds: <Database as PayableExt>::SUPPLIER_PAYMENTS,
            allocations: <Database as PayableExt>::PAYMENT_ALLOCATIONS,
            invoices: <Database as PayableExt>::PURCHASE_INVOICE_ALLOCATIONS,
            account_key: "payable_account_id",
            entry_key: "payable_entry_id",
            fund_key: "supplier_payment_id",
            fund_number: "payment_no",
            party_key: "supplier_id",
        }
    }
    // 按主体名称搜索时，将应收匹配到结算主体身份、应付匹配到供应商身份。
    fn parties<'a>(&self, facts: &'a FinanceKeyword) -> &'a [String] {
        if self.sales {
            return &facts.party_ids;
        }
        &facts.supplier_ids
    }
}
// 把关键词转成字面量正则，和现有仓储的匹配方式一致，不解释用户输入的元字符。
fn literal(field: &str, q: &str) -> Document {
    let mut filter = Document::new();
    insert_literal_regex_filter(&mut filter, field, Some(q));
    filter
}
// 按业务单据关联账户时将来源类型与来源 ID 配对，避免不同单据类型的同名 ID 串联。
fn source_filter(side: &Side, facts: &FinanceKeyword) -> Document {
    if side.sales {
        return doc! { "sales_order_id": { "$in": &facts.sales_order_ids } };
    }
    doc! { "$or": [
        { "source_type": "purchase_order", "source_document_id": { "$in": &facts.purchase_order_ids } },
        { "source_type": "supplier_settlement", "source_document_id": { "$in": &facts.statement_ids } }
    ] }
}
// 构造名称及显式业务单据关联的账户筛选；应收侧仍允许按历史接口支持的内部身份匹配。
fn account_filter(side: &Side, facts: &FinanceKeyword, accounts: &[String]) -> Document {
    let mut clauses =
        vec![doc! { "id": { "$in": accounts } }, doc! { side.party_key: { "$in": side.parties(facts) } }];
    if side.sales {
        clauses.extend(
            ["id", "sales_order_id", "customer_id", "counterparty_party_id"]
                .into_iter()
                .map(|field| literal(field, &facts.q)),
        );
    }
    doc! { "$or": clauses }
}

// 生成发票关键词筛选时将固定的进销方向与按字面量匹配的票号请求合并。
fn invoice_filter(side: &Side, facts: &FinanceKeyword) -> Document {
    let mut filter = literal("invoice_no", &facts.q);
    filter.insert("invoice_direction", side.direction);
    filter
}
// 合并各关键词匹配路径找到的来源身份并去重，供后续按身份查询使用。
fn joined(a: &[String], b: &[String]) -> Vec<String> {
    let mut ids = a.iter().chain(b).cloned().collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoice_number_is_literal_and_direction_cannot_be_widened() {
        let facts = FinanceKeyword { q: "Inv.[1]*".into(), ..Default::default() };
        for (target, direction) in
            [(FinanceSearchTarget::SalesInvoice, "sales"), (FinanceSearchTarget::PurchaseInvoice, "purchase")]
        {
            let query = invoice_filter(&Side::for_target(target), &facts);
            assert_eq!(query.get_str("invoice_direction").unwrap(), direction);
            let regex = query.get_document("invoice_no").unwrap();
            assert_eq!(regex.get_str("$regex").unwrap(), r"Inv\.\[1\]\*");
            assert_eq!(regex.get_str("$options").unwrap(), "i");
        }
    }

    #[test]
    fn payable_source_ids_remain_paired_with_document_type() {
        let facts = FinanceKeyword {
            purchase_order_ids: vec!["purchase".into()],
            statement_ids: vec!["statement".into()],
            ..Default::default()
        };
        assert_eq!(
            source_filter(&Side::for_target(FinanceSearchTarget::Payable), &facts),
            doc! { "$or": [
                { "source_type": "purchase_order", "source_document_id": { "$in": ["purchase"] } },
                { "source_type": "supplier_settlement", "source_document_id": { "$in": ["statement"] } }
            ] }
        );
    }

    #[test]
    fn empty_source_search_keeps_a_match_nothing_condition() {
        assert_eq!(
            source_filter(&Side::for_target(FinanceSearchTarget::Receipt), &FinanceKeyword::default()),
            doc! { "sales_order_id": { "$in": [] } }
        );
        assert_eq!(joined(&["a".into(), "b".into()], &["b".into()]), vec!["a", "b"]);
    }
}
