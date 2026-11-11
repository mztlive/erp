//! 预存款账户的事实适配；范围规则由 identity 解析和执行。
use std::sync::Arc;

use async_trait::async_trait;
use erp_finance::entity::supplier_deposit::DepositAccountIdentity;
use erp_finance::repository::SupplierDepositExt;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, QueryModelKey,
    QueryOutput, ReadAccess, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{
    AccessFactsPort, AccessMapping, AccessMode, AccessPolicy, AccessRegistration, AccessSubject,
    AuthorizationRegistry, AuthorizationService, DataAccess, Error as IdentityError, Permission,
    ResourceTargets, Result as IdentityResult, SharedRbacService,
};
use erp_party::{PartyExt, PartyStatus};
use erp_supplier::SupplierExt;
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;

const ACTIONS: &[&str] = &[
    "list",
    "detail",
    "create",
    "recharge_request",
    "approve_recharge",
    "pay_recharge",
    "receive_recharge",
    "authorize",
    "revoke",
    "reconcile",
    "resolve",
    "release",
    "reserve",
    "use",
    "refund",
    "bind",
];

pub(in crate::supplier_deposits) fn authorization(
    db: &Database,
    rbac: &SharedRbacService,
    candidate: Option<(String, DepositAccountIdentity)>,
) -> (AuthorizationService, Arc<dyn AccessMapping>) {
    let mapping: Arc<dyn AccessMapping> = Arc::new(DepositMapping::new());
    let facts: Arc<dyn AccessFactsPort> = Arc::new(DepositFacts { db: db.clone(), candidate });
    let entries = ACTIONS
        .iter()
        .map(|action| {
            let permission = Permission::parse(format!("supplier_deposit:{action}")).expect("固定预存款权限");
            AccessRegistration {
                permission: permission.clone(),
                required_permissions: vec![],
                access: AccessMode::QueryAndObject {
                    data: DataAccess {
                        subject: AccessSubject::Direct,
                        required_facts: [FactKind::Organization, FactKind::SettlementParty].into(),
                        policy: AccessPolicy::Scope { permission, read: ReadAccess::default() },
                    },
                    mapping: mapping.clone(),
                    facts: facts.clone(),
                },
            }
        })
        .collect();
    let registry = AuthorizationRegistry::new(entries).expect("固定预存款授权登记完整");
    (AuthorizationService::new(db.clone(), rbac.clone(), registry), mapping)
}

struct DepositFacts {
    db: Database,
    candidate: Option<(String, DepositAccountIdentity)>,
}

#[async_trait]
impl AccessFactsPort for DepositFacts {
    async fn load(
        &self,
        _: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        let mut objects = Vec::new();
        for id in ids.ids() {
            let (identity, version) = match &self.candidate {
                Some((key, identity)) if key == id => (identity.clone(), key.clone()),
                _ => {
                    let Some(account) = self
                        .db
                        .supplier_deposit_accounts()
                        .find_by_id(id, executor)
                        .await
                        .map_err(identity_error)?
                    else {
                        continue;
                    };
                    (account.identity, account.base.version.to_string())
                },
            };
            objects.push(identity_facts(&self.db, id, &identity, &version, executor).await?);
        }
        Ok(objects)
    }
}

async fn identity_facts(
    db: &Database,
    id: &str,
    identity: &DepositAccountIdentity,
    version: &str,
    executor: &mut dyn Executor,
) -> IdentityResult<ObjectFacts> {
    let payer = db
        .parties()
        .find_by_id(&identity.payer_party_id, executor)
        .await
        .map_err(identity_error)?
        .ok_or_else(|| IdentityError::NotFound("我方付款主体不存在".into()))?;
    if !payer.is_active() || payer.company_profile.is_none() {
        return Err(IdentityError::BusinessLogicError("预存款付款主体必须是有效的我方公司".into()));
    }
    let supplier = db
        .supplier_accounts()
        .find_by_id(&identity.supplier_id, executor)
        .await
        .map_err(identity_error)?
        .ok_or_else(|| IdentityError::NotFound("供应商不存在".into()))?;
    Ok(ObjectFacts {
        resource: "supplier_deposit".into(),
        id: id.into(),
        version: format!("{version}:{}:{}", payer.base.version, supplier.base.version),
        org_unit_id: FactValue::Known(Some(supplier.business_org_unit_id)),
        settlement_party_id: FactValue::Known(Some(identity.payer_party_id.clone())),
        ..Default::default()
    })
}

struct DepositMapping {
    model: QueryModel,
}

impl DepositMapping {
    fn new() -> Self {
        Self {
            model: QueryModel {
                key: QueryModelKey::parse("supplier_deposit_accounts").expect("固定模型标识"),
                resource: "supplier_deposit".into(),
                collection: "supplier_deposit_accounts".into(),
                id_field: "id".into(),
                output: QueryOutput::Object,
                facts: [FactKind::Organization, FactKind::SettlementParty].into(),
            },
        }
    }
}

