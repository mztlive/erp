use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_finance::entity::supplier_deposit::SupplierDepositAccount;
use erp_identity::AuthorizationService;
use erp_read_models::purchase_center::PurchaseDepositAccessPort;
use erp_read_models::{Error as ReadError, Result as ReadResult};
use mongodb::Database;
use persistence_core::Executor;

use super::authorize_deposit_account;
use crate::Error;

/// 原采购资金摘要使用相同的财务账号范围资格，禁止全局权限代替主体和组织范围。
pub struct SupplierDepositReadAccess {
    db: Database,
    authorization: Arc<AuthorizationService>,
}

impl SupplierDepositReadAccess {
    /// 绑定应用数据库及当前RBAC服务，不缓存账号授权。
    ///
    /// `db`、`authorization` 由应用组合根提供。
    ///
    /// 可注入采购只读查询的财务权限适配器。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }
}

#[async_trait]
impl PurchaseDepositAccessPort for SupplierDepositReadAccess {
    async fn authorize(
        &self,
        account_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> ReadResult<SupplierDepositAccount> {
        authorize_deposit_account(&self.db, &self.authorization, account_id, actor, executor)
            .await
            .map_err(ReadError::from)
    }
}

impl From<Error> for ReadError {
    // 保留供应商流程与采购读取边界共有的错误类别及载荷。
    //
    // `error` 为财务账号授权返回的流程错误。
    //
    // 返回同类别的读取错误，不改写字段校验明细、稳定码或持久化错误。
    fn from(error: Error) -> Self {
        match error {
            Error::Internal(payload) => Self::Internal(payload),
            Error::NotFound(payload) => Self::NotFound(payload),
            Error::ValidationError(payload) => Self::ValidationError(payload),
            Error::InvalidInput(payload) => Self::InvalidInput(payload),
            Error::BusinessLogicError(payload) => Self::BusinessLogicError(payload),
            Error::ConflictError(payload) => Self::ConflictError(payload),
            Error::ReceiptDuplicate(payload) => Self::ReceiptDuplicate(payload),
            Error::TransientTransaction(payload) => Self::TransientTransaction(payload),
            Error::Forbidden(payload) => Self::Forbidden(payload),
            Error::Unauthenticated(payload) => Self::Unauthenticated(payload),
            Error::Logic(payload) => Self::Logic(payload),
            Error::Rbac(payload) => Self::Rbac(payload),
            Error::OutcomeUnknown(payload) => Self::OutcomeUnknown(payload),
            Error::RepositoryError(payload) => Self::RepositoryError(payload),
            Error::Coded(payload) => Self::Coded(payload),
        }
    }
}

#[cfg(test)]
mod tests {
    use application_core::ValidationIssues;
    use erp_workflow::ErrorCode;
    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::{Error, ReadError};

    #[test]
    fn deposit_access_errors_preserve_message_classification() {
        macro_rules! preserves {
            ($($variant:ident),+ $(,)?) => {
                $(assert!(matches!(
                    ReadError::from(Error::$variant("account-access".into())),
                    ReadError::$variant(message) if message == "account-access"
                ));)+
            };
        }
        preserves!(
            ConflictError,
            Unauthenticated,
            ValidationError,
            Forbidden,
            NotFound,
            BusinessLogicError,
            Internal,
            Rbac,
        );
    }

    #[test]
    fn deposit_access_errors_preserve_structured_payloads() {
        let issues = ValidationIssues::field("account_id", "财务账号无效");
        assert!(matches!(
            ReadError::from(Error::InvalidInput(issues.clone())),
            ReadError::InvalidInput(actual) if actual == issues
        ));
        let code = ErrorCode::ResourceAuthorizationNotReady;
        assert!(matches!(ReadError::from(Error::Coded(code)), ReadError::Coded(actual) if actual == code));
        assert!(matches!(
            ReadError::from(Error::Logic("invalid scope".into())),
            ReadError::Logic(error) if error.to_string() == "invalid scope"
        ));
        assert!(matches!(
            ReadError::from(Error::RepositoryError(PersistenceError::EntityMetadataOutOfRange("account"))),
            ReadError::RepositoryError(PersistenceError::EntityMetadataOutOfRange("account"))
        ));
    }

    #[test]
    fn deposit_access_errors_preserve_transaction_sources() {
        let cases = [
            (
                Error::ReceiptDuplicate(PersistenceError::DuplicateKey(MongoError::custom("receipt"))),
                "receipt",
            ),
            (
                Error::TransientTransaction(PersistenceError::TransientTransactionConflict(
                    MongoError::custom("retry"),
                )),
                "retry",
            ),
            (
                Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom("unknown"))),
                "unknown",
            ),
        ];
        for (error, expected) in cases {
            let error = ReadError::from(error);
            assert!(std::error::Error::source(&error).is_some());
            let source = match error {
                ReadError::ReceiptDuplicate(PersistenceError::DuplicateKey(source))
                | ReadError::TransientTransaction(PersistenceError::TransientTransactionConflict(source))
                | ReadError::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(source)) => source,
                other => panic!("transaction source changed classification: {other:?}"),
            };
            assert_eq!(source.get_custom::<&str>(), Some(&expected));
        }
    }
}
