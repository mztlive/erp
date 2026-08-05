//! `warehouse_revision` 仓库修订（数据模型 §6.3、§4.4、§4.5.5，不可变修订）。
//!
//! 保存 `warehouse_id`、`revision_no`、仓库名称、**加密地址和联系人**、有效期及
//! 变更原因。地址与联系人是低熵敏感值：数据库加密列保存密文，精确查询使用
//! 带密钥的规范化 HMAC 指纹（禁止裸摘要、禁止可离线枚举）。`SensitiveText`
//! 值对象不保存明文，`Debug` 只输出指纹，不泄漏密文。
//! 同一仓库有效期不得重叠（唯一约束跨行，属 P3/索引校验）。
//! 修订一经形成不得修改，本实体不提供 `update()`。

use std::fmt;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::revision::RevisionBase;
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::ids::{WarehouseId, WarehouseRevisionId};
use crate::validation::normalize_required_text;

/// 仓库名称最大长度。
const NAME_MAX_LEN: usize = 128;
/// 加密值最大长度。
const ENCRYPTED_MAX_LEN: usize = 2048;
/// 指纹最大长度。
const FINGERPRINT_MAX_LEN: usize = 128;
/// 变更原因最大长度。
const CHANGE_REASON_MAX_LEN: usize = 512;

/// 带密钥 HMAC 指纹的密文值（数据模型 §4.5.5 敏感字段）。
///
/// 只保存加密列密文与查询指纹，**不保存明文**；`Debug`/`Display` 均不输出
/// 密文与明文。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SensitiveText {
    /// 加密列密文。
    encrypted: String,
    /// 带密钥 HMAC 查询指纹。
    fingerprint: String,
}

impl fmt::Debug for SensitiveText {
    /// 调试输出只暴露查询指纹，不输出密文与明文。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SensitiveText")
            .field("fingerprint", &self.fingerprint)
            .finish()
    }
}

impl SensitiveText {
    /// 构造敏感值。
    ///
    /// 校验密文与指纹非空、长度不超上限，并要求指纹是 64 位十六进制
    /// （HMAC-SHA256 规范形态，数据模型 §13.6 固化为唯一实现）。
    ///
    /// # 参数
    /// * `encrypted` - 数据库加密列的密文（由 P3 服务层加密产生）
    /// * `fingerprint` - 带密钥 HMAC 查询指纹（由指纹函数计算产生）
    ///
    /// # 返回
    /// 返回敏感值实例。
    ///
    /// # 错误
    /// 当密文/指纹为空、超长或指纹不是 64 位十六进制时返回错误。
    pub fn new(encrypted: String, fingerprint: String) -> Result<Self> {
        let encrypted =
            normalize_required_text(encrypted, "加密值不能为空", ENCRYPTED_MAX_LEN, "加密值过长")?;
        let fingerprint =
            normalize_required_text(fingerprint, "指纹不能为空", FINGERPRINT_MAX_LEN, "指纹过长")?;
        if fingerprint.len() != 64 || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::from("指纹必须是 64 位十六进制"));
        }

        Ok(Self {
            encrypted,
            fingerprint,
        })
    }

    /// 返回加密列密文。
    ///
    /// # 返回
    /// 返回密文字符串（调用方负责按权限展示）。
    pub fn encrypted(&self) -> &str {
        &self.encrypted
    }

    /// 返回带密钥 HMAC 查询指纹。
    ///
    /// # 返回
    /// 返回指纹字符串。
    pub fn fingerprint_value(&self) -> &str {
        &self.fingerprint
    }
}

/// 仓库修订创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WarehouseRevisionData {
    /// 所属仓库。
    pub warehouse_id: WarehouseId,
    /// 修订序号（同一仓库内从 1 递增）。
    pub revision_no: u32,
    /// 仓库名称（结构化快照）。
    pub name: String,
    /// 加密地址与查询指纹。
    pub address: SensitiveText,
    /// 加密联系人与查询指纹。
    pub contact: SensitiveText,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
    /// 变更原因。
    pub change_reason: String,
}

/// 仓库修订实体（不可变修订，数据模型 §6.3、§4.4）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct WarehouseRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 所属仓库。
    pub warehouse_id: WarehouseId,
    /// 仓库名称（结构化快照）。
    pub name: String,
    /// 加密地址与查询指纹。
    pub address: SensitiveText,
    /// 加密联系人与查询指纹。
    pub contact: SensitiveText,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
    /// 变更原因。
    pub change_reason: String,
}

