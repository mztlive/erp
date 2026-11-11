//! 采购授权组合根；事实和字段归拥有域，全部权限规则由 identity 执行。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, ReadAccess,
    RelationKey,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_identity::{
    AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget, AuthorizationRegistry,
    AuthorizationService, DataAccess, Permission, ResourceTargets, SharedRbacService,
};
use erp_procurement::authorization::{PurchaseFacts, PurchaseMapping};
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_procurement::{PurchaseAccess, PurchaseDataScopePort, PurchaseResolvedScope};
use erp_workflow::DocumentRegistryExt;
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::policy::{ALL_DOCUMENT_TYPES, DocumentApprovalPolicy, policy_of};
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;

use super::scope_support;

#[derive(Clone)]
pub struct MongoPurchaseDataScope {
    db: Database,
    rbac: SharedRbacService,
}
impl MongoPurchaseDataScope {
    /// 绑定数据库及身份服务。
    /// # 参数
    /// db 为原业务库，rbac 为共享身份服务。
    /// # 返回
    /// 无缓存权限适配器。
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }
    /// 创建采购窄端口。
    /// # 参数
    /// db、rbac 为组合根能力。
    /// # 返回
    /// 可注入领域服务的共享端口。
    /// # 错误
    /// 无。
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn PurchaseDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }

    fn authorization(
        &self,
        action: &str,
        permissions: &[String],
        candidate: Option<PurchaseOrder>,
    ) -> erp_procurement::Result<(AuthorizationService, Arc<dyn AccessMapping>, Permission)> {
        let mapping: Arc<dyn AccessMapping> = Arc::new(PurchaseHistoryMapping {
            inner: PurchaseMapping::new("purchase_order")?,
            model: purchase_model()?,
        });
        let object = candidate.is_some();
        let facts = Arc::new(PurchaseObjectFacts { db: self.db.clone(), candidate });
        let entry = purchase_registration(action, permissions, mapping.clone(), facts, object)?;
        let permission = entry.permission.clone();
        Ok((
            AuthorizationService::new(
                self.db.clone(),
                self.rbac.clone(),
                AuthorizationRegistry::new(vec![entry])?,
            ),
            mapping,
            permission,
        ))
    }
}

fn purchase_registration(
    action: &str,
    required: &[String],
    mapping: Arc<dyn AccessMapping>,
    facts: Arc<dyn AccessFactsPort>,
    object: bool,
) -> erp_procurement::Result<AccessRegistration> {
    let permission = Permission::parse(format!("purchase_order:{action}"))?;
    let data = DataAccess {
        subject: AccessSubject::Direct,
        required_facts: [FactKind::Owner, FactKind::Organization].into(),
        policy: AccessPolicy::Scope {
            permission: permission.clone(),
            read: ReadAccess {
                history: matches!(action, "list" | "detail"),
                records: matches!(action, "list" | "detail"),
            },
        },
    };
    Ok(AccessRegistration {
        permission,
        required_permissions: required.iter().map(Permission::parse).collect::<erp_core::Result<Vec<_>>>()?,
        access: if object {
            AccessMode::Object { data, facts }
        } else {
            AccessMode::QueryAndObject { data, mapping, facts }
        },
    })
}

