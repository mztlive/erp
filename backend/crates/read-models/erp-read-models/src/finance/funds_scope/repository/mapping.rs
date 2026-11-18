//! 固定资金查询根、真实来源关系及数据库内关联投影。
use std::collections::BTreeMap;
use std::sync::Arc;

use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, MappingFacts, QueryModel, QueryModelKey, QueryOutput, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{AccessMapping, Error, Result};
use erp_procurement::authorization::PurchaseMapping;
use erp_workflow::WorkItemType;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};

use super::settlement_mapping::SettlementMapping;
use crate::sales_center::repository::authorization::SalesMapping;

/// 同一个固定根连接销售、采购与结算来源，不由请求选择政策。
pub struct FundsMapping {
    model: QueryModel,
    sources: BTreeMap<RelationKey, Arc<dyn AccessMapping>>,
    stages: Vec<Document>,
}
impl FundsMapping {
    /// 初始化固定资金根和它全部可能的来源，供财务来源范围使用。
    ///
    /// # 参数
    /// db 为共享数据库；resource 为已有资金资源。
    ///
    /// # 返回
    /// 只在数据库内准备完整关系的固定映射。
    ///
    /// # 错误
    /// 未支持的资源或映射描述非法时拒绝。
    pub fn new(db: Database, resource: &str) -> Result<Self> {
        let sources: BTreeMap<_, Arc<dyn AccessMapping>> = [
            (
                RelationKey::parse("sales_source")?,
                Arc::new(SalesMapping::new(db, "sales_order", "funds_sales")?) as Arc<dyn AccessMapping>,
            ),
            (
                RelationKey::parse("purchase_source")?,
                Arc::new(PurchaseMapping::new("purchase_order")?) as Arc<dyn AccessMapping>,
            ),
            (
                RelationKey::parse("settlement_source")?,
                Arc::new(SettlementMapping::new()?) as Arc<dyn AccessMapping>,
            ),
        ]
        .into();

        let model = query_model(resource)?;
        Ok(Self {
            model,
            sources,
            stages: source_projection(if resource == "cost_allocation" { "cost_entry" } else { resource })?,
        })
    }
    /// 生成财务来源范围视图时，在服务构造前复用组合根已有的完整来源映射实例。
    ///
    /// # 参数
    /// sales、purchase、settlement 分别为对应拥有领域的固定来源模型。
    ///
    /// # 返回
    /// 保留当前根模型并复用来源实现的 Mapping。
    ///
    /// # 错误
    /// 来源资源不匹配时拒绝。
    pub fn with_sources(
        mut self,
        sales: Arc<dyn AccessMapping>,
        purchase: Arc<dyn AccessMapping>,
        settlement: Arc<dyn AccessMapping>,
    ) -> Result<Self> {
        for (key, resource, mapping) in [
            ("sales_source", "sales_order", sales),
            ("purchase_source", "purchase_order", purchase),
            ("settlement_source", "supplier_settlement_statement", settlement),
        ] {
            if mapping.model().resource != resource {
                return Err(Error::ValidationError("资金来源资源不匹配".into()));
            }
            self.sources.insert(RelationKey::parse(key)?, mapping);
        }
        Ok(self)
    }
}
impl AccessMapping for FundsMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        self.sources.clone()
    }
    fn stages(&self) -> Vec<Document> {
        self.stages.clone()
    }
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> Result<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! { "deleted_at": 0, "__funds_sources_valid": true }))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<QueryFragment> {
        if self.model.resource == "purchase_payment_request" && fact == &AccessFact::AssignedTo {
            return Ok(QueryFragment::Exists {
                collection: "work_items".into(),
                bindings: doc! { "request": "$id" },
                pipeline: vec![
                    doc! { "$match": { "deleted_at": 0, "business_object_type": "purchase_payment_request",
                    "work_item_type": WorkItemType::SupplierPaymentExecution.as_str(), "owner_user_id": context.actor.id(),
                    "$expr": { "$eq": ["$business_object_id", "$$request"] } } },
                ],
            });
        }
        Err(Error::ValidationError("资金根不提供该授权事实".into()))
    }
    fn relation(
        &self,
        key: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<RelationMapping> {
        let mapping =
            self.sources.get(key).ok_or_else(|| Error::ValidationError("未知资金来源关系".into()))?;
        let field = match key.as_str() {
            "sales_source" => "__funds_sales_ids",
            "purchase_source" => "__funds_purchase_ids",
            "settlement_source" => "__funds_settlement_ids",
            _ => return Err(Error::ValidationError("未知资金来源".into())),
        };
        Ok(RelationMapping { expected_ids: format!("${field}").into(), mapping: mapping.clone() })
    }
}

