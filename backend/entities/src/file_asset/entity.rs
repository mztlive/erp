//! `file_asset`：文件元数据、保留策略及业务关联（数据模型 §6.1）。

use std::fmt;

use entity_core::BaseModel;
use entity_macros::Entity;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::FileAssetId;
use crate::validation::normalize_required_text;

/// 对象键最大长度。
const OBJECT_KEY_MAX_LEN: usize = 512;
/// 文件名最大长度。
const FILE_NAME_MAX_LEN: usize = 256;
/// 内容类型最大长度。
const CONTENT_TYPE_MAX_LEN: usize = 128;
/// 创建人标识最大长度。
const CREATED_BY_MAX_LEN: usize = 128;

/// HMAC-SHA256 十六进制指纹长度（64 字符）。
const HMAC_HEX_LEN: usize = 64;

/// 计算内容指纹（keyed HMAC-SHA256，十六进制）。
///
/// 数据模型 §4.5.5 / §6.1：`content_hmac` 必须使用带密钥 HMAC，禁止保存可被
/// 离线枚举的裸摘要；本函数是全系统唯一实现（§13.6）。同一明文在不同密钥下
/// 产生不同指纹，同一密钥下指纹稳定可复现。
///
/// # 参数
/// * `plain` - 文件内容（或内容规范化摘要）的明文形态
/// * `key` - 密钥字节
///
/// # 返回
/// 返回 64 位小写十六进制 HMAC-SHA256 指纹。
pub fn content_fingerprint(plain: &str, key: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC 接受任意长度密钥");
    mac.update(plain.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 内容指纹值对象（keyed HMAC-SHA256 十六进制）。
///
/// 构造时校验形态（恰好 64 位十六进制字符）；指纹本身可安全展示与检索，
/// 不携带明文信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentHmac(String);

impl ContentHmac {
    /// 解析并校验内容指纹。
    ///
    /// # 参数
    /// * `value` - 十六进制指纹（64 字符）
    ///
    /// # 返回
    /// 校验通过返回 `Ok`，否则返回 `Err`。
    ///
    /// # 错误
    /// 指纹长度不是 64 或含非十六进制字符时返回 `LogicError`。
    pub fn parse(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim().to_ascii_lowercase();
        if value.len() != HMAC_HEX_LEN || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(Error::from("内容指纹必须是 64 位十六进制字符"));
        }
        Ok(Self(value))
    }

    /// 返回指纹字符串。
    ///
    /// # 返回
    /// 返回小写十六进制指纹。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContentHmac {
    /// 以十六进制字符串展示指纹。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// 安全检查状态（数据模型 §6.1：待扫描、通过、拒绝、隔离）。
///
/// 固定状态机（无运行时扩展）：
/// `PENDING → PASSED | REJECTED | QUARANTINED`；`QUARANTINED → PASSED | REJECTED`
/// （人工复核后放行或拒绝）。`PASSED` / `REJECTED` 是终态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityScanStatus {
    /// 待扫描。
    #[default]
    Pending,
    /// 通过。
    Passed,
    /// 拒绝。
    Rejected,
    /// 隔离。
    Quarantined,
}

impl SecurityScanStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待扫描",
            Self::Passed => "通过",
            Self::Rejected => "拒绝",
            Self::Quarantined => "隔离",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Passed => "passed",
            Self::Rejected => "rejected",
            Self::Quarantined => "quarantined",
        }
    }
}

impl DocumentState for SecurityScanStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Pending => &[Self::Passed, Self::Rejected, Self::Quarantined],
            Self::Quarantined => &[Self::Passed, Self::Rejected],
            Self::Passed | Self::Rejected => &[],
        }
    }
}

/// 敏感级别（数据模型 §6.1：敏感级别和保留策略）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SensitivityClass {
    /// 一般。
    General,
    /// 敏感。
    Sensitive,
    /// 高敏感。
    HighlySensitive,
}

impl SensitivityClass {
    /// 返回敏感级别的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::General => "一般",
            Self::Sensitive => "敏感",
            Self::HighlySensitive => "高敏感",
        }
    }

    /// 返回敏感级别的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Sensitive => "sensitive",
            Self::HighlySensitive => "highly_sensitive",
        }
    }
}

