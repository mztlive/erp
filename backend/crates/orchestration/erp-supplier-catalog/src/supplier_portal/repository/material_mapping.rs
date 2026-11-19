//! 三类真实申请的固定查询分区；授权组合由 identity 决定。

use std::collections::BTreeMap;
use std::sync::Arc;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_catalog::portal::CatalogPortalExt;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, QueryModel, QueryModelKey, QueryOutput, RelationKey,
};
use erp_identity::ports::{AccessMapping, QueryFragment, RelationMapping};
use erp_identity::{Error, Result};
use erp_supplier::portal::COOPERATION_APPLICATIONS;
use erp_supply::portal::PortalSupplyExt;
use mongodb::Database;
use mongodb::bson::{Bson, doc};

/// 构造材料事实对应的三个真实查询分区，供固定工作项来源计划使用。
///
/// `supplier`：已绑定的完整供应商 Mapping；`offering`：已绑定的完整供给 Mapping。
///
/// 按 `application`、`new_product` 和 `cooperation` 索引的不可变 Mapping。
///
/// 来源模型资源或输出粒度不符时拒绝；本函数不执行 I/O。
pub fn material_mappings(
    supplier: Arc<dyn AccessMapping>,
    offering: Arc<dyn AccessMapping>,
) -> Result<BTreeMap<String, Arc<dyn AccessMapping>>> {
    for (mapping, resource) in [(&supplier, "supplier"), (&offering, "supplier_offering")] {
        if mapping.model().resource != resource || mapping.model().output != QueryOutput::Object {
            return Err(Error::ValidationError("门户材料来源必须使用对应资源的完整对象模型".into()));
        }
    }

    let mut mappings = BTreeMap::new();
    for (partition, collection) in [
        ("application", Database::PORTAL_APPLICATIONS),
        ("new_product", Database::NEW_PRODUCT_DRAFTS),
        ("cooperation", COOPERATION_APPLICATIONS),
    ] {
        let mapping = MaterialMapping {
            model: QueryModel {
                key: QueryModelKey::parse(format!("portal_material_{partition}"))?,
                resource: "supplier_portal_material".into(),
                collection: collection.into(),
                id_field: "id".into(),
                output: QueryOutput::Object,
                facts: Default::default(),
            },
            supplier: supplier.clone(),
            offering: (partition == "application").then(|| offering.clone()),
        };
        mapping.model.validate()?;
        mappings.insert(partition.into(), Arc::new(mapping) as Arc<dyn AccessMapping>);
    }
    Ok(mappings)
}

struct MaterialMapping {
    model: QueryModel,
    supplier: Arc<dyn AccessMapping>,
    offering: Option<Arc<dyn AccessMapping>>,
}

