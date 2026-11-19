//! 工作项详情的固定真实来源关系；只描述业务外键，不声明授权政策。
use std::collections::BTreeMap;
use std::sync::Arc;

use erp_catalog::portal::CatalogPortalExt;
use erp_fulfillment::repository::FulfillmentExt;
use erp_identity::entity::authorization::{AccessContext, AccessFact, QueryModel, RelationKey};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{AccessMapping, Error, Result};
use erp_permissions::{internal, purchase_order, sales_order, supplier, supplier_offering};
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::SalesReviewExt;
use erp_supplier::portal::COOPERATION_APPLICATIONS;
use erp_supplier_catalog::supplier_portal::authorization::material_mappings;
use erp_supply::portal::PortalSupplyExt;
use erp_workflow::repository::BpmExt;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};

use super::super::{MappingBindings, mapping};

pub(in super::super) struct TaskDetailMapping {
    base: Arc<dyn AccessMapping>,
    sources: BTreeMap<RelationKey, Arc<dyn AccessMapping>>,
}

impl TaskDetailMapping {
    // 装配详情专用映射时复用真实来源模型，不覆盖列表与任务管理使用的映射。
    pub(in super::super) fn shared(
        base: Arc<dyn AccessMapping>,
        mappings: &MappingBindings,
    ) -> Result<Arc<dyn AccessMapping>> {
        let portal = material_mappings(
            mapping(mappings, &supplier::DETAIL)?,
            mapping(mappings, &supplier_offering::LIST)?,
        )?;
        let sources = [
            ("detail_other", base.clone()),
            ("detail_approval_owner", base.clone()),
            ("detail_sales", mapping(mappings, &sales_order::DETAIL)?),
            ("detail_purchase", mapping(mappings, &purchase_order::DETAIL)?),
            ("detail_portal_application", portal["application"].clone()),
            ("detail_portal_offering", portal["application"].clone()),
            ("detail_portal_new_product", portal["new_product"].clone()),
            ("detail_portal_cooperation", portal["cooperation"].clone()),
            ("detail_approval_management", mapping(mappings, &internal::APPROVAL_MANAGEMENT_READ)?),
        ]
        .into_iter()
        .map(|(key, value)| Ok((RelationKey::parse(key)?, value)))
        .collect::<Result<_>>()?;
        Ok(Arc::new(Self { base, sources }))
    }
}

impl AccessMapping for TaskDetailMapping {
    fn model(&self) -> &QueryModel {
        self.base.model()
    }
    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        self.sources.clone()
    }
    fn stages(&self) -> Vec<Document> {
        self.base.stages().into_iter().chain(source_stages()).collect()
    }
    fn key(&self, context: &AccessContext) -> Result<Bson> {
        self.base.key(context)
    }
    fn records(&self, context: &AccessContext) -> Result<QueryFragment> {
        self.base.records(context)
    }
    fn predicate(&self, fact: &AccessFact, context: &AccessContext) -> Result<QueryFragment> {
        self.base.predicate(fact, context)
    }
    fn relation(&self, key: &RelationKey, _: &AccessContext) -> Result<RelationMapping> {
        let mapping =
            self.sources.get(key).ok_or_else(|| Error::ValidationError("任务详情来源关系未登记".into()))?;
        Ok(RelationMapping { mapping: mapping.clone(), expected_ids: reference(key.as_str())? })
    }
}

// 为工作项详情搭建只补充身份和真实外键的读取阶段，不过滤、展开或分页根工作项。
fn source_stages() -> Vec<Document> {
    let mut stages = Vec::new();
    for (kind, collection, alias, field) in [
        (
            "sales_change_order",
            <Database as SalesReviewExt>::SALES_CHANGE_ORDERS,
            "sales_change",
            "sales_order_id",
        ),
        (
            "purchase_change_order",
            <Database as PurchaseOrderExt>::PURCHASE_CHANGE_ORDERS,
            "purchase_change",
            "purchase_order_id",
        ),
        ("delivery", <Database as FulfillmentExt>::DELIVERIES, "delivery", "sales_order_id"),
        ("purchase_receipt", <Database as FulfillmentExt>::PURCHASE_RECEIPTS, "receipt", "purchase_order_id"),
        (
            "electronic_delivery",
            <Database as FulfillmentExt>::ELECTRONIC_DELIVERIES,
            "electronic",
            "purchase_order_id",
        ),
        (
            "service_fulfillment",
            <Database as FulfillmentExt>::SERVICE_FULFILLMENTS,
            "service",
            "purchase_order_id",
        ),
    ] {
        stages.push(lookup(collection, alias, "$business_object_id", kind, field));
    }
    for (collection, alias) in [
        (<Database as PortalSupplyExt>::PORTAL_APPLICATIONS, "application"),
        (<Database as CatalogPortalExt>::NEW_PRODUCT_DRAFTS, "new_product"),
        (COOPERATION_APPLICATIONS, "cooperation"),
    ] {
        stages.push(lookup(
            collection,
            alias,
            "$business_object_id",
            "supplier_portal_request",
            "snapshot.kind",
        ));
    }
    stages.push(approval_lookup());
    stages
}

