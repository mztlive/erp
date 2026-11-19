use persistence_core::Executor;

use super::AuthorizationService;
use crate::entity::authorization::AccessContext;
use crate::repository::authorization::{AccessProof, CompiledAccess, Replay, ResolvedAccess};
use crate::{Error, Result};

impl AuthorizationService {
    /// 在业务结果交付前，沿原执行器复验本服务签发的查询凭证。事务内核对原快照的边界和有效期；非事务路径重读身份、范围及实际动态映射依赖。
    ///
    /// 依赖变化或到期返回范围变化错误，调用方须丢弃结果；设施读取错误不降级。成功会结束凭证所属的读取阶段。
    pub async fn validate(&self, access: &CompiledAccess, executor: &mut dyn Executor) -> Result<()> {
        if access.validity.needs_revalidation(&self.token, executor)? {
            let resolved = access
                .revalidation
                .as_deref()
                .ok_or_else(|| Error::ValidationError("缺少查询授权复验依据".into()))?;
            self.revalidate(&access.context, resolved, executor).await?;
            access.validity.ensure_current(executor)?;
        }
        for dependency in &access.dependencies {
            dependency.validate(executor).await?;
        }

        access.ensure_current(executor)?;
        executor.next_read();
        Ok(())
    }

    // 交付前验证凭证所属服务和执行边界。非事务读取须用原授权时点重算身份、范围与实际规则，不能只比较记录授权修订号；原快照无需重读。
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
            },
        }
        .map_err(revalidation_error)?;

        if current.resolved.version != resolved.version || current.resolved.rules != resolved.rules {
            return Err(changed());
        }
        Ok(())
    }
}

// 统一构造 `DATA_SCOPE_CHANGED`，通知读取用例丢弃现有结果并重新发起授权查询。
fn changed() -> Error {
    Error::ConflictError("DATA_SCOPE_CHANGED：授权范围已变化，请重新读取".into())
}

// 交付复验时，将已授权身份失效、权限撤销或版本冲突报告为范围变化；保留取证设施故障的原始错误类别。
pub(super) fn revalidation_error(error: Error) -> Error {
    match error {
        Error::Forbidden(_) | Error::Unauthenticated(_) | Error::ConflictError(_) => changed(),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
