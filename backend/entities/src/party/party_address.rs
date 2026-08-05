//! `party_address`：地址（数据模型 §6.2 归属、P1 §2.1 敏感字段）。
//!
//! 履约地址（`AddressType::Fulfillment`）等地址内容是低熵敏感值
//! （§4.5.5）：实体不保存明文，只保存 `address_ciphertext`（P3 加密
//! 填充）与 `address_query_hmac`（带密钥 HMAC 查询指纹）；明文字段
//! 只出现在创建入参 [`PartyAddressData`] 中。地址按「有效期事实追加」
//! 维护（W03），内容变更不原地修改。

use std::fmt;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::field_update::FieldUpdate;
use crate::validation::normalize_optional_text;

use super::sensitive::{hmac_sha256_hex, normalize_address};
use super::status::EffectiveRecordStatus;

pub use crate::ids::{PartyAddressId, PartyId};

/// 联系人（地址联系人）最大长度。
const CONTACT_NAME_MAX_LEN: usize = 100;
/// 地址内容最大长度。
const ADDRESS_MAX_LEN: usize = 512;

/// 地址类型（§6.2 归属；履约地址按 §4.5.5 视为敏感值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressType {
    /// 注册地址。
    Registered,
    /// 经营地址。
    Operating,
    /// 履约地址（敏感值，加密存储）。
    Fulfillment,
}

impl AddressType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Registered => "注册地址",
            Self::Operating => "经营地址",
            Self::Fulfillment => "履约地址",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Registered => "registered",
            Self::Operating => "operating",
            Self::Fulfillment => "fulfillment",
        }
    }

    /// 判断地址内容是否属于敏感值。
    ///
    /// # 返回
    /// 履约地址返回 `true`（§4.5.5 加密列 + 带密钥 HMAC）。
    pub fn is_sensitive(&self) -> bool {
        matches!(self, Self::Fulfillment)
    }
}

/// 地址创建数据（不含系统字段）。
///
/// `address` 为结构化地址的规范化明文（省市区与详细地址的规范拼接），
/// 仅用于指纹计算与后续加密，实体不保留明文。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyAddressData {
    /// 所属企业主体 ID。
    pub party_id: PartyId,
    /// 地址类型。
    pub address_type: AddressType,
    /// 联系人。
    pub contact_name: Option<String>,
    /// 地址内容（明文入参；敏感值，§4.5.5）。
    pub address: String,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 是否为当前默认地址。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
}

/// 地址更新数据。
///
/// 地址内容按有效期事实追加维护，原地更新只允许切换启停状态、
/// 结束有效期与调整默认标记。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyAddressUpdate {
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

/// 地址实体（§6.2；敏感值密文 + 带密钥 HMAC 查询指纹，P1 §2.1）。
///
/// 自定义 `Debug`：地址密文与指纹字段不进入任何输出，明文永不入库。
#[derive(Serialize, Deserialize, Clone, Entity)]
pub struct PartyAddress {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属企业主体 ID。
    pub party_id: PartyId,
    /// 地址类型。
    pub address_type: AddressType,
    /// 联系人。
    pub contact_name: Option<String>,
    /// 地址密文（数据库加密列；P3 按密钥体系填充，P1 定义字段与校验）。
    pub address_ciphertext: String,
    /// 规范化地址的带密钥 HMAC 查询指纹（低熵值精确查询，禁止裸摘要）。
    pub address_query_hmac: String,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期。
    pub valid_to: Option<BusinessDate>,
    /// 是否为当前默认地址。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 创建人。
    pub created_by: String,
    /// 最后更新人。
    pub updated_by: String,
}

impl fmt::Debug for PartyAddress {
    /// Redacted Debug：不输出地址密文与指纹（明文字段永不进入 Debug 输出）。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PartyAddress")
            .field("base", &self.base)
            .field("party_id", &self.party_id)
            .field("address_type", &self.address_type)
            .field("contact_name", &self.contact_name)
            .field("address", &"[REDACTED]")
            .field("valid_from", &self.valid_from)
            .field("valid_to", &self.valid_to)
            .field("is_default", &self.is_default)
            .field("status", &self.status)
            .field("created_by", &self.created_by)
            .field("updated_by", &self.updated_by)
            .finish()
    }
}

