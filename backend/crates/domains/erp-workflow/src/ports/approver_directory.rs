//! 动态审批消费的人员来源端口，组合层实现组织及账号读取。
use std::future::Future;

use bpm::model::{ApprovalParticipant, AssigneeResolution, ParticipantId};
use erp_core::common::time::Instant;
use persistence_core::Executor;

use crate::service::approval::assignee_source::AssigneeSource;
use crate::{Error, Result};

/// 同一读取时点已验证账号、角色及审批权限的负责人。
pub struct ResolvedApprover {
    pub participant: ApprovalParticipant,
    pub resolution: AssigneeResolution,
}
impl ResolvedApprover {
    /// 从独立姓名和来源快照构造已校验的负责人。
    /// # 参数
    /// 人员身份及解析快照；组织名称仅保留在 `source_name`。
    /// # 返回
    /// 姓名规范化后保持参与人与来源快照一致的负责人。
    /// # 错误
    /// 姓名为空或超过节点执行的字节限制时拒绝，不截断姓名或组织名称。
    pub fn new(participant_id: ParticipantId, mut resolution: AssigneeResolution) -> Result<Self> {
        let participant = ApprovalParticipant::new(participant_id, resolution.participant_name)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        resolution.participant_name = participant.name.clone();
        Ok(Self { participant, resolution })
    }
}

/// 组织负责人窄接口，不查询业务单据或创建事务。
pub trait ApproverDirectory {
    /// 解析申请人最近团队或部门的负责人及审批资格。
    /// # 参数
    /// 申请人、来源、解析时点、调用方执行器。
    /// # 返回
    /// 返回唯一合格负责人及来源依据。
    /// # 错误
    /// 组织或负责人缺失、资格失败及读取错误时拒绝。
    fn resolve(
        &self,
        applicant: &str,
        source: AssigneeSource,
        at: Instant,
        ex: &mut dyn Executor,
    ) -> impl Future<Output = Result<ResolvedApprover>> + Send;
}

#[cfg(test)]
mod tests {
    use bpm::Timestamp;

    use super::*;

    fn snapshot(name: &str, org_chars: usize) -> AssigneeResolution {
        AssigneeResolution {
            rule: "APPLICANT_TEAM_LEADER".into(),
            subject_id: "applicant".into(),
            source_id: "team".into(),
            source_name: "部".repeat(org_chars),
            assignment_id: "assignment".into(),
            participant_name: name.into(),
            resolved_at: Timestamp::from_unix_secs(1).unwrap(),
        }
    }
    #[test]
    fn resolved_name_keeps_full_organization_separate() {
        for org_chars in [40, 100] {
            let result =
                ResolvedApprover::new(ParticipantId::new("leader").unwrap(), snapshot(" 张三 ", org_chars))
                    .unwrap();
            assert_eq!(result.participant.name, "张三");
            assert_eq!(result.resolution.participant_name, "张三");
            assert_eq!(result.resolution.source_name, "部".repeat(org_chars));
        }
    }
    #[test]
    fn resolved_names_follow_execution_byte_limit() {
        for name in ["a".repeat(128), format!("{}ab", "人".repeat(42))] {
            assert!(
                ResolvedApprover::new(ParticipantId::new("leader").unwrap(), snapshot(&name, 100)).is_ok()
            );
        }
        for name in ["a".repeat(129), "人".repeat(43), " ".into()] {
            assert!(
                ResolvedApprover::new(ParticipantId::new("leader").unwrap(), snapshot(&name, 100)).is_err()
            );
        }
    }
}
