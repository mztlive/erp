//! 每条规则的负责人池独立推进；一个任务批次对每个命中池只占一个轮次。
use std::collections::BTreeMap;

use persistence_core::Executor;

use super::{CandidateResolution, ProcurementResponsibilityService};
use crate::repository::ProcurementResponsibilityExt;
use crate::repository::prelude::*;
use crate::repository::procurement_responsibility::rotation::{advance, rotation};
use crate::{Error, Result};

impl ProcurementResponsibilityService {
    /// 为本单所有命中的规则池各占用一次轮次。
    /// # 参数
    /// `candidates` 为完整候选；`executor` 必须为创建任务的原事务。
    /// # 返回
    /// 同规则多商品只推进一次，不同规则独立推进。
    /// # 错误
    /// 同池出现多个负责人、规则停用、成员或轮次变化时回滚。
    pub async fn claim_pools(
        &self,
        candidates: &[CandidateResolution],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for (rule_id, owner) in pool_choices(candidates)? {
            let current = rotation(&self.db, rule_id, executor).await?;
            let rule = self
                .db
                .procurement_responsibility_rules()
                .find_procurement_responsibility_rule(rule_id, executor)
                .await?
                .filter(|rule| rule.is_active())
                .ok_or_else(|| Error::ConflictError("采购责任规则已变化，请重新提交".into()))?;
            if rule
                .strategy
                .select(&rule.owner_user_ids, current.last_owner_user_id.as_deref())
                .map_err(Error::Logic)?
                != owner
            {
                return Err(Error::ConflictError("负责人池分配顺序已变化，请重新提交".into()));
            }
            advance(&self.db, rule_id, &current, Some(owner), executor).await?;
        }
        Ok(())
    }

    /// 配置维护与当前池的分派串行化，保留最近负责人。
    /// # 参数
    /// `rule_id` 为池所属规则，`executor` 为维护事务。
    /// # 返回
    /// 仅更新并发版本，不占分派轮次。
    /// # 错误
    /// 缺少事务或写冲突时拒绝。
    pub(super) async fn guard_rotation(&self, rule_id: &str, executor: &mut dyn Executor) -> Result<()> {
        let current = rotation(&self.db, rule_id, executor).await?;
        advance(&self.db, rule_id, &current, current.last_owner_user_id.as_deref(), executor).await
    }
}

/// 按规则聚合整单选择，拒绝同一池被拆给不同人员。
fn pool_choices(candidates: &[CandidateResolution]) -> Result<BTreeMap<&str, &str>> {
    let mut choices = BTreeMap::new();
    for candidate in candidates {
        if let Some(previous) = choices.insert(candidate.rule_id.as_str(), candidate.owner_user_id.as_str())
            && previous != candidate.owner_user_id
        {
            return Err(Error::ValidationError("同一单命中同一负责人池的商品必须由同一采购负责".into()));
        }
    }
    Ok(choices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::procurement_responsibility::ProcurementResponsibilityRuleType;
    fn candidate(rule: &str, owner: &str) -> CandidateResolution {
        CandidateResolution {
            line_key: "line".into(),
            owner_user_id: owner.into(),
            owner_name: "采购".into(),
            rule_id: rule.into(),
            rule_type: ProcurementResponsibilityRuleType::Sku,
        }
    }
    #[test]
    fn one_claim_per_matched_pool_and_independent_pools() {
        assert!(pool_choices(&[]).unwrap().is_empty());
        assert_eq!(
            pool_choices(&[candidate("a", "u1"), candidate("a", "u1"), candidate("b", "u2")]).unwrap(),
            BTreeMap::from([("a", "u1"), ("b", "u2")])
        );
        assert!(pool_choices(&[candidate("a", "u1"), candidate("a", "u2")]).is_err());
    }
}