impl fmt::Debug for PartyAddressData {
    /// Redacted Debug：地址明文不进入任何输出。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PartyAddressData")
            .field("party_id", &self.party_id)
            .field("address_type", &self.address_type)
            .field("contact_name", &self.contact_name)
            .field("address", &"[REDACTED]")
            .field("valid_from", &self.valid_from)
            .field("valid_to", &self.valid_to)
            .field("is_default", &self.is_default)
            .field("status", &self.status)
            .finish()
    }
}

impl PartyAddress {
    /// 生成地址查询指纹。
    ///
    /// 规范化规则：去首尾空白并折叠内部连续空白后，以带密钥
    /// HMAC-SHA256 计算并输出 64 位小写 hex（§4.5.5：禁止裸摘要；
    /// 换密钥后旧指纹全部失效）。
    ///
    /// # 参数
    /// * `plain` - 地址明文
    /// * `key` - 查询密钥
    ///
    /// # 返回
    /// 返回指纹字符串。
    pub fn address_fingerprint(plain: &str, key: &[u8]) -> String {
        hmac_sha256_hex(key, normalize_address(plain).as_bytes())
    }

    /// 创建地址。
    ///
    /// 完成联系人可选文本规范化与地址明文规范化（去空白、长度上限）；
    /// 以 `fingerprint_key` 从明文地址生成查询指纹，密文字段留空由
    /// P3 加密填充；强制 `valid_to` 晚于 `valid_from`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PartyAddressId`）
    /// * `data` - 创建数据（含地址明文）
    /// * `fingerprint_key` - 查询指纹密钥
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的地址实体。
    ///
    /// # 错误
    /// 当地址为空/超长、联系人超长或生效区间倒挂时返回错误。
    pub fn new(
        id: PartyAddressId,
        data: PartyAddressData,
        fingerprint_key: &[u8],
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let contact_name = normalize_optional_text(data.contact_name, "联系人", CONTACT_NAME_MAX_LEN)?;
        let address = normalize_required_address(data.address)?;
        let created_by = created_by.into();
        ensure_window_valid(data.valid_from, data.valid_to)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            party_id: data.party_id,
            address_type: data.address_type,
            contact_name,
            address_ciphertext: String::new(),
            address_query_hmac: Self::address_fingerprint(&address, fingerprint_key),
            valid_from: data.valid_from,
            valid_to: data.valid_to,
            is_default: data.is_default,
            status: data.status,
            updated_by: created_by.clone(),
            created_by,
        })
    }

    /// 更新地址（仅限生命周期字段）。
    ///
    /// 地址内容变更必须通过新的有效期事实行追加（W03）；原地更新
    /// 只允许切换启停状态（固定状态机）、结束有效期与调整默认标记。
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
    pub fn update(&mut self, update: PartyAddressUpdate, updated_by: impl Into<String>) -> Result<()> {
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

    /// 判断地址是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }
}

