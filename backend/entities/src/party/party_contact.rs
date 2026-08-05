//! `party_contact`：联系人（数据模型 §6.2 归属、P1 §2.1 敏感字段）。
//!
//! 手机号是低熵敏感值（§4.5.5）：实体不保存明文，只保存
//! `mobile_ciphertext`（P3 加密填充）与 `mobile_query_hmac`（带密钥
//! HMAC 查询指纹）；明文字段只出现在创建入参 [`PartyContactData`] 中。
//! 联系人按「有效期事实追加」维护（W03），内容变更不原地修改。

use std::fmt;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::field_update::FieldUpdate;
use crate::validation::{
    normalize_optional_email, normalize_optional_phone, normalize_optional_text, normalize_required_text,
};

use super::sensitive::{hmac_sha256_hex, normalize_mobile};
use super::status::EffectiveRecordStatus;

pub use crate::ids::{PartyContactId, PartyId};

/// 联系人姓名最大长度。
const CONTACT_NAME_MAX_LEN: usize = 100;
/// 职务/用途最大长度。
const TITLE_MAX_LEN: usize = 100;
/// 手机号最大长度。
const MOBILE_MAX_LEN: usize = 32;
/// 电话最大长度。
const TELEPHONE_MAX_LEN: usize = 32;
/// 邮箱最大长度。
const EMAIL_MAX_LEN: usize = 128;

/// 联系人创建数据（不含系统字段）。
///
/// `mobile` 为手机号明文，仅用于指纹计算与后续加密，实体不保留明文。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyContactData {
    /// 所属企业主体 ID。
    pub party_id: PartyId,
    /// 联系人姓名。
    pub contact_name: String,
    /// 职务/用途。
    pub title: Option<String>,
    /// 手机号（明文入参；敏感值，§4.5.5）。
    pub mobile: String,
    /// 电话。
    pub telephone: Option<String>,
    /// 邮箱。
    pub email: Option<String>,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 是否为当前默认联系人。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
}

/// 联系人更新数据。
///
/// 联系人内容（姓名、手机号、电话、邮箱）按有效期事实追加维护，
/// 原地更新只允许切换启停状态、结束有效期与调整默认标记。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyContactUpdate {
    /// 启停状态；`None` 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<EffectiveRecordStatus>,
    /// 生效结束日期更新意图（`Set` 时校验晚于 `valid_from`）。
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub valid_to: FieldUpdate<BusinessDate>,
    /// 默认标记；`None` 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
}

/// 联系人实体（§6.2；敏感值密文 + 带密钥 HMAC 查询指纹，P1 §2.1）。
///
/// 自定义 `Debug`：手机号密文与指纹字段不进入任何输出，明文永不入库。
#[derive(Serialize, Deserialize, Clone, Entity)]
pub struct PartyContact {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属企业主体 ID。
    pub party_id: PartyId,
    /// 联系人姓名。
    pub contact_name: String,
    /// 职务/用途。
    pub title: Option<String>,
    /// 手机号密文（数据库加密列；P3 按密钥体系填充，P1 定义字段与校验）。
    pub mobile_ciphertext: String,
    /// 规范化手机号的带密钥 HMAC 查询指纹（低熵值精确查询，禁止裸摘要）。
    pub mobile_query_hmac: String,
    /// 电话。
    pub telephone: Option<String>,
    /// 邮箱。
    pub email: Option<String>,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期。
    pub valid_to: Option<BusinessDate>,
    /// 是否为当前默认联系人。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 创建人。
    pub created_by: String,
    /// 最后更新人。
    pub updated_by: String,
}

impl fmt::Debug for PartyContact {
    /// Redacted Debug：不输出手机号密文与指纹（明文字段永不进入 Debug 输出）。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PartyContact")
            .field("base", &self.base)
            .field("party_id", &self.party_id)
            .field("contact_name", &self.contact_name)
            .field("title", &self.title)
            .field("mobile", &"[REDACTED]")
            .field("telephone", &self.telephone)
            .field("email", &self.email)
            .field("valid_from", &self.valid_from)
            .field("valid_to", &self.valid_to)
            .field("is_default", &self.is_default)
            .field("status", &self.status)
            .field("created_by", &self.created_by)
            .field("updated_by", &self.updated_by)
            .finish()
    }
}

impl fmt::Debug for PartyContactData {
    /// Redacted Debug：手机号明文不进入任何输出。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PartyContactData")
            .field("party_id", &self.party_id)
            .field("contact_name", &self.contact_name)
            .field("title", &self.title)
            .field("mobile", &"[REDACTED]")
            .field("telephone", &self.telephone)
            .field("email", &self.email)
            .field("valid_from", &self.valid_from)
            .field("valid_to", &self.valid_to)
            .field("is_default", &self.is_default)
            .field("status", &self.status)
            .finish()
    }
}