impl AccessMapping for MaterialMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }

    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        let mut relations = BTreeMap::from([(
            RelationKey::parse("supplier").expect("固定供应商关系有效"),
            self.supplier.clone(),
        )]);
        if let Some(offering) = &self.offering {
            relations.insert(RelationKey::parse("offering").expect("固定供给关系有效"), offering.clone());
        }
        relations
    }

    fn key(&self, _: &AccessContext) -> Result<Bson> {
        Ok("$id".into())
    }

    fn records(&self, _: &AccessContext) -> Result<QueryFragment> {
        let mut filter = doc! {
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "supplier_id": {"$type": "string", "$ne": ""},
        };
        if self.offering.is_some() {
            filter.insert("snapshot.kind", doc! {"$in": ["EXISTING_QUOTE", "TERMS_CHANGE", "STOP_SUPPLY"]});
        }
        Ok(QueryFragment::Match(filter))
    }

    fn predicate(&self, _: &AccessFact, _: &AccessContext) -> Result<QueryFragment> {
        Err(Error::ValidationError("门户材料归属必须沿真实供应商或供给来源检查".into()))
    }

    fn relation(&self, relation: &RelationKey, _: &AccessContext) -> Result<RelationMapping> {
        let (expected_ids, mapping) = match relation.as_str() {
            "supplier" => (Bson::Array(vec!["$supplier_id".into()]), self.supplier.clone()),
            "offering" => {
                let mapping = self
                    .offering
                    .clone()
                    .ok_or_else(|| Error::ValidationError("此材料分区没有既有供给来源".into()))?;
                let ids = doc! {"$cond": [
                    {"$in": ["$snapshot.kind", ["TERMS_CHANGE", "STOP_SUPPLY"]]},
                    ["$snapshot.offering_id"],
                    [],
                ]};
                (Bson::Document(ids), mapping)
            },
            _ => return Err(Error::ValidationError("门户材料未登记该来源关系".into())),
        };
        Ok(RelationMapping { expected_ids, mapping })
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_permissions::internal;
    use erp_supply::portal::OfferingApplicationSnapshot;
    use mongodb::bson::serialize_to_document;

    use super::*;

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin),
            permission: internal::PORTAL_MATERIAL_READ,
            resource: "supplier_portal_material".into(),
            model: None,
            as_of: Instant::from_unix_secs(10),
        }
    }

    async fn mappings() -> BTreeMap<String, Arc<dyn AccessMapping>> {
        let db = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("portal_material_mapping");
        let (_, supplier) = erp_supplier::authorization::bindings(db).unwrap();
        let offering = erp_supply::authorization::mapping("supplier_offering").unwrap();
        material_mappings(supplier, offering).unwrap()
    }

    #[tokio::test]
    async fn partitions_use_real_collections_and_keep_exact_supplier_reference() {
        let context = context();
        let mappings = mappings().await;
        assert_eq!(mappings.len(), 3);
        for (partition, collection) in [
            ("application", Database::PORTAL_APPLICATIONS),
            ("new_product", Database::NEW_PRODUCT_DRAFTS),
            ("cooperation", COOPERATION_APPLICATIONS),
        ] {
            let mapping = &mappings[partition];
            assert_eq!(mapping.model().collection, collection);
            assert_eq!(mapping.model().output, QueryOutput::Object);

            let relation = mapping.relation(&RelationKey::parse("supplier").unwrap(), &context).unwrap();
            assert_eq!(relation.expected_ids, Bson::Array(vec!["$supplier_id".into()]));
            assert!(Arc::ptr_eq(
                &relation.mapping,
                &mapping.relations()[&RelationKey::parse("supplier").unwrap()]
            ));
            let QueryFragment::Match(filter) = mapping.records(&context).unwrap() else {
                panic!("必须输出真实有效记录条件")
            };
            assert_eq!(filter.get("deleted_at"), Some(&Bson::Int64(0)));
            assert!(mapping.predicate(&AccessFact::Owned, &context).is_err());
        }
    }

    #[tokio::test]
    async fn offering_reference_follows_persisted_snapshot_and_has_no_creation_fallback() {
        let mappings = mappings().await;
        let context = context();
        let application = &mappings["application"];
        let reference = application.relation(&RelationKey::parse("offering").unwrap(), &context).unwrap();
        let snapshot = OfferingApplicationSnapshot::StopSupply {
            offering_id: "actual-offering".into(),
            expected_offering_version: 7,
            expected_revision_no: 2,
        };
        let serialized = serialize_to_document(&snapshot).unwrap();
        assert_eq!(serialized.get_str("kind").unwrap(), "STOP_SUPPLY");
        assert_eq!(serialized.get_str("offering_id").unwrap(), snapshot.target().unwrap().0);
        assert_eq!(
            reference.expected_ids,
            Bson::Document(doc! {"$cond": [
                {"$in": ["$snapshot.kind", ["TERMS_CHANGE", "STOP_SUPPLY"]]},
                ["$snapshot.offering_id"], [],
            ]})
        );

        for partition in ["new_product", "cooperation"] {
            assert!(matches!(
                mappings[partition].relation(&RelationKey::parse("offering").unwrap(), &context),
                Err(Error::ValidationError(_))
            ));
        }
    }

    #[test]
    fn swapped_source_models_are_rejected() {
        let offering = erp_supply::authorization::mapping("supplier_offering").unwrap();
        assert!(matches!(material_mappings(offering.clone(), offering), Err(Error::ValidationError(_))));
    }
}
