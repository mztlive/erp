//! 内部材料资格政策；调用方先证明任务或申请入口，再检查真实申请的完整来源。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_catalog::portal::{CatalogPortalExt, NewProductDraft};
use erp_identity::entity::authorization::{
    AccessContext, ObjectFacts, ReadAccess, RelatedObjects, RelationKey, RelationMatch,
};
use erp_identity::{
    AccessFactsPort, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget,
    AuthorizationRegistry, AuthorizationService, DataAccess, Error as IdentityError, Permission,
    ResourceTargets, Result as IdentityResult, SharedRbacService,
};
use erp_supplier::SupplierExt;
use erp_supplier::portal::{CooperationApplication, CooperationRepository};
use erp_supply::SupplierOfferingExt;
use erp_supply::portal::{OfferingApplication, PortalSupplyExt};
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

const MATERIAL_RESOURCE: &str = "supplier_portal_material";

#[derive(Clone, Copy)]
pub(super) enum RequestAction {
    Read,
    Review,
}

impl RequestAction {
    fn permission(self) -> IdentityResult<Permission> {
        Ok(Permission::parse(match self {
            Self::Read => "supplier_portal_material:read",
            Self::Review => "supplier_portal_material:review",
        })?)
    }
}

#[derive(Clone, Copy)]
enum RequestKind {
    OfferingCreate,
    OfferingUpdate,
    NewProduct,
    Cooperation,
}

pub(super) struct RequestTarget {
    id: String,
    version: String,
    supplier: String,
    offering: Option<String>,
    kind: RequestKind,
}

impl RequestTarget {
    /// 从原执行器读取的供给申请提取完整来源身份。
    /// # 参数
    /// app 为拥有领域返回的真实申请。
    /// # 返回
    /// 固定申请类型及当前供应商、供给来源。
    /// # 错误
    /// 不执行 I/O。
    pub(super) fn offering(app: &OfferingApplication) -> Self {
        let offering = app.snapshot.target().map(|(id, _, _)| id.to_string());
        Self {
            id: app.base.id.clone(),
            version: app.base.version.to_string(),
            supplier: app.supplier_id.clone(),
            kind: if offering.is_some() { RequestKind::OfferingUpdate } else { RequestKind::OfferingCreate },
            offering,
        }
    }

    /// 从真实新品草稿提取其供应商来源。
    /// # 参数
    /// draft 为拥有领域返回的草稿。
    /// # 返回
    /// 新品申请固定身份。
    /// # 错误
    /// 不执行 I/O。
    pub(super) fn new_product(draft: &NewProductDraft) -> Self {
        Self {
            id: draft.base.id.clone(),
            version: draft.base.version.to_string(),
            supplier: draft.supplier_id.clone(),
            offering: None,
            kind: RequestKind::NewProduct,
        }
    }

    fn cooperation(app: &CooperationApplication) -> Self {
        Self {
            id: app.base.id.clone(),
            version: app.base.version.to_string(),
            supplier: app.supplier_id.clone(),
            offering: None,
            kind: RequestKind::Cooperation,
        }
    }

    /// 按拥有领域加载精确申请，不由客户端选择来源类型。
    /// # 参数
    /// db、id、executor 为原数据库、申请身份及读取执行器。
    /// # 返回
    /// 真实申请及其完整来源；不存在时返回 None。
    /// # 错误
    /// 拥有领域读取失败保留原分类。
    pub(super) async fn load(db: &Database, id: &str, executor: &mut dyn Executor) -> Result<Option<Self>> {
        if let Some(app) = db.portal_applications().find_by_id(id, executor).await? {
            Ok(Some(Self::offering(&app)))
        } else if let Some(draft) = db.new_product_drafts().find_by_id(id, executor).await? {
            Ok(Some(Self::new_product(&draft)))
        } else {
            Ok(CooperationRepository::new(db).find_any(id, executor).await?.as_ref().map(Self::cooperation))
        }
    }
}

/// 在原执行器执行申请固定政策，来源功能资格允许分别来自不同有效角色。
/// # 参数
/// db、rbac 为共享身份设施，target 为可信申请事实，action 为固定读取或审核入口。
/// # 返回
/// 真实来源的功能资格、范围及必要启用状态均成立时成功。
/// # 错误
/// 缺失来源、无权、事实变化或身份设施失败时返回原错误。
pub(super) async fn require_request(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    target: RequestTarget,
    action: RequestAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    let permission = action.permission()?;
    let targets = ResourceTargets::new(&[target.id.as_str()])?;
    let policy = request_policy(target.kind, action)?;
    let registry = AuthorizationRegistry::new(vec![AccessRegistration {
        permission: permission.clone(),
        required_permissions: vec![],
        access: AccessMode::Object {
            data: DataAccess { subject: AccessSubject::Direct, required_facts: Default::default(), policy },
            facts: Arc::new(RequestFacts { db: db.clone(), target, action }),
        },
    }])?;
    AuthorizationService::new(db.clone(), rbac.clone(), registry)
        .require(
            actor,
            &permission,
            AccessTarget::Records { subject: AccessSubject::Direct, ids: targets },
            executor,
        )
        .await?;
    Ok(())
}

