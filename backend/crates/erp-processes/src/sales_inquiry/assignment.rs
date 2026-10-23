//! 询价复用采购责任规则；销售请求不提供责任人，任务权限在写事务中重新证明。

use erp_core::Result as CoreResult;
use erp_core::ids::SkuId;
use erp_procurement::service::procurement_responsibility::{
    CandidateResolution, ProcurementResponsibilityService,
};
use erp_sales::entity::sales_inquiry::SalesInquiry;
use erp_workflow::entity::work_item::{InquiryTaskAssignment, InquiryTaskAssignments};
use persistence_core::Executor;

use super::SalesInquiryProcess;
use crate::procure_to_pay::responsibility::{ProcurementResponsibilityProcess, ResolutionInput};
use crate::{Error, Result};

impl SalesInquiryProcess {
    /// 按当前冻结需求自动解析责任，并在创建任务前核验全部采购人员。
    /// # 参数
    /// `inquiry` 为已授权询价；`executor` 为提交或重询的原写事务。
    /// # 返回
    /// 按首个负责需求行排序的责任组及已核验主属组织。
    /// # 错误
    /// 需求、规则、账号、组织或询价处理权限不满足时拒绝整个分派。
    pub(super) async fn resolve_assignments(
        &self,
        inquiry: &SalesInquiry,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(InquiryTaskAssignment, String)>> {
        let candidates = self.assignment_candidates(inquiry, executor).await?;
        self.authorize_assignments(inquiry, candidates, executor).await
    }

    /// 为实际新建的一轮询价任务分派责任，并在原事务中占用一次默认轮次。
    /// # 参数
    /// `inquiry` 为冻结需求；`executor` 为创建任务的事务。
    /// # 返回
    /// 返回已核验人员和组织的整单分组。
    /// # 错误
    /// 规则、权限、组织或轮次变化时回滚整个分派。
    pub(super) async fn allocate_assignments(
        &self,
        inquiry: &SalesInquiry,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(InquiryTaskAssignment, String)>> {
        let candidates = self.assignment_candidates(inquiry, executor).await?;
        ProcurementResponsibilityService::new(self.db.clone()).claim_pools(&candidates, executor).await?;
        self.authorize_assignments(inquiry, candidates, executor).await
    }

    /// 按整单冻结需求读取候选，不占用默认调度轮次。
    pub(super) async fn assignment_candidates(
        &self,
        inquiry: &SalesInquiry,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CandidateResolution>> {
        let inputs = inquiry
            .current_requirement()?
            .data
            .lines
            .iter()
            .map(|line| ResolutionInput::new(line.line_id.clone(), SkuId::new(line.sku_id.clone())))
            .collect::<CoreResult<Vec<_>>>()?;
        ProcurementResponsibilityProcess::new(self.db.clone(), self.rbac.clone())
            .resolve_inquiry_candidates(&inputs, executor)
            .await
            .map_err(|error| match error {
                Error::Logic(reason) => {
                    Error::BusinessLogicError(format!("无法自动分派采购，请检查采购责任配置：{reason}"))
                },
                Error::ValidationError(_) => {
                    Error::ValidationError("采购责任配置中的人员不存在或已停用，请维护配置后重新提交".into())
                },
                other => other,
            })
    }

    /// 采购任务权限与主属组织均在调用方事务中核验。
    async fn authorize_assignments(
        &self,
        inquiry: &SalesInquiry,
        candidates: Vec<CandidateResolution>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(InquiryTaskAssignment, String)>> {
        let line_ids = inquiry
            .current_requirement()?
            .data
            .lines
            .iter()
            .map(|line| line.line_id.clone())
            .collect::<Vec<_>>();
        let assignments = group_assignments(candidates, &line_ids)?;
        let mut authorized = Vec::with_capacity(assignments.len());
        for assignment in assignments {
            let organization = self.access().assignment_owner(&assignment.owner_user_id, executor).await?;
            authorized.push((assignment, organization));
        }
        Ok(authorized)
    }
}

/// 将规则解析结果映射为一人一组任务；完整覆盖规则仍由工作项领域校验。
fn group_assignments(
    candidates: Vec<CandidateResolution>,
    line_ids: &[String],
) -> Result<Vec<InquiryTaskAssignment>> {
    let mut assignments: Vec<InquiryTaskAssignment> = Vec::new();
    for candidate in candidates {
        if let Some(group) = assignments.iter_mut().find(|item| item.owner_user_id == candidate.owner_user_id)
        {
            group.line_ids.push(candidate.line_key);
        } else {
            assignments.push(InquiryTaskAssignment {
                owner_user_id: candidate.owner_user_id,
                line_ids: vec![candidate.line_key],
            });
        }
    }
    let mut assignments = InquiryTaskAssignments::new(assignments, line_ids)?.into_values();
    // 以需求行顺序确定首位责任采购，不依赖数据库返回或任务创建顺序。
    assignments.sort_by_key(|item| {
        line_ids.iter().position(|id| item.line_ids.contains(id)).expect("已校验每组包含有效需求行")
    });
    Ok(assignments)
}

#[cfg(test)]
mod tests {
    use erp_procurement::entity::procurement_responsibility::ProcurementResponsibilityRuleType;

    use super::*;

    fn candidate(line: &str, owner: &str) -> CandidateResolution {
        CandidateResolution {
            line_key: line.into(),
            owner_user_id: owner.into(),
            owner_name: "采购".into(),
            rule_id: "rule".into(),
            rule_type: ProcurementResponsibilityRuleType::Sku,
        }
    }

    #[test]
    fn automatic_assignments_group_by_owner_in_requirement_order() {
        let groups = group_assignments(
            vec![candidate("c", "buyer-b"), candidate("b", "buyer-a"), candidate("a", "buyer-a")],
            &["a".into(), "b".into(), "c".into()],
        )
        .unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].owner_user_id, "buyer-a");
        assert_eq!(groups[0].line_ids.len(), 2);
        assert!(groups[0].line_ids.contains(&"a".to_string()));
        assert!(groups[0].line_ids.contains(&"b".to_string()));
        assert_eq!(groups[1].line_ids, vec!["c"]);
    }

    #[test]
    fn automatic_assignments_reject_incomplete_duplicate_or_foreign_results() {
        for candidates in [
            vec![],
            vec![candidate("a", "buyer")],
            vec![candidate("a", "buyer"), candidate("a", "other")],
            vec![candidate("a", "buyer"), candidate("outside", "other")],
            vec![candidate("a", ""), candidate("b", "other")],
        ] {
            assert!(group_assignments(candidates, &["a".into(), "b".into()]).is_err());
        }
    }
}
