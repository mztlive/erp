//! 审批定义动作依据服务端候选中的真实主体类型核对固定管理资格。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::{approval_admin, approval_process, internal};
use persistence_core::{ExecutionBoundary, Executor};

use super::AuthorizationService;
use crate::entity::authorization::{AccessTarget, ObjectFacts};
use crate::{Error, Permission, QualifiedPermissions, Result};

/// 在目标检查时匹配审批定义专用入口：写入须携带真实类型候选，只有定义详情也接受无对象操作。
pub(super) fn target(permission: &Permission, target: &AccessTarget) -> Option<bool> {
    entry(permission).map(|_| {
        matches!(target, AccessTarget::Candidate(_))
            || (permission == &approval_process::DEFINITION_DETAIL
                && matches!(target, AccessTarget::Operation))
    })
}

/// 识别固定审批定义动作；内部候选人查询复用定义详情资格，不另授内部权限码。
pub(super) fn entry(permission: &Permission) -> Option<Permission> {
    if permission == &internal::APPROVAL_DEFINITION_ASSIGNEES {
        return Some(approval_process::DEFINITION_DETAIL);
    }
    [
        approval_process::DEFINITION_DETAIL,
        approval_process::CREATE_DEFINITION_DRAFT,
        approval_process::REPLACE_DEFINITION_NODES,
        approval_process::PUBLISH_DEFINITION,
        approval_process::RETIRE_DEFINITION,
    ]
    .into_iter()
    .find(|entry| entry == permission)
}

// 按服务端候选的冻结主体类型匹配管理资格。动作与类型资格独立取证，定义详情还可由该类型运行管理员读取。
fn allows(permission: &Permission, object: &ObjectFacts, qualified: &QualifiedPermissions) -> Result<bool> {
    if object.resource != "approval_process" || !object.id.is_empty() || !object.version.is_empty() {
        return Err(Error::ValidationError("审批定义必须使用服务端确认的主体类型候选".into()));
    }

    let subject = object.subject_type.get()?;
    let (_, definition, runtime) = approval_admin::TYPES
        .iter()
        .find(|(kind, _, _)| *kind == subject)
        .ok_or_else(|| Error::ValidationError("审批定义主体类型未登记".into()))?;
    let entry = entry(permission).ok_or_else(|| Error::ValidationError("未知审批定义入口".into()))?;
    Ok(qualified.same_role(&[entry])
        && (qualified.same_role(std::slice::from_ref(definition))
            || (permission == &approval_process::DEFINITION_DETAIL
                && qualified.same_role(std::slice::from_ref(runtime)))))
}

impl AuthorizationService {
    /// 审批定义操作前，在原快照分别检查动作和服务端主体类型的管理资格，两项可由不同启用角色提供。
    ///
    /// 非快照、目标无效或资格不足时拒绝；候选到期或跨阶段返回 `DATA_SCOPE_CHANGED`。
    pub(super) async fn require_definition(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        target: &AccessTarget,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let AccessTarget::Candidate(object) = target else {
            return Err(Error::ValidationError("审批定义动作缺少真实主体类型".into()));
        };
        let mut requested = vec![entry(permission).expect("固定审批定义入口")];
        for (_, definition, runtime) in approval_admin::TYPES {
            requested.extend([definition.clone(), runtime.clone()]);
        }

        let boundary = executor
            .boundary()
            .filter(ExecutionBoundary::is_snapshot)
            .ok_or_else(|| Error::ValidationError("审批定义授权必须使用快照事务".into()))?;
        let qualified = self.permissions(actor, &requested, executor).await?;
        if !qualified.as_ref().map(|q| allows(permission, object, q)).transpose()?.unwrap_or(false) {
            return Err(Error::Forbidden("账号缺少审批定义动作或该主体类型的管理资格".into()));
        }

        if !boundary.is_current(executor) || object.valid_until.is_some_and(|until| until <= Instant::now()) {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：审批定义授权目标或执行阶段已失效".into()));
        }
        Ok(())
    }
}