impl PartyContact {
    /// 生成手机号查询指纹。
    ///
    /// 规范化规则：去首尾空白后，以带密钥 HMAC-SHA256 计算并输出
    /// 64 位小写 hex（§4.5.5：禁止裸摘要；换密钥后旧指纹全部失效）。
    ///
    /// # 参数
    /// * `plain` - 手机号明文
    /// * `key` - 查询密钥
    ///
    /// # 返回
    /// 返回指纹字符串。
    pub fn mobile_fingerprint(plain: &str, key: &[u8]) -> String {
        hmac_sha256_hex(key, normalize_mobile(plain).as_bytes())
    }

    /// 创建联系人。
    ///
    /// 完成姓名的必填校验与全部文本字段的规范化（去首尾空白、格式、
    /// 长度上限）；以 `fingerprint_key` 从明文手机号生成查询指纹，
    /// 密文字段留空由 P3 加密填充；强制 `valid_to` 晚于 `valid_from`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PartyContactId`）
    /// * `data` - 创建数据（含手机号明文）
    /// * `fingerprint_key` - 查询指纹密钥
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的联系人实体。
    ///
    /// # 错误
    /// 当必填字段为空/超长、手机号格式非法、邮箱格式非法或生效区间倒挂时返回错误。
    pub fn new(
        id: PartyContactId,
        data: PartyContactData,
        fingerprint_key: &[u8],
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let contact_name = normalize_required_text(
            data.contact_name,
            "联系人姓名不能为空",
            CONTACT_NAME_MAX_LEN,
            "联系人姓名过长",
        )?;
        let title = normalize_optional_text(data.title, "职务/用途", TITLE_MAX_LEN)?;
        let mobile = normalize_optional_phone(Some(data.mobile), MOBILE_MAX_LEN)?
            .ok_or_else(|| Error::from("手机号不能为空"))?;
        let telephone = normalize_optional_text(data.telephone, "电话", TELEPHONE_MAX_LEN)?;
        let email = normalize_optional_email(data.email, EMAIL_MAX_LEN)?;
        let created_by = created_by.into();
        ensure_window_valid(data.valid_from, data.valid_to)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            party_id: data.party_id,
            contact_name,
            title,
            mobile_ciphertext: String::new(),
            mobile_query_hmac: Self::mobile_fingerprint(&mobile, fingerprint_key),
            telephone,
            email,
            valid_from: data.valid_from,
            valid_to: data.valid_to,
            is_default: data.is_default,
            status: data.status,
            updated_by: created_by.clone(),
            created_by,
        })
    }

    /// 更新联系人（仅限生命周期字段）。
    ///
    /// 内容变更必须通过新的有效期事实行追加（W03）；原地更新只允许
    /// 切换启停状态（固定状态机）、结束有效期与调整默认标记。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当状态迁移非法或 `valid_to` 不晚于 `valid_from` 时返回错误。
    pub fn update(&mut self, update: PartyContactUpdate, updated_by: impl Into<String>) -> Result<()> {
        if let Some(to) = update.status {
            self.status.transition_to(to)?;
        }
        if let Some(valid_to) = update.valid_to.into_option() {
            ensure_window_valid(self.valid_from, Some(valid_to))?;
            self.valid_to = Some(valid_to);
        }
        if let Some(is_default) = update.is_default {
            self.is_default = is_default;
        }
        self.updated_by = updated_by.into();
        Ok(())
    }

    /// 判断联系人是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }
}

