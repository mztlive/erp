//! 客户资料命令幂等记录、指纹、事务恢复与重放。

use database::{CustomerExt, NoTransaction};
use entities::{
    customer::{CustomerAccount, CustomerProfileCommand, CustomerProfileCommandData},
    party::{Party, PartyRevision},
};
use id_generator::next_id;
use sha2::{Digest, Sha256};

use crate::errors::{Error, Result};

use super::super::{CustomerProfileMutationView, SaveCustomerProfileRequest};
use super::CustomerProfileService;

/// 事务失败后核对幂等命令所需的请求上下文。
#[derive(Clone, Copy)]
pub(super) struct TransactionResolutionContext<'a> {
    pub(super) idempotency_key: &'a str,
    pub(super) operation: &'a str,
    pub(super) customer_id: Option<&'a str>,
    pub(super) initiated_by: &'a str,
    pub(super) fingerprint: &'a str,
}

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
        context: TransactionResolutionContext<'_>,
    ) -> Result<CustomerProfileMutationView> {
        match transaction {
            Ok(()) => Ok(intended),
            Err(error) => match self.command_record(context.idempotency_key).await? {
                Some(command) => replay_command(
                    command,
                    context.operation,
                    context.customer_id,
                    context.initiated_by,
                    context.fingerprint,
                ),
                None => Err(error),
            },
        }
    }
}

/// 构造幂等命令所需的请求与结果版本上下文。
pub(super) struct ProfileCommandInput<'a> {
    pub(super) operation: &'a str,
    pub(super) req: SaveCustomerProfileRequest,
    pub(super) fingerprint: String,
    pub(super) initiated_by: &'a str,
    pub(super) customer_version: u64,
    pub(super) party_version: u64,
}

/// 构造幂等命令实体。
pub(super) fn profile_command(
    party: &Party,
    revision: &PartyRevision,
    account: &CustomerAccount,
    input: ProfileCommandInput<'_>,
) -> Result<CustomerProfileCommand> {
    let ProfileCommandInput {
        operation,
        req,
        fingerprint,
        initiated_by,
        customer_version,
        party_version,
    } = input;
    Ok(CustomerProfileCommand::new(
        next_id(),
        CustomerProfileCommandData {
            idempotency_key: req.idempotency_key,
            operation: operation.to_string(),
            initiated_by: initiated_by.to_string(),
            request_fingerprint: fingerprint,
            customer_id: account.base.id.clone(),
            customer_no: account.customer_no.clone(),
            party_id: party.base.id.clone(),
            revision_id: revision.base.id.clone(),
            revision_no: revision.revision.revision_no,
            customer_version,
            party_version,
            effective_from: req.effective_from,
            change_reason: req.change_reason,
        },
    )?)
}

/// 计算请求指纹；只落摘要，不持久化敏感请求正文。
pub(super) fn request_fingerprint(req: &SaveCustomerProfileRequest) -> Result<String> {
    let payload =
        serde_json::to_vec(req).map_err(|_| Error::Internal("客户资料请求指纹计算失败".to_string()))?;
    Ok(hex::encode(Sha256::digest(payload)))
}

/// 重放已成功命令，并确保幂等键未被复用于其他请求。
pub(super) fn replay_command(
    command: CustomerProfileCommand,
    operation: &str,
    customer_id: Option<&str>,
    initiated_by: &str,
    fingerprint: &str,
) -> Result<CustomerProfileMutationView> {
    let same_customer = customer_id.is_none_or(|id| id == command.customer_id);
    if command.operation != operation
        || !same_customer
        || command.initiated_by != initiated_by
        || command.request_fingerprint != fingerprint
    {
        return Err(Error::ConflictError("幂等键已用于另一项客户资料请求".to_string()));
    }
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
