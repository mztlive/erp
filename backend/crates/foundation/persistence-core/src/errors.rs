use mongodb::bson;
use mongodb::error::{ErrorKind, TRANSIENT_TRANSACTION_ERROR, WriteFailure};
use thiserror::Error;

const DUPLICATE_KEY_CODE: i32 = 11000;

#[derive(Debug, Error)]
pub enum Error {
    #[error("database error: {0}")]
    DatabaseError(mongodb::error::Error),

    #[error("duplicate key: {0}")]
    DuplicateKey(mongodb::error::Error),

    #[error("bson error: {0}")]
    BsonError(#[from] bson::error::Error),

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
    // 仓储接收驱动错误时优先识别 `11000` 唯一键冲突，再按 `TransientTransactionError` 区分瞬态事务冲突；
    // 其余保留为数据库错误。
    fn from(error: mongodb::error::Error) -> Self {
        if duplicate_key_message(&error).is_some() {
            return Self::DuplicateKey(error);
        }
        if error.contains_label(TRANSIENT_TRANSACTION_ERROR) {
            return Self::TransientTransactionConflict(error);
        }
        Self::DatabaseError(error)
    }
}

impl Error {
    /// 从唯一键冲突的 `E11000` 消息提取索引名，供领域将已知索引映射为业务提示；
    /// 非唯一键冲突或消息无法识别时返回 `None`。
    pub fn duplicate_index_name(&self) -> Option<&str> {
        match self {
            Self::DuplicateKey(error) => duplicate_key_message(error).and_then(parse_index_name_from_message),
            _ => None,
        }
    }
}

// 命令、单笔及批量写入共用 `11000` 判定；同时取出同一错误文本，保证错误分类与索引名提取按相同条件识别。
fn duplicate_key_message(error: &mongodb::error::Error) -> Option<&str> {
    match error.kind.as_ref() {
        ErrorKind::Command(error) if error.code == DUPLICATE_KEY_CODE => Some(error.message.as_str()),
        ErrorKind::Write(WriteFailure::WriteError(error)) if error.code == DUPLICATE_KEY_CODE => {
            Some(error.message.as_str())
        },
        ErrorKind::InsertMany(error) => error
            .write_errors
            .as_ref()?
            .iter()
            .find(|error| error.code == DUPLICATE_KEY_CODE)
            .map(|error| error.message.as_str()),
        ErrorKind::BulkWrite(error) => error
            .write_errors
            .values()
            .find(|error| error.code == DUPLICATE_KEY_CODE)
            .map(|error| error.message.as_str()),
        _ => None,
    }
}

// 取 `index: ` 后的首个词并剪去外围标点；识别失败留给调用方走通用冲突提示。
fn parse_index_name_from_message(message: &str) -> Option<&str> {
    let after_index = message.split("index: ").nth(1)?;
    let name = after_index.split_whitespace().next()?.trim_matches([',', '.', ';', ':']);
    (!name.is_empty()).then_some(name)
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use mongodb::bson::{deserialize_from_document, doc};
    use mongodb::error::{Error as MongoError, ErrorKind, WriteError, WriteFailure};

    use super::Error;

    fn write_error(code: i32) -> MongoError {
        let write_error: WriteError = deserialize_from_document(doc! {
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

    #[test]
    fn parses_duplicate_index_name_from_write_error_message() {
        let write_error: WriteError = deserialize_from_document(doc! {
            "code": 11000,
            "codeName": "DuplicateKey",
            "errmsg": "E11000 duplicate key error collection: erp.parties index: uk_parties_party_no dup key: { party_no: \"PTY-1\" }",
            "errInfo": null,
        })
        .expect("write error fixture should deserialize");
        let mongo_error: MongoError = ErrorKind::Write(WriteFailure::WriteError(write_error)).into();
        let error = Error::from(mongo_error);

        assert_eq!(error.duplicate_index_name(), Some("uk_parties_party_no"));
    }

    #[test]
    fn unknown_duplicate_message_has_no_index_name() {
        let error = Error::from(write_error(11000));
        assert_eq!(error.duplicate_index_name(), None);
    }

    #[test]
    fn index_name_parse_trims_punctuation_and_rejects_missing() {
        let parsed = super::parse_index_name_from_message(
            "E11000 duplicate key error collection: erp.parties index: uk_parties_party_no, dup key: {}",
        );
        assert_eq!(parsed, Some("uk_parties_party_no"));

        let parsed = super::parse_index_name_from_message(
            "E11000 duplicate key error collection: erp.parties index: uk_parties_party_no. dup key: {}",
        );
        assert_eq!(parsed, Some("uk_parties_party_no"));

        assert_eq!(super::parse_index_name_from_message("E11000 duplicate key error"), None);
        assert_eq!(super::parse_index_name_from_message("E11000 index: "), None);
        assert_eq!(super::parse_index_name_from_message("E11000 index: ,.;: dup key: {}"), None);
        // 大小写敏感： Mongo 文本固定为小写 `index: `，大写前缀不识别（保持既有语义）。
        assert_eq!(super::parse_index_name_from_message("E11000 Index: foo"), None);
    }
}
