//! 财务列表关键词关联。只读取财务自有集合，外域名称和单号由读取模型解析。
//! 关联包含已登记分配历史（含后续冲减）及回款待审批分配，不推定同主体全部单据相关。
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Result, insert_literal_regex_filter};

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
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 返回完整的关键词命中身份，后续必须与列表结构化条件取交集。
    ///
    /// 未命中返回空集合，查询失败返回错误，禁止截取部分候选充当完整结果。
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

    /// 按财务集合读取去重字段；所有读取均排除软删除。
    async fn ids(
        &self,
        collection: &str,
        mut filter: Document,
        field: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
        let collection = self.db.collection::<Document>(collection);
        let mut query = collection.distinct(field, filter);
        if let Some(session) = executor.session() {
            query = query.session(session);
        }
        Ok(query.await?.into_iter().filter_map(|value| value.as_str().map(str::to_owned)).collect())
    }

    /// 空关联身份直接短路；不得去掉空集合条件扩大范围。
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

    /// 沿付款/回款的正式分配及回款待审批分配定位账户。
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

    /// 按自身编号/主体或关联账户匹配收付款单，其他同主体单据不参与扩展。
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

    /// 发票号/主体 OR 同一账户的指定来源单据，方向始终限制在当前财务侧。
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
    /// 按返回对象选择应收或应付关系，保持集合归属一致。
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
    /// 名称搜索在应收侧使用主体身份，在应付侧使用供应商身份。
    fn parties<'a>(&self, facts: &'a FinanceKeyword) -> &'a [String] {
        if self.sales {
            return &facts.party_ids;
        }
        &facts.supplier_ids
    }
}
/// 字面量正则与现有仓储一致，元字符不执行为表达式。
fn literal(field: &str, q: &str) -> Document {
    let mut filter = Document::new();
    insert_literal_regex_filter(&mut filter, field, Some(q));
    filter
}
/// 来源类型必须与来源 ID 配对，避免不同类型的身份碰撞。
fn source_filter(side: &Side, facts: &FinanceKeyword) -> Document {
    if side.sales {
        return doc! { "sales_order_id": { "$in": &facts.sales_order_ids } };
    }
    doc! { "$or": [
        { "source_type": "purchase_order", "source_document_id": { "$in": &facts.purchase_order_ids } },
        { "source_type": "supplier_settlement", "source_document_id": { "$in": &facts.statement_ids } }
    ] }
}
/// 账户按名称及显式单据关联匹配，应收保留原接口对内部身份的兼容检索。
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

/// 发票方向限制与字面量编号条件共同执行。
fn invoice_filter(side: &Side, facts: &FinanceKeyword) -> Document {
    let mut filter = literal("invoice_no", &facts.q);
    filter.insert("invoice_direction", side.direction);
    filter
}
/// 合并关联身份并去重，避免相同账户多次参与后续查询。
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
