use persistence_core::Executor;

use super::AuthorizationService;
use crate::repository::authorization::{AccessProof, CompiledAccess};
use crate::{Error, Result};

impl AuthorizationService {
    /// 复验完整身份、范围、映射及有效期。
    /// # 参数
    /// access 为本服务编译的查询结果；executor 为原执行器。
    /// # 返回
    /// 当前依赖仍满足原授权时成功。
    /// # 错误
    /// 任一版本、会话、模型或有效期变化时拒绝。
    pub async fn validate(&self, access: &CompiledAccess, executor: &mut dyn Executor) -> Result<()> {
        self.validate_proof(&access.proof, executor).await?;
        for dependency in &access.dependencies {
            dependency.validate(executor).await?;
        }

        access.ensure_current(executor)?;
        executor.next_read();
        Ok(())
    }

    /// 重读所有授权依赖，而非只比较记录授权修订号。
    pub(super) async fn validate_proof(
        &self,
        proof: &AccessProof,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !proof.needs_revalidation(&self.token, executor)? {
            return Ok(());
        }

        let model = self
            .mappings
            .get(&proof.context.permission)
            .filter(|_| proof.context.model.is_some())
            .map(|mapping| mapping.model());
        let current = if let Some(source) = &proof.approval_source {
            let qualified = self
                .qualify(&proof.context.actor, &proof.context.permission, executor)
                .await
                .map_err(revalidation_error)?;
            self.resolve_approval_source(
                &proof.context,
                &source.source,
                &source.required,
                &qualified,
                executor,
            )
            .await
        } else {
            self.resolve_target(
                &proof.context.actor,
                &proof.context.permission,
                model,
                Some(proof.context.as_of),
                proof.task,
                executor,
            )
            .await
        }
        .map_err(revalidation_error)?;

        if current.version != proof.version
            || current.rule != proof.rule
            || current.whole_rule != proof.whole_rule
            || current.management_rule != proof.management_rule
        {
            return Err(changed());
        }
        proof.ensure_current(executor)
    }
}

/// 稳定范围失效错误，调用方须丢弃读取结果。
fn changed() -> Error {
    Error::ConflictError("DATA_SCOPE_CHANGED：授权范围已变化，请重新读取".into())
}

/// 已经授权后的资格失效统一按范围变化处理；取证故障保留原错误。
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