fn lookup(collection: &str, alias: &str, id: &str, kind: &str, field: &str) -> Document {
    let mut projection = doc! { "_id": 0, "id": 1 };
    projection.insert(field, 1);
    doc! { "$lookup": {
        "from": collection, "let": { "business": id, "kind": "$business_object_type" },
        "pipeline": [{ "$match": { "deleted_at": 0_i64, "$expr": { "$and": [
            { "$eq": ["$id", "$$business"] }, { "$eq": ["$$kind", kind] }
        ] } } }, { "$project": projection }], "as": format!("_task_detail_{alias}"),
    } }
}

fn approval_lookup() -> Document {
    doc! { "$lookup": {
        "from": <Database as BpmExt>::APPROVAL_NODE_EXECUTIONS,
        "let": { "execution": "$approval_node_execution_id", "kind": "$work_item_type" },
        "pipeline": [{ "$match": { "deleted_at": 0_i64, "$expr": { "$and": [
            { "$eq": ["$id", "$$execution"] }, { "$eq": ["$$kind", "DOCUMENT_APPROVAL"] }
        ] } } }, { "$project": { "_id": 0, "id": 1, "process_instance_id": 1 } }],
        "as": "_task_detail_approval",
    } }
}

fn reference(key: &str) -> Result<Bson> {
    let not_approval = doc! { "$ne": ["$work_item_type", "DOCUMENT_APPROVAL"] };
    let value = match key {
        "detail_other" => gated(
            doc! { "$and": [not_approval, { "$not": [{ "$in": ["$business_object_type", [
                "sales_order", "voucher_sales_order", "sales_change_order", "delivery", "purchase_order", "purchase_change_order", "purchase_receipt", "electronic_delivery", "service_fulfillment", "supplier_portal_request"
            ]] }] }] },
            singleton("$id"),
        ),
        "detail_sales" => gated(not_approval, order_reference(true)),
        "detail_purchase" => gated(not_approval, order_reference(false)),
        "detail_approval_owner" => {
            gated(doc! { "$eq": ["$work_item_type", "DOCUMENT_APPROVAL"] }, singleton("$id"))
        },
        "detail_approval_management" => unique("approval", "process_instance_id"),
        "detail_portal_application"
        | "detail_portal_offering"
        | "detail_portal_new_product"
        | "detail_portal_cooperation" => gated(not_approval, portal_reference(key)),
        _ => return Err(Error::ValidationError("任务详情来源引用未登记".into())),
    };
    Ok(value)
}

fn order_reference(sales: bool) -> Bson {
    let branches: Vec<_> = if sales {
        vec![
            (vec!["sales_order", "voucher_sales_order"], singleton("$business_object_id")),
            (vec!["sales_change_order"], unique("sales_change", "sales_order_id")),
            (vec!["delivery"], unique("delivery", "sales_order_id")),
        ]
    } else {
        vec![
            (vec!["purchase_order"], singleton("$business_object_id")),
            (vec!["purchase_change_order"], unique("purchase_change", "purchase_order_id")),
            (vec!["purchase_receipt"], unique("receipt", "purchase_order_id")),
            (vec!["electronic_delivery"], unique("electronic", "purchase_order_id")),
            (vec!["service_fulfillment"], unique("service", "purchase_order_id")),
        ]
    }
    .into_iter()
    .map(|(kinds, value)| doc! { "case": { "$in": ["$business_object_type", kinds] }, "then": value })
    .collect();
    doc! { "$switch": { "branches": branches, "default": [] } }.into()
}