fn purchase_model() -> erp_identity::Result<QueryModel> {
    let mut model = PurchaseMapping::new("purchase_order")?.model().clone();
    model.facts.extend([FactKind::HistoryParticipants, FactKind::Collaborators]);
    Ok(model)
}
struct PurchaseHistoryMapping {
    inner: PurchaseMapping,
    model: QueryModel,
}
#[async_trait]
impl AccessMapping for PurchaseHistoryMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, c: &AccessContext, f: &MappingFacts) -> erp_identity::Result<Bson> {
        self.inner.key(c, f)
    }
    async fn prepare(&self, c: &AccessContext, e: &mut dyn Executor) -> erp_identity::Result<MappingFacts> {
        self.inner.prepare(c, e).await
    }
    fn records(&self, c: &AccessContext, f: &MappingFacts) -> erp_identity::Result<QueryFragment> {
        self.inner.records(c, f)
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        c: &AccessContext,
        f: &MappingFacts,
    ) -> erp_identity::Result<QueryFragment> {
        match fact {
            AccessFact::HistoryParticipant => Ok(QueryFragment::Exists {
                collection: Database::DOCUMENT_PARTICIPANTS.into(),
                bindings: doc! { "document": "$id" },
                pipeline: vec![
                    doc! { "$match": { "participant_user_id": c.actor.id(), "deleted_at": 0_i64, "$expr": { "$eq": ["$document_id", "$$document"] } } },
                ],
            }),
            AccessFact::Collaborating => Ok(QueryFragment::Match(doc! { "id": { "$in": [] } })),
            _ => self.inner.predicate(fact, c, f),
        }
    }
    fn relation(
        &self,
        r: &RelationKey,
        c: &AccessContext,
        f: &MappingFacts,
    ) -> erp_identity::Result<RelationMapping> {
        self.inner.relation(r, c, f)
    }
}
struct PurchaseObjectFacts {
    db: Database,
    candidate: Option<PurchaseOrder>,
}
#[async_trait]
impl AccessFactsPort for PurchaseObjectFacts {
    async fn load(
        &self,
        c: &AccessContext,
        ids: &ResourceTargets,
        e: &mut dyn Executor,
    ) -> erp_identity::Result<Vec<ObjectFacts>> {
        let mut rows = if let Some(order) = &self.candidate {
            if ids.ids().iter().any(|id| id == &order.base.id) {
                vec![PurchaseFacts::object(order, "purchase_order")]
            } else {
                vec![]
            }
        } else {
            PurchaseFacts::new(self.db.clone()).load(c, ids, e).await?
        };
        let targets = ids.ids().iter().cloned().collect::<Vec<_>>();
        let participants = self.db.document_participants().document_ids_in(c.actor.id(), &targets, e).await?;
        for row in &mut rows {
            row.collaborators = FactValue::Known(BTreeSet::new());
            row.history_participants = FactValue::Known(if participants.contains(&row.id) {
                [c.actor.id().into()].into()
            } else {
                BTreeSet::new()
            });
            for source in row.relations.values_mut().flat_map(|relation| relation.objects.iter_mut()) {
                source.collaborators = FactValue::Known(BTreeSet::new());
                source.history_participants = FactValue::Known(if participants.contains(&source.id) {
                    [c.actor.id().into()].into()
                } else {
                    BTreeSet::new()
                });
            }
        }
        Ok(rows)
    }
}
#[async_trait]
impl PurchaseDataScopePort for MongoPurchaseDataScope {
    async fn qualify(
        &self,
        actor: &AuditActor,
        action: &str,
        required: &[String],
        executor: &mut dyn Executor,
    ) -> erp_procurement::Result<()> {
        let (service, _, permission) = self.authorization(action, required, None)?;
        let mut permissions = required.iter().map(Permission::parse).collect::<erp_core::Result<Vec<_>>>()?;
        permissions.push(permission);
        if !service
            .permissions(actor, &permissions, executor)
            .await?
            .is_some_and(|qualified| qualified.same_role(&permissions))
        {
            return Err(erp_procurement::Error::Forbidden("账号或有效角色不具备本操作的完整权限".into()));
        }
        Ok(())
    }

