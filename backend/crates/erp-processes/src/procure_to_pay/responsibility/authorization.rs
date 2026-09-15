//! 采购责任授权快照与事务内重验；真实 RBAC 适配器和纯测试共用顺序函数。

use std::collections::HashSet;

use async_trait::async_trait;
use erp_core::AccountKind;
use erp_identity::{SharedRbacService, subject};
use erp_procurement::service::procurement_responsibility::CandidateResolution;
use persistence_core::Executor;

use super::resolver::{
    AuthorizedResolutionPlan, authorized_line, plan_identities, purchase_create_permission,
};
use crate::{Error, Result};

#[async_trait]
trait ResponsibilityPolicy: Send + Sync {
    async fn revision(&self) -> Result<u64>;
    async fn enforce_owner(&self, owner_id: &str) -> Result<bool>;
    async fn revision_with_executor(&self, executor: &mut dyn Executor) -> Result<u64>;
}

struct RbacPolicy<'a>(&'a SharedRbacService);

#[async_trait]
impl ResponsibilityPolicy for RbacPolicy<'_> {
    async fn revision(&self) -> Result<u64> {
        Ok(self.0.current_policy_revision().await?)
    }
    async fn enforce_owner(&self, owner_id: &str) -> Result<bool> {
        let permission = purchase_create_permission()?;
        Ok(self.0.enforce(&subject(AccountKind::Admin, owner_id), &permission).await?)
    }
    async fn revision_with_executor(&self, executor: &mut dyn Executor) -> Result<u64> {
        Ok(self.0.policy_revision_with_executor(executor).await?)
    }
}

pub(super) async fn authorize_candidates(
    rbac: &SharedRbacService,
    candidates: Vec<CandidateResolution>,
) -> Result<AuthorizedResolutionPlan> {
    authorize(&RbacPolicy(rbac), candidates).await
}

pub(super) async fn revalidate_candidates(
    rbac: &SharedRbacService,
    candidates: &[CandidateResolution],
    expected: &AuthorizedResolutionPlan,
    executor: &mut dyn Executor,
) -> Result<()> {
    revalidate(&RbacPolicy(rbac), candidates, expected, executor).await
}

async fn authorize(
    policy: &dyn ResponsibilityPolicy,
    candidates: Vec<CandidateResolution>,
) -> Result<AuthorizedResolutionPlan> {
    let policy_revision = policy.revision().await?;
    let mut owners = HashSet::new();
    for candidate in &candidates {
        if owners.insert(candidate.owner_user_id.as_str())
            && !policy.enforce_owner(candidate.owner_user_id.as_str()).await?
        {
            return Err(Error::ValidationError(format!(
                "采购负责人 {} 缺少 purchase_order:create 权限",
                candidate.owner_user_id
            )));
        }
    }
    let after = policy.revision().await?;
    if after != policy_revision {
        return Err(Error::ConflictError("采购负责人授权策略正在变化，请重试".to_string()));
    }
    let mut lines = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        lines.push(authorized_line(candidate)?);
    }
    Ok(AuthorizedResolutionPlan { lines, policy_revision })
}

