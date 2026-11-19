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
use crate::repository::authorization::binding::FrozenMapping;
use crate::repository::authorization::unavailable::UnavailableMapping;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Permission, QualifiedPermissions, Result};

impl AuthorizationService {
    /// 构造不连接数据库、无 Facts 和 Mapping 且身份设施不可用的测试服务，验证支持的查询先报告绑定缺失。
    pub(crate) fn unbound_for_test() -> Arc<Self> {
        Arc::new(Self {
            facilities: Arc::new(Unavailable),
            facts: BTreeMap::new(),
            mappings: BTreeMap::new(),
            token: Arc::new(()),
        })
    }

    /// 为所有固定查询提供仅含静态描述的测试映射。身份设施不读库且始终报错，查询不能进入映射转换。
    pub(crate) fn unavailable_for_test() -> Arc<Self> {
        Arc::new(Self {
            facilities: Arc::new(Unavailable),
            facts: BTreeMap::new(),
            mappings: rules::queries::permissions()
                .map(|permission| {
                    let mapping = Arc::new(UnavailableMapping {
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
                    let frozen = FrozenMapping::freeze(mapping, &mut Vec::new(), 0)
                        .expect("不可用查询夹具的固定模型有效");
                    (permission.clone(), frozen)
                })
                .collect(),
            token: Arc::new(()),
        })
    }
}
// 不连接数据库的身份设施替身；常规身份、范围和记录授权读取均失败。
struct Unavailable;

// 为测试身份设施故障返回统一内部错误，便于断言错误未被降级。
fn unavailable() -> Error {
    Error::Internal("测试授权设施不可用".into())
}

#[async_trait]
impl Facilities for Unavailable {
    // 在身份取证入口立即报错，验证查询不会继续编译。
    async fn permissions(
        &self,
        _: &AuditActor,
        _: &[Permission],
        _: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        Err(unavailable())
    }
    // 拒绝范围解析，避免测试替身意外放行数据范围。
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
    // 拒绝读取记录授权修订号，不伪造有效版本。
    async fn grant_revision(&self, _: &AuditActor, _: ResourceKind, _: &mut dyn Executor) -> Result<u64> {
        Err(unavailable())
    }
    // 拒绝读取目标授权，不以空补权掩盖设施故障。
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

    // 遍历所有查询入口，验证静态绑定可描述模型，但身份取证失败先于映射转换。
    #[tokio::test]
    async fn every_registered_query_fails_at_identity_before_any_mapping_conversion() {
        let authorization = AuthorizationService::unavailable_for_test();
        let actor = AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin);
        for permission in rules::queries::permissions() {
            let mapping = authorization.mappings.get(permission).unwrap();
            mapping.model().validate().unwrap();
            assert_eq!(mapping.model().resource, permission.resource());
            assert!(mapping.stages().is_empty());

            assert!(
                matches!(authorization.query(&actor, permission, &mut SnapshotExecutor::default()).await,
                Err(Error::Internal(reason)) if reason == "测试授权设施不可用"),
                "{permission}"
            );
        }
    }

    // 验证所有支持的查询在未装配时先报告缺少 Mapping，而不是身份设施故障。
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

    // 验证不可用映射无法产生查询键、记录条件、事实谓词或来源关联。
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
            matches!(mapping.key(&context), Err(Error::Internal(reason)) if reason == "测试查询映射不可执行")
        );
        assert!(
            matches!(mapping.records(&context), Err(Error::Internal(reason)) if reason == "测试查询映射不可执行")
        );
        assert!(
            matches!(mapping.predicate(&AccessFact::Owned, &context), Err(Error::Internal(reason)) if reason == "测试查询映射不可执行")
        );
        assert!(
            matches!(mapping.relation(&RelationKey::parse("source").unwrap(), &context), Err(Error::Internal(reason)) if reason == "测试查询映射不可执行")
        );
    }
}
