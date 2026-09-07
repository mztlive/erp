//! 现场地点加密能力；领域不依赖具体主体编解码器。

/// 已规范化地点的加密接口；提供方错误由调用方保持原类型传播。
pub trait ServiceLocationCryptoPort {
    /// 调用方错误同时承接本域校验错误，避免丢失提供方失败分类。
    type Error: From<crate::Error>;

    /// 加密已通过地点规则的明文；密钥与明文不得进入日志或错误。
    fn encrypt(&self, plaintext: &str) -> std::result::Result<String, Self::Error>;
}