    async fn require(
        &self,
        actor: &AuditActor,
        action: &str,
        order: &PurchaseOrder,
        permissions: &[String],
        e: &mut dyn Executor,
    ) -> erp_procurement::Result<()> {
        let (service, _, permission) = self.authorization(action, permissions, Some(order.clone()))?;
        service
            .require(
                actor,
                &permission,
                AccessTarget::Records {
                    subject: AccessSubject::Direct,
                    ids: ResourceTargets::new(&[order.base.id.as_str()])?,
                },
                e,
            )
            .await?;
        Ok(())
    }
    async fn require_source_permissions(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        ids: &[String],
        required: &[Permission],
        e: &mut dyn Executor,
    ) -> erp_procurement::Result<()> {
        if !matches!(
            (entry.resource(), entry.action()),
            ("work_item", "detail" | "manage")
                | ("purchase_payment_request", "submit")
                | (
                    "approval_instance",
                    "read" | "decide" | "resume" | "cancel" | "cancel_blocked" | "upgrade_binding"
                )
        ) {
            return Err(erp_procurement::Error::ValidationError("未登记采购来源入口".into()));
        }
        source_permissions(entry, required)?;
        let relation = RelationKey::parse("self_purchase")?;
        let subject = AccessSubject::Source(relation.clone());
        let policy = AccessPolicy::Related {
            relation,
            resource: "purchase_order".into(),
            mode: erp_identity::entity::authorization::RelationMatch::AllNonEmpty,
            required_facts: [FactKind::Owner, FactKind::Organization].into(),
            policy: Box::new(AccessPolicy::Scope {
                permission: Permission::parse("purchase_order:detail")?,
                read: ReadAccess { history: true, records: true },
            }),
        };
        let registration = AccessRegistration {
            permission: entry.clone(),
            required_permissions: required.to_vec(),
            access: AccessMode::Object {
                data: DataAccess { subject: subject.clone(), required_facts: BTreeSet::new(), policy },
                facts: Arc::new(PurchaseObjectFacts { db: self.db.clone(), candidate: None }),
            },
        };
        let service = AuthorizationService::new(
            self.db.clone(),
            self.rbac.clone(),
            AuthorizationRegistry::new(vec![registration])?,
        );
        service
            .require(
                actor,
                entry,
                AccessTarget::Records {
                    subject,
                    ids: ResourceTargets::new(&ids.iter().map(String::as_str).collect::<Vec<_>>())?,
                },
                e,
            )
            .await?;
        Ok(())
    }
    async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        e: &mut dyn Executor,
    ) -> erp_procurement::Result<PurchaseResolvedScope> {
        self.resolve_permissions(actor, action, &[], e).await
    }
    async fn resolve_permissions(
        &self,
        actor: &AuditActor,
        action: &str,
        extra: &[String],
        e: &mut dyn Executor,
    ) -> erp_procurement::Result<PurchaseResolvedScope> {
        let (service, mapping, permission) = self.authorization(action, extra, None)?;
        let query = service.query(actor, &permission, AccessSubject::Direct, e).await?;
        let compiled = Arc::new(query.compile(mapping.as_ref(), e).await?);
        service.validate(&compiled, e).await?;
        let qualification = service
            .permissions(actor, std::slice::from_ref(&permission), e)
            .await?
            .ok_or_else(|| erp_procurement::Error::Forbidden("账号已失效".into()))?;
        Ok(PurchaseResolvedScope {
            scope_version: compiled.version(),
            compiled,
            authorization: service,
            user_id: actor.id().into(),
            resource: "purchase_order".into(),
            action: action.into(),
            policy_version: qualification.policy_revision(),
            as_of: Instant::now(),
        })
    }
    async fn expand_org_units(
        &self,
        ids: &[String],
        descendants: bool,
        e: &mut dyn Executor,
    ) -> erp_procurement::Result<BTreeSet<String>> {
        scope_support::expand_org_units(&self.db, ids, descendants, e, Into::into, Into::into).await
    }
    async fn org_member_ids(
        &self,
        ids: &BTreeSet<String>,
        at: Instant,
        e: &mut dyn Executor,
    ) -> erp_procurement::Result<Vec<String>> {
        scope_support::org_member_ids(&self.db, ids, at, e, Into::into).await
    }
    async fn own_org(
        &self,
        user: &str,
        at: Instant,
        e: &mut dyn Executor,
    ) -> erp_procurement::Result<Option<String>> {
        scope_support::own_org(&self.db, user, at, e, Into::into, Into::into).await
    }
}
/// 装配采购对象访问器。
/// # 参数
/// db、rbac 为共享应用能力。
/// # 返回
/// 使用统一授权的采购服务。
/// # 错误
/// 无。
pub fn purchase_access(db: Database, rbac: SharedRbacService) -> PurchaseAccess {
    PurchaseAccess::new(db.clone(), MongoPurchaseDataScope::shared(db, rbac))
}

/// 来源入口只接收审批政策中固定的完整功能资格组合。
fn source_permissions(entry: &Permission, required: &[Permission]) -> erp_procurement::Result<()> {
    if required.is_empty() {
        if entry.resource() == "approval_instance" && entry.action() != "read" {
            return Err(erp_procurement::Error::ValidationError(
                "审批管理及决定来源必须携带完整功能资格".into(),
            ));
        }
        return Ok(());
    }
    if entry.resource() != "approval_instance" {
        return Err(erp_procurement::Error::ValidationError("采购来源入口不支持附加资格".into()));
    }
    let provided = required
        .iter()
        .filter(|permission| *permission != entry)
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    if entry.action() == "decide" && provided == ["approval_instance:read".to_string()].into() {
        return Ok(());
    }
    for document_type in ALL_DOCUMENT_TYPES {
        let policy =
            policy_of(document_type).map_err(|error| erp_procurement::Error::Internal(error.to_string()))?;
        let DocumentApprovalPolicy::ProcessRequired(policy) = policy else {
            continue;
        };
        let admin = match entry.action() {
            "read" => {
                for permission in [&policy.runtime_admin_permission, &policy.definition_admin_permission] {
                    if provided == [permission.clone()].into() {
                        return Ok(());
                    }
                }
                continue;
            },
            "resume" | "cancel" | "cancel_blocked" => &policy.runtime_admin_permission,
            "upgrade_binding" => &policy.definition_admin_permission,
            _ => continue,
        };
        let expected = ["approval_instance:read".to_string(), admin.clone()].into();
        if provided == expected {
            return Ok(());
        }
    }
    Err(erp_procurement::Error::ValidationError("未登记的采购来源完整资格组合".into()))
}