fn query_model(resource: &str) -> Result<QueryModel> {
    let collection = if resource == "cost_allocation" { "cost_entries" } else { collection(resource)? };
    let model = QueryModel {
        key: QueryModelKey::parse(format!("funds_{resource}"))?,
        resource: resource.into(),
        collection: collection.into(),
        id_field: "id".into(),
        output: if matches!(
            resource,
            "customer_receipt"
                | "supplier_payment"
                | "invoice"
                | "cost_entry"
                | "cost_allocation"
                | "purchase_invoice_allocation"
        ) {
            QueryOutput::Existence { fields: ["id".into()].into() }
        } else {
            QueryOutput::Object
        },
        facts: if resource == "purchase_payment_request" {
            [FactKind::Assignee].into()
        } else {
            Default::default()
        },
    };
    model.validate()?;
    Ok(model)
}

pub(super) fn collection(resource: &str) -> Result<&'static str> {
    match resource {
        "receivable_account" => Ok("receivable_accounts"),
        "sales_invoice_request" => Ok("sales_invoice_requests"),
        "payable_account" => Ok("payable_accounts"),
        "purchase_payment_request" => Ok("purchase_payment_requests"),
        "customer_receipt" => Ok("customer_receipts"),
        "supplier_payment" => Ok("supplier_payments"),
        "invoice" => Ok("invoices"),
        "cost_entry" => Ok("cost_entries"),
        "cost_allocation" => Ok("cost_allocations"),
        "purchase_invoice_allocation" => Ok("purchase_invoice_allocations"),
        _ => Err(Error::ValidationError("未支持的资金查询资源".into())),
    }
}

pub(super) fn source_projection(resource: &str) -> Result<Vec<Document>> {
    let mut stages = vec![
        doc! { "$set": { "__funds_sales_ids": [], "__funds_purchase_ids": [], "__funds_settlement_ids": [], "__funds_sources_valid": true } },
    ];

    match resource {
        "receivable_account" | "cost_allocation" => {
            stages.push(doc! { "$set": { "__funds_sales_ids": single("$sales_order_id"), "__funds_sources_valid": { "$ne": [{ "$ifNull": ["$sales_order_id", ""] }, ""] } } })
        },
        "sales_invoice_request" => stages.extend(invoice_request_stages()),
        "purchase_payment_request" => stages.extend(payment_request_stages()),
        "payable_account" => stages.push(payable_fields("$$ROOT")),
        "purchase_invoice_allocation" => {
            stages.push(lookup("payable_accounts", "$payable_account_id".into(), "id", "__funds_accounts"));
            stages.push(doc! { "$set": { "__funds_sources_valid": complete(single("$payable_account_id"), "__funds_accounts") } });
            stages.extend(payable_arrays("__funds_accounts"));
        },
        "customer_receipt" => stages.extend(receipt_stages()),
        "supplier_payment" => stages.extend(payment_stages()),
        "invoice" => stages.extend(invoice_stages()),
        "cost_entry" => {
            stages.push(lookup("cost_allocations", "$id".into(), "cost_entry_id", "__funds_allocations"));
            stages.push(doc! { "$set": { "__funds_sales_ids": "$__funds_allocations.sales_order_id" } });
        },
        _ => return Err(Error::ValidationError("未知资金关联投影".into())),
    }
    Ok(stages)
}

