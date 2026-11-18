//! 审批管理查询的完整冻结身份与真实业务来源映射。
use std::collections::BTreeMap;
use std::sync::Arc;

use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, MappingFacts, QueryModel, QueryModelKey, QueryOutput, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{AccessMapping, Error, Result};
use erp_procurement::repository::PurchaseOrderExt;
use erp_returns::repository::ReturnsExt;
use erp_sales::repository::SalesReviewExt;
use erp_workflow::repository::ApprovalIntegrationExt;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};

/// 实例本体复用工作流映射；跨域部分只描述冻结身份和真实强外键。
pub struct ApprovalManagementMapping {
    base: Arc<dyn AccessMapping>,
    sources: BTreeMap<RelationKey, Arc<dyn AccessMapping>>,
    references: BTreeMap<RelationKey, Bson>,
}

impl ApprovalManagementMapping {
    /// 连接已有实例模型与拥有领域的来源模型，不声明账号资格或授权组合。
    /// # 参数
    /// base 为完整实例映射；roots 按来源资源提供同一组合根已构造的模型。
    /// # 返回
    /// 固定真实来源分区的只读映射。
    /// # 错误
    /// 实例模型错配、来源缺失或来源不是完整对象时拒绝。
    pub fn new(
        base: Arc<dyn AccessMapping>,
        roots: &BTreeMap<String, Arc<dyn AccessMapping>>,
    ) -> Result<Self> {
        if base.model().resource != "approval_instance" || base.model().output != QueryOutput::Object {
            return Err(Error::ValidationError("审批管理必须绑定完整实例模型".into()));
        }

        let mut sources = BTreeMap::new();
        let mut references = BTreeMap::new();
        let mut complete = BTreeMap::new();
        for (kind, resource, reference) in source_references() {
            let mapping = roots
                .get(resource)
                .filter(|mapping| mapping.model().resource == resource)
                .ok_or_else(|| {
                    tracing::warn!(resource, "审批管理缺少来源映射");
                    Error::ValidationError("暂时无法查看关联的审批单据，请联系管理员核对审批管理设置".into())
                })?;
            if !complete.contains_key(resource) {
                complete.insert(resource.to_owned(), complete_source(resource, mapping.clone())?);
            }
            let mapping = complete[resource].clone();
            let key = RelationKey::parse(format!("management_{kind}_{resource}"))?;
            sources.insert(key.clone(), mapping);
            references.insert(key, typed_reference(kind, reference));
        }
        Ok(Self { base, sources, references })
    }
}