// 确定门户材料的申请类型时与材料事实端口保持一致：选择首个真实存在且未删除的申请，不按授权结果换源。
fn portal_reference(key: &str) -> Bson {
    let application = "$_task_detail_application";
    let new_product = "$_task_detail_new_product";
    let cooperation = "$_task_detail_cooperation";
    let kind = doc! { "$arrayElemAt": ["$_task_detail_application.snapshot.kind", 0] };
    let update = doc! { "$in": [kind, ["TERMS_CHANGE", "STOP_SUPPLY"]] };
    let (condition, reference) = match key {
        "detail_portal_application" => (
            doc! { "$and": [{ "$eq": [{ "$size": application }, 1] }, { "$not": [update] }] },
            unique("application", "id"),
        ),
        "detail_portal_offering" => {
            (doc! { "$and": [{ "$eq": [{ "$size": application }, 1] }, update] }, unique("application", "id"))
        },
        "detail_portal_new_product" => (
            doc! { "$and": [{ "$eq": [{ "$size": application }, 0] }, { "$eq": [{ "$size": new_product }, 1] }] },
            unique("new_product", "id"),
        ),
        "detail_portal_cooperation" => (
            doc! { "$and": [{ "$eq": [{ "$size": application }, 0] }, { "$eq": [{ "$size": new_product }, 0] }, { "$eq": [{ "$size": cooperation }, 1] }] },
            unique("cooperation", "id"),
        ),
        _ => unreachable!("只由固定门户分区调用"),
    };
    gated(condition, reference)
}

