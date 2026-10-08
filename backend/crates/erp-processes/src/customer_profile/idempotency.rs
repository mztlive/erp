//! 客户资料命令幂等记录、事务恢复与返回视图映射。

use erp_customer::repository::prelude::*;
use erp_customer::{
    CustomerExt, CustomerProfileCommand, CustomerProfileMutationView, CustomerProfileReplayContext,
};
use persistence_core::NoTransaction;

use super::CustomerProfileService;
use crate::{Error, Result};

impl CustomerProfileService {
    /// 按幂等键查询已成功客户资料命令的稳定结果。
    ///
    /// # 参数
    /// * `idempotency_key` - 客户资料命令幂等键。
    ///
    /// # 返回
    /// 已有成功命令时返回稳定视图；没有记录时返回 `None`。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub async fn command_result(&self, idempotency_key: &str) -> Result<Option<CustomerProfileMutationView>> {
        Ok(self.command_record(idempotency_key).await?.map(command_view))
    }

    /// 按幂等键加载已成功的客户资料命令记录。
    ///
    /// # 参数
    /// * `idempotency_key` - 客户资料命令幂等键。
    ///
    /// # 返回
    /// 找到记录时返回命令；没有记录时返回 `None`。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub(super) async fn command_record(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<CustomerProfileCommand>> {
        Ok(self
            .db
            .customer_profile_commands()
            .find_by_idempotency_key(idempotency_key, &mut NoTransaction)
            .await?)
    }

    /// 解析客户资料事务结果，并在失败后尝试重放同幂等键的已提交命令。
    ///
    /// # 参数
    /// * `transaction` - 本次事务结果。
    /// * `intended` - 事务成功时返回的稳定视图。
    /// * `context` - 幂等核对字段。
    ///
    /// # 返回
    /// 事务成功时返回 `intended`；事务失败但并发请求已提交相同命令时返回已提交结果。
    ///
    /// # 错误
    /// 查询幂等记录失败、记录与请求上下文冲突，或原事务失败且没有已提交记录时返回错误。
    pub(super) async fn resolve_transaction(
        &self,
        transaction: Result<()>,
        intended: CustomerProfileMutationView,
        context: &CustomerProfileReplayContext,
    ) -> Result<CustomerProfileMutationView> {
        match transaction {
            Ok(()) => Ok(intended),
            Err(error) => match self.command_record(context.idempotency_key()).await? {
                Some(command) => checked_command_view(command, context),
                None => Err(error),
            },
        }
    }
}

/// 核对命令与本次请求上下文后，映射为稳定返回视图。
///
/// # 参数
/// * `command` - 已提交的客户资料命令。
/// * `context` - 本次请求的幂等核对字段。
///
/// # 返回
/// 上下文一致时返回稳定视图。
///
/// # 错误
/// 幂等键已用于另一项请求时返回冲突。
pub(super) fn checked_command_view(
    command: CustomerProfileCommand,
    context: &CustomerProfileReplayContext,
) -> Result<CustomerProfileMutationView> {
    command
        .ensure_replay_matches(context)
        .map_err(|_| Error::ConflictError("幂等键已用于另一项客户资料请求".to_string()))?;
    Ok(command_view(command))
}

/// 把命令实体转换为稳定返回视图，不重新核对幂等上下文。
///
/// # 参数
/// * `command` - 已提交的客户资料命令。
///
/// # 返回
/// 返回由命令字段组成的稳定视图。
///
/// # 错误
/// 不返回错误。
pub(super) fn command_view(command: CustomerProfileCommand) -> CustomerProfileMutationView {
    CustomerProfileMutationView {
        initiated_by: command.initiated_by,
        customer_id: command.customer_id,
        customer_no: command.customer_no,
        party_id: command.party_id,
        revision_id: command.revision_id,
        revision_no: command.revision_no,
        customer_version: command.customer_version,
        party_version: command.party_version,
        effective_from: command.effective_from.to_string(),
        recorded_at: command.base.created_at,
        change_reason: command.change_reason,
    }
}
