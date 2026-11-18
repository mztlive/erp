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
use erp_core::common::revision::RevisionBase;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{WarehouseId, WarehouseRevisionId};
use erp_core::region::RegionCode;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::warehouse_sku_policy::WarehouseSkuPolicyPeriod;

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
    // 调试敏感值时只显示查询指纹，避免日志输出明文或密文内容。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("SensitiveText").field("fingerprint", &self.fingerprint).finish()
    }
}

impl SensitiveText {
    /// 将调用方准备的密文列内容与带密钥 HMAC-SHA256 指纹封装为敏感值。两项去空白后不能为空、不得超长，指纹必须是 64 位十六进制；任一不合规时返回错误。
    pub fn new(encrypted: String, fingerprint: String) -> Result<Self> {
        let encrypted =
            normalize_required_text(encrypted, "加密值不能为空", ENCRYPTED_MAX_LEN, "加密值过长")?;
        let fingerprint =
            normalize_required_text(fingerprint, "指纹不能为空", FINGERPRINT_MAX_LEN, "指纹过长")?;
        if fingerprint.len() != 64 || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::from("指纹必须是 64 位十六进制"));
        }

        Ok(Self { encrypted, fingerprint })
    }

    /// 提供加密列内容供受控读取，调用方负责按权限展示；不得用于列表投影或调试日志。
    pub fn encrypted(&self) -> &str {
        &self.encrypted
    }

    /// 提供带密钥的 HMAC 查询指纹，供匹配敏感字段而不读取原值。
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
    /// 所在地区（行政区划最末级）。
    pub region_code: RegionCode,
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
    /// 所在地区（行政区划最末级）；本字段上线前建立的修订为空。
    #[serde(default)]
    pub region_code: Option<RegionCode>,
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
    /// 建立仓库资料修订，验证名称和变更原因去空白后非空且不超长、序号从 `1` 开始、地区为叶级编码、结束日严格晚于开始日。地址和联系人由 [`SensitiveText::new`] 预先校验后随修订保存。
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
        let region_code = RegionCode::parse_leaf(data.region_code.as_str())?;
        // 半开生效区间不倒挂（与策略区间同一规则、同一文案，见 `WarehouseSkuPolicyPeriod::new`）。
        WarehouseSkuPolicyPeriod::new(data.effective_from, data.effective_to)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            warehouse_id: data.warehouse_id,
            name,
            region_code: Some(region_code),
            address: data.address,
            contact: data.contact,
            effective_from: data.effective_from,
            effective_to: data.effective_to,
            change_reason,
        })
    }

    /// 按 [`WarehouseSkuPolicyPeriod::contains`] 判断业务日是否位于修订半开区间；包括开始日，不包括结束日，历史非法区间始终返回 `false`。
    pub fn is_effective_on(&self, business_day: BusinessDate) -> bool {
        WarehouseSkuPolicyPeriod::new(self.effective_from, self.effective_to)
            .is_ok_and(|period| period.contains(business_day))
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WarehouseRevisionId;
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;

    use super::*;

    type HmacSha256 = Hmac<Sha256>;

    // 测试使用开发依赖按去空白后的明文计算带密钥 HMAC-SHA256，固定指纹形状和匹配语义；生产指纹由注入端口提供，不可用裸摘要。
    fn fingerprint(plain: &str, key: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC 密钥接受任意长度");
        mac.update(plain.trim().as_bytes());
        mac.finalize().into_bytes().iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn data() -> WarehouseRevisionData {
        let key = b"test-key-1";
        WarehouseRevisionData {
            warehouse_id: WarehouseId::new("wh-1"),
            revision_no: 1,
            name: " 北京一号仓 ".to_string(),
            region_code: RegionCode::parse("110105").unwrap(),
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

    // 验证名称和变更原因去首尾空白，且序号与敏感值随修订保存。
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

    // 验证必填字段为空或超过上限时不构造修订，避免保存无法展示或追溯的快照。
    #[test]
    fn new_rejects_empty_and_overlong_fields() {
        let empty_name = WarehouseRevisionData { name: "  ".to_string(), ..data() };
        assert!(WarehouseRevision::new(WarehouseRevisionId::new("rev-1"), empty_name).is_err());

        let overlong_reason = WarehouseRevisionData { change_reason: "r".repeat(513), ..data() };
        assert!(WarehouseRevision::new(WarehouseRevisionId::new("rev-1"), overlong_reason).is_err());
    }

    // 验证零序号与倒挂日期均被拒绝，保证修订有可排序身份与有效窗口。
    #[test]
    fn new_rejects_zero_revision_no_and_reversed_window() {
        let zero_revision = WarehouseRevisionData { revision_no: 0, ..data() };
        assert!(WarehouseRevision::new(WarehouseRevisionId::new("rev-1"), zero_revision).is_err());

        let reversed = WarehouseRevisionData {
            effective_from: BusinessDate::from_ymd(2026, 3, 1).unwrap(),
            effective_to: Some(BusinessDate::from_ymd(2026, 2, 1).unwrap()),
            ..data()
        };
        assert!(WarehouseRevision::new(WarehouseRevisionId::new("rev-1"), reversed).is_err());
    }

    // 验证同一规范化值和密钥生成稳定指纹，改变密钥或原值后结果不同，不以裸摘要替代。
    #[test]
    fn fingerprint_is_stable_keyed_and_distinguishable() {
        let plain = "北京市朝阳区望京街道 1 号";
        let padded = "  北京市朝阳区望京街道 1 号  ";

        assert_eq!(fingerprint(plain, b"key-1"), fingerprint(plain, b"key-1"));
        assert_eq!(fingerprint(plain, b"key-1"), fingerprint(padded, b"key-1"), "首尾空白不参与指纹");
        assert_ne!(fingerprint(plain, b"key-1"), fingerprint(plain, b"key-2"));
        assert_ne!(fingerprint(plain, b"key-1"), fingerprint("另一个地址", b"key-1"));
        assert_eq!(fingerprint(plain, b"key-1").len(), 64);
        assert!(
            fingerprint(plain, b"key-1").bytes().all(|byte| byte.is_ascii_hexdigit()),
            "指纹必须是十六进制"
        );
    }

    // 验证调试敏感字段只显示指纹，不能泄漏明文或密文。
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

    // 验证空的密文列值、空指纹和非规范十六进制指纹均不能构造敏感值。
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

    // 验证修订包含首日、不包含结束日，以同一边界决定业务日是否有效。
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

    // 验证修订 JSON 往返保留敏感值的密文列和指纹两个字段。
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
