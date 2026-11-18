//! 审批定义动作依据服务端候选中的真实主体类型核对固定管理资格。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::{approval_admin, approval_process, internal};
use persistence_core::{ExecutionBoundary, Executor};

use super::AuthorizationService;
use super::rules::TargetForm;
use crate::entity::authorization::{AccessTarget, ObjectFacts};
use crate::{Error, Permission, QualifiedPermissions, Result};

/// 审批定义入口的目标形态由固定动作确定，写操作不能省略真实类型。
pub(super) fn target(permission: &Permission, form: TargetForm) -> Option<bool> {
    entry(permission).map(|_| {
        form == TargetForm::Candidate
            || (permission == &approval_process::DEFINITION_DETAIL && form == TargetForm::Operation)
    })
}

/// 内部候选人查询复用既有读取资格，不要求授予内部编码。
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
    /// 固定操作和类型管理资格分别取证，保留既有跨角色独立资格语义。
    /// # 参数
    /// actor、permission 为真实身份及定义动作；target 为服务端类型候选；executor 为原快照执行器。
    /// # 返回
    /// 资格满足且候选、原快照阶段仍有效时成功。
    /// # 错误
    /// 非快照、目标无效或资格不足时拒绝；候选到期或执行阶段失效返回 DATA_SCOPE_CHANGED。
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
