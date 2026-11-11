//! 集成记录的登记、查询映射与对象事实；全部判定由 identity 执行。
use std::collections::BTreeMap;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, QueryModelKey,
    QueryOutput, ReadAccess, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{
    AccessFactsPort, AccessMapping, AccessMode, AccessPolicy, AccessRegistration, AccessSubject,
    AccessTarget, AuthorizationRegistry, AuthorizationService, CompiledAccess, DataAccess,
    Error as IdentityError, Permission, ResourceTargets, Result as IdentityResult, SharedRbacService,
};
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;

use crate::repository::IntegrationOpsExt;

/// 固定登记的集成授权设施；查询凭证绑定本实例。
#[derive(Clone)]
pub struct IntegrationAuthorization {
    service: AuthorizationService,
    mappings: BTreeMap<String, Arc<dyn AccessMapping>>,
}
impl IntegrationAuthorization {
    /// 登记集成列表、详情、创建与差异决定的明确入口。
    /// # 参数
    /// db、rbac 为组合根共享身份设施。
    /// # 返回
    /// 固定完整登记的授权设施。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        let mut entries = Vec::new();
        let mut mappings = BTreeMap::new();
        for (resource, collection) in [
            ("integration_error_task", Database::INTEGRATION_ERROR_TASKS),
            ("reconciliation_difference", Database::RECONCILIATION_DIFFERENCES),
        ] {
            let mapping: Arc<dyn AccessMapping> = Arc::new(IntegrationMapping::new(resource, collection));
            let facts: Arc<dyn AccessFactsPort> = Arc::new(IntegrationFacts { db: db.clone() });
            for action in ["list", "detail"] {
                let permission = Permission::parse(format!("{resource}:{action}")).expect("固定集成权限");
                entries.push(AccessRegistration {
                    permission: permission.clone(),
                    required_permissions: vec![],
                    access: AccessMode::QueryAndObject {
                        data: DataAccess {
                            subject: AccessSubject::Direct,
                            required_facts: [FactKind::Owner, FactKind::Organization].into(),
                            policy: AccessPolicy::Scope { permission, read: ReadAccess::default() },
                        },
                        mapping: mapping.clone(),
                        facts: facts.clone(),
                    },
                });
            }
            entries.push(AccessRegistration {
                permission: Permission::parse(format!("{resource}:create")).expect("固定集成创建权限"),
                required_permissions: vec![],
                access: AccessMode::Operation,
            });
            mappings.insert(resource.into(), mapping);
        }
        let registry = AuthorizationRegistry::new(entries).expect("固定集成登记完整");
        Self { service: AuthorizationService::new(db, rbac, registry), mappings }
    }

    /// 查询并编译固定资源的完整授权条件。
    /// # 参数
    /// actor、resource、action 指定服务端入口，executor 为原执行器。
    /// # 返回
    /// identity 生成的不可伪造查询凭证。
    /// # 错误
    /// 未登记入口、无权、映射或事实失败时拒绝。
    pub async fn query(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> IdentityResult<CompiledAccess> {
        let mapping = self
            .mappings
            .get(resource)
            .ok_or_else(|| IdentityError::ValidationError("集成资源未登记".into()))?;
        self.service
            .query(
                actor,
                &Permission::parse(format!("{resource}:{action}"))?,
                AccessSubject::Direct,
                executor,
            )
            .await?
            .compile(mapping.as_ref(), executor)
            .await
    }

    /// 复验同一服务生成的读取凭证。
    /// # 参数
    /// access、executor 为本次凭证及原执行器。
    /// # 返回
    /// 仍有效时成功。
    /// # 错误
    /// 身份、权限、映射或版本变化时拒绝。
    pub async fn validate(&self, access: &CompiledAccess, executor: &mut dyn Executor) -> IdentityResult<()> {
        self.service.validate(access, executor).await
    }

    /// 按真实记录身份执行完整对象授权。
    /// # 参数
    /// actor、resource、action 为固定入口，id 为实际根记录，executor 为原执行器。
    /// # 返回
    /// 全部对象事实及身份检查通过时成功。
    /// # 错误
    /// 对象缺失、无权或版本变化拒绝。
    pub async fn require(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> IdentityResult<()> {
        let target = if action == "create" {
            AccessTarget::Operation
        } else {
            AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? }
        };
        self.service
            .require(actor, &Permission::parse(format!("{resource}:{action}"))?, target, executor)
            .await
    }
}

