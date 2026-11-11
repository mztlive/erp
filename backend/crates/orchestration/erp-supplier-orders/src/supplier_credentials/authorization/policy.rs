//! 京东连接的固定入口与真实供应商来源事实。

use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, ObjectFacts, ReadAccess, RelatedObjects, RelationKey, RelationMatch,
};
use erp_identity::{
    AccessFactsPort, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AuthorizationRegistry,
    AuthorizationService, DataAccess, Error, Permission, ResourceTargets, Result, SharedRbacService,
};
use erp_supplier::SupplierExt;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::repository::SupplierApiExt;
use mongodb::Database;
use persistence_core::Executor;

const SUPPLIER_RELATION: &str = "supplier";

/// 固定登记全部支持的连接动作，保留供应商详情功能的独立角色资格。
pub(super) fn authorization(
    db: &Database,
    rbac: SharedRbacService,
    connection: &SupplierApiConnection,
) -> Result<AuthorizationService> {
    let supplier = Permission::parse("supplier:detail")?;
    let facts: Arc<dyn AccessFactsPort> = Arc::new(ConnectionFacts {
        db: db.clone(),
        connection_id: connection.base.id.clone(),
        version: connection.base.version,
    });
    let mut entries = Vec::new();
    for action in ["detail", "authorize"] {
        entries.push(AccessRegistration {
            permission: Permission::parse(format!("supplier_api_connection:{action}"))?,
            required_permissions: vec![],
            access: AccessMode::Object {
                data: DataAccess {
                    subject: AccessSubject::Direct,
                    required_facts: Default::default(),
                    policy: AccessPolicy::All(vec![
                        AccessPolicy::Permission(supplier.clone()),
                        AccessPolicy::Related {
                            relation: RelationKey::parse(SUPPLIER_RELATION)?,
                            resource: "supplier".into(),
                            mode: RelationMatch::AllNonEmpty,
                            required_facts: Default::default(),
                            policy: Box::new(AccessPolicy::Scope {
                                permission: supplier.clone(),
                                read: ReadAccess::default(),
                            }),
                        },
                    ]),
                },
                facts: facts.clone(),
            },
        });
    }
    Ok(AuthorizationService::new(db.clone(), rbac, AuthorizationRegistry::new(entries)?))
}

struct ConnectionFacts {
    db: Database,
    connection_id: String,
    version: u64,
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
            if connection.base.id != self.connection_id || connection.base.version != self.version {
                return Err(Error::ConflictError("DATA_SCOPE_CHANGED：连接来源已变化".into()));
            }
            let supplier = self.db.supplier_accounts().find_by_id(&connection.supplier_id, executor).await?;
            let sources = supplier
                .map(|supplier| ObjectFacts {
                    resource: "supplier".into(),
                    id: supplier.base.id,
                    version: supplier.base.version.to_string(),
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