impl WarehouseRevision {
    /// 创建仓库修订。
    ///
    /// 完成 name/change_reason 的校验与规范化（去首尾空白、非空、长度上限），
    /// 校验修订序号从 1 开始、生效区间不倒挂；地址与联系人由
    /// [`SensitiveText::new`] 完成密文/指纹校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::WarehouseRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的仓库修订实体。
    ///
    /// # 错误
    /// 当 name/change_reason 为空/超长、revision_no 为 0、生效区间倒挂，
    /// 或密文/指纹非法时返回错误。
    pub fn new(id: WarehouseRevisionId, data: WarehouseRevisionData) -> Result<Self> {
        let name = normalize_required_text(data.name, "仓库名称不能为空", NAME_MAX_LEN, "仓库名称过长")?;
        let change_reason = normalize_required_text(
            data.change_reason,
            "变更原因不能为空",
            CHANGE_REASON_MAX_LEN,
            "变更原因过长",
        )?;
        if data.revision_no == 0 {
            return Err(Error::from("修订序号必须从 1 开始"));
        }
        if let Some(effective_to) = data.effective_to {
            if effective_to <= data.effective_from {
                return Err(Error::from("生效结束日必须晚于生效开始日"));
            }
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            warehouse_id: data.warehouse_id,
            name,
            address: data.address,
            contact: data.contact,
            effective_from: data.effective_from,
            effective_to: data.effective_to,
            change_reason,
        })
    }

    /// 判断修订当前是否处于生效区间（不含结束日）。
    ///
    /// # 参数
    /// * `business_day` - 当前业务日
    ///
    /// # 返回
    /// 业务日落在 `[effective_from, effective_to)` 时返回 `true`。
    pub fn is_effective_on(&self, business_day: BusinessDate) -> bool {
        business_day >= self.effective_from
            && self
                .effective_to
                .is_none_or(|effective_to| business_day < effective_to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::WarehouseRevisionId;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    /// 带密钥的规范化 HMAC-SHA256 指纹（数据模型 §4.5.5：禁止裸摘要）。
    ///
    /// `hmac`/`sha2` 当前是 `[dev-dependencies]`（P0 修订提交 3786fac），
    /// 指纹函数暂在测试内定义；生产使用需地基修订提升为正式依赖。
    fn fingerprint(plain: &str, key: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC 密钥接受任意长度");
        mac.update(plain.trim().as_bytes());
        mac.finalize()
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn data() -> WarehouseRevisionData {
        let key = b"test-key-1";
        WarehouseRevisionData {
            warehouse_id: WarehouseId::new("wh-1"),
            revision_no: 1,
            name: " 北京一号仓 ".to_string(),
            address: SensitiveText::new(
                "cipher-address".to_string(),
                fingerprint("北京市朝阳区望京街道 1 号", key),
            )
            .unwrap(),
            contact: SensitiveText::new("cipher-contact".to_string(), fingerprint("张三 13900000000", key))
                .unwrap(),
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            effective_to: None,
            change_reason: " 期初建仓 ".to_string(),
        }
    }

    /// happy path：名称与变更原因 trim 规范化，敏感值与修订序号落位。
    #[test]
    fn new_trims_and_normalizes_fields() {
        let revision = WarehouseRevision::new(WarehouseRevisionId::new("rev-1"), data()).unwrap();

        assert_eq!(revision.name, "北京一号仓");
        assert_eq!(revision.change_reason, "期初建仓");
        assert_eq!(revision.revision.revision_no, 1);
        assert_eq!(revision.address.encrypted(), "cipher-address");
        assert_eq!(revision.contact.fingerprint_value().len(), 64);
        assert!(revision.is_effective_on(BusinessDate::from_ymd(2026, 6, 1).unwrap()));
    }

    /// 失败路径：必填空与超长各一条。
    #[test]
    fn new_rejects_empty_and_overlong_fields() {
        let empty_name = WarehouseRevisionData {
            name: "  ".to_string(),
            ..data()
        };
        assert!(WarehouseRevision::new(WarehouseRevisionId::new("rev-1"), empty_name).is_err());

        let overlong_reason = WarehouseRevisionData {
            change_reason: "r".repeat(513),
            ..data()
        };
        assert!(WarehouseRevision::new(WarehouseRevisionId::new("rev-1"), overlong_reason).is_err());
    }

    /// 失败路径：越界（修订序号为 0）与关联不一致（生效区间倒挂）各一条。
    #[test]
    fn new_rejects_zero_revision_no_and_reversed_window() {
        let zero_revision = WarehouseRevisionData {
            revision_no: 0,
            ..data()
        };
        assert!(WarehouseRevision::new(WarehouseRevisionId::new("rev-1"), zero_revision).is_err());

        let reversed = WarehouseRevisionData {
            effective_from: BusinessDate::from_ymd(2026, 3, 1).unwrap(),
            effective_to: Some(BusinessDate::from_ymd(2026, 2, 1).unwrap()),
            ..data()
        };
        assert!(WarehouseRevision::new(WarehouseRevisionId::new("rev-1"), reversed).is_err());
    }

    /// 敏感字段：指纹稳定且带密钥——同一明文同密钥结果一致，
    /// 不同密钥/不同明文结果不同；不是可离线枚举的裸摘要。
    #[test]
    fn fingerprint_is_stable_keyed_and_distinguishable() {
        let plain = "北京市朝阳区望京街道 1 号";
        let padded = "  北京市朝阳区望京街道 1 号  ";

        assert_eq!(fingerprint(plain, b"key-1"), fingerprint(plain, b"key-1"));
        assert_eq!(
            fingerprint(plain, b"key-1"),
            fingerprint(padded, b"key-1"),
            "首尾空白不参与指纹"
        );
        assert_ne!(fingerprint(plain, b"key-1"), fingerprint(plain, b"key-2"));
        assert_ne!(fingerprint(plain, b"key-1"), fingerprint("另一个地址", b"key-1"));
        assert_eq!(fingerprint(plain, b"key-1").len(), 64);
        assert!(
            fingerprint(plain, b"key-1")
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()),
            "指纹必须是十六进制"
        );
    }

    /// 敏感字段：Debug 不泄漏明文与密文，只暴露指纹。
    #[test]
    fn debug_never_leaks_plaintext_or_ciphertext() {
        let revision = WarehouseRevision::new(WarehouseRevisionId::new("rev-1"), data()).unwrap();

        let debug = format!("{:?}", revision);
        assert!(!debug.contains("北京市朝阳区"), "Debug 泄漏了地址明文");
        assert!(!debug.contains("张三"), "Debug 泄漏了联系人明文");
        assert!(!debug.contains("cipher-address"), "Debug 泄漏了密文");
        assert!(!debug.contains("cipher-contact"), "Debug 泄漏了密文");
        assert!(
            debug.contains(&revision.address.fingerprint_value().to_string()),
            "Debug 应保留指纹用于定位"
        );
    }

    /// 敏感字段：空密文/空指纹/非法指纹均被拒绝。
    #[test]
    fn sensitive_text_rejects_empty_and_malformed_values() {
        let key = b"test-key-1";
        let valid_fingerprint = fingerprint("地址", key);

        assert!(SensitiveText::new("  ".to_string(), valid_fingerprint.clone()).is_err());
        assert!(SensitiveText::new("cipher".to_string(), "  ".to_string()).is_err());
        assert!(SensitiveText::new("cipher".to_string(), "not-hex".to_string()).is_err());
        assert!(SensitiveText::new("cipher".to_string(), "a".repeat(63)).is_err());
        assert!(SensitiveText::new("cipher".to_string(), "a".repeat(65)).is_err());
    }

    /// 生效区间判定：结束日当天不在生效区间内。
    #[test]
    fn effectiveness_window_is_half_open() {
        let revision = WarehouseRevision::new(
            WarehouseRevisionId::new("rev-1"),
            WarehouseRevisionData {
                effective_to: Some(BusinessDate::from_ymd(2026, 3, 1).unwrap()),
                ..data()
            },
        )
        .unwrap();

        assert!(revision.is_effective_on(BusinessDate::from_ymd(2026, 2, 28).unwrap()));
        assert!(!revision.is_effective_on(BusinessDate::from_ymd(2026, 3, 1).unwrap()));
        assert!(!revision.is_effective_on(BusinessDate::from_ymd(2025, 12, 31).unwrap()));
    }

    /// 实体 JSON 往返：敏感值以密文 + 指纹两个字段持久化。
    #[test]
    fn revision_roundtrips_through_json() {
        let revision = WarehouseRevision::new(WarehouseRevisionId::new("rev-1"), data()).unwrap();
        let json = serde_json::to_string(&revision).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["address"]["encrypted"], serde_json::json!("cipher-address"));
        assert_eq!(value["address"]["fingerprint"].as_str().unwrap().len(), 64);
        assert!(value["name"].as_str().unwrap().contains("北京一号仓"));

        let back: WarehouseRevision = serde_json::from_str(&json).unwrap();
        assert_eq!(back, revision);
    }
}
