//! 合同创建分别核对拟写入合同归属与真实客户当前来源；范围复用不证明来源资格。
use std::collections::BTreeSet;

use erp_permissions::customer;
use persistence_core::Executor;

use super::resolve::Resolution;
use super::{AuthorizationService, rules};
use crate::entity::authorization::{AccessContext, AccessRule, FactKind, RelationKey, RelationMatch};
use crate::{QualifiedPermissions, Result};

impl AuthorizationService {
    /// 客户详情资格独立取证，但开展新业务只使用当前范围，不接受历史参与补充。
    pub(super) async fn contract_creation_condition(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        resolution: &mut Resolution,
        executor: &mut dyn Executor,
    ) -> Result<AccessRule> {
        let target = self.resolve_scope(context, &context.permission, qualified, executor).await?;
        resolution.include(&target);
        let target_rule = rules::data_condition(&context.permission, Some(&target.scope))?;
        let source_context = AccessContext {
            permission: customer::DETAIL,
            resource: "customer".into(),
            model: None,
            ..context.clone()
        };
        let source = self.resolve_scope(&source_context, &customer::DETAIL, qualified, executor).await?;
        resolution.include(&source);
        let source_rule = AccessRule::scope(&source.scope, false)
            .requiring(&BTreeSet::from([FactKind::Owner, FactKind::Organization]));
        Ok(AccessRule::all(vec![
            target_rule,
            AccessRule::Independent {
                permission: customer::DETAIL,
                rule: Box::new(AccessRule::Related {
                    relation: RelationKey::parse("customer")?,
                    resource: "customer".into(),
                    mode: RelationMatch::ExactlyOne,
                    rule: Box::new(source_rule),
                }),
            },
        ]))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use application_core::AuditActor;
    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_permissions::contract;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::entity::access_control::{ResolvedScope, ScopeClause};
    use crate::entity::authorization::{FactValue, ObjectFacts, RelatedObjects, ResourceTargets};
    use crate::service::authorization::fixture::AuthorizationFixture;
    use crate::{AccessFactsPort, AccessTarget, Error};

    struct Customers {
        rows: Mutex<Vec<ObjectFacts>>,
        calls: AtomicUsize,
    }

    #[async_trait]
    impl AccessFactsPort for Customers {
        async fn load(
            &self,
            context: &AccessContext,
            ids: &ResourceTargets,
            executor: &mut dyn Executor,
        ) -> Result<Vec<ObjectFacts>> {
            assert_eq!(context.resource, "customer");
            assert_eq!(ids.ids(), &["customer".to_owned()].into());
            assert!(executor.boundary().unwrap().is_snapshot());
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.rows.lock().unwrap().clone())
        }
    }

    fn actor() -> AuditActor {
        AuditActor::new("user".into(), "login".into(), AccountKind::Admin)
    }

    fn customer() -> ObjectFacts {
        ObjectFacts {
            resource: "customer".into(),
            id: "customer".into(),
            version: "7".into(),
            owner_ids: FactValue::Known(["other".into(), "user".into()].into()),
            org_unit_ids: FactValue::Known(["source-org".into()].into()),
            history_participants: FactValue::Known(["user".into()].into()),
            ..Default::default()
        }
    }

    fn target() -> AccessTarget {
        AccessTarget::Candidate(ObjectFacts {
            resource: "contract".into(),
            owner_id: FactValue::Known(Some("user".into())),
            org_unit_id: FactValue::Known(Some("actor-org".into())),
            relations: [(
                RelationKey::parse("customer").unwrap(),
                RelatedObjects { expected_ids: ["customer".into()].into(), objects: vec![] },
            )]
            .into(),
            ..Default::default()
        })
    }

    fn customers() -> Arc<Customers> {
        Arc::new(Customers { rows: Mutex::new(vec![customer()]), calls: AtomicUsize::new(0) })
    }