struct IntegrationFacts {
    db: Database,
}
#[async_trait]
impl AccessFactsPort for IntegrationFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        let mut objects = Vec::new();
        for id in ids.ids() {
            let object = match context.resource.as_str() {
                "integration_error_task" => {
                    self.db.integration_error_tasks().find_by_id(id, executor).await?.map(|row| ObjectFacts {
                        resource: context.resource.clone(),
                        id: row.base.id,
                        version: row.base.version.to_string(),
                        owner_id: FactValue::Known(row.owner_user_id),
                        org_unit_id: FactValue::Known(Some(row.owner_org_unit_id)),
                        ..Default::default()
                    })
                },
                "reconciliation_difference" => {
                    self.db.reconciliation_differences().find_by_id(id, executor).await?.map(|row| {
                        ObjectFacts {
                            resource: context.resource.clone(),
                            id: row.base.id,
                            version: row.base.version.to_string(),
                            owner_id: FactValue::Known(Some(row.owner_user_id)),
                            org_unit_id: FactValue::Known(Some(row.owner_org_unit_id)),
                            ..Default::default()
                        }
                    })
                },
                _ => return Err(IdentityError::ValidationError("集成事实资源未登记".into())),
            };
            if let Some(object) = object {
                objects.push(object);
            }
        }
        Ok(objects)
    }
}

struct IntegrationMapping {
    model: QueryModel,
}
impl IntegrationMapping {
    fn new(resource: &str, collection: &str) -> Self {
        Self {
            model: QueryModel {
                key: QueryModelKey::parse(collection).expect("固定集成模型"),
                resource: resource.into(),
                collection: collection.into(),
                id_field: "id".into(),
                output: QueryOutput::Object,
                facts: [FactKind::Owner, FactKind::Organization].into(),
            },
        }
    }
}
#[async_trait]
impl AccessMapping for IntegrationMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, _: &AccessContext, _: &MappingFacts) -> IdentityResult<Bson> {
        Ok("$id".into())
    }
    async fn prepare(&self, _: &AccessContext, _: &mut dyn Executor) -> IdentityResult<MappingFacts> {
        Ok(MappingFacts::default())
    }
    fn records(&self, _: &AccessContext, _: &MappingFacts) -> IdentityResult<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at": 0_i64}))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<QueryFragment> {
        match fact {
            AccessFact::Owned => Ok(QueryFragment::Match(doc! {"owner_user_id": context.actor.id()})),
            AccessFact::OrganizationIn(ids) => Ok(QueryFragment::Match(
                doc! {"owner_org_unit_id": {"$in": ids.iter().cloned().collect::<Vec<_>>()}},
            )),
            _ => Err(IdentityError::ValidationError("集成模型不支持该事实维度".into())),
        }
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<RelationMapping> {
        Err(IdentityError::ValidationError("集成模型没有来源授权关系".into()))
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;

    use super::*;
    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("handler".into(), "handler".into(), AccountKind::Admin),
            permission: Permission::parse("integration_error_task:list").unwrap(),
            resource: "integration_error_task".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        }
    }
    #[test]
    fn actual_handler_and_handler_organization_are_independent_facts() {
        let mapping = IntegrationMapping::new("integration_error_task", Database::INTEGRATION_ERROR_TASKS);
        let own = mapping.predicate(&AccessFact::Owned, &context(), &MappingFacts::default()).unwrap();
        assert!(
            matches!(own, QueryFragment::Match(document) if document == doc! {"owner_user_id":"handler"})
        );
        let org = mapping
            .predicate(
                &AccessFact::OrganizationIn(["org".into()].into()),
                &context(),
                &MappingFacts::default(),
            )
            .unwrap();
        assert!(
            matches!(org, QueryFragment::Match(document) if document == doc! {"owner_org_unit_id":{"$in":["org"]}})
        );
    }
    #[test]
    fn history_and_unknown_relations_cannot_grant_integration_read_access() {
        let mapping =
            IntegrationMapping::new("reconciliation_difference", Database::RECONCILIATION_DIFFERENCES);
        assert!(
            mapping.predicate(&AccessFact::HistoryParticipant, &context(), &MappingFacts::default()).is_err()
        );
        assert!(
            mapping
                .relation(&RelationKey::parse("source").unwrap(), &context(), &MappingFacts::default())
                .is_err()
        );
    }
}
