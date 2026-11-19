//! 测试中的不可用授权设施，用于证明输入拒绝先于任何身份或业务读取。
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use persistence_core::Executor;

use super::facilities::Facilities;
use super::{AuthorizationService, rules};
use crate::entity::authorization::{AccessContext, QueryModel, QueryModelKey, QueryOutput, ResourceTargets};
use crate::entity::resource_access::ResourceKind;
use crate::ports::AccessMapping;
use crate::repository::authorization::unavailable::UnavailableMapping;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Permission, QualifiedPermissions, Result};

impl AuthorizationService {
    // 构造用于验证缺少查询绑定的失败关闭测试服务。
    // 没有Facts或Mapping、身份设施始终报错的测试实例。
    //
    // 不连接数据库；受支持查询在身份取证前返回缺少Mapping错误。
    pub(crate) fn unbound_for_test() -> Arc<Self> {
        Arc::new(Self {
            facilities: Arc::new(Unavailable),
            facts: BTreeMap::new(),
            mappings: BTreeMap::new(),
            token: Arc::new(()),
        })
    }

    // 为输入校验和设施故障测试提供失败关闭的统一授权入口。
    // 所有登记查询具有静态描述、身份设施始终报错的测试服务。
    //
    // 不读取数据库；查询无法通过身份取证，也不能执行映射转换。
    pub(crate) fn unavailable_for_test() -> Arc<Self> {
        Arc::new(Self {
            facilities: Arc::new(Unavailable),
            facts: BTreeMap::new(),
            mappings: rules::queries::permissions()
                .map(|permission| {
                    let mapping: Arc<dyn AccessMapping> = Arc::new(UnavailableMapping {
                        model: QueryModel {
                            key: QueryModelKey::parse(format!("unavailable_{}", permission.resource()))
                                .expect("固定查询权限资源组成合法测试模型标识"),
                            resource: permission.resource().into(),
                            collection: "unavailable_query_fixture".into(),
                            id_field: "id".into(),
                            output: QueryOutput::Object,
                            facts: BTreeSet::new(),
                        },
                    });
                    (permission.clone(), mapping)
                })
                .collect(),
            token: Arc::new(()),
        })
    }
}
struct Unavailable;

fn unavailable() -> Error {
    Error::Internal("测试授权设施不可用".into())
}

#[async_trait]
impl Facilities for Unavailable {
    async fn permissions(
        &self,
        _: &AuditActor,
        _: &[Permission],
        _: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        Err(unavailable())
    }
    async fn scope(
        &self,
        _: &AccessContext,
        _: &Permission,
        _: &[Permission],
        _: &QualifiedPermissions,
        _: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        Err(unavailable())
    }
    async fn grant_revision(&self, _: &AuditActor, _: ResourceKind, _: &mut dyn Executor) -> Result<u64> {
        Err(unavailable())
    }
    async fn grants(
        &self,
        _: &AccessContext,
        _: ResourceKind,
        _: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        Err(unavailable())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::entity::authorization::{AccessFact, RelationKey};

    #[tokio::test]
    async fn every_registered_query_fails_at_identity_before_any_mapping_conversion() {
        let authorization = AuthorizationService::unavailable_for_test();
        let actor = AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin);
        for permission in rules::queries::permissions() {
            let mapping = authorization.mappings.get(permission).unwrap();
            mapping.model().validate().unwrap();
            assert_eq!(mapping.model().resource, permission.resource());
            assert!(mapping.preparation().is_none());
            assert!(mapping.stages().is_empty());

            assert!(
                matches!(authorization.query(&actor, permission, &mut SnapshotExecutor::default()).await,
                Err(Error::Internal(reason)) if reason == "测试授权设施不可用"),
                "{permission}"
            );
        }
    }

    #[tokio::test]
    async fn unbound_queries_preserve_missing_binding_errors_before_identity_failure() {
        let authorization = AuthorizationService::unbound_for_test();
        let actor = AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin);
        for permission in rules::queries::permissions() {
            assert!(
                matches!(authorization.query(&actor, permission, &mut SnapshotExecutor::default()).await,
                Err(Error::ValidationError(reason)) if reason == "缺少查询 Mapping 绑定"),
                "{permission}"
            );
        }
    }

    #[test]
    fn unavailable_query_mapping_cannot_generate_an_authorized_key_or_filter() {
        let authorization = AuthorizationService::unavailable_for_test();
        let permission = rules::queries::permissions().next().unwrap();
        let mapping = authorization.mappings.get(permission).unwrap();
        let context = AccessContext {
            actor: AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin),
            permission: permission.clone(),
            resource: permission.resource().into(),
            model: Some(mapping.model().key.clone()),
            as_of: Instant::from_unix_secs(1),
        };

        assert!(
            matches!(mapping.key(&context, None), Err(Error::Internal(reason)) if reason == "测试查询映射不可执行")
        );
        assert!(
            matches!(mapping.records(&context, None), Err(Error::Internal(reason)) if reason == "测试查询映射不可执行")
        );
        assert!(
            matches!(mapping.predicate(&AccessFact::Owned, &context, None), Err(Error::Internal(reason)) if reason == "测试查询映射不可执行")
        );
        assert!(
            matches!(mapping.relation(&RelationKey::parse("source").unwrap(), &context, None), Err(Error::Internal(reason)) if reason == "测试查询映射不可执行")
        );
    }
}