/// 保留策略（数据模型 §4.5.7：成功资产长期保留、失败诊断 30 天、导出 7 天；
/// 两类资产不得使用同一个 `file_asset_id`，P3 校验）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RetentionClass {
    /// 长期保留（成功白名单包、manifest、规则版本、成功结果及映射审计）。
    LongTerm,
    /// 保留 30 天（失败合规包及行列诊断明细）。
    ThirtyDays,
    /// 保留 7 天（导出结果）。
    SevenDays,
}

impl RetentionClass {
    /// 返回保留策略的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::LongTerm => "长期保留",
            Self::ThirtyDays => "保留 30 天",
            Self::SevenDays => "保留 7 天",
        }
    }

    /// 返回保留策略的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LongTerm => "long_term",
            Self::ThirtyDays => "thirty_days",
            Self::SevenDays => "seven_days",
        }
    }

    /// 判断保留策略是否要求显式到期时间。
    ///
    /// # 返回
    /// 非长期保留策略返回 `true`。
    pub fn requires_expiry(self) -> bool {
        !matches!(self, Self::LongTerm)
    }
}

/// 文件资产创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileAssetData {
    /// 加密受控对象存储中的不可猜测对象键。
    pub storage_object_key: String,
    /// 展示文件名。
    pub file_name: String,
    /// 内容类型。
    pub content_type: String,
    /// 字节大小。
    pub byte_size: u64,
    /// 内容指纹（带密钥 HMAC-SHA256 十六进制）。
    pub content_hmac: ContentHmac,
    /// 敏感级别。
    pub sensitivity_class: SensitivityClass,
    /// 保留策略。
    pub retention_class: RetentionClass,
    /// 到期时间（非长期保留策略必填）。
    pub expires_at: Option<Instant>,
    /// 创建人。
    pub created_by: String,
}

/// 受控文件资产实体（数据模型 §6.1）。
///
/// `storage_object_key` 与 `content_hmac` 按 §4.5 敏感数据处理：对象键自定义
/// `Debug` 不输出正文；指纹必须为带密钥 HMAC。安全检查与保留信息用于治理、
/// 查询和审计，不作为关联正式业务对象的前置条件。
#[derive(Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct FileAsset {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 加密受控对象存储中的不可猜测对象键。
    pub storage_object_key: String,
    /// 展示文件名。
    pub file_name: String,
    /// 内容类型。
    pub content_type: String,
    /// 字节大小。
    pub byte_size: u64,
    /// 内容指纹（带密钥 HMAC-SHA256 十六进制）。
    pub content_hmac: ContentHmac,
    /// 安全检查状态。
    pub security_scan_status: SecurityScanStatus,
    /// 敏感级别。
    pub sensitivity_class: SensitivityClass,
    /// 保留策略。
    pub retention_class: RetentionClass,
    /// 到期时间。
    pub expires_at: Option<Instant>,
    /// 销毁审计时间。
    pub destroyed_at: Option<Instant>,
    /// 创建人。
    pub created_by: String,
}

impl fmt::Debug for FileAsset {
    /// 自定义调试输出：不泄漏对象存储键正文（§6.1 对象存储地址不得写业务日志）。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileAsset")
            .field("base", &self.base)
            .field("storage_object_key", &"<redacted>")
            .field("file_name", &self.file_name)
            .field("content_type", &self.content_type)
            .field("byte_size", &self.byte_size)
            .field("content_hmac", &self.content_hmac)
            .field("security_scan_status", &self.security_scan_status)
            .field("sensitivity_class", &self.sensitivity_class)
            .field("retention_class", &self.retention_class)
            .field("expires_at", &self.expires_at)
            .field("destroyed_at", &self.destroyed_at)
            .field("created_by", &self.created_by)
            .finish()
    }
}

