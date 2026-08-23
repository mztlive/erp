//! 写时授权重验结果收敛为引擎资格。

use bpm::engine::Eligibility;
use bpm::model::types::ApprovalBlockerCode;
use bpm::model::ParticipantId;

use crate::errors::{Error, Result};

/// 写时重验失败分类。人员失效必须提交 BLOCKED，而不是回滚。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationFailure {
    /// 账号停用。
    AccountInactive,
    /// 任职失效。
    EmploymentInvalid,
    /// 不再具备审批资格。
    NotEligible,
    /// 超出 DataScope。
    OutOfDataScope,
    /// 不能读取被审对象。
    CannotReadSubject,
    /// 岗位分离冲突。
    SeparationOfDuties,
}

impl AuthorizationFailure {
    /// 映射为合同稳定 blocker。
    ///
    /// # 返回
    /// 返回人员失效类别 blocker。
    pub fn blocker_code(self) -> ApprovalBlockerCode {
        match self {
            Self::AccountInactive => ApprovalBlockerCode::ApproverAccountInactive,
            Self::EmploymentInvalid => ApprovalBlockerCode::ApproverEmploymentInvalid,
            Self::NotEligible => ApprovalBlockerCode::ApproverNotEligible,
            Self::OutOfDataScope => ApprovalBlockerCode::ApproverOutOfAuthorizedScope,
            Self::CannotReadSubject => ApprovalBlockerCode::ApproverCannotReadSubject,
            Self::SeparationOfDuties => ApprovalBlockerCode::SeparationOfDutiesViolation,
        }
    }
}

/// 将写时重验收敛为引擎资格。
///
/// # 参数
/// * `participant` - 责任人
/// * `name_snapshot` - 显示名
/// * `failure` - 失败分类
///
/// # 返回
/// 无失败时返回 `Eligible`。
///
/// # 错误
/// 处理人引用非法时返回错误。
pub fn converge_eligibility(
    participant: &str,
    name_snapshot: &str,
    failure: Option<AuthorizationFailure>,
) -> Result<Eligibility> {
    let participant =
        ParticipantId::new(participant).map_err(|_| Error::ValidationError("处理人引用无效".to_string()))?;
    let name = name_snapshot.trim();
    if name.is_empty() {
        return Err(Error::ValidationError("审批人显示名不能为空".to_string()));
    }
    match failure {
        None => Ok(Eligibility::Eligible {
            participant,
            assignee_name_snapshot: name.to_string(),
        }),
        Some(failure) => Ok(Eligibility::Blocked {
            participant,
            code: failure.blocker_code(),
            assignee_name_snapshot: name.to_string(),
        }),
    }
}

/// 判断 blocker 是否属于人员失效类别。
///
/// # 参数
/// * `code` - 结构化阻塞码
///
/// # 返回
/// 原审批人恢复后允许继续时返回 `true`。
pub fn is_personnel_blocker(code: ApprovalBlockerCode) -> bool {
    code.allows_personnel_reassign()
}

/// 非人员一致性 blocker 只能走受阻取消。
///
/// # 参数
/// * `code` - 当前 blocker
///
/// # 返回
/// 结构、任务、版本或内部不变量返回 `true`。
pub fn requires_blocked_cancel(code: ApprovalBlockerCode) -> bool {
    !code.allows_personnel_reassign()
}

/// 幂等回读失权时不得泄露资源存在性。
///
/// # 返回
/// 返回不包含资源细节的禁止错误。
pub fn hidden_forbidden() -> Error {
    Error::Forbidden("无权执行该审批动作".to_string())
}

/// 三方责任必须一致：任务 owner、执行 assignee、实例节点当前审批人。
///
/// # 参数
/// * `actor_id` - 当前操作人
/// * `task_owner_id` - 任务责任人
/// * `execution_assignee` - 执行审批人
/// * `instance_assignee` - 实例节点当前审批人
///
/// # 错误
/// 任一不一致返回冲突。
pub fn ensure_triple_responsibility(
    actor_id: &str,
    task_owner_id: &str,
    execution_assignee: &str,
    instance_assignee: &str,
) -> Result<()> {
    if actor_id == task_owner_id && actor_id == execution_assignee && actor_id == instance_assignee {
        return Ok(());
    }
    Err(Error::ConflictError(
        "APPROVAL_RESPONSIBILITY_CONFLICT".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        converge_eligibility, ensure_triple_responsibility, hidden_forbidden, is_personnel_blocker,
        requires_blocked_cancel, AuthorizationFailure,
    };
    use bpm::model::types::ApprovalBlockerCode;

    /// 写时失效收敛为人员 blocker，不得回落默认人。
    #[test]
    fn execution_auth_converges_personnel_blocker() {
        let eligibility =
            converge_eligibility("u1", "张三", Some(AuthorizationFailure::OutOfDataScope)).unwrap();
        assert_eq!(
            eligibility.blocked_code(),
            Some(ApprovalBlockerCode::ApproverOutOfAuthorizedScope)
        );
        assert!(is_personnel_blocker(
            ApprovalBlockerCode::ApproverOutOfAuthorizedScope
        ));
        assert!(requires_blocked_cancel(ApprovalBlockerCode::OpenTaskConflict));
    }

    /// 三方责任不一致返回稳定冲突。
    #[test]
    fn execution_auth_requires_triple_responsibility() {
        assert!(ensure_triple_responsibility("u1", "u1", "u1", "u1").is_ok());
        assert!(ensure_triple_responsibility("u1", "u1", "u1", "u2").is_err());
        assert_eq!(hidden_forbidden().to_string(), "权限不足: 无权执行该审批动作");
    }
}
