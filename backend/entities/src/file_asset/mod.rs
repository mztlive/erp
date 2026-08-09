//! 域 D05 `file_asset`：file_asset、document_attachment（页面：W04、W18）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与必需约束见数据模型 §6.1；公共字段归属按 §4.3 判定：
//! - `file_asset` 是「文件元数据、保留策略及业务关联」的受控文件资产，不属
//!   稳定基础资料或正式事实 → 只用 `BaseModel` 持久化元数据，`security_scan_status`
//!   状态机与审计字段按 §6.1 各自建模；
//! - `content_hmac` 是带密钥 HMAC（§4.5.5：禁止可离线枚举的裸摘要），指纹算法
//!   固化唯一实现 [`file_asset::content_fingerprint`]（§13.6）；
//! - `storage_object_key` 是加密受控对象存储中的不可猜测对象键（§6.1），
//!   自定义 `Debug` 不输出正文，避免写入业务日志。
//!
//! 安全检查、保留期与销毁状态只作治理记录，不阻断业务对象关联；下载授权仍按
//! 当前业务对象、角色与数据范围重验（P3）。成功/失败/导出三类资产必须拆成
//! 不同 `file_asset` 且不得混用保留期（§4.5.7，P3 校验）。

// 域 D05 与表 `file_asset` 同名（domains.md 模块命名），表模块声明允许同名。
pub mod document_attachment;
#[allow(clippy::module_inception)]
pub mod file_asset;

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{DocumentAttachmentId, FileAssetId};
pub use document_attachment::{AttachmentUsage, DocumentAttachment, DocumentAttachmentData};
pub use file_asset::{
    content_fingerprint, ContentHmac, FileAsset, FileAssetData, RetentionClass, SecurityScanStatus,
    SensitivityClass,
};