impl FileAsset {
    /// 创建文件资产。
    ///
    /// 完成对象键/文件名/内容类型的校验与规范化（trim、非空、长度上限），
    /// 并强制保留策略一致性：非长期保留策略必须携带 `expires_at`
    /// （§4.5.7：失败诊断与导出按保留期销毁）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::FileAssetId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的文件资产实体（安全检查状态 `PENDING`）。
    ///
    /// # 错误
    /// 当对象键/文件名/内容类型为空或超长，或非长期保留策略缺少到期时间时
    /// 返回错误。
    pub fn new(id: FileAssetId, data: FileAssetData) -> Result<Self> {
        let storage_object_key = normalize_required_text(
            data.storage_object_key,
            "对象键不能为空",
            OBJECT_KEY_MAX_LEN,
            "对象键过长",
        )?;
        let file_name =
            normalize_required_text(data.file_name, "文件名不能为空", FILE_NAME_MAX_LEN, "文件名过长")?;
        let content_type = normalize_required_text(
            data.content_type,
            "内容类型不能为空",
            CONTENT_TYPE_MAX_LEN,
            "内容类型过长",
        )?;
        let created_by = normalize_required_text(
            data.created_by,
            "创建人不能为空",
            CREATED_BY_MAX_LEN,
            "创建人过长",
        )?;
        if data.retention_class.requires_expiry() && data.expires_at.is_none() {
            return Err(Error::from("非长期保留策略必须提供到期时间"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            storage_object_key,
            file_name,
            content_type,
            byte_size: data.byte_size,
            content_hmac: data.content_hmac,
            security_scan_status: SecurityScanStatus::Pending,
            sensitivity_class: data.sensitivity_class,
            retention_class: data.retention_class,
            expires_at: data.expires_at,
            destroyed_at: None,
            created_by,
        })
    }

    /// 记录安全检查结果。
    ///
    /// # 参数
    /// * `status` - 检查结果（通过/拒绝/隔离；隔离后经人工复核可放行或拒绝）
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当迁移不在安全检查状态机内（如已通过后再次隔离）时返回错误。
    pub fn mark_scan_result(&mut self, status: SecurityScanStatus) -> Result<()> {
        ensure_transition(self.security_scan_status, status)?;
        self.security_scan_status = status;
        Ok(())
    }

    /// 标记销毁。
    ///
    /// 销毁审计只记录一次；销毁状态保留为治理记录，不影响既有或新增业务关联。
    ///
    /// # 参数
    /// * `at` - 销毁时刻
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当资产已标记销毁时返回错误。
    pub fn destroy(&mut self, at: Instant) -> Result<()> {
        if self.destroyed_at.is_some() {
            return Err(Error::from("文件资产已销毁"));
        }
        self.destroyed_at = Some(at);
        Ok(())
    }

    /// 判断文件资产在指定时点是否可用作受控证据。
    ///
    /// 仅当安全扫描已通过、未被销毁且未过期（`expires_at` 为空或晚于 `now`）时视为可用；
    /// 该判断为纯时点快照，不执行 I/O、时钟或加密。
    ///
    /// # 参数
    /// * `now` - 校验的统一时点
    ///
    /// # 返回
    /// 可用时返回 `true`。
    pub fn is_usable_at(&self, now: Instant) -> bool {
        self.security_scan_status == SecurityScanStatus::Passed
            && self.destroyed_at.is_none()
            && self.expires_at.is_none_or(|expires_at| expires_at > now)
    }

    /// 校验文件资产在指定时点可用作 W13 受控证据。
    ///
    /// 与 [`Self::is_usable_at`] 使用同一规则；不可用时以既有业务错误文案失败关闭，
    /// 供 `CardFundsReviewEvidence::validate_assets` 按首错顺序复用。
    ///
    /// # 参数
    /// * `now` - 校验的统一时点
    ///
    /// # 返回
    /// 可用时返回 `Ok(())`。
    ///
    /// # 错误
    /// 扫描未通过、已销毁或已过期时返回 `LogicError("复核证据文件未通过安全检查、已销毁或已过期")`。
    pub fn validate_usable_at(&self, now: Instant) -> Result<()> {
        if self.is_usable_at(now) {
            Ok(())
        } else {
            Err(Error::from("复核证据文件未通过安全检查、已销毁或已过期"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        content_fingerprint, ContentHmac, FileAsset, FileAssetData, RetentionClass, SecurityScanStatus,
        SensitivityClass,
    };
    use crate::common::state::ensure_transition;
    use crate::common::time::Instant;
    use crate::ids::FileAssetId;
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::{Digest, Sha256};

    fn hmac_value(plain: &str, key: &[u8]) -> ContentHmac {
        ContentHmac::parse(content_fingerprint(plain, key)).unwrap()
    }

    fn data() -> FileAssetData {
        FileAssetData {
            storage_object_key: " obj/2025/08/abc123 ".to_string(),
            file_name: " 导入清单.xlsx ".to_string(),
            content_type: " application/vnd.ms-excel ".to_string(),
            byte_size: 2048,
            content_hmac: hmac_value("content", b"secret-key"),
            sensitivity_class: SensitivityClass::Sensitive,
            retention_class: RetentionClass::ThirtyDays,
            expires_at: Some(Instant::from_unix_secs(1_703_260_800)),
            created_by: " admin-1 ".to_string(),
        }
    }

    /// happy path：字段 trim、初始待扫描、指纹稳定且带密钥。
    #[test]
    fn new_trims_fields_and_fingerprint_is_keyed_and_stable() {
        let asset = FileAsset::new(FileAssetId::new("fa-1"), data()).unwrap();
        assert_eq!(asset.storage_object_key, "obj/2025/08/abc123");
        assert_eq!(asset.file_name, "导入清单.xlsx");
        assert_eq!(asset.content_type, "application/vnd.ms-excel");
        assert_eq!(asset.created_by, "admin-1");
        assert_eq!(asset.security_scan_status, SecurityScanStatus::Pending);

        let fingerprint = content_fingerprint("content", b"secret-key");
        assert_eq!(
            fingerprint,
            content_fingerprint("content", b"secret-key"),
            "指纹稳定"
        );
        assert_ne!(
            fingerprint,
            content_fingerprint("content", b"other-key"),
            "不同密钥产生不同指纹"
        );

        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(b"secret-key").unwrap();
        mac.update(b"content");
        let expected = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(fingerprint, expected);
    }

    /// 敏感字段：Debug 不泄漏对象键正文；指纹校验形态。
    #[test]
    fn debug_redacts_object_key_and_hmac_validates_shape() {
        let asset = FileAsset::new(FileAssetId::new("fa-1"), data()).unwrap();
        let debug = format!("{asset:?}");
        assert!(!debug.contains("obj/2025/08/abc123"), "Debug 不得输出对象键");
        assert!(debug.contains("<redacted>"));
        assert!(debug.contains("导入清单.xlsx"), "非敏感字段正常展示");

        assert!(ContentHmac::parse("abcd").is_err());
        assert!(ContentHmac::parse("z".repeat(64)).is_err(), "含非十六进制字符");
        assert!(ContentHmac::parse("A".repeat(64)).is_ok(), "大小写十六进制均可");
    }

    /// 失败路径：必填为空被拒。
    #[test]
    fn new_rejects_empty_file_name() {
        let payload = FileAssetData {
            file_name: "  ".to_string(),
            ..data()
        };
        assert!(FileAsset::new(FileAssetId::new("fa-1"), payload).is_err());
    }

    /// 失败路径：超长对象键被拒。
    #[test]
    fn new_rejects_overlong_object_key() {
        let payload = FileAssetData {
            storage_object_key: "x".repeat(513),
            ..data()
        };
        assert!(FileAsset::new(FileAssetId::new("fa-1"), payload).is_err());
    }

    /// 失败路径：保留策略不一致（非长期缺少到期时间）被拒。
    #[test]
    fn new_rejects_missing_expiry_for_short_retention() {
        let payload = FileAssetData {
            expires_at: None,
            ..data()
        };
        assert!(FileAsset::new(FileAssetId::new("fa-1"), payload).is_err());

        let payload = FileAssetData {
            retention_class: RetentionClass::LongTerm,
            expires_at: None,
            ..data()
        };
        assert!(FileAsset::new(FileAssetId::new("fa-2"), payload).is_ok());
    }

    /// 状态机：安全检查合法迁移（隔离后可复核放行/拒绝）。
    #[test]
    fn scan_lifecycle_passes_and_reviews() {
        let mut asset = FileAsset::new(FileAssetId::new("fa-1"), data()).unwrap();
        asset.mark_scan_result(SecurityScanStatus::Quarantined).unwrap();
        assert_eq!(asset.security_scan_status, SecurityScanStatus::Quarantined);
        asset.mark_scan_result(SecurityScanStatus::Passed).unwrap();
        assert_eq!(asset.security_scan_status, SecurityScanStatus::Passed);
    }

    /// 状态机：非法迁移被拒（终态回退、隔离后放行再隔离）。
    #[test]
    fn illegal_scan_transitions_are_rejected() {
        let mut asset = FileAsset::new(FileAssetId::new("fa-1"), data()).unwrap();
        assert!(asset.mark_scan_result(SecurityScanStatus::Quarantined).is_ok());
        assert!(
            asset.mark_scan_result(SecurityScanStatus::Pending).is_err(),
            "不能回退"
        );
        asset.mark_scan_result(SecurityScanStatus::Passed).unwrap();
        assert!(
            asset.mark_scan_result(SecurityScanStatus::Quarantined).is_err(),
            "放行后不能再次隔离"
        );

        let mut rejected = FileAsset::new(FileAssetId::new("fa-2"), data()).unwrap();
        rejected.mark_scan_result(SecurityScanStatus::Rejected).unwrap();
        assert!(rejected.mark_scan_result(SecurityScanStatus::Pending).is_err());
        assert!(rejected
            .mark_scan_result(SecurityScanStatus::Quarantined)
            .is_err());
    }

    /// 状态机：逐边定向断言（含不可逆终态）。
    #[test]
    fn directed_edge_assertions() {
        for &(from, to) in &[
            (SecurityScanStatus::Pending, SecurityScanStatus::Passed),
            (SecurityScanStatus::Pending, SecurityScanStatus::Rejected),
            (SecurityScanStatus::Pending, SecurityScanStatus::Quarantined),
            (SecurityScanStatus::Quarantined, SecurityScanStatus::Passed),
            (SecurityScanStatus::Quarantined, SecurityScanStatus::Rejected),
        ] {
            assert!(ensure_transition(from, to).is_ok(), "{from:?} → {to:?}");
        }
        assert!(ensure_transition(SecurityScanStatus::Passed, SecurityScanStatus::Quarantined).is_err());
        assert!(ensure_transition(SecurityScanStatus::Rejected, SecurityScanStatus::Passed).is_err());
        assert!(ensure_transition(SecurityScanStatus::Pending, SecurityScanStatus::Pending).is_ok());
    }

    /// 销毁审计只记录一次，且状态保留供治理查询。
    #[test]
    fn destroy_is_recorded_once() {
        let mut asset = FileAsset::new(FileAssetId::new("fa-1"), data()).unwrap();
        asset.destroy(Instant::from_unix_secs(1_703_000_000)).unwrap();
        assert_eq!(asset.destroyed_at, Some(Instant::from_unix_secs(1_703_000_000)));
        assert!(
            asset.destroy(Instant::from_unix_secs(1_703_000_000)).is_err(),
            "销毁审计只记录一次"
        );
    }

    /// 裸摘要对比：指纹不等于无密钥 SHA-256，证明使用了带密钥 HMAC。
    #[test]
    fn fingerprint_differs_from_bare_digest() {
        let mut hasher = Sha256::new();
        hasher.update(b"content");
        let bare = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_ne!(content_fingerprint("content", b"secret-key"), bare);
    }

    /// 枚举序列化与标签稳定。
    #[test]
    fn enums_codes_and_labels_are_stable() {
        assert_eq!(
            serde_json::to_string(&SecurityScanStatus::Quarantined).unwrap(),
            "\"quarantined\""
        );
        assert_eq!(
            serde_json::to_string(&RetentionClass::SevenDays).unwrap(),
            "\"seven_days\""
        );
        assert_eq!(SensitivityClass::HighlySensitive.as_str(), "highly_sensitive");
        assert_eq!(SecurityScanStatus::Pending.label(), "待扫描");
        assert_eq!(RetentionClass::ThirtyDays.label(), "保留 30 天");
        assert_eq!(SensitivityClass::General.label(), "一般");
    }

    /// BSON 往返。
    #[test]
    fn entity_roundtrips_through_bson() {
        let asset = FileAsset::new(FileAssetId::new("fa-1"), data()).unwrap();
        let roundtrip: FileAsset =
            bson::deserialize_from_document(bson::serialize_to_document(&asset).unwrap()).unwrap();
        assert_eq!(roundtrip, asset);
    }
}