/// 校验生效区间：`valid_to` 必须晚于 `valid_from`。
///
/// # 参数
/// * `valid_from` - 生效开始日期
/// * `valid_to` - 生效结束日期（可空）
///
/// # 返回
/// 区间合法返回 `Ok(())`。
///
/// # 错误
/// 结束日期不晚于开始日期时返回错误。
fn ensure_window_valid(valid_from: BusinessDate, valid_to: Option<BusinessDate>) -> Result<()> {
    if let Some(valid_to) = valid_to {
        if valid_to <= valid_from {
            return Err(Error::from("生效结束日期必须晚于生效开始日期"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PartyContact, PartyContactData, PartyContactUpdate};
    use crate::common::time::BusinessDate;
    use crate::field_update::FieldUpdate;
    use crate::ids::{PartyContactId, PartyId};
    use crate::party::status::EffectiveRecordStatus;

    const KEY: &[u8] = b"test-fingerprint-key";

    fn contact_data() -> PartyContactData {
        PartyContactData {
            party_id: PartyId::new("party-1"),
            contact_name: " 张三 ".to_string(),
            title: Some(" 采购负责人 ".to_string()),
            mobile: " 13800138000 ".to_string(),
            telephone: Some("021-12345678".to_string()),
            email: Some(" zhangsan@example.com ".to_string()),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            is_default: true,
            status: EffectiveRecordStatus::Active,
        }
    }

    /// happy path：字段去空白、邮箱校验、指纹由明文生成且实体不含明文。
    #[test]
    fn new_normalizes_and_fingerprints() {
        let contact =
            PartyContact::new(PartyContactId::new("contact-1"), contact_data(), KEY, "admin-1").unwrap();
        assert_eq!(contact.contact_name, "张三");
        assert_eq!(contact.title.as_deref(), Some("采购负责人"));
        assert_eq!(contact.email.as_deref(), Some("zhangsan@example.com"));
        assert_eq!(contact.telephone.as_deref(), Some("021-12345678"));
        assert_eq!(
            contact.mobile_query_hmac,
            PartyContact::mobile_fingerprint("13800138000", KEY)
        );
        assert!(contact.mobile_ciphertext.is_empty(), "P3 加密填充");
        assert!(contact.is_active());
    }

    /// 失败路径：姓名为空/超长、手机号格式非法、邮箱格式非法、区间倒挂。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_name = PartyContactData {
            contact_name: "   ".to_string(),
            ..contact_data()
        };
        assert!(PartyContact::new(PartyContactId::new("c"), blank_name, KEY, "admin-1").is_err());

        let bad_mobile = PartyContactData {
            mobile: "12345".to_string(),
            ..contact_data()
        };
        assert!(PartyContact::new(PartyContactId::new("c"), bad_mobile, KEY, "admin-1").is_err());

        let bad_email = PartyContactData {
            email: Some("invalid".to_string()),
            ..contact_data()
        };
        assert!(PartyContact::new(PartyContactId::new("c"), bad_email, KEY, "admin-1").is_err());

        let reversed = PartyContactData {
            valid_to: Some(BusinessDate::from_ymd(2025, 12, 31).unwrap()),
            ..contact_data()
        };
        assert!(PartyContact::new(PartyContactId::new("c"), reversed, KEY, "admin-1").is_err());
    }

    /// 状态机：启停切换经固定矩阵校验，非法目标被拒。
    #[test]
    fn status_transitions_are_validated() {
        let mut contact =
            PartyContact::new(PartyContactId::new("contact-2"), contact_data(), KEY, "admin-1").unwrap();

        contact
            .update(
                PartyContactUpdate {
                    status: Some(EffectiveRecordStatus::Disabled),
                    valid_to: FieldUpdate::Unchanged,
                    is_default: None,
                },
                "admin-2",
            )
            .unwrap();
        assert!(!contact.is_active());

        contact
            .update(
                PartyContactUpdate {
                    status: Some(EffectiveRecordStatus::Active),
                    valid_to: FieldUpdate::Unchanged,
                    is_default: None,
                },
                "admin-3",
            )
            .unwrap();
        assert!(contact.is_active());
        assert_eq!(contact.updated_by, "admin-3");
    }

    /// 生命周期更新：valid_to 只能结束（晚于 valid_from），不能提前倒挂。
    #[test]
    fn valid_to_update_is_window_checked() {
        let mut contact =
            PartyContact::new(PartyContactId::new("contact-3"), contact_data(), KEY, "admin-1").unwrap();
        contact
            .update(
                PartyContactUpdate {
                    status: None,
                    valid_to: FieldUpdate::Set(BusinessDate::from_ymd(2026, 6, 30).unwrap()),
                    is_default: Some(false),
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(
            contact.valid_to,
            Some(BusinessDate::from_ymd(2026, 6, 30).unwrap())
        );
        assert!(!contact.is_default);

        let reversed = PartyContactUpdate {
            status: None,
            valid_to: FieldUpdate::Set(BusinessDate::from_ymd(2025, 1, 1).unwrap()),
            is_default: None,
        };
        assert!(contact.update(reversed, "admin-3").is_err());
    }

    /// 敏感字段：Debug 与 Debug(Data) 均不泄漏手机号明文与指纹。
    #[test]
    fn debug_never_leaks_plaintext() {
        let data = contact_data();
        let contact =
            PartyContact::new(PartyContactId::new("contact-4"), data.clone(), KEY, "admin-1").unwrap();

        let debug_entity = format!("{contact:?}");
        assert!(!debug_entity.contains("13800138000"));
        assert!(!debug_entity.contains("mobile_query_hmac"));
        assert!(!debug_entity.contains("mobile_ciphertext"));
        assert!(debug_entity.contains("[REDACTED]"));

        let debug_data = format!("{data:?}");
        assert!(!debug_data.contains("13800138000"));
        assert!(debug_data.contains("[REDACTED]"));
    }

    /// 敏感字段：指纹稳定（同密钥同明文）且带密钥（换密钥指纹不同）。
    #[test]
    fn fingerprint_is_stable_and_keyed() {
        let a = PartyContact::mobile_fingerprint("13800138000", b"key-a");
        let b = PartyContact::mobile_fingerprint("13800138000", b"key-a");
        let c = PartyContact::mobile_fingerprint("13800138000", b"key-b");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
        assert_ne!(a, "13800138000", "指纹不得等于明文");
    }

    /// 实体 BSON 往返（密文与指纹保持原样，不出现明文）。
    #[test]
    fn bson_roundtrip() {
        let contact =
            PartyContact::new(PartyContactId::new("contact-5"), contact_data(), KEY, "admin-1").unwrap();
        let roundtrip: PartyContact = bson::from_document(bson::to_document(&contact).unwrap()).unwrap();
        assert_eq!(roundtrip.base, contact.base);
        assert_eq!(roundtrip.mobile_query_hmac, contact.mobile_query_hmac);
    }
}