#[cfg(test)]
mod tests {
    use std::slice;

    use super::*;

    struct UnusedFacts;

    #[async_trait]
    impl AccessFactsPort for UnusedFacts {
        async fn load(
            &self,
            _: &AccessContext,
            _: &ResourceTargets,
            _: &mut dyn Executor,
        ) -> erp_identity::Result<Vec<ObjectFacts>> {
            panic!("登记校验不读取业务对象")
        }
    }

    fn object_registration(action: &str, required: &[String]) -> AccessRegistration {
        let mapping = Arc::new(PurchaseHistoryMapping {
            inner: PurchaseMapping::new("purchase_order").unwrap(),
            model: purchase_model().unwrap(),
        });
        purchase_registration(action, required, mapping, Arc::new(UnusedFacts), true).unwrap()
    }

    #[test]
    fn object_preflight_registration_keeps_complete_permissions_and_current_action_scope() {
        let required = ["purchase_order:create".to_string()];
        for action in ["detail", "update", "submit", "cancel_approval", "delete"] {
            let registration = object_registration(action, &required);
            assert_eq!(
                registration.permission,
                Permission::parse(format!("purchase_order:{action}")).unwrap()
            );
            assert_eq!(registration.required_permissions, [Permission::parse(&required[0]).unwrap()]);
            let AccessMode::Object { data, .. } = &registration.access else {
                panic!("单对象预读不能依赖查询映射");
            };
            assert_eq!(data.required_facts, [FactKind::Owner, FactKind::Organization].into());
            let AccessPolicy::Scope { permission, read } = &data.policy else {
                panic!("采购对象必须走实际动作的完整范围");
            };
            assert_eq!(permission, &registration.permission);
            assert_eq!(*read, ReadAccess { history: action == "detail", records: action == "detail" });
            AuthorizationRegistry::new(vec![registration]).unwrap();
        }
    }

    #[test]
    fn object_preflight_registration_cannot_use_read_supplements_to_authorize_writes() {
        let mut registration = object_registration("submit", &[]);
        let AccessMode::Object { data, .. } = &mut registration.access else {
            panic!("本测试需要对象登记");
        };
        let AccessPolicy::Scope { read, .. } = &mut data.policy else {
            panic!("本测试需要采购动作范围");
        };
        read.records = true;
        assert!(matches!(
            AuthorizationRegistry::new(vec![registration]),
            Err(erp_identity::Error::ValidationError(_))
        ));
    }

    #[test]
    fn management_source_requires_the_registered_complete_permission_set() {
        let resume = Permission::parse("approval_instance:resume").unwrap();
        let read = Permission::parse("approval_instance:read").unwrap();
        let runtime = Permission::parse("purchase_order:approval_runtime_admin").unwrap();
        let definition = Permission::parse("purchase_order:approval_definition_admin").unwrap();
        assert!(source_permissions(&resume, &[]).is_err());
        assert!(source_permissions(&resume, &[read.clone(), runtime.clone()]).is_ok());
        assert!(source_permissions(&resume, &[resume.clone(), read.clone(), runtime.clone()]).is_ok());
        assert!(source_permissions(&resume, &[read.clone(), definition.clone()]).is_err());
        let upgrade = Permission::parse("approval_instance:upgrade_binding").unwrap();
        assert!(source_permissions(&upgrade, &[read.clone(), definition]).is_ok());
        assert!(source_permissions(&upgrade, &[read.clone(), runtime]).is_err());
        let decide = Permission::parse("approval_instance:decide").unwrap();
        assert!(source_permissions(&decide, &[]).is_err());
        assert!(source_permissions(&decide, slice::from_ref(&read)).is_ok());
        assert!(source_permissions(&read, &[]).is_ok());
        let payment = Permission::parse("purchase_payment_request:submit").unwrap();
        assert!(source_permissions(&payment, &[]).is_ok());
        assert!(source_permissions(&payment, &[read]).is_err());
    }
}