impl AccessMapping for ApprovalManagementMapping {
    fn model(&self) -> &QueryModel {
        self.base.model()
    }

    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        self.sources.clone()
    }

    fn stages(&self) -> Vec<Document> {
        self.base.stages().into_iter().chain(source_stages()).collect()
    }

    fn key(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<Bson> {
        self.base.key(context, prepared)
    }

    fn records(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<QueryFragment> {
        self.base.records(context, prepared)
    }

    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        prepared: Option<&MappingFacts>,
    ) -> Result<QueryFragment> {
        self.base.predicate(fact, context, prepared)
    }

    fn relation(
        &self,
        key: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<RelationMapping> {
        let mapping =
            self.sources.get(key).ok_or_else(|| Error::ValidationError("审批管理来源关系未登记".into()))?;
        let expected_ids = self
            .references
            .get(key)
            .ok_or_else(|| Error::ValidationError("审批管理来源引用未登记".into()))?;
        Ok(RelationMapping { mapping: mapping.clone(), expected_ids: expected_ids.clone() })
    }
}

/// 资金列表只签发存在性结果；审批关系使用独立完整模型并由identity施加整账条件。
fn complete_source(resource: &str, mapping: Arc<dyn AccessMapping>) -> Result<Arc<dyn AccessMapping>> {
    if mapping.model().output == QueryOutput::Object {
        return Ok(mapping);
    }

    if !matches!(resource, "customer_receipt" | "supplier_payment")
        || !matches!(mapping.model().output, QueryOutput::Existence { .. })
    {
        return Err(Error::ValidationError("审批来源必须是完整对象模型".into()));
    }

    let mut model = mapping.model().clone();
    model.key = QueryModelKey::parse(format!("approval_complete_{resource}"))?;
    model.output = QueryOutput::Object;
    model.validate()?;
    Ok(Arc::new(CompleteSourceMapping { model, inner: mapping }))
}

/// 仅作为管理查询的嵌套来源；不会替换既有资金查询模型或签发独立对象凭证。
struct CompleteSourceMapping {
    model: QueryModel,
    inner: Arc<dyn AccessMapping>,
}
impl AccessMapping for CompleteSourceMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        self.inner.relations()
    }
    fn stages(&self) -> Vec<Document> {
        self.inner.stages()
    }
    fn key(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<Bson> {
        self.inner.key(context, prepared)
    }
    fn records(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<QueryFragment> {
        self.inner.records(context, prepared)
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        prepared: Option<&MappingFacts>,
    ) -> Result<QueryFragment> {
        self.inner.predicate(fact, context, prepared)
    }
    fn relation(
        &self,
        key: &RelationKey,
        context: &AccessContext,
        prepared: Option<&MappingFacts>,
    ) -> Result<RelationMapping> {
        self.inner.relation(key, context, prepared)
    }
}

/// 每个分区只取对应类型的真实引用；缺失、重复或互斥外键损坏形成空来源。
fn source_references() -> Vec<(&'static str, &'static str, Bson)> {
    let mut references = vec![
        ("sales_order", "sales_order", singleton("$subject.subject_id")),
        ("voucher_sales_order", "sales_order", singleton("$subject.subject_id")),
        ("sales_change_order", "sales_order", unique_reference("sales_change", "sales_order_id")),
        ("purchase_order", "purchase_order", singleton("$subject.subject_id")),
        ("purchase_change_order", "purchase_order", unique_reference("purchase_change", "purchase_order_id")),
        ("stock_adjustment", "stock_adjustment", singleton("$subject.subject_id")),
        ("sales_invoice_request", "sales_invoice_request", singleton("$subject.subject_id")),
        ("purchase_payment_request", "purchase_payment_request", singleton("$subject.subject_id")),
        ("customer_receipt", "customer_receipt", singleton("$subject.subject_id")),
        (
            "receipt_reversal",
            "customer_receipt",
            unique_reference("receipt_reversal", "original_customer_receipt_id"),
        ),
        (
            "payment_reversal",
            "supplier_payment",
            unique_reference("payment_reversal", "original_supplier_payment_id"),
        ),
    ];
    references.extend(refund_references());
    references
}

fn refund_references() -> Vec<(&'static str, &'static str, Bson)> {
    vec![
        (
            "customer_refund",
            "customer_receipt",
            refund_reference("customer_refund", "original_receipt_id", "original_receivable_entry_id", None),
        ),
        (
            "customer_refund",
            "receivable_account",
            refund_reference(
                "customer_refund",
                "original_receivable_entry_id",
                "original_receipt_id",
                Some(("receivable_entry", "receivable_account_id")),
            ),
        ),
        (
            "supplier_refund",
            "supplier_payment",
            refund_reference("supplier_refund", "original_payment_id", "original_payable_entry_id", None),
        ),
        (
            "supplier_refund",
            "payable_account",
            refund_reference(
                "supplier_refund",
                "original_payable_entry_id",
                "original_payment_id",
                Some(("payable_entry", "payable_account_id")),
            ),
        ),
    ]
}

fn singleton(field: &str) -> Bson {
    doc! {"$cond": [{"$and": [{"$eq": [{"$type": field}, "string"]}, {"$ne": [field, ""]}]}, [field], []]}
        .into()
}

fn typed_reference(kind: &str, reference: Bson) -> Bson {
    doc! {"$cond": [{"$and": ["$_management_snapshot_exact", {"$eq": ["$process_kind", kind]}]}, reference, []]}.into()
}

fn unique_reference(alias: &str, field: &str) -> Bson {
    let rows = format!("$_management_{alias}");
    let value = doc! {"$arrayElemAt": [format!("{rows}.{field}"), 0]};
    doc! {"$cond": [{"$eq": [{"$size": &rows}, 1]}, {"$let": {"vars": {"reference": value}, "in": singleton("$$reference")}}, []]}.into()
}

fn refund_reference(alias: &str, selected: &str, other: &str, entry: Option<(&str, &str)>) -> Bson {
    let rows = format!("$_management_{alias}");
    let selected_value = doc! {"$arrayElemAt": [format!("{rows}.{selected}"), 0]};
    let other_value = doc! {"$ifNull": [{"$arrayElemAt": [format!("{rows}.{other}"), 0]}, Bson::Null]};
    let reference =
        entry.map_or_else(|| singleton("$$selected"), |(entry, field)| unique_reference(entry, field));
    doc! {"$let": {"vars": {"selected": selected_value}, "in": {"$cond": [
        {"$and": [{"$eq": [{"$size": rows}, 1]}, {"$eq": [other_value, Bson::Null]}, {"$eq": [{"$type": "$$selected"}, "string"]}, {"$ne": ["$$selected", ""]}]},
        reference, []
    ]}}}.into()
}

/// 只关联完整身份和强外键，不展开、不过滤根记录，也不执行请求外动态准备。
fn source_stages() -> Vec<Document> {
    let mut stages = snapshot_stages();
    for (kind, collection, alias) in [
        ("sales_change_order", <Database as SalesReviewExt>::SALES_CHANGE_ORDERS, "sales_change"),
        ("purchase_change_order", <Database as PurchaseOrderExt>::PURCHASE_CHANGE_ORDERS, "purchase_change"),
        ("receipt_reversal", <Database as ReturnsExt>::RECEIPT_REVERSALS, "receipt_reversal"),
        ("payment_reversal", <Database as ReturnsExt>::PAYMENT_REVERSALS, "payment_reversal"),
        ("customer_refund", <Database as ReturnsExt>::CUSTOMER_REFUNDS, "customer_refund"),
        ("supplier_refund", <Database as ReturnsExt>::SUPPLIER_REFUNDS, "supplier_refund"),
    ] {
        stages.push(source_lookup(collection, alias, "$subject.subject_id".into(), kind));
    }

    for (kind, collection, alias, source, field) in [
        (
            "customer_refund",
            <Database as ReceivableExt>::RECEIVABLE_ENTRIES,
            "receivable_entry",
            "customer_refund",
            "original_receivable_entry_id",
        ),
        (
            "supplier_refund",
            <Database as PayableExt>::PAYABLE_ENTRIES,
            "payable_entry",
            "supplier_refund",
            "original_payable_entry_id",
        ),
    ] {
        let id = doc! {"$arrayElemAt": [format!("$_management_{source}.{field}"), 0]}.into();
        stages.push(source_lookup(collection, alias, id, kind));
    }
    stages
}

fn source_lookup(collection: &str, alias: &str, id: Bson, kind: &str) -> Document {
    let projection = source_projection(alias);
    doc! {"$lookup": {
        "from": collection, "let": {"business": id, "kind": "$process_kind"},
        "pipeline": [{"$match": {"deleted_at": 0_i64, "$expr": {"$and": [
            {"$eq": ["$$kind", kind]}, {"$eq": ["$id", "$$business"]}
        ]}}}, {"$project": projection}], "as": format!("_management_{alias}"),
    }}
}

fn source_projection(alias: &str) -> Document {
    let fields: &[&str] = match alias {
        "sales_change" => &["sales_order_id"],
        "purchase_change" => &["purchase_order_id"],
        "receipt_reversal" => &["original_customer_receipt_id"],
        "payment_reversal" => &["original_supplier_payment_id"],
        "customer_refund" => &["original_receipt_id", "original_receivable_entry_id"],
        "supplier_refund" => &["original_payment_id", "original_payable_entry_id"],
        "receivable_entry" => &["receivable_account_id"],
        "payable_entry" => &["payable_account_id"],
        _ => unreachable!("来源关联仅由静态拥有领域定义构造"),
    };
    let mut projection = doc! {"_id": 0, "id": 1};
    projection.extend(fields.iter().map(|field| ((*field).into(), Bson::Int32(1))));
    projection
}

fn snapshot_stages() -> Vec<Document> {
    vec![
        doc! {"$lookup": {"from": <Database as ApprovalIntegrationExt>::APPROVAL_SUBJECT_SNAPSHOTS,
        "let": {"instance": "$id"}, "pipeline": [{"$match": {"deleted_at": 0_i64,
            "$expr": {"$eq": ["$approval_process_instance_id", "$$instance"]}
        }}, {"$project": {"_id": 0, "approval_process_instance_id": 1, "document_type": 1,
            "business_object_id": 1, "subject_version": 1}}], "as": "_management_snapshots"}},
        doc! {"$set": {"_management_snapshot": {"$arrayElemAt": ["$_management_snapshots", 0]}}},
        doc! {"$set": {"_management_snapshot_exact": {"$and": [
            {"$eq": [{"$size": "$_management_snapshots"}, 1]},
            {"$eq": ["$_management_snapshot.approval_process_instance_id", "$id"]},
            {"$eq": ["$_management_snapshot.document_type", "$process_kind"]},
            {"$eq": ["$_management_snapshot.document_type", "$subject.subject_kind"]},
            {"$eq": ["$_management_snapshot.business_object_id", "$subject.subject_id"]},
            {"$eq": ["$_management_snapshot.subject_version", "$subject_version"]},
        ]}}},
    ]
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use erp_identity::entity::authorization::RelatedObjects;
    use erp_inventory::InventoryAction;
    use erp_inventory::repository::inventory::authorization::InventoryMapping;
    use erp_procurement::authorization::PurchaseMapping;
    use erp_warehouse::authorization::WarehouseMapping;
    use erp_workflow::authorization::approval_instances;
    use erp_workflow::ports::ApprovalSubjectFactsPort;
    use mongodb::Client;
    use persistence_core::Executor;

    use super::*;
    use crate::finance::funds_scope::FundsMapping;
    use crate::sales_center::repository::authorization::SalesMapping;

    struct UnusedSubjects;
    #[async_trait]
    impl ApprovalSubjectFactsPort for UnusedSubjects {
        async fn load(
            &self,
            _: &AccessContext,
            _: &str,
            _: &str,
            _: &mut dyn Executor,
        ) -> Result<RelatedObjects> {
            panic!("构造静态Mapping不应读取业务事实")
        }
    }

    #[tokio::test]
    async fn real_financial_sources_get_separate_complete_models_without_widening_existing_queries() {
        let client = Client::with_uri_str("mongodb://127.0.0.1:27017").await.unwrap();
        let db = client.database("management_mapping_construction");
        let (_, base) = approval_instances::bindings(db.clone(), Arc::new(UnusedSubjects)).unwrap();

        let mut roots: BTreeMap<String, Arc<dyn AccessMapping>> = BTreeMap::new();
        roots.insert(
            "sales_order".into(),
            Arc::new(SalesMapping::new(db.clone(), "sales_order", "management_sales").unwrap()),
        );
        roots.insert("purchase_order".into(), Arc::new(PurchaseMapping::new("purchase_order").unwrap()));
        roots.insert(
            "stock_adjustment".into(),
            InventoryMapping::shared(
                InventoryAction::AdjustmentList,
                WarehouseMapping::shared("warehouse").unwrap(),
            )
            .unwrap(),
        );
        for resource in [
            "customer_receipt",
            "supplier_payment",
            "receivable_account",
            "payable_account",
            "purchase_payment_request",
            "sales_invoice_request",
        ] {
            roots.insert(resource.into(), Arc::new(FundsMapping::new(db.clone(), resource).unwrap()));
        }

        let management = ApprovalManagementMapping::new(base, &roots).unwrap();
        let relations = management.relations();
        assert_eq!(relations.len(), 15);
        for (kind, resource) in
            [("customer_receipt", "customer_receipt"), ("payment_reversal", "supplier_payment")]
        {
            let source = &relations[&RelationKey::parse(format!("management_{kind}_{resource}")).unwrap()];
            let original = &roots[resource];
            assert!(matches!(original.model().output, QueryOutput::Existence { .. }));
            assert_eq!(source.model().output, QueryOutput::Object);
            assert_ne!(source.model().key, original.model().key);
            assert_eq!(source.model().key.as_str(), format!("approval_complete_{resource}"));
            assert_eq!(source.model().collection, original.model().collection);
            for (key, nested) in source.relations() {
                assert_eq!(nested.model().output, QueryOutput::Object);
                assert!(Arc::ptr_eq(&nested, &original.relations()[&key]));
            }
        }

        let direct = &relations[&RelationKey::parse("management_customer_receipt_customer_receipt").unwrap()];
        let refund = &relations[&RelationKey::parse("management_customer_refund_customer_receipt").unwrap()];
        assert!(Arc::ptr_eq(direct, refund));
    }

    fn path(row: &Document, field: &str) -> Bson {
        let (head, tail) = field.split_once('.').unwrap_or((field, ""));
        let Some(value) = row.get(head) else { return Bson::Null };
        if tail.is_empty() {
            return value.clone();
        }
        match value {
            Bson::Document(row) => path(row, tail),
            Bson::Array(rows) => rows.iter().map(|row| path(row.as_document().unwrap(), tail)).collect(),
            _ => Bson::Null,
        }
    }

    /// 对生成的Mongo表达式执行固定样本；未知操作立即失败，不接触数据库。
    fn evaluate(expression: &Bson, row: &Document, variables: &Document) -> Bson {
        match expression {
            Bson::String(field) if field.starts_with("$$") => path(variables, &field[2..]),
            Bson::String(field) if field.starts_with('$') => path(row, &field[1..]),
            Bson::Array(items) => items.iter().map(|value| evaluate(value, row, variables)).collect(),
            Bson::Document(operation) => operation_value(operation, row, variables),
            value => value.clone(),
        }
    }

    fn operation_value(operation: &Document, row: &Document, variables: &Document) -> Bson {
        assert_eq!(operation.len(), 1);
        let (name, argument) = operation.iter().next().unwrap();
        match name.as_str() {
            "$type" => match evaluate(argument, row, variables) {
                Bson::String(_) => "string".into(),
                _ => "null".into(),
            },
            "$size" => {
                i32::try_from(evaluate(argument, row, variables).as_array().unwrap().len()).unwrap().into()
            },
            "$let" => {
                let binding = argument.as_document().unwrap();
                let mut bound = variables.clone();
                for (name, value) in binding.get_document("vars").unwrap() {
                    bound.insert(name, evaluate(value, row, variables));
                }
                evaluate(binding.get("in").unwrap(), row, &bound)
            },
            _ => array_operation(name, argument.as_array().unwrap(), row, variables),
        }
    }

    fn array_operation(name: &str, arguments: &[Bson], row: &Document, variables: &Document) -> Bson {
        if name == "$cond" {
            let condition = evaluate(&arguments[0], row, variables).as_bool().unwrap();
            return evaluate(&arguments[if condition { 1 } else { 2 }], row, variables);
        }
        let values = arguments.iter().map(|value| evaluate(value, row, variables)).collect::<Vec<_>>();
        match name {
            "$eq" => (values[0] == values[1]).into(),
            "$ne" => (values[0] != values[1]).into(),
            "$and" => values.iter().all(|value| value.as_bool().unwrap_or(false)).into(),
            "$ifNull" => {
                if values[0] == Bson::Null {
                    values[1].clone()
                } else {
                    values[0].clone()
                }
            },
            "$arrayElemAt" => {
                let index = usize::try_from(values[1].as_i32().unwrap()).unwrap();
                values[0].as_array().and_then(|items| items.get(index)).cloned().unwrap_or(Bson::Null)
            },
            _ => panic!("unsupported fixture expression: {name}"),
        }
    }

    fn references(kind: &str, resource: &str, row: &Document) -> Bson {
        let (_, _, expression) = source_references()
            .into_iter()
            .find(|(candidate, source, _)| *candidate == kind && *source == resource)
            .unwrap();
        evaluate(&typed_reference(kind, expression), row, &doc! {})
    }

    #[test]
    fn exact_frozen_type_and_unique_actual_change_reference_are_required() {
        let mut row = doc! {"process_kind": "sales_change_order", "_management_snapshot_exact": true,
        "_management_sales_change": [{"sales_order_id": "real-order"}]};
        assert_eq!(
            references("sales_change_order", "sales_order", &row),
            Bson::Array(vec!["real-order".into()])
        );
        assert_eq!(references("sales_order", "sales_order", &row), Bson::Array(vec![]));

        row.insert("_management_snapshot_exact", false);
        assert_eq!(references("sales_change_order", "sales_order", &row), Bson::Array(vec![]));

        row.insert("_management_snapshot_exact", true);
        for sources in
            [vec![], vec![doc! {}], vec![doc! {"sales_order_id": "one"}, doc! {"sales_order_id": "two"}]]
        {
            row.insert("_management_sales_change", sources);
            assert_eq!(references("sales_change_order", "sales_order", &row), Bson::Array(vec![]));
        }
    }

    #[test]
    fn refund_keeps_exclusive_original_reference_and_requires_the_actual_entry() {
        let mut row = doc! {"process_kind": "customer_refund", "_management_snapshot_exact": true,
        "_management_customer_refund": [{"original_receipt_id": "receipt"}],
        "_management_receivable_entry": []};
        assert_eq!(
            references("customer_refund", "customer_receipt", &row),
            Bson::Array(vec!["receipt".into()])
        );
        assert_eq!(references("customer_refund", "receivable_account", &row), Bson::Array(vec![]));

        row.insert("_management_customer_refund", vec![doc! {"original_receivable_entry_id": "entry"}]);
        assert_eq!(references("customer_refund", "receivable_account", &row), Bson::Array(vec![]));
        row.insert("_management_receivable_entry", vec![doc! {"receivable_account_id": "account"}]);
        assert_eq!(
            references("customer_refund", "receivable_account", &row),
            Bson::Array(vec!["account".into()])
        );

        row.insert(
            "_management_customer_refund",
            vec![doc! {"original_receipt_id": "receipt", "original_receivable_entry_id": "entry"}],
        );
        assert_eq!(references("customer_refund", "customer_receipt", &row), Bson::Array(vec![]));
        assert_eq!(references("customer_refund", "receivable_account", &row), Bson::Array(vec![]));
    }

    #[test]
    fn snapshot_expression_rejects_missing_duplicate_and_identity_drift() {
        let expression = snapshot_stages()[2]
            .get_document("$set")
            .unwrap()
            .get("_management_snapshot_exact")
            .unwrap()
            .clone();
        let snapshot = doc! {"approval_process_instance_id": "instance", "document_type": "sales_order",
        "business_object_id": "order", "subject_version": 7};
        let mut row = doc! {"id": "instance", "process_kind": "sales_order", "subject_version": 7,
        "subject": {"subject_kind": "sales_order", "subject_id": "order"},
        "_management_snapshot": snapshot.clone(), "_management_snapshots": [snapshot.clone()]};
        assert_eq!(evaluate(&expression, &row, &doc! {}), Bson::Boolean(true));

        row.insert("subject_version", 8);
        assert_eq!(evaluate(&expression, &row, &doc! {}), Bson::Boolean(false));
        row.insert("subject_version", 7);
        for snapshots in [vec![], vec![snapshot.clone(), snapshot]] {
            row.insert("_management_snapshots", snapshots);
            assert_eq!(evaluate(&expression, &row, &doc! {}), Bson::Boolean(false));
        }
    }
}