fn single(field: &str) -> Bson {
    doc! { "$cond": [{ "$ne": [{ "$ifNull": [field, ""] }, ""] }, [field], []] }.into()
}
fn lookup(collection: &str, id: Bson, foreign: &str, alias: &str) -> Document {
    doc! { "$lookup": { "from": collection, "let": { "source": id }, "pipeline": [
        { "$match": { "deleted_at": 0, "$expr": { "$eq": [format!("${foreign}"), "$$source"] } } }
    ], "as": alias } }
}
fn array_lookup(collection: &str, ids: Bson, alias: &str) -> Document {
    doc! { "$lookup": { "from": collection, "let": { "sources": ids }, "pipeline": [
        { "$match": { "deleted_at": 0, "$expr": { "$in": ["$id", { "$ifNull": ["$$sources", []] }] } } }
    ], "as": alias } }
}
fn complete(expected: Bson, actual: &str) -> Bson {
    doc! { "$setEquals": [{ "$ifNull": [expected, []] }, format!("${actual}.id")] }.into()
}
fn payable_fields(prefix: &str) -> Document {
    doc! { "$set": {
        "__funds_purchase_ids": { "$cond": [{ "$eq": [format!("{prefix}.source_type"), "purchase_order"] }, [format!("{prefix}.source_document_id")], []] },
        "__funds_settlement_ids": { "$cond": [{ "$eq": [format!("{prefix}.source_type"), "supplier_settlement"] }, [format!("{prefix}.source_document_id")], []] },
        "__funds_sources_valid": { "$in": [format!("{prefix}.source_type"), ["purchase_order", "supplier_settlement"]] },
    } }
}
fn payable_arrays(alias: &str) -> Vec<Document> {
    let field = |kind: &str| doc! { "$map": { "input": { "$filter": { "input": format!("${alias}"), "as": "account", "cond": { "$eq": ["$$account.source_type", kind] } } }, "as": "account", "in": "$$account.source_document_id" } };
    vec![
        doc! { "$set": { "__funds_purchase_ids": field("purchase_order"), "__funds_settlement_ids": field("supplier_settlement"),
        "__funds_sources_valid": { "$and": ["$__funds_sources_valid", { "$allElementsTrue": [{ "$map": { "input": format!("${alias}"), "as": "account", "in": { "$in": ["$$account.source_type", ["purchase_order", "supplier_settlement"]] } } }] }] } } },
    ]
}
fn receipt_stages() -> Vec<Document> {
    vec![
        lookup("receipt_allocations", "$id".into(), "customer_receipt_id", "__funds_allocations"),
        doc! { "$set": { "__funds_entry_ids": { "$setUnion": ["$__funds_allocations.receivable_entry_id", { "$ifNull": ["$pending_allocations.receivable_entry_id", []] }] } } },
        array_lookup("receivable_entries", "$__funds_entry_ids".into(), "__funds_entries"),
        array_lookup(
            "receivable_accounts",
            "$__funds_entries.receivable_account_id".into(),
            "__funds_accounts",
        ),
        doc! { "$set": { "__funds_sales_ids": "$__funds_accounts.sales_order_id", "__funds_sources_valid": { "$and": [complete("$__funds_entry_ids".into(), "__funds_entries"), complete("$__funds_entries.receivable_account_id".into(), "__funds_accounts")] } } },
    ]
}
fn payment_stages() -> Vec<Document> {
    let mut stages = vec![
        lookup("payment_allocations", "$id".into(), "supplier_payment_id", "__funds_allocations"),
        array_lookup("payable_entries", "$__funds_allocations.payable_entry_id".into(), "__funds_entries"),
        doc! { "$set": { "__funds_account_ids": { "$setUnion": ["$__funds_entries.payable_account_id", { "$ifNull": ["$request_allocations.payable_account_id", []] }] } } },
        array_lookup("payable_accounts", "$__funds_account_ids".into(), "__funds_accounts"),
        doc! { "$set": { "__funds_sources_valid": { "$and": [complete("$__funds_allocations.payable_entry_id".into(), "__funds_entries"), complete("$__funds_account_ids".into(), "__funds_accounts")] } } },
    ];
    stages.extend(payable_arrays("__funds_accounts"));
    stages
}
fn invoice_stages() -> Vec<Document> {
    let mut stages = vec![
        lookup("sales_invoice_allocations", "$id".into(), "invoice_id", "__funds_sales_allocations"),
        lookup("purchase_invoice_allocations", "$id".into(), "invoice_id", "__funds_purchase_allocations"),
        array_lookup(
            "receivable_accounts",
            "$__funds_sales_allocations.receivable_account_id".into(),
            "__funds_receivables",
        ),
        array_lookup(
            "payable_accounts",
            "$__funds_purchase_allocations.payable_account_id".into(),
            "__funds_accounts",
        ),
        doc! { "$set": { "__funds_sales_ids": "$__funds_receivables.sales_order_id", "__funds_sources_valid": { "$and": [complete("$__funds_sales_allocations.receivable_account_id".into(), "__funds_receivables"), complete("$__funds_purchase_allocations.payable_account_id".into(), "__funds_accounts")] } } },
    ];
    stages.extend(payable_arrays("__funds_accounts"));
    stages
}