/// 规范化地址明文：去首尾空白、折叠内部连续空白并校验非空与长度。
///
/// # 参数
/// * `value` - 地址明文
///
/// # 返回
/// 返回规范化后的地址。
///
/// # 错误
/// 地址为空或超长时返回错误。
fn normalize_required_address(value: String) -> Result<String> {
    let address = normalize_address(&value);
    if address.is_empty() {
        return Err(Error::from("地址不能为空"));
    }
    if address.chars().count() > ADDRESS_MAX_LEN {
        return Err(Error::from("地址过长"));
    }
    Ok(address)
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
    use super::{AddressType, PartyAddress, PartyAddressData, PartyAddressUpdate};
    use crate::common::time::BusinessDate;
    use crate::field_update::FieldUpdate;
    use crate::ids::{PartyAddressId, PartyId};
    use crate::party::status::EffectiveRecordStatus;

    const KEY: &[u8] = b"test-fingerprint-key";

    fn address_data() -> PartyAddressData {
        PartyAddressData {
            party_id: PartyId::new("party-1"),
            address_type: AddressType::Fulfillment,
            contact_name: Some(" 张三 ".to_string()),
            address: " 北京市  朝阳区 望京街 10 号 ".to_string(),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            is_default: true,
            status: EffectiveRecordStatus::Active,
        }
    }

    /// happy path：地址明文折叠空白、指纹由规范化明文生成且实体不含明文。
    #[test]
    fn new_normalizes_and_fingerprints() {
        let address =
            PartyAddress::new(PartyAddressId::new("addr-1"), address_data(), KEY, "admin-1").unwrap();
        assert_eq!(address.contact_name.as_deref(), Some("张三"));
        assert_eq!(address.address_type, AddressType::Fulfillment);
        assert!(address.address_type.is_sensitive());
        assert_eq!(
            address.address_query_hmac,
            PartyAddress::address_fingerprint("北京市  朝阳区 望京街 10 号", KEY)
        );
        assert!(address.address_ciphertext.is_empty(), "P3 加密填充");
        assert!(address.is_active());
    }

    /// 失败路径：地址为空/超长、联系人超长、区间倒挂。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank = PartyAddressData {
            address: "   ".to_string(),
            ..address_data()
        };
        assert!(PartyAddress::new(PartyAddressId::new("a"), blank, KEY, "admin-1").is_err());

        let overlong = PartyAddressData {
            address: "x".repeat(513),
            ..address_data()
        };
        assert!(PartyAddress::new(PartyAddressId::new("a"), overlong, KEY, "admin-1").is_err());

        let reversed = PartyAddressData {
            valid_to: Some(BusinessDate::from_ymd(2025, 12, 31).unwrap()),
            ..address_data()
        };
        assert!(PartyAddress::new(PartyAddressId::new("a"), reversed, KEY, "admin-1").is_err());
    }

    /// 状态机：启停切换经固定矩阵校验。
    #[test]
    fn status_transitions_are_validated() {
        let mut address =
            PartyAddress::new(PartyAddressId::new("addr-2"), address_data(), KEY, "admin-1").unwrap();
        address
            .update(
                PartyAddressUpdate {
                    status: Some(EffectiveRecordStatus::Disabled),
                    valid_to: FieldUpdate::Unchanged,
                    is_default: None,
                },
                "admin-2",
            )
            .unwrap();
        assert!(!address.is_active());
        address
            .update(
                PartyAddressUpdate {
                    status: Some(EffectiveRecordStatus::Active),
                    valid_to: FieldUpdate::Set(BusinessDate::from_ymd(2026, 6, 30).unwrap()),
                    is_default: Some(false),
                },
                "admin-3",
            )
            .unwrap();
        assert!(address.is_active());
        assert_eq!(
            address.valid_to,
            Some(BusinessDate::from_ymd(2026, 6, 30).unwrap())
        );
        assert!(!address.is_default);
    }

    /// 敏感字段：Debug 与 Debug(Data) 均不泄漏地址明文与指纹。
    #[test]
    fn debug_never_leaks_plaintext() {
        let data = address_data();
        let address = PartyAddress::new(PartyAddressId::new("addr-3"), data.clone(), KEY, "admin-1").unwrap();

        let debug_entity = format!("{address:?}");
        assert!(!debug_entity.contains("望京街"));
        assert!(!debug_entity.contains("address_query_hmac"));
        assert!(!debug_entity.contains("address_ciphertext"));
        assert!(debug_entity.contains("[REDACTED]"));

        let debug_data = format!("{data:?}");
        assert!(!debug_data.contains("望京街"));
        assert!(debug_data.contains("[REDACTED]"));
    }

    /// 敏感字段：指纹稳定（同密钥同明文）且带密钥（换密钥指纹不同）。
    #[test]
    fn fingerprint_is_stable_and_keyed() {
        let a = PartyAddress::address_fingerprint("北京市 朝阳区 望京街 10 号", b"key-a");
        let b = PartyAddress::address_fingerprint("北京市 朝阳区 望京街 10 号", b"key-a");
        let c = PartyAddress::address_fingerprint("北京市 朝阳区 望京街 10 号", b"key-b");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    /// 实体 BSON 往返。
    #[test]
    fn bson_roundtrip() {
        let address =
            PartyAddress::new(PartyAddressId::new("addr-4"), address_data(), KEY, "admin-1").unwrap();
        let roundtrip: PartyAddress = bson::from_document(bson::to_document(&address).unwrap()).unwrap();
        assert_eq!(roundtrip.base, address.base);
        assert_eq!(roundtrip.address_query_hmac, address.address_query_hmac);
    }
}
