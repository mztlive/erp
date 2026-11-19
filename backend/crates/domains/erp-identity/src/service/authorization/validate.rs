use persistence_core::Executor;

use super::AuthorizationService;
use crate::entity::authorization::AccessContext;
use crate::repository::authorization::{AccessProof, CompiledAccess, Replay, ResolvedAccess};
use crate::{Error, Result};

impl AuthorizationService {
    /// 交付查询结果前，沿原执行器复验本服务签发的凭证。事务内只检查原快照阶段与期限；非事务还重读身份、范围和已使用的读取授权修订。
    ///
    /// 依赖变化或到期返回 `DATA_SCOPE_CHANGED`，调用方须丢弃结果；设施错误保持原类别。成功后结束当前读取阶段。
    pub async fn validate(&self, access: &CompiledAccess, executor: &mut dyn Executor) -> Result<()> {
        if access.validity.needs_revalidation(&self.token, executor)? {
            let revalidation = access
                .revalidation
                .as_deref()
                .ok_or_else(|| Error::ValidationError("缺少查询授权复验依据".into()))?;
            self.revalidate(&access.context, revalidation, executor).await?;
        }

        access.ensure_current(executor)?;
        executor.next_read();
        Ok(())
    }

    /// 对象交付前检查凭证归属、执行边界和有效期。非事务按原授权时点重算身份、范围和规则，不能只比记录授权版本；原快照不重读。
    pub(super) async fn validate_proof(
        &self,
        proof: &AccessProof,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if proof.validity.needs_revalidation(&self.token, executor)? {
            self.revalidate(&proof.context, &proof.resolved, executor).await?;
            proof.validity.ensure_current(executor)?;
        }
        Ok(())
    }

    // 非事务交付时按普通目标、精确任务或审批来源重放原授权，比较完整规则和版本，不使用新的授权时点续期。
    async fn revalidate(
        &self,
        context: &AccessContext,
        resolved: &ResolvedAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = match &resolved.replay {
            Replay::ApprovalSource { source, required } => {
                let qualified = self
                    .qualify(&context.actor, &context.permission, executor)
                    .await
                    .map_err(revalidation_error)?;
                self.resolve_approval_source(context, source, required, &qualified, executor).await
            },
            Replay::Target | Replay::ExactTask => {
                let model = context.model.as_ref().and_then(|_| self.mappings.get(&context.permission));
                self.resolve_target(
                    &context.actor,
                    &context.permission,
                    model.map(|mapping| mapping.model()),
                    Some(context.as_of),
                    matches!(resolved.replay, Replay::ExactTask),
                    executor,
                )
                .await
                .map(|(proof, _)| proof)
            },
        }
        .map_err(revalidation_error)?;

        if current.resolved.version != resolved.version || current.resolved.rules != resolved.rules {
            return Err(changed());
        }
        Ok(())
    }
}

// 构造统一的 `DATA_SCOPE_CHANGED`，通知读取用例丢弃结果并重新授权查询。
fn changed() -> Error {
    Error::ConflictError("DATA_SCOPE_CHANGED：授权范围已变化，请重新读取".into())
}

/// 复验时将身份失效、撤权和版本冲突报告为范围变化，保留设施和接入错误的原始类别。
pub(super) fn revalidation_error(error: Error) -> Error {
    match error {
        Error::Forbidden(_) | Error::Unauthenticated(_) | Error::ConflictError(_) => changed(),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证复验将撤权和版本冲突转换为范围变化，但不吞掉存储或接入错误。
    #[test]
    fn revoked_identity_is_changed_but_evidence_failures_keep_their_class() {
        for error in [
            Error::Forbidden("revoked".into()),
            Error::Unauthenticated("disabled".into()),
            Error::ConflictError("policy revision".into()),
        ] {
            assert!(
                matches!(revalidation_error(error), Error::ConflictError(message) if message.starts_with("DATA_SCOPE_CHANGED"))
            );
        }

        assert!(
            matches!(revalidation_error(Error::Internal("storage".into())), Error::Internal(message) if message == "storage")
        );
        assert!(matches!(
            revalidation_error(Error::ValidationError("mapping".into())),
            Error::ValidationError(_)
        ));
    }
}
