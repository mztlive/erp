//! 多责任采购工作的完整性与当前归属验证。
use std::collections::HashSet;

use erp_core::common::time::Instant;

use super::{InquiryFeedbackData, InquiryReassignmentData, InquiryStatus, SalesInquiry, validate_text};
use crate::{Error, Result};

impl SalesInquiry {
    /// 按当前任务责任合并本轮工作，不能复用旧任务或旧需求版本。
    /// # 参数
    /// `responsibilities` 为事务内重验的当前责任采购及稳定需求行。
    /// # 返回
    /// 返回覆盖当前全部需求的完整反馈候选；发布仍执行最终规则校验。
    /// # 错误
    /// 责任重复、缺行、草稿与当前任务不一致或费用越出责任范围时拒绝。
    pub fn merge_feedback_work(
        &self,
        responsibilities: &[(String, Vec<String>)],
    ) -> Result<InquiryFeedbackData> {
        let mut result = InquiryFeedbackData::default();
        let mut covered = HashSet::new();
        let mut fee_ids = HashSet::new();
        for (actor, lines) in responsibilities {
            let expected: HashSet<_> = lines.iter().collect();
            if expected.len() != lines.len() || lines.iter().any(|id| !covered.insert(id.clone())) {
                return Err(Error::BusinessLogicError("当前采购责任行重复".into()));
            }
            let work = self
                .feedback_work
                .iter()
                .rev()
                .find(|work| {
                    !work.frozen
                        && work.requirement_version == self.requirement_version
                        && &work.actor_id == actor
                        && work.line_ids.iter().collect::<HashSet<_>>() == expected
                })
                .ok_or_else(|| Error::BusinessLogicError("全部责任采购必须完成当前版本工作后发布".into()))?;
            if work.data.lines.iter().map(|line| &line.line_id).collect::<HashSet<_>>() != expected
                || work.data.lines.len() != lines.len()
            {
                return Err(Error::BusinessLogicError("采购工作尚未覆盖全部责任需求".into()));
            }
            for fee in &work.data.fees {
                if !fee_ids.insert(fee.fee_id.clone()) || fee.line_ids.iter().any(|id| !lines.contains(id)) {
                    return Err(Error::BusinessLogicError("费用重复或超出当前采购责任".into()));
                }
                if fee.line_ids.is_empty() && self.shared_fee_owner_user_id.as_deref() != Some(actor.as_str())
                {
                    return Err(Error::BusinessLogicError("整单费用必须由明确指定的费用责任采购维护".into()));
                }
            }
            result.lines.extend(work.data.lines.clone());
            result.fees.extend(work.data.fees.clone());
        }
        if covered
            != self
                .current_requirement()?
                .data
                .lines
                .iter()
                .map(|line| line.line_id.clone())
                .collect::<HashSet<_>>()
        {
            return Err(Error::BusinessLogicError("尚有需求未分派当前责任采购".into()));
        }
        Ok(result)
    }

    /// 设置本轮明确整单费用责任人，不从任务排列顺序推断。
    /// # 参数
    /// `requested` 为显式责任人；`owners` 为本轮责任采购账号集合。
    /// # 返回
    /// 单人可默认选定，多人必须显式指定责任组内账号。
    /// # 错误
    /// 无责任人、多采购未指定或指定账号不在责任组内时拒绝。
    pub fn assign_shared_fee_owner(&mut self, requested: Option<String>, owners: &[String]) -> Result<()> {
        let unique: HashSet<_> = owners.iter().filter(|owner| !owner.trim().is_empty()).cloned().collect();
        let owner = match requested {
            Some(owner) if unique.contains(&owner) => owner,
            None if unique.len() == 1 => unique
                .into_iter()
                .next()
                .ok_or_else(|| Error::ValidationError("须指定费用责任采购".into()))?,
            _ => return Err(Error::ValidationError("多采购须明确指定责任组内的整单费用责任人".into())),
        };
        self.shared_fee_owner_user_id = Some(owner);
        Ok(())
    }

    /// 改派时冻结原责任工作，防止旧资料在再次改派后复活。
    /// # 参数
    /// `data` 为事务内核验的原责任、新责任、需求范围及原因；`actor` 为改派操作人。
    /// # 返回
    /// 保留冻结工作及改派事件；不自行修改外域任务。
    /// # 错误
    /// 版本、状态、责任或原因非法时拒绝。
    pub fn reassign_work(
        &mut self,
        expected_version: u64,
        actor: &str,
        data: InquiryReassignmentData,
        now: Instant,
    ) -> Result<()> {
        let InquiryReassignmentData { previous_owner, target_owner, line_ids, reason } = data;
        self.ensure_version(expected_version)?;
        self.ensure_state(&[InquiryStatus::AwaitingProcurement, InquiryStatus::ProcurementProcessing])?;
        validate_text(&previous_owner, "原责任采购", 128)?;
        validate_text(&target_owner, "新责任采购", 128)?;
        validate_text(&reason, "改派原因", 2000)?;
        if previous_owner == target_owner || line_ids.is_empty() {
            return Err(Error::ValidationError("改派责任未发生变化".into()));
        }
        for work in &mut self.feedback_work {
            if work.actor_id == previous_owner && work.line_ids.iter().any(|id| line_ids.contains(id)) {
                work.frozen = true;
            }
        }
        if self.shared_fee_owner_user_id.as_deref() == Some(previous_owner.as_str()) {
            self.shared_fee_owner_user_id = Some(target_owner);
        }
        self.event(actor, "reassigned", reason, now);
        Ok(())
    }
}