    #[tokio::test]
    async fn creation_requires_independent_customer_qualification_and_real_current_multi_owner_source() {
        let customers = customers();
        let service = AuthorizationFixture::new(&[
            ("creator", &[contract::CREATE]),
            ("customer-reader", &[customer::DETAIL]),
        ])
        .facts("customer", customers.clone())
        .build();
        service
            .require(&actor(), &contract::CREATE, target(), &mut SnapshotExecutor::default())
            .await
            .unwrap();
        assert_eq!(customers.calls.load(Ordering::SeqCst), 1);
        let unqualified = AuthorizationFixture::new(&[("creator", &[contract::CREATE])])
            .facts("customer", customers.clone())
            .build();
        assert!(matches!(
            unqualified
                .require(&actor(), &contract::CREATE, target(), &mut SnapshotExecutor::default())
                .await,
            Err(Error::Forbidden(_))
        ));
        assert_eq!(customers.calls.load(Ordering::SeqCst), 1);
        for owners in [BTreeSet::new(), ["other".into()].into()] {
            customers.rows.lock().unwrap()[0].owner_ids = FactValue::Known(owners);
            assert!(matches!(
                service
                    .require(&actor(), &contract::CREATE, target(), &mut SnapshotExecutor::default())
                    .await,
                Err(Error::Forbidden(_))
            ));
        }
        customers.rows.lock().unwrap()[0].owner_ids = FactValue::Missing;
        assert!(matches!(
            service.require(&actor(), &contract::CREATE, target(), &mut SnapshotExecutor::default()).await,
            Err(Error::ValidationError(_))
        ));
        {
            let mut rows = customers.rows.lock().unwrap();
            rows[0].owner_ids = FactValue::Known(["user".into()].into());
            rows[0].org_unit_ids = FactValue::Known(BTreeSet::new());
        }
        service
            .require(&actor(), &contract::CREATE, target(), &mut SnapshotExecutor::default())
            .await
            .unwrap();
        customers.rows.lock().unwrap()[0].org_unit_ids = FactValue::Missing;
        assert!(matches!(
            service.require(&actor(), &contract::CREATE, target(), &mut SnapshotExecutor::default()).await,
            Err(Error::ValidationError(_))
        ));
    }

    #[tokio::test]
    async fn source_organization_cannot_replace_the_new_contract_owners_organization() {
        let customers = customers();
        let service = AuthorizationFixture::new(&[("role", &[contract::CREATE, customer::DETAIL])])
            .scope(ResolvedScope {
                role_clauses: vec![ScopeClause {
                    org_unit_ids: ["source-org".into()].into(),
                    ..Default::default()
                }],
                user_limit: None,
            })
            .facts("customer", customers)
            .build();
        assert!(matches!(
            service.require(&actor(), &contract::CREATE, target(), &mut SnapshotExecutor::default()).await,
            Err(Error::Forbidden(_))
        ));
        let AccessTarget::Candidate(mut matching) = target() else { unreachable!() };
        matching.org_unit_id = FactValue::Known(Some("source-org".into()));
        service
            .require(
                &actor(),
                &contract::CREATE,
                AccessTarget::Candidate(matching),
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn creation_rejects_incomplete_source_identity_expiry_and_non_snapshot_before_effects() {
        let customers = customers();
        let service = AuthorizationFixture::new(&[("role", &[contract::CREATE, customer::DETAIL])])
            .facts("customer", customers.clone())
            .build();
        assert!(matches!(
            service.require(&actor(), &contract::CREATE, target(), &mut NoTransaction).await,
            Err(Error::ValidationError(_))
        ));
        assert_eq!(customers.calls.load(Ordering::SeqCst), 0);
        for (rows, expected) in [
            (vec![], "forbidden"),
            (vec![customer(), customer()], "validation"),
            (vec![ObjectFacts { resource: "supplier".into(), ..customer() }], "validation"),
            (vec![ObjectFacts { id: "wrong".into(), ..customer() }], "validation"),
            (vec![ObjectFacts { valid_until: Some(Instant::from_unix_secs(1)), ..customer() }], "conflict"),
        ] {
            *customers.rows.lock().unwrap() = rows;
            let error = service
                .require(&actor(), &contract::CREATE, target(), &mut SnapshotExecutor::default())
                .await
                .unwrap_err();
            match expected {
                "forbidden" => assert!(matches!(error, Error::Forbidden(_))),
                "validation" => assert!(matches!(error, Error::ValidationError(_))),
                "conflict" => assert!(matches!(error, Error::ConflictError(_))),
                _ => unreachable!(),
            }
        }
        let missing = AuthorizationFixture::new(&[("role", &[contract::CREATE, customer::DETAIL])]).build();
        assert!(
            matches!(missing.require(&actor(), &contract::CREATE, target(), &mut SnapshotExecutor::default()).await,
            Err(Error::ValidationError(message)) if message.contains("缺少来源 Facts"))
        );
    }

    #[tokio::test]
    async fn creation_requires_one_nonempty_source_reference_and_never_trusts_supplied_source_facts() {
        let service = AuthorizationFixture::new(&[("role", &[contract::CREATE, customer::DETAIL])])
            .facts("customer", customers())
            .build();
        let AccessTarget::Candidate(original) = target() else { unreachable!() };
        let mut missing = original.clone();
        missing.relations.clear();
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &contract::CREATE,
                    AccessTarget::Candidate(missing),
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::ValidationError(_))
        ));
        let mut empty = original.clone();
        empty.relations.values_mut().next().unwrap().expected_ids.clear();
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &contract::CREATE,
                    AccessTarget::Candidate(empty),
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::Forbidden(_))
        ));
        let mut supplied = original;
        supplied.relations.values_mut().next().unwrap().objects.push(customer());
        assert!(
            matches!(service.require(&actor(), &contract::CREATE, AccessTarget::Candidate(supplied), &mut SnapshotExecutor::default()).await,
            Err(Error::ValidationError(message)) if message == "候选不能提供现存来源的授权事实")
        );
    }
}