fn invoice_request_stages() -> Vec<Document> {
    vec![
        lookup("receivable_accounts", "$receivable_account_id".into(), "id", "__funds_accounts"),
        doc! { "$set": { "__funds_sales_ids": single("$sales_order_id"),
        "__funds_sources_valid": { "$and": [complete(single("$receivable_account_id"), "__funds_accounts"),
            { "$setEquals": [single("$sales_order_id"), "$__funds_accounts.sales_order_id"] }] } } },
    ]
}
fn payment_request_stages() -> Vec<Document> {
    vec![
        array_lookup("payable_accounts", "$data.lines.payable_account_id".into(), "__funds_accounts"),
        doc! { "$set": { "__funds_purchase_ids": { "$cond": [{ "$gt": [{ "$size": { "$ifNull": ["$lines", []] } }, 0] },
        "$lines.purchase_order_id", "$__funds_accounts.source_document_id"] },
        "__funds_sources_valid": { "$and": [complete("$data.lines.payable_account_id".into(), "__funds_accounts"),
            { "$allElementsTrue": [{ "$map": { "input": "$__funds_accounts", "as": "account", "in": { "$eq": ["$$account.source_type", "purchase_order"] } } }] }] } } },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixed_financial_root_has_a_complete_static_source_projection() {
        for resource in [
            "receivable_account",
            "sales_invoice_request",
            "payable_account",
            "purchase_payment_request",
            "customer_receipt",
            "supplier_payment",
            "invoice",
            "cost_entry",
            "cost_allocation",
            "purchase_invoice_allocation",
        ] {
            assert!(!collection(resource).unwrap().is_empty());
            let stages = source_projection(resource).unwrap();
            let fields = stages[0].get_document("$set").unwrap();
            for field in ["__funds_sales_ids", "__funds_purchase_ids", "__funds_settlement_ids"] {
                assert_eq!(fields.get_array(field).unwrap().len(), 0);
            }
            assert!(fields.get_bool("__funds_sources_valid").unwrap());
        }

        assert!(source_projection("unknown").is_err());
        assert!(collection("unknown").is_err());
    }

    #[test]
    fn multiple_source_funds_roots_only_expose_membership_identity() {
        for resource in [
            "customer_receipt",
            "supplier_payment",
            "invoice",
            "cost_entry",
            "cost_allocation",
            "purchase_invoice_allocation",
        ] {
            let model = query_model(resource).unwrap();
            assert_eq!(model.output, QueryOutput::Existence { fields: ["id".into()].into() });
            assert_eq!(model.id_field, "id");
            model.validate().unwrap();
        }

        assert_eq!(query_model("cost_allocation").unwrap().collection, "cost_entries");
        assert_eq!(collection("cost_allocation").unwrap(), "cost_allocations");
        assert_eq!(query_model("receivable_account").unwrap().output, QueryOutput::Object);
    }

    #[test]
    fn intermediate_financial_references_require_complete_identity_sets() {
        assert_eq!(
            complete("$entry_ids".into(), "entries"),
            Bson::Document(doc! {
                "$setEquals": [{ "$ifNull": ["$entry_ids", []] }, "$entries.id"]
            })
        );

        let stage = array_lookup("receivable_entries", "$entry_ids".into(), "entries");
        let lookup = stage.get_document("$lookup").unwrap();
        assert_eq!(lookup.get_document("let").unwrap().get_str("sources").unwrap(), "$entry_ids");
        assert_eq!(lookup.get_str("from").unwrap(), "receivable_entries");
    }
}
