//! 京东连接的固定入口与真实供应商来源事实。
use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, FactValue, ObjectFacts, RelatedObjects, RelationKey,
};
use erp_identity::{AccessFactsPort, ResourceTargets, Result};
use erp_supplier::SupplierExt;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::repository::SupplierApiExt;
use mongodb::Database;
use persistence_core::Executor;

const SUPPLIER_RELATION: &str = "supplier";

/// 构造连接及其真实供应商来源的持久化事实读取器。
/// # 参数
/// db 为连接与供应商所在数据库。
/// # 返回
/// 不保存请求对象的共享事实适配器。
/// # 错误
/// 构造不执行 I/O。
pub fn connection_facts_port(db: Database) -> Arc<dyn AccessFactsPort> {
    Arc::new(ConnectionFacts { db })
}
struct ConnectionFacts {
    db: Database,
}

#[async_trait]
impl AccessFactsPort for ConnectionFacts {
    async fn load(
        &self,
        _: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let mut objects = Vec::new();
        for id in ids.ids() {
            let Some(connection) = self.db.supplier_api_connections().find_by_id(id, executor).await? else {
                continue;
            };

            let supplier = self.db.supplier_accounts().find_by_id(&connection.supplier_id, executor).await?;
            let sources = supplier
                .map(|supplier| ObjectFacts {
                    resource: "supplier".into(),
                    id: supplier.base.id,
                    version: supplier.base.version.to_string(),
                    owner_id: FactValue::Known(Some(supplier.maintainer_user_id)),
                    org_unit_id: FactValue::Known(Some(supplier.business_org_unit_id)),
                    ..Default::default()
                })
                .into_iter()
                .collect();
            objects.push(connection_facts(&connection, sources)?);
        }
        Ok(objects)
    }
}

/// 保留实际引用，缺失供应商交由统一 AllNonEmpty 政策拒绝。
fn connection_facts(connection: &SupplierApiConnection, sources: Vec<ObjectFacts>) -> Result<ObjectFacts> {
    Ok(ObjectFacts {
        resource: "supplier_api_connection".into(),
        id: connection.base.id.clone(),
        version: connection.base.version.to_string(),
        relations: [(
            RelationKey::parse(SUPPLIER_RELATION)?,
            RelatedObjects { expected_ids: [connection.supplier_id.to_string()].into(), objects: sources },
        )]
        .into(),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::super::tests::connection;
    use super::*;

    #[test]
    fn supplier_source_keeps_the_real_reference_when_the_supplier_is_missing() {
        let connection = connection();
        let facts = connection_facts(&connection, vec![]).unwrap();
        let relation = &facts.relations[&RelationKey::parse(SUPPLIER_RELATION).unwrap()];
        assert_eq!(facts.id, connection.base.id);
        assert_eq!(relation.expected_ids, [connection.supplier_id.to_string()].into());
        assert!(relation.objects.is_empty());
    }

    #[test]
    fn existing_supplier_keeps_its_identity_and_version_for_revalidation() {
        let connection = connection();
        let source = ObjectFacts {
            resource: "supplier".into(),
            id: connection.supplier_id.to_string(),
            version: "7".into(),
            ..Default::default()
        };
        let facts = connection_facts(&connection, vec![source.clone()]).unwrap();
        let relation = &facts.relations[&RelationKey::parse(SUPPLIER_RELATION).unwrap()];
        assert_eq!(relation.expected_ids, [source.id.clone()].into());
        assert_eq!(relation.objects, vec![source]);
    }
}