fn request_policy(kind: RequestKind, action: RequestAction) -> IdentityResult<AccessPolicy> {
    let supplier = match (kind, action) {
        (RequestKind::Cooperation, RequestAction::Review) => "supplier:update",
        _ => "supplier:detail",
    };
    let mut rules = vec![source_policy("supplier", supplier)?];
    match (kind, action) {
        (RequestKind::OfferingUpdate, RequestAction::Read) => {
            rules.push(source_policy("offering", "supplier_offering:list")?);
        },
        (RequestKind::OfferingUpdate, RequestAction::Review) => {
            rules.push(source_policy("offering", "supplier_offering:update")?);
        },
        (RequestKind::OfferingCreate | RequestKind::NewProduct, RequestAction::Review) => {
            if matches!(kind, RequestKind::NewProduct) {
                rules.push(AccessPolicy::Permission(Permission::parse("product:create")?));
            }
            rules.push(AccessPolicy::Permission(Permission::parse("supplier_offering:create")?));
        },
        _ => {},
    }
    // 任务和材料入口已有自己的功能资格；此处只证明精确申请的来源资格。
    Ok(AccessPolicy::Account { policy: Box::new(AccessPolicy::All(rules)) })
}

fn source_policy(relation: &str, code: &str) -> IdentityResult<AccessPolicy> {
    let permission = Permission::parse(code)?;
    Ok(AccessPolicy::Related {
        relation: RelationKey::parse(relation)?,
        resource: permission.resource().into(),
        mode: RelationMatch::AllNonEmpty,
        required_facts: Default::default(),
        policy: Box::new(AccessPolicy::Independent {
            permission: permission.clone(),
            policy: Box::new(AccessPolicy::Scope { permission, read: ReadAccess::default() }),
        }),
    })
}

struct RequestFacts {
    db: Database,
    target: RequestTarget,
    action: RequestAction,
}

#[async_trait]
impl AccessFactsPort for RequestFacts {
    async fn load(
        &self,
        _: &AccessContext,
        _: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        let mut root = ObjectFacts {
            resource: MATERIAL_RESOURCE.into(),
            id: self.target.id.clone(),
            version: self.target.version.clone(),
            ..Default::default()
        };
        let supplier = self.db.supplier_accounts().find_by_id(&self.target.supplier, executor).await?;
        if matches!(self.action, RequestAction::Review)
            && supplier.as_ref().is_some_and(|row| !row.is_active())
        {
            return Err(IdentityError::Forbidden("供应商已停用".into()));
        }
        root.relations.insert(
            RelationKey::parse("supplier")?,
            RelatedObjects {
                expected_ids: [self.target.supplier.clone()].into(),
                objects: vec![source_facts("supplier", supplier.map(|row| (row.base.id, row.base.version)))?],
            },
        );
        if let Some(id) = &self.target.offering {
            let offering = self.db.supplier_offerings().find_by_id(id, executor).await?;
            root.relations.insert(
                RelationKey::parse("offering")?,
                RelatedObjects {
                    expected_ids: [id.clone()].into(),
                    objects: vec![source_facts(
                        "supplier_offering",
                        offering.map(|row| (row.base.id, row.base.version)),
                    )?],
                },
            );
        }
        Ok(vec![root])
    }
}

fn source_facts(resource: &str, source: Option<(String, u64)>) -> IdentityResult<ObjectFacts> {
    let (id, version) = source.ok_or_else(|| IdentityError::NotFound("对象不存在或必要目标不完整".into()))?;
    Ok(ObjectFacts { resource: resource.into(), id, version: version.to_string(), ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct UnusedFacts;
    #[async_trait]
    impl AccessFactsPort for UnusedFacts {
        async fn load(
            &self,
            _: &AccessContext,
            _: &ResourceTargets,
            _: &mut dyn Executor,
        ) -> IdentityResult<Vec<ObjectFacts>> {
            panic!("目录构造不读取事实")
        }
    }

    #[test]
    fn every_request_shape_registers_complete_object_policy_without_extra_portal_feature() {
        for kind in [
            RequestKind::OfferingCreate,
            RequestKind::OfferingUpdate,
            RequestKind::NewProduct,
            RequestKind::Cooperation,
        ] {
            for action in [RequestAction::Read, RequestAction::Review] {
                let registration = AccessRegistration {
                    permission: action.permission().unwrap(),
                    required_permissions: vec![],
                    access: AccessMode::Object {
                        data: DataAccess {
                            subject: AccessSubject::Direct,
                            required_facts: Default::default(),
                            policy: request_policy(kind, action).unwrap(),
                        },
                        facts: Arc::new(UnusedFacts),
                    },
                };
                assert!(AuthorizationRegistry::new(vec![registration]).is_ok());
            }
        }
    }

    #[test]
    fn missing_sources_keep_not_found_in_direct_review_paths() {
        for resource in ["supplier", "supplier_offering"] {
            assert!(matches!(source_facts(resource, None), Err(IdentityError::NotFound(_))));
            let fact = source_facts(resource, Some(("source".into(), 3))).unwrap();
            assert_eq!(fact.resource, resource);
            assert_eq!(fact.id, "source");
            assert_eq!(fact.version, "3");
        }
    }
}
