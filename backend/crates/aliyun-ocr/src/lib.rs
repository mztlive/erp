//! 阿里云 RecognizeDocumentStructure 图片客户端，不依赖 ERP 业务领域。
mod client;
mod credentials;
mod response;
mod signing;

pub use client::Client;
pub use credentials::Credentials;
pub use response::Page;

/// 官方杭州公网接入点；禁止由上传请求控制目标地址。
pub const ENDPOINT: &str = "ocr-api.cn-hangzhou.aliyuncs.com";
/// OpenAPI 版本，区别于供应商算法版本。
pub const API_VERSION: &str = "2021-07-07";
/// 单张上传图片上限。
pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
/// 结构化响应上限，防止无界 JSON 进入内存。
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// 稳定脱敏错误；不持有 HTTP 原始错误、响应正文或凭据。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("阿里云 OCR 配置无效")]
    Configuration,
    #[error("识别图片为空或超过 10 MB")]
    ImageSize,
    #[error("阿里云 OCR 请求超时")]
    Timeout,
    #[error("阿里云 OCR 网络请求失败")]
    Transport,
    #[error("阿里云 OCR 凭证无效或未获授权")]
    Authorization,
    #[error("阿里云 OCR 请求限流")]
    Throttled,
    #[error("阿里云 OCR 服务暂不可用")]
    Unavailable,
    #[error("阿里云 OCR 拒绝识别请求")]
    Rejected,
    #[error("阿里云 OCR 响应格式无效")]
    InvalidResponse,
    #[error("阿里云 OCR 响应超过限制")]
    ResponseSize,
}

/// 本客户端统一结果。
pub type Result<T> = std::result::Result<T, Error>;
