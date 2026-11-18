//! 内部材料事实适配；固定资格及来源规则由 identity 执行。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_catalog::portal::{CatalogPortalExt, NewProductDraft};
use erp_identity::entity::authorization::{
    AccessContext, FactValue, ObjectFacts, RelatedObjects, RelationKey,
};
use erp_identity::{
    AccessFactsPort, AccessTarget, AuthorizationService, Error as IdentityError, ResourceTargets,
    Result as IdentityResult,
};
use erp_permissions::{Permission, internal};
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
    fn permission(self, kind: RequestKind) -> Permission {
        match (self, kind) {
            (Self::Read, RequestKind::OfferingUpdate) => internal::PORTAL_MATERIAL_READ_OFFERING,
            (Self::Read, _) => internal::PORTAL_MATERIAL_READ,
            (Self::Review, RequestKind::OfferingCreate) => internal::PORTAL_MATERIAL_REVIEW_OFFERING_CREATE,
            (Self::Review, RequestKind::OfferingUpdate) => internal::PORTAL_MATERIAL_REVIEW_OFFERING_UPDATE,
            (Self::Review, RequestKind::NewProduct) => internal::PORTAL_MATERIAL_REVIEW_NEW_PRODUCT,
            (Self::Review, RequestKind::Cooperation) => internal::PORTAL_MATERIAL_REVIEW_COOPERATION,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RequestKind {
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

    pub(super) fn kind_for_supplier(&self, supplier_id: Option<&str>) -> Option<RequestKind> {
        supplier_id.is_none_or(|id| id == self.supplier.as_str()).then_some(self.kind)
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

/// 按持久化申请种类执行固定材料授权。
/// # 参数
/// authorization 为共享服务，target 为同一执行器读取的真实申请。
/// # 返回
/// 完整来源和功能资格均成立时成功。
/// # 错误
/// 任一来源、真实申请种类或必要资格不符时拒绝。
pub(super) async fn require_request(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    target: RequestTarget,
    action: RequestAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization
        .require(actor, &action.permission(target.kind), AccessTarget::record(&target.id)?, executor)
        .await?;
    Ok(())
}

/// 构造从拥有领域读取真实申请和完整来源的事实适配器。
/// # 参数
/// db 为各拥有领域所在数据库。
/// # 返回
/// 不保存请求种类或临时目标的资源事实读取器。
/// # 错误
/// 不执行 I/O。
pub fn material_facts_port(db: Database) -> Arc<dyn AccessFactsPort> {
    Arc::new(RequestFacts { db })
}
struct RequestFacts {
    db: Database,
}
#[async_trait]
impl AccessFactsPort for RequestFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        let mut objects = Vec::new();
        for id in ids.ids() {
            let Some(target) = RequestTarget::load(&self.db, id, executor).await.map_err(identity_error)?
            else {
                continue;
            };
            check_kind(&context.permission, target.kind)?;
            objects.push(self.object(context, target, executor).await?);
        }
        Ok(objects)
    }
}
impl RequestFacts {
    async fn object(
        &self,
        context: &AccessContext,
        target: RequestTarget,
        executor: &mut dyn Executor,
    ) -> IdentityResult<ObjectFacts> {
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(&target.supplier, executor)
            .await?
            .ok_or_else(|| IdentityError::NotFound("申请供应商不存在".into()))?;
        if (context.permission.action().starts_with("review_")
            || context.permission == internal::WORK_ITEM_EXECUTE)
            && !supplier.is_active()
        {
            return Err(IdentityError::Forbidden("供应商已停用".into()));
        }
        let supplier_facts = source_facts(
            "supplier",
            supplier.base.id,
            supplier.base.version,
            supplier.maintainer_user_id,
            supplier.business_org_unit_id,
        );
        let mut root = request_root(&target, supplier_facts)?;
        if let Some(id) = target.offering {
            let offering = self
                .db
                .supplier_offerings()
                .find_by_id(&id, executor)
                .await?
                .ok_or_else(|| IdentityError::NotFound("申请供给不存在".into()))?;
            root.relations.insert(
                RelationKey::parse("offering")?,
                RelatedObjects {
                    expected_ids: [id].into(),
                    objects: vec![source_facts(
                        "supplier_offering",
                        offering.base.id,
                        offering.base.version,
                        offering.maintainer_user_id,
                        offering.business_org_unit_id,
                    )],
                },
            );
        }
        Ok(root)
    }
}
/// 原申请身份、种类及供应商引用均来自同次持久化读取。
fn request_root(target: &RequestTarget, supplier: ObjectFacts) -> IdentityResult<ObjectFacts> {
    let mut root = ObjectFacts {
        resource: MATERIAL_RESOURCE.into(),
        id: target.id.clone(),
        version: target.version.clone(),
        subject_type: FactValue::Known(
            match target.kind {
                RequestKind::OfferingCreate => "offering_create",
                RequestKind::OfferingUpdate => "offering_update",
                RequestKind::NewProduct => "new_product",
                RequestKind::Cooperation => "cooperation",
            }
            .into(),
        ),
        ..Default::default()
    };
    root.relations.insert(
        RelationKey::parse("supplier")?,
        RelatedObjects { expected_ids: [target.supplier.clone()].into(), objects: vec![supplier] },
    );
    Ok(root)
}
fn source_facts(resource: &str, id: String, version: u64, owner: String, org: String) -> ObjectFacts {
    ObjectFacts {
        resource: resource.into(),
        id,
        version: version.to_string(),
        owner_id: FactValue::Known(Some(owner)),
        org_unit_id: FactValue::Known(Some(org)),
        ..Default::default()
    }
}
fn check_kind(permission: &Permission, kind: RequestKind) -> IdentityResult<()> {
    if permission == &internal::WORK_ITEM_EXECUTE {
        return Ok(());
    }
    let expected =
        if permission.action().starts_with("review_") { RequestAction::Review } else { RequestAction::Read };
    if permission != &expected.permission(kind) {
        return Err(IdentityError::ValidationError("材料权限与持久化申请种类不符".into()));
    }
    Ok(())
}
fn identity_error(error: crate::Error) -> IdentityError {
    match error {
        crate::Error::Internal(value) => IdentityError::Internal(value),
        crate::Error::NotFound(value) => IdentityError::NotFound(value),
        crate::Error::ValidationError(value) => IdentityError::ValidationError(value),
        crate::Error::InvalidInput(value) => IdentityError::InvalidInput(value),
        crate::Error::BusinessLogicError(value) => IdentityError::BusinessLogicError(value),
        crate::Error::ConflictError(value) => IdentityError::ConflictError(value),
        crate::Error::ReceiptDuplicate(value) => IdentityError::ReceiptDuplicate(value),
        crate::Error::TransientTransaction(value) => IdentityError::TransientTransaction(value),
        crate::Error::Forbidden(value) => IdentityError::Forbidden(value),
        crate::Error::Unauthenticated(value) => IdentityError::Unauthenticated(value),
        crate::Error::Logic(value) => IdentityError::Logic(value),
        crate::Error::Rbac(value) => IdentityError::Rbac(value),
        crate::Error::OutcomeUnknown(value) => IdentityError::OutcomeUnknown(value),
        crate::Error::RepositoryError(value) => IdentityError::RepositoryError(value),
        crate::Error::Coded(value) => IdentityError::BusinessLogicError(value.to_string()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dispatch_kind_respects_portal_supplier_scope_and_retains_offering_subtype() {
        let target = RequestTarget {
            id: "request".into(),
            version: "1".into(),
            supplier: "supplier".into(),
            offering: Some("offering".into()),
            kind: RequestKind::OfferingUpdate,
        };
        assert!(target.kind_for_supplier(Some("other")).is_none());
        assert!(target.kind_for_supplier(Some("supplier")) == Some(RequestKind::OfferingUpdate));
        assert!(target.kind_for_supplier(None) == Some(RequestKind::OfferingUpdate));
    }

    #[test]
    fn weaker_material_actions_cannot_access_another_persisted_request_kind() {
        for kind in [
            RequestKind::OfferingCreate,
            RequestKind::OfferingUpdate,
            RequestKind::NewProduct,
            RequestKind::Cooperation,
        ] {
            for action in [RequestAction::Read, RequestAction::Review] {
                assert!(check_kind(&action.permission(kind), kind).is_ok());
            }
        }
        assert!(check_kind(&internal::PORTAL_MATERIAL_READ, RequestKind::OfferingUpdate).is_err());
        assert!(
            check_kind(&internal::PORTAL_MATERIAL_REVIEW_OFFERING_CREATE, RequestKind::NewProduct).is_err()
        );
        assert!(
            check_kind(&internal::PORTAL_MATERIAL_REVIEW_COOPERATION, RequestKind::OfferingUpdate).is_err()
        );
    }
    #[test]
    fn material_source_errors_preserve_business_and_identity_categories() {
        assert!(
            matches!(identity_error(crate::Error::BusinessLogicError("blocked".into())), IdentityError::BusinessLogicError(value) if value == "blocked")
        );
        assert!(
            matches!(identity_error(crate::Error::Unauthenticated("expired".into())), IdentityError::Unauthenticated(value) if value == "expired")
        );
    }

    #[test]
    fn material_sources_keep_current_ownership_and_version() {
        let facts = source_facts("supplier", "supplier".into(), 7, "owner".into(), "org".into());
        assert_eq!(facts.version, "7");
        assert_eq!(facts.owner_id, FactValue::Known(Some("owner".into())));
        assert_eq!(facts.org_unit_id, FactValue::Known(Some("org".into())));
    }
}