fn gated(condition: Document, reference: Bson) -> Bson {
    doc! { "$cond": [condition, reference, []] }.into()
}
fn singleton(field: &str) -> Bson {
    doc! { "$cond": [{ "$and": [{ "$eq": [{ "$type": field }, "string"] }, { "$ne": [field, ""] }] }, [field], []] }.into()
}
fn unique(alias: &str, field: &str) -> Bson {
    let rows = format!("$_task_detail_{alias}");
    let value = doc! { "$arrayElemAt": [format!("{rows}.{field}"), 0] };
    doc! { "$cond": [{ "$eq": [{ "$size": &rows }, 1] }, { "$let": { "vars": { "reference": value }, "in": singleton("$$reference") } }, []] }.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 在组装工作项授权映射时求值静态来源引用；只解析表达式，不模拟权限或查询数据库。
    fn evaluate(value: &Bson, row: &Document, variables: &Document) -> Bson {
        match value {
            Bson::String(field) if field.starts_with("$$") => {
                variables.get(&field[2..]).cloned().unwrap_or(Bson::Null)
            },
            Bson::String(field) if field.starts_with('$') => path(&Bson::Document(row.clone()), &field[1..]),
            Bson::Array(values) => values.iter().map(|value| evaluate(value, row, variables)).collect(),
            Bson::Document(expression) => {
                let (operator, value) = expression.iter().next().unwrap();
                if operator == "$let" {
                    let body = value.as_document().unwrap();
                    let mut variables = variables.clone();
                    for (key, value) in body.get_document("vars").unwrap() {
                        variables.insert(key, evaluate(value, row, &variables));
                    }
                    return evaluate(&body["in"], row, &variables);
                }
                if operator == "$switch" {
                    let body = value.as_document().unwrap();
                    for branch in body.get_array("branches").unwrap() {
                        let branch = branch.as_document().unwrap();
                        if evaluate(&branch["case"], row, variables).as_bool().unwrap() {
                            return evaluate(&branch["then"], row, variables);
                        }
                    }
                    return evaluate(&body["default"], row, variables);
                }
                if operator == "$cond" {
                    let values = value.as_array().unwrap();
                    return evaluate(
                        &values[if evaluate(&values[0], row, variables).as_bool().unwrap() { 1 } else { 2 }],
                        row,
                        variables,
                    );
                }
                let arguments = evaluate(value, row, variables);
                let values = arguments.as_array();
                match operator.as_str() {
                    "$and" => values.unwrap().iter().all(|value| value.as_bool().unwrap()).into(),
                    "$not" => (!values.unwrap()[0].as_bool().unwrap()).into(),
                    "$eq" => (values.unwrap()[0] == values.unwrap()[1]).into(),
                    "$ne" => (values.unwrap()[0] != values.unwrap()[1]).into(),
                    "$in" => values.unwrap()[1].as_array().unwrap().contains(&values.unwrap()[0]).into(),
                    "$size" => i32::try_from(arguments.as_array().unwrap().len()).unwrap().into(),
                    "$type" => if matches!(arguments, Bson::String(_)) { "string" } else { "null" }.into(),
                    "$arrayElemAt" => values.unwrap()[0]
                        .as_array()
                        .unwrap()
                        .get(usize::try_from(values.unwrap()[1].as_i32().unwrap()).unwrap())
                        .cloned()
                        .unwrap_or(Bson::Null),
                    _ => panic!("未支持的静态来源表达式：{operator}"),
                }
            },
            value => value.clone(),
        }
    }

    fn path(value: &Bson, field: &str) -> Bson {
        if field.is_empty() {
            return value.clone();
        }
        if let Bson::Array(rows) = value {
            return rows.iter().map(|row| path(row, field)).collect();
        }
        let (head, tail) = field.split_once('.').unwrap_or((field, ""));
        value.as_document().and_then(|row| row.get(head)).map_or(Bson::Null, |value| path(value, tail))
    }

    fn ids(key: &str, row: &Document) -> Bson {
        evaluate(&reference(key).unwrap(), row, &doc! {})
    }

    #[test]
    fn fulfillment_and_change_references_use_authoritative_foreign_keys() {
        let row = doc! {"id":"task","work_item_type":"FULFILLMENT_OPERATION","business_object_type":"delivery","business_object_id":"delivery", "_task_detail_delivery":[{"id":"delivery","sales_order_id":"real-sale"}]};
        assert_eq!(ids("detail_sales", &row), Bson::Array(vec!["real-sale".into()]));
        assert_eq!(ids("detail_purchase", &row), Bson::Array(vec![]));
        let mut missing = row.clone();
        missing.insert("_task_detail_delivery", vec![doc! {"id":"delivery"}]);
        assert_eq!(ids("detail_sales", &missing), Bson::Array(vec![]));
        let change = doc! {"work_item_type":"APPROVAL_REWORK","business_object_type":"purchase_change_order","business_object_id":"change","_task_detail_purchase_change":[{"id":"change","purchase_order_id":"real-purchase"}]};
        assert_eq!(ids("detail_purchase", &change), Bson::Array(vec!["real-purchase".into()]));
    }

    #[test]
    fn portal_uses_frozen_snapshot_kind_and_original_first_found_collection_order() {
        let mut row = doc! {"work_item_type":"SUPPLIER_PORTAL_REVIEW","business_object_type":"supplier_portal_request", "_task_detail_application":[{"id":"request","kind":"EXISTING_QUOTE","snapshot":{"kind":"TERMS_CHANGE"}}], "_task_detail_new_product":[{"id":"request"}], "_task_detail_cooperation":[{"id":"request"}]};
        assert_eq!(ids("detail_portal_offering", &row), Bson::Array(vec!["request".into()]));
        for key in ["detail_portal_application", "detail_portal_new_product", "detail_portal_cooperation"] {
            assert_eq!(ids(key, &row), Bson::Array(vec![]));
        }
        row.insert("_task_detail_application", Vec::<Document>::new());
        assert_eq!(ids("detail_portal_new_product", &row), Bson::Array(vec!["request".into()]));
        assert_eq!(ids("detail_portal_cooperation", &row), Bson::Array(vec![]));
        row.insert("_task_detail_new_product", Vec::<Document>::new());
        assert_eq!(ids("detail_portal_cooperation", &row), Bson::Array(vec!["request".into()]));
    }

    #[test]
    fn approval_task_does_not_fall_back_to_the_ordinary_order_or_other_partitions() {
        let row = doc! {"id":"task","work_item_type":"DOCUMENT_APPROVAL","business_object_type":"sales_order","business_object_id":"sale","_task_detail_approval":[{"id":"execution","process_instance_id":"instance"}]};
        assert_eq!(ids("detail_approval_owner", &row), Bson::Array(vec!["task".into()]));
        assert_eq!(ids("detail_approval_management", &row), Bson::Array(vec!["instance".into()]));
        assert_eq!(ids("detail_sales", &row), Bson::Array(vec![]));
        assert_eq!(ids("detail_other", &row), Bson::Array(vec![]));
    }
}
