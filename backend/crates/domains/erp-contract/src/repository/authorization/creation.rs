//! 从拥有领域提供的真实客户事实构造合同创建候选。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{AccessContext, FactValue, ObjectFacts};
use erp_identity::{AccessFactsPort, ResourceTargets};
use erp_permissions::contract;
use persistence_core::Executor;

use crate::ports::ContractCreationPort;
use crate::{Error, Result};

/// 为合同创建连接同一组合根提供的客户事实读取器。
/// # 参数
/// customers 为拥有领域的真实客户事实实例；本模块只读取本次指定来源。
/// # 返回
/// 在原事务中校验客户身份并转换为合同候选的窄端口。
/// # 错误
/// 构造不执行 I/O；读取时的缺失、错配和设施错误保持失败。
pub fn creation_facts(customers: Arc<dyn AccessFactsPort>) -> Arc<dyn ContractCreationPort> {
    Arc::new(CreationFacts(customers))
}

/// 从真实客户当前归属构造合同候选，统一服务随后执行合同创建规则。
struct CreationFacts(Arc<dyn AccessFactsPort>);
#[async_trait]
impl ContractCreationPort for CreationFacts {
    async fn candidate(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ObjectFacts> {
        let context = AccessContext {
            actor: actor.clone(),
            permission: contract::CREATE,
            resource: "customer".into(),
            model: None,
            as_of: Instant::now(),
        };
        let ids = ResourceTargets::new(&[customer_id]).map_err(Error::from)?;
        let mut rows = self.0.load(&context, &ids, executor).await.map_err(Error::from)?;
        if rows.len() != 1 || rows[0].resource != "customer" || rows[0].id != customer_id {
            return Err(Error::NotFound("客户不存在或无权访问".into()));
        }

        let mut candidate = rows.remove(0);
        candidate.resource = "contract".into();
        candidate.id.clear();
        candidate.version.clear();
        candidate.history_participants = FactValue::Known(BTreeSet::new());
        candidate.collaborators = FactValue::Known(BTreeSet::new());
        Ok(candidate)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::{AccountKind, Error as CoreError};
    use erp_identity::{Error as IdentityError, Result as IdentityResult};
    use persistence_core::NoTransaction;

    use super::*;

    struct Customers {
        rows: Vec<ObjectFacts>,
        calls: Mutex<Vec<Vec<String>>>,
        unavailable: bool,
    }

    #[async_trait]
    impl AccessFactsPort for Customers {
        async fn load(
            &self,
            context: &AccessContext,
            ids: &ResourceTargets,
            _: &mut dyn Executor,
        ) -> IdentityResult<Vec<ObjectFacts>> {
            assert_eq!(context.permission, contract::CREATE);
            assert_eq!(context.resource, "customer");
            assert_eq!(context.actor.id(), "actor");
            self.calls.lock().unwrap().push(ids.ids().iter().cloned().collect());
            if self.unavailable {
                return Err(IdentityError::ConflictError("客户归属已变化".into()));
            }
            Ok(self.rows.clone())
        }
    }

    fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin)
    }

    fn customer() -> ObjectFacts {
        ObjectFacts {
            resource: "customer".into(),
            id: "customer".into(),
            version: "7".into(),
            owner_id: FactValue::Known(Some("owner".into())),
            org_unit_id: FactValue::Known(Some("org".into())),
            collaborators: FactValue::Known(["collaborator".into()].into()),
            history_participants: FactValue::Known(["historical".into()].into()),
            valid_until: Some(Instant::from_unix_secs(100)),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn creation_uses_only_the_requested_real_customer_and_preserves_ownership_expiry() {
        let customers =
            Arc::new(Customers { rows: vec![customer()], calls: Mutex::new(Vec::new()), unavailable: false });
        let port = creation_facts(customers.clone());
        assert!(customers.calls.lock().unwrap().is_empty());
        let candidate = port.candidate(&actor(), "customer", &mut NoTransaction).await.unwrap();
        assert_eq!(*customers.calls.lock().unwrap(), vec![vec!["customer".to_owned()]]);
        assert_eq!(candidate.resource, "contract");
        assert!(candidate.id.is_empty());
        assert!(candidate.version.is_empty());
        assert_eq!(candidate.owner_id, customer().owner_id);
        assert_eq!(candidate.org_unit_id, customer().org_unit_id);
        assert_eq!(candidate.valid_until, customer().valid_until);
        assert_eq!(candidate.collaborators, FactValue::Known(BTreeSet::new()));
        assert_eq!(candidate.history_participants, FactValue::Known(BTreeSet::new()));
    }

    #[tokio::test]
    async fn creation_rejects_missing_duplicate_foreign_or_wrong_customer_identity() {
        let mut foreign = customer();
        foreign.resource = "supplier".into();
        let mut wrong = customer();
        wrong.id = "other".into();
        for rows in [vec![], vec![customer(), customer()], vec![foreign], vec![wrong]] {
            let port = creation_facts(Arc::new(Customers {
                rows,
                calls: Mutex::new(Vec::new()),
                unavailable: false,
            }));
            assert!(matches!(
                port.candidate(&actor(), "customer", &mut NoTransaction).await,
                Err(Error::NotFound(_))
            ));
        }
    }

    #[tokio::test]
    async fn creation_preserves_source_failure_instead_of_returning_an_empty_candidate() {
        let customers =
            Arc::new(Customers { rows: vec![], calls: Mutex::new(Vec::new()), unavailable: true });
        let port = creation_facts(customers.clone());
        assert!(matches!(
            port.candidate(&actor(), "customer", &mut NoTransaction).await,
            Err(Error::ConflictError(message)) if message == "客户归属已变化"
        ));
        assert!(matches!(
            port.candidate(&actor(), "", &mut NoTransaction).await,
            Err(Error::Logic(CoreError::LogicError(message))) if message == "目标不能为空"
        ));
        assert_eq!(customers.calls.lock().unwrap().len(), 1);
    }
}
