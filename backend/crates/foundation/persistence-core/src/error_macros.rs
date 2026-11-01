//! 仓储错误分派；唯一冲突的业务提示和领域错误类型由调用方提供。

/// 为上层错误实现通用仓储错误转换，保留并发及提交结果未知的原始错误。
///
/// # 参数
/// * `$error` - 提供标准仓储错误包装变体的上层错误类型。
/// * `$duplicate` - 接收拥有所有权的唯一键错误并返回领域错误的函数或无捕获闭包。
///
/// # 返回
/// 生成 `From<persistence_core::Error>`；唯一键提示由拥有领域决定。
///
/// # 错误
/// 不执行 I/O；瞬态事务和提交结果未知不得被压成字符串或通用冲突。
#[macro_export]
macro_rules! impl_repository_error {
    ($error:ty, duplicate = $duplicate:expr) => {
        impl From<$crate::Error> for $error {
            /// 将仓储失败转换为领域错误，保留源错误和事务结果语义。
            ///
            /// # 参数
            /// * `error` - 仓储或事务错误。
            /// # 返回
            /// 对应领域错误；唯一键业务提示由领域声明。
            /// # 错误
            /// 不返回错误；结果未知保持独立，不能作为自动重放依据。
            fn from(error: $crate::Error) -> Self {
                match error {
                    error @ $crate::Error::DuplicateKey(_) => {
                        let duplicate: fn($crate::Error) -> Self = $duplicate;
                        duplicate(error)
                    },
                    $crate::Error::OptimisticLockingError => {
                        Self::ConflictError("数据已被其他请求修改，请刷新后重试".to_string())
                    },
                    error @ $crate::Error::TransientTransactionConflict(_) => {
                        Self::TransientTransaction(error)
                    },
                    error @ $crate::Error::CommitOutcomeUnknown(_) => Self::OutcomeUnknown(error),
                    other => Self::RepositoryError(other),
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use mongodb::error::Error as MongoError;

    use crate::Error;

    #[derive(Debug)]
    enum ConsumerError {
        ConflictError(String),
        ReceiptDuplicate(Error),
        TransientTransaction(Error),
        OutcomeUnknown(Error),
        RepositoryError(Error),
    }

    impl_repository_error!(ConsumerError, duplicate = ConsumerError::ReceiptDuplicate);

    #[test]
    fn duplicate_handler_can_retain_the_original_receipt_error() {
        let source = Error::DuplicateKey(MongoError::custom("duplicate-source"));
        let ConsumerError::ReceiptDuplicate(actual) = ConsumerError::from(source) else {
            panic!("唯一键错误必须交给调用方处理");
        };
        let Error::DuplicateKey(mongo) = actual else {
            panic!("回执冲突必须保留原始唯一键错误");
        };
        assert_eq!(mongo.get_custom::<&str>().copied(), Some("duplicate-source"));
    }

    #[test]
    fn transaction_and_database_failures_keep_distinct_sources() {
        let transient = Error::TransientTransactionConflict(MongoError::custom("transient-source"));
        let ConsumerError::TransientTransaction(actual) = ConsumerError::from(transient) else {
            panic!("瞬态事务冲突须保留为独立变体");
        };
        let Error::TransientTransactionConflict(mongo) = actual else {
            panic!("瞬态事务包装必须保留原始冲突");
        };
        assert_eq!(mongo.get_custom::<&str>().copied(), Some("transient-source"));

        let unknown = Error::CommitOutcomeUnknown(MongoError::custom("unknown-source"));
        let ConsumerError::OutcomeUnknown(actual) = ConsumerError::from(unknown) else {
            panic!("提交结果未知不得作为可重试冲突");
        };
        let Error::CommitOutcomeUnknown(mongo) = actual else {
            panic!("结果未知包装必须保留原始提交错误");
        };
        assert_eq!(mongo.get_custom::<&str>().copied(), Some("unknown-source"));

        let database = Error::DatabaseError(MongoError::custom("database-source"));
        let ConsumerError::RepositoryError(actual) = ConsumerError::from(database) else {
            panic!("数据库失败须保留为仓储错误");
        };
        let Error::DatabaseError(mongo) = actual else {
            panic!("仓储错误包装必须保留原始数据库错误");
        };
        assert_eq!(mongo.get_custom::<&str>().copied(), Some("database-source"));
    }

    #[test]
    fn optimistic_lock_keeps_the_refresh_instruction() {
        let ConsumerError::ConflictError(message) = ConsumerError::from(Error::OptimisticLockingError) else {
            panic!("乐观锁失败须提示刷新");
        };
        assert_eq!(message, "数据已被其他请求修改，请刷新后重试");
    }
}
