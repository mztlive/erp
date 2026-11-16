//! 仓储守卫的测试凭证：只签发恒拒绝管道，不证明生产授权或真实数据库行为。
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use mongodb::bson::doc;
use persistence_core::Executor;

use crate::entity::authorization::{AccessContext, AccessRule, QueryModel};
use crate::repository::authorization::AccessProof;
use crate::{CompiledAccess, Error, Permission, QualifiedPermissions, Result};

/// 为拥有领域的库单测构造绑定原执行器的恒拒绝查询凭证。
/// # 参数
/// model 为待核验的合法查询描述，executor 为持久化设施提供的执行器；
/// valid_until 可指定已到期时间，直接验证真实守卫的冲突分类，无需等待时钟。
/// # 返回
/// 只供模型、管道顺序和执行边界回归使用的凭证；管道始终拒绝所有行。
/// # 错误
/// 模型非法或执行器没有可核验边界时拒绝；不会读取数据库或放宽生产 Builder。
pub fn compiled_access(
    model: QueryModel,
    executor: &dyn Executor,
    valid_until: Option<Instant>,
) -> Result<CompiledAccess> {
    model.validate()?;
    let boundary =
        executor.boundary().ok_or_else(|| Error::ValidationError("测试查询需要可核验的执行边界".into()))?;
    let proof = AccessProof {
        context: AccessContext {
            actor: AuditActor::new("test".into(), "test".into(), AccountKind::Admin),
            permission: Permission::new("test", "read"),
            resource: model.resource.clone(),
            model: Some(model.key.clone()),
            as_of: Instant::now(),
        },
        qualified: QualifiedPermissions::test_grants(&[]),
        approval_source: None,
        task: false,
        rule: AccessRule::Deny,
        whole_rule: None,
        management_rule: None,
        has_role_scope: false,
        version: "test".into(),
        valid_until,
        policy_version: 1,
        organization_version: 1,
        boundary,
        service: Arc::new(()),
    };
    Ok(CompiledAccess {
        proof,
        model,
        stages: vec![doc! { "$match": { "$expr": false } }],
        dependencies: Vec::new(),
        version: "test".into(),
        sources: Default::default(),
        whole: None,
        management: None,
    })
}

#[cfg(test)]
mod tests {
    use persistence_core::{NoTransaction, ReadExecutor};

    use super::*;
    use crate::entity::authorization::{QueryModelKey, QueryOutput};

    fn model() -> QueryModel {
        QueryModel {
            key: QueryModelKey::parse("test").unwrap(),
            resource: "supplier_deposit".into(),
            collection: "supplier_deposit_accounts".into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: Default::default(),
        }
    }

    #[test]
    fn fixture_preserves_model_and_only_issues_a_deny_pipeline() {
        let mut executor = ReadExecutor::default();
        let access = compiled_access(model(), &executor, None).unwrap();
        assert_eq!(access.model(), &model());
        assert_eq!(access.stages(), &[doc! { "$match": { "$expr": false } }]);
        access.ensure_current(&mut executor).unwrap();
        executor.next_read();
        assert!(matches!(access.ensure_current(&mut executor), Err(Error::ValidationError(_))));
    }

    #[test]
    fn fixture_requires_a_valid_model_and_managed_boundary() {
        assert!(matches!(compiled_access(model(), &NoTransaction, None), Err(Error::ValidationError(_))));
        let mut invalid = model();
        invalid.collection = "$invalid".into();
        assert!(matches!(
            compiled_access(invalid, &ReadExecutor::default(), None),
            Err(Error::ValidationError(_))
        ));
    }

    #[test]
    fn expired_fixture_uses_the_real_conflict_classification() {
        let mut executor = ReadExecutor::default();
        let access = compiled_access(model(), &executor, Some(Instant::from_unix_secs(1))).unwrap();
        assert!(matches!(
            access.ensure_current(&mut executor),
            Err(Error::ConflictError(message)) if message == "DATA_SCOPE_CHANGED：授权关系已到期"
        ));
    }
}
