use mongodb::{
    bson::{self, document},
    error::{ErrorKind, WriteFailure, TRANSIENT_TRANSACTION_ERROR},
};
use thiserror::Error;

const DUPLICATE_KEY_CODE: i32 = 11000;

#[derive(Debug, Error)]
pub enum Error {
    #[error("database error: {0}")]
    DatabaseError(mongodb::error::Error),

    #[error("duplicate key: {0}")]
    DuplicateKey(mongodb::error::Error),

    #[error("bson error: {0}")]
    BsonError(#[from] bson::ser::Error),

    #[error("can not read value from document: {0}")]
    AccessValueError(#[from] document::ValueAccessError),

    #[error("optimistic locking error")]
    OptimisticLockingError,

    #[error("transaction commit outcome is unknown: {0}")]
    CommitOutcomeUnknown(mongodb::error::Error),

    #[error("transient transaction conflict: {0}")]
    TransientTransactionConflict(mongodb::error::Error),

    #[error("entity metadata is out of range: {0}")]
    EntityMetadataOutOfRange(&'static str),

    #[error("unsupported MongoDB deployment: {0}")]
    UnsupportedDeployment(&'static str),
}

impl From<mongodb::error::Error> for Error {
    /// 将 MongoDB 错误转换为稳定的仓储错误分类。
    ///
    /// 服务端错误码 `11000` 归类为唯一键冲突；带
    /// `TransientTransactionError` 标签的错误归类为可重试并发事务冲突；
    /// 其他写入或连接错误保持数据库错误。
    fn from(error: mongodb::error::Error) -> Self {
        if is_duplicate_key(&error) {
            return Self::DuplicateKey(error);
        }
        if error.contains_label(TRANSIENT_TRANSACTION_ERROR) {
            return Self::TransientTransactionConflict(error);
        }
        Self::DatabaseError(error)
    }
}

/// 判断 MongoDB 错误是否包含服务端唯一键冲突码。
fn is_duplicate_key(error: &mongodb::error::Error) -> bool {
    match error.kind.as_ref() {
        ErrorKind::Command(error) => error.code == DUPLICATE_KEY_CODE,
        ErrorKind::Write(WriteFailure::WriteError(error)) => error.code == DUPLICATE_KEY_CODE,
        ErrorKind::InsertMany(error) => error
            .write_errors
            .as_ref()
            .is_some_and(|errors| errors.iter().any(|error| error.code == DUPLICATE_KEY_CODE)),
        ErrorKind::BulkWrite(error) => error
            .write_errors
            .values()
            .any(|error| error.code == DUPLICATE_KEY_CODE),
        _ => false,
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use mongodb::{
        bson::{doc, from_document},
        error::{Error as MongoError, ErrorKind, WriteError, WriteFailure},
    };

    use super::Error;

    fn write_error(code: i32) -> MongoError {
        let write_error: WriteError = from_document(doc! {
            "code": code,
            "codeName": if code == 11000 { "DuplicateKey" } else { "Other" },
            "errmsg": "write failed",
            "errInfo": null,
        })
        .expect("write error fixture should deserialize");
        ErrorKind::Write(WriteFailure::WriteError(write_error)).into()
    }

    #[test]
    fn duplicate_key_code_maps_to_dedicated_error() {
        let error = Error::from(write_error(11000));

        assert!(matches!(error, Error::DuplicateKey(_)));
    }

    #[test]
    fn other_write_code_remains_database_error() {
        let error = Error::from(write_error(121));

        assert!(matches!(error, Error::DatabaseError(_)));
    }
}
