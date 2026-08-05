//! 测试夹具的统一错误类型。

use mongodb::bson;

/// 测试夹具统一错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// MongoDB 连接或写入失败。
    #[error("MongoDB 操作失败: {0}")]
    Mongo(#[from] mongodb::error::Error),

    /// BSON 序列化失败。
    #[error("BSON 处理失败: {0}")]
    Bson(#[from] bson::ser::Error),

    /// 环境变量缺失。
    #[error("环境变量 {0} 未设置或为空")]
    EnvMissing(&'static str),

    /// JWT 签发失败。
    #[error("JWT 签发失败: {0}")]
    Jwt(#[from] jwt::Error),

    /// HMAC 密钥不合法。
    #[error("JWT HMAC 密钥不合法: {0}")]
    Hmac(#[from] hmac::digest::InvalidLength),

    /// JWT 密钥过短（与 web-api 一致，至少 32 字节）。
    #[error("JWT 密钥过短，至少需要 32 字节")]
    JwtSecretTooShort,

    /// 指定集合缺少期望的命名索引。
    #[error("集合 {collection} 缺少索引 {missing:?}")]
    IndexMissing {
        /// 被检查的集合名。
        collection: String,
        /// 缺失的索引名列表。
        missing: Vec<String>,
    },

    /// 实体构造/校验失败。
    #[error("实体校验失败: {0}")]
    Entity(#[from] entities::Error),
}

/// 测试夹具统一结果类型。
pub type Result<T> = std::result::Result<T, Error>;