#[async_trait]
impl AccessMapping for DepositMapping {
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
        Ok(QueryFragment::Exists {
            collection: Database::PARTIES.into(),
            bindings: doc! {
                "payer": "$identity.payer_party_id",
                "supplier": "$identity.supplier_id",
                "account_deleted": "$deleted_at",
            },
            pipeline: vec![
                doc! {"$match": {
                    "deleted_at": 0_i64,
                    "status": PartyStatus::Active.as_str(),
                    "company_profile": {"$type": "object"},
                    "$expr": {"$and": [
                        {"$eq": ["$id", "$$payer"]},
                        {"$eq": ["$$account_deleted", 0_i64]},
                    ]},
                }},
                doc! {"$lookup": {
                    "from": Database::SUPPLIER_ACCOUNTS,
                    "let": {"supplier_id": "$$supplier"},
                    "pipeline": [{"$match": {
                        "deleted_at": 0_i64,
                        "$expr": {"$eq": ["$id", "$$supplier_id"]},
                    }}],
                    "as": "__deposit_supplier",
                }},
                doc! {"$match": {"__deposit_supplier.0": {"$exists": true}}},
            ],
        })
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<QueryFragment> {
        match fact {
            AccessFact::SettlementPartyIn(ids) => Ok(QueryFragment::Match(
                doc! {"identity.payer_party_id": {"$in": ids.iter().cloned().collect::<Vec<_>>()}},
            )),
            AccessFact::OrganizationIn(ids) => Ok(QueryFragment::Exists {
                collection: Database::SUPPLIER_ACCOUNTS.into(),
                bindings: doc! {"supplier": "$identity.supplier_id"},
                pipeline: vec![
                    doc! {"$match": {"deleted_at": 0_i64, "business_org_unit_id": {"$in": ids.iter().cloned().collect::<Vec<_>>()}, "$expr": {"$eq": ["$id", "$$supplier"]}}},
                ],
            }),
            _ => Err(IdentityError::ValidationError("预存款模型不支持该事实维度".into())),
        }
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<RelationMapping> {
        Err(IdentityError::ValidationError("预存款账户没有来源授权关系".into()))
    }
}

/// 保留业务事实仓储失败的稳定错误分类。
fn identity_error(error: impl Into<crate::Error>) -> IdentityError {
    match error.into() {
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
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;

    use super::*;

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin),
            permission: Permission::parse("supplier_deposit:list").unwrap(),
            resource: "supplier_deposit".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        }
    }

    #[test]
    fn payer_and_supplier_organization_use_their_actual_foreign_keys() {
        let mapping = DepositMapping::new();
        let facts = MappingFacts::default();
        let payer = mapping
            .predicate(&AccessFact::SettlementPartyIn(["payer".into()].into()), &context(), &facts)
            .unwrap();
        assert!(
            matches!(payer, QueryFragment::Match(filter) if filter == doc! {"identity.payer_party_id": {"$in": ["payer"]}})
        );
        let organization = mapping
            .predicate(&AccessFact::OrganizationIn(["org".into()].into()), &context(), &facts)
            .unwrap();
        let QueryFragment::Exists { collection, bindings, pipeline } = organization else {
            panic!("组织事实必须从实际供应商读取")
        };
        assert_eq!(collection, Database::SUPPLIER_ACCOUNTS);
        assert_eq!(bindings, doc! {"supplier": "$identity.supplier_id"});
        assert_eq!(
            pipeline,
            vec![
                doc! {"$match": {"deleted_at": 0_i64, "business_org_unit_id": {"$in": ["org"]}, "$expr": {"$eq": ["$id", "$$supplier"]}}}
            ]
        );
    }

    #[test]
    fn unsupported_fact_and_relation_fail_closed() {
        let mapping = DepositMapping::new();
        assert!(matches!(
            mapping.predicate(&AccessFact::Owned, &context(), &MappingFacts::default()),
            Err(IdentityError::ValidationError(_))
        ));
        assert!(
            mapping
                .relation(&RelationKey::parse("sales").unwrap(), &context(), &MappingFacts::default())
                .is_err()
        );
        assert!(mapping.model().validate().is_ok());
    }

    #[test]
    fn business_fact_errors_preserve_their_category() {
        assert!(
            matches!(identity_error(crate::Error::ConflictError("changed".into())), IdentityError::ConflictError(value) if value == "changed")
        );
        assert!(
            matches!(identity_error(crate::Error::Forbidden("denied".into())), IdentityError::Forbidden(value) if value == "denied")
        );
    }

    #[test]
    fn record_eligibility_requires_the_same_actual_payer_and_supplier_as_object_facts() {
        let fragment = DepositMapping::new().records(&context(), &MappingFacts::default()).unwrap();
        let QueryFragment::Exists { collection, bindings, pipeline } = fragment else {
            panic!("资金账户必须校验真实来源资料");
        };
        assert_eq!(collection, Database::PARTIES);
        assert_eq!(bindings.get_str("payer").unwrap(), "$identity.payer_party_id");
        assert_eq!(bindings.get_str("supplier").unwrap(), "$identity.supplier_id");
        assert_eq!(bindings.get_str("account_deleted").unwrap(), "$deleted_at");
        let payer = pipeline[0].get_document("$match").unwrap();
        assert_eq!(payer.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(payer.get_str("status").unwrap(), PartyStatus::Active.as_str());
        assert_eq!(payer.get_document("company_profile").unwrap(), &doc! {"$type": "object"});
        let supplier = pipeline[1].get_document("$lookup").unwrap();
        assert_eq!(supplier.get_str("from").unwrap(), Database::SUPPLIER_ACCOUNTS);
        assert_eq!(supplier.get_document("let").unwrap(), &doc! {"supplier_id": "$$supplier"});
        assert_eq!(
            supplier.get_array("pipeline").unwrap(),
            &vec![Bson::Document(
                doc! {"$match": {"deleted_at": 0_i64, "$expr": {"$eq": ["$id", "$$supplier_id"]}}}
            )]
        );
        assert_eq!(pipeline[2], doc! {"$match": {"__deposit_supplier.0": {"$exists": true}}});
    }
}