async fn revalidate(
    policy: &dyn ResponsibilityPolicy,
    candidates: &[CandidateResolution],
    expected: &AuthorizedResolutionPlan,
    executor: &mut dyn Executor,
) -> Result<()> {
    let actual = plan_identities(candidates)?;
    let expected_identity = expected.identities();
    if actual != expected_identity {
        return Err(Error::ConflictError("采购责任规则或目录事实已变化，请重新提交审批".to_string()));
    }
    let revision = policy.revision_with_executor(executor).await?;
    if revision != expected.policy_revision {
        return Err(Error::ConflictError("采购负责人授权策略已变化，请重新提交审批".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use erp_procurement::entity::procurement_responsibility::ProcurementResponsibilityRuleType;
    use mongodb::ClientSession;

    use super::*;

    struct Fixture {
        revisions: Mutex<VecDeque<u64>>,
        events: Mutex<Vec<String>>,
        executor: Mutex<Option<usize>>,
        denied: Option<&'static str>,
    }
    impl Fixture {
        fn new(revisions: &[u64]) -> Self {
            Self {
                revisions: Mutex::new(revisions.iter().copied().collect()),
                events: Mutex::new(Vec::new()),
                executor: Mutex::new(None),
                denied: None,
            }
        }
    }
    #[async_trait]
    impl ResponsibilityPolicy for Fixture {
        async fn revision(&self) -> Result<u64> {
            self.events.lock().unwrap().push("revision".into());
            Ok(self.revisions.lock().unwrap().pop_front().expect("固定查询次数"))
        }
        async fn enforce_owner(&self, owner_id: &str) -> Result<bool> {
            self.events.lock().unwrap().push(format!("enforce:{owner_id}"));
            Ok(self.denied != Some(owner_id))
        }
        async fn revision_with_executor(&self, executor: &mut dyn Executor) -> Result<u64> {
            self.events.lock().unwrap().push("transaction.revision".into());
            *self.executor.lock().unwrap() = Some(executor as *mut dyn Executor as *mut () as usize);
            assert!(executor.session().is_none());
            Ok(self.revisions.lock().unwrap().pop_front().expect("固定查询次数"))
        }
    }
    #[derive(Default)]
    struct RecordingExecutor {
        visits: usize,
    }
    impl Executor for RecordingExecutor {
        fn session(&mut self) -> Option<&mut ClientSession> {
            self.visits += 1;
            None
        }
    }
    fn candidate(line: &str, owner: &str, name: &str) -> CandidateResolution {
        CandidateResolution {
            line_key: line.into(),
            owner_user_id: owner.into(),
            owner_name: name.into(),
            rule_id: "rule-1".into(),
            rule_type: ProcurementResponsibilityRuleType::Sku,
        }
    }
    fn plan() -> AuthorizedResolutionPlan {
        AuthorizedResolutionPlan {
            lines: vec![authorized_line(candidate("line-1", "owner-1", "旧姓名")).unwrap()],
            policy_revision: 7,
        }
    }

    #[tokio::test]
    async fn authorization_freezes_revision_and_checks_each_owner_once_in_first_order() {
        let fixture = Fixture::new(&[7, 7]);
        let plan = authorize(
            &fixture,
            vec![
                candidate("line-2", "owner-2", "李四"),
                candidate("line-1", "owner-1", "张三"),
                candidate("line-3", "owner-2", "李四"),
            ],
        )
        .await
        .unwrap();
        assert_eq!(plan.policy_revision, 7);
        assert_eq!(
            plan.lines.iter().map(|line| line.identity.line_key.as_str()).collect::<Vec<_>>(),
            ["line-2", "line-1", "line-3"]
        );
        assert_eq!(
            *fixture.events.lock().unwrap(),
            ["revision", "enforce:owner-2", "enforce:owner-1", "revision"]
        );
    }

    #[tokio::test]
    async fn missing_permission_fails_before_later_owners_and_second_revision() {
        let mut fixture = Fixture::new(&[7]);
        fixture.denied = Some("owner-1");
        let error = authorize(
            &fixture,
            vec![candidate("line-1", "owner-1", "管理员"), candidate("line-2", "owner-2", "李四")],
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, Error::ValidationError(message) if message == "采购负责人 owner-1 缺少 purchase_order:create 权限")
        );
        assert_eq!(*fixture.events.lock().unwrap(), ["revision", "enforce:owner-1"]);
    }

    #[tokio::test]
    async fn policy_change_prevents_creation_of_authorized_plan() {
        let fixture = Fixture::new(&[7, 8]);
        let error = authorize(&fixture, vec![candidate("line-1", "owner-1", "张三")]).await.unwrap_err();
        assert!(
            matches!(error, Error::ConflictError(message) if message == "采购负责人授权策略正在变化，请重试")
        );
    }

    #[tokio::test]
    async fn revalidation_ignores_name_but_reads_revision_in_original_executor() {
        let fixture = Fixture::new(&[7]);
        let mut executor = RecordingExecutor::default();
        let address = &mut executor as *mut RecordingExecutor as usize;
        revalidate(&fixture, &[candidate("line-1", "owner-1", "新姓名")], &plan(), &mut executor)
            .await
            .unwrap();
        assert_eq!(*fixture.events.lock().unwrap(), ["transaction.revision"]);
        assert_eq!(*fixture.executor.lock().unwrap(), Some(address));
        assert_eq!(executor.visits, 1);
    }

    #[tokio::test]
    async fn changed_identity_fails_before_reading_policy_revision() {
        let fixture = Fixture::new(&[]);
        let error = revalidate(
            &fixture,
            &[candidate("line-1", "owner-2", "旧姓名")],
            &plan(),
            &mut RecordingExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, Error::ConflictError(message) if message == "采购责任规则或目录事实已变化，请重新提交审批")
        );
        assert!(fixture.events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn changed_transaction_policy_revision_preserves_conflict_message() {
        let fixture = Fixture::new(&[8]);
        let error = revalidate(
            &fixture,
            &[candidate("line-1", "owner-1", "旧姓名")],
            &plan(),
            &mut RecordingExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, Error::ConflictError(message) if message == "采购负责人授权策略已变化，请重新提交审批")
        );
    }
}
