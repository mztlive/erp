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
    /// # Errors
    /// 查询失败时返回仓储错误。
    pub async fn command_result(&self, idempotency_key: &str) -> Result<Option<CustomerProfileMutationView>> {
        Ok(self.command_record(idempotency_key).await?.map(command_view))
    }

    /// 加载已成功命令记录。
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
    /// `transaction` 是本次事务结果，`intended` 是成功时的稳定视图，`context` 提供幂等核对字段。
    /// 事务成功时直接返回预期视图；事务失败但并发请求已提交相同命令时返回已提交结果。
    /// 查询幂等记录失败、记录与请求上下文冲突或原事务失败且没有已提交记录时返回错误。
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

/// 将领域层已核对的命令映射为服务层稳定返回视图。
pub(super) fn checked_command_view(
    command: CustomerProfileCommand,
    context: &CustomerProfileReplayContext,
) -> Result<CustomerProfileMutationView> {
    command
        .ensure_replay_matches(context)
        .map_err(|_| Error::ConflictError("幂等键已用于另一项客户资料请求".to_string()))?;
    Ok(command_view(command))
}

/// 将命令实体转换为稳定返回视图。
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
