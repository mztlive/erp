//! 客户资料命令幂等记录、事务恢复与返回视图映射。

use database::{CustomerExt, NoTransaction};
use entities::customer::{CustomerProfileCommand, CustomerProfileReplayContext};

use crate::errors::{Error, Result};

use super::super::CustomerProfileMutationView;
use super::CustomerProfileService;

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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use database::{ensure_indexes, CustomerExt, NoTransaction, Transactional};
    use entities::{
        common::time::BusinessDate,
        customer::{
            CustomerProfileCommand, CustomerProfileCommandResultData, CustomerProfileOperation,
            CustomerProfileReplayContext, CustomerProfileRequestFingerprint,
        },
    };
    use test_support::{require_mongo, TestDb};

    use crate::{errors::Result, party::SensitiveDataCodec};

    use super::{command_view, CustomerProfileService};

    /// 真实唯一键竞争使事务失败后，Service 必须退出原 session 并从事务外重查胜者结果。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn failed_transaction_replays_committed_winner_outside_failed_session() {
        require_mongo!(async {
            let fixture = TestDb::new("customer_profile_service_replay")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");

            let context = replay_context();
            let winner = command("command-winner", &context);
            fixture
                .db()
                .customer_profile_commands()
                .create(&winner, &mut NoTransaction)
                .await
                .expect("并发胜者命令写入失败");
            let expected = command_view(winner);

            let loser = command("command-loser", &context);
            let mut intended = command_view(loser.clone());
            intended.customer_no = "MUST_NOT_RETURN".to_string();
            let db = fixture.db().clone();
            let client = db.client().clone();
            let transaction: Result<()> = client
                .with_transaction(move |session| {
                    Box::pin(async move {
                        db.customer_profile_commands().create(&loser, session).await?;
                        Ok(())
                    })
                })
                .await;
            assert!(transaction.is_err(), "同一幂等键的事务竞争者必须失败");

            let service = CustomerProfileService::new(
                fixture.db().clone(),
                Arc::new(SensitiveDataCodec::from_secret(
                    b"customer-profile-idempotency-test-secret",
                )),
            );
            let resolved = service
                .resolve_transaction(transaction, intended, &context)
                .await
                .expect("事务失败后必须从事务外重放已提交胜者");
            assert_eq!(resolved, expected);
        });
    }

    fn replay_context() -> CustomerProfileReplayContext {
        CustomerProfileReplayContext::new(
            "profile-key-service-replay",
            CustomerProfileOperation::Update,
            Some("customer-1".to_string()),
            "admin-1",
            CustomerProfileRequestFingerprint::parse_compatible("0".repeat(64)).expect("测试指纹必须合法"),
        )
        .expect("测试重放上下文必须合法")
    }

    fn command(id: &str, context: &CustomerProfileReplayContext) -> CustomerProfileCommand {
        CustomerProfileCommand::record_success(
            id,
            context,
            CustomerProfileCommandResultData {
                customer_id: "customer-1".to_string(),
                customer_no: "KH-1".to_string(),
                party_id: "party-1".to_string(),
                revision_id: "revision-2".to_string(),
                revision_no: 2,
                customer_version: 2,
                party_version: 2,
                effective_from: BusinessDate::from_ymd(2026, 8, 31).expect("测试日期必须合法"),
                change_reason: "资料修订".to_string(),
            },
        )
        .expect("测试命令必须合法")
    }
}
