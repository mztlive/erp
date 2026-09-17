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

impl Error {
    /// 从唯一键冲突错误中提取 MongoDB 索引名（若可识别）。
    ///
    /// 解析服务端 `E11000` 错误信息中的 `index: <name>` 片段，供上层将
    /// 已知唯一索引映射为面向用户的业务冲突提示；无法解析时返回 `None`。
    ///
    /// # 返回
    /// 唯一索引名；非 `DuplicateKey` 或无法解析时返回 `None`。
    pub fn duplicate_index_name(&self) -> Option<&str> {
        match self {
            Self::DuplicateKey(error) => extract_duplicate_index_name(error),
            _ => None,
        }
    }
}

/// 以一次匹配提取唯一键冲突的错误文本；命中返回 `Some`，否则返回 `None`。
///
/// `is_duplicate_key` 与 `extract_duplicate_index_name` 共用同一视图，
/// 判重码 `11000` 与标签逻辑只维护一处。
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

/// 判断 MongoDB 错误是否包含服务端唯一键冲突码。
fn is_duplicate_key(error: &mongodb::error::Error) -> bool {
    duplicate_key_message(error).is_some()
}

/// 从 MongoDB 唯一键冲突错误信息中提取索引名。
///
/// # 参数
/// * `error` - MongoDB 原始错误
///
/// # 返回
/// 解析到的索引名；无法从错误文本中定位时返回 `None`。
fn extract_duplicate_index_name(error: &mongodb::error::Error) -> Option<&str> {
    let message = duplicate_key_message(error)?;
    parse_index_name_from_message(message)
}

/// 从 `E11000` 错误文本中解析 `index: <name>`。
///
/// # 参数
/// * `message` - MongoDB 错误消息
///
/// # 返回
/// 索引名切片；未匹配时返回 `None`。
fn parse_index_name_from_message(message: &str) -> Option<&str> {
    let after_index = message.split("index: ").nth(1)?;
    let name =
        after_index.split_whitespace().next().map(|token| token.trim().trim_matches([',', '.', ';', ':']))?;
    if name.is_empty() {
        return None;
    }
    Some(name)
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
        // 大小写敏感： Mongo 文本固定为小写 `index: `，大写前缀不识别（保持既有语义）。
        assert_eq!(super::parse_index_name_from_message("E11000 Index: foo"), None);
    }
}
