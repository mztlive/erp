//! 发布修订内容身份：版本化 canonical 编码与 FNV-1a 64 指纹。
//!
//! v1 保持既有持久化合同：对固定字段顺序的 UTF-8 canonical 文本做 FNV-1a 64，
//! 写入 16 位小写十六进制，不带算法前缀。字段顺序、可选值编码和能力清单
//! Debug 变体名均冻结；算法或编码变更必须升级版本并提供兼容读取。

use crate::common::time::Instant;
use crate::money::{Amount, Quantity, Rate};

use super::revision::{ProductCapability, SaleStatus};

/// v1 内容指纹的持久化长度（16 位小写十六进制）。
const V1_WIRE_LEN: usize = 16;
/// FNV-1a 64 偏移基数。
const FNV1A64_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64 质数。
const FNV1A64_PRIME: u64 = 0x0000_0100_0000_01b3;

/// 参与发布内容指纹的规范化快照。
///
/// 调用方必须先完成文本规范化、能力去重和销售不变式校验；本结构只负责
/// 稳定编码，不再次清洗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicationContentSnapshot<'a> {
    /// 已规范化的商城展示名称。
    pub name: &'a str,
    /// 已规范化的规格快照。
    pub specification: Option<&'a str>,
    /// 已规范化的销售说明。
    pub sales_description: &'a str,
    /// 最小购买量。
    pub minimum_purchase_quantity: Quantity,
    /// 含税销售价。
    pub sales_price_gross: Amount,
    /// 销项税率。
    pub sales_tax_rate: Rate,
    /// 已规范化的计量单位代码。
    pub base_unit_code: &'a str,
    /// 已规范化的可销售区域。
    pub sales_region: Option<&'a str>,
    /// 上架状态。
    pub sale_status: SaleStatus,
    /// 已去重并保留首次顺序的商品级能力清单。
    pub product_capabilities: &'a [ProductCapability],
    /// 生效开始时间。
    pub valid_from: Instant,
    /// 生效结束时间。
    pub valid_to: Option<Instant>,
}

/// 发布修订内容指纹（v1：FNV-1a 64 的 16 位小写十六进制）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PublicationContentFingerprint(String);

impl PublicationContentFingerprint {
    /// 由已规范化快照派生 v1 内容指纹。
    ///
    /// # 参数
    /// * `snapshot` - 参与指纹的规范化内容字段
    ///
    /// # 返回
    /// 返回 16 位小写十六进制指纹。
    ///
    /// # 错误
    /// 无。编码与哈希均为确定性纯函数。
    ///
    /// # 关键业务约束
    /// 相同规范化内容必须得到相同指纹；任一参与字段变化必须改变指纹。
    /// 不包含修订号、实体 ID 或占位值。
    pub fn from_snapshot(snapshot: &PublicationContentSnapshot<'_>) -> Self {
        let hash = fnv1a64(canonical_v1(snapshot).as_bytes());
        Self(format!("{hash:016x}"))
    }

    /// 返回可持久化的 v1 指纹字符串。
    ///
    /// # 返回
    /// 返回 16 位小写十六进制切片。
    ///
    /// # 错误
    /// 无。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 消费自身并返回可写入修订实体的指纹字符串。
    ///
    /// # 返回
    /// 返回 16 位小写十六进制所有权字符串。
    ///
    /// # 错误
    /// 无。
    pub fn into_wire(self) -> String {
        debug_assert_eq!(self.0.len(), V1_WIRE_LEN);
        self.0
    }
}

/// 按 v1 字段顺序拼接 canonical 文本。
///
/// # 参数
/// * `snapshot` - 已规范化的内容快照
///
/// # 返回
/// 返回以 `|` 分隔的 UTF-8 canonical 字符串。
///
/// # 错误
/// 无。
fn canonical_v1(snapshot: &PublicationContentSnapshot<'_>) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        snapshot.name,
        encode_optional_text(snapshot.specification),
        snapshot.sales_description,
        snapshot.minimum_purchase_quantity,
        snapshot.sales_price_gross,
        snapshot.sales_tax_rate,
        snapshot.base_unit_code,
        encode_optional_text(snapshot.sales_region),
        snapshot.sale_status.as_str(),
        encode_capabilities(snapshot.product_capabilities),
        snapshot.valid_from.unix_secs(),
        encode_optional_unix(snapshot.valid_to.map(Instant::unix_secs)),
    )
}

/// 按 v1 合同编码可选文本，等价于 `Option<String>` 的 Debug。
///
/// # 参数
/// * `value` - 已规范化的可选文本
///
/// # 返回
/// `None` 返回字面量 `None`；有值返回 `Some("...")`。
///
/// # 错误
/// 无。
fn encode_optional_text(value: Option<&str>) -> String {
    match value {
        None => "None".to_string(),
        Some(text) => format!("Some({text:?})"),
    }
}

/// 按 v1 合同编码可选 Unix 秒，等价于 `Option<i64>` 的 Debug。
///
/// # 参数
/// * `value` - 可选 Unix 秒
///
/// # 返回
/// `None` 返回字面量 `None`；有值返回 `Some(<secs>)`。
///
/// # 错误
/// 无。
fn encode_optional_unix(value: Option<i64>) -> String {
    match value {
        None => "None".to_string(),
        Some(secs) => format!("Some({secs})"),
    }
}

/// 按 v1 合同编码能力清单，等价于 `Vec<ProductCapability>` 的 Debug。
///
/// # 参数
/// * `capabilities` - 已去重的能力清单
///
/// # 返回
/// 返回 `[Cancel, Refund]` 形态的稳定文本。
///
/// # 错误
/// 无。
fn encode_capabilities(capabilities: &[ProductCapability]) -> String {
    let mut encoded = String::from("[");
    for (index, capability) in capabilities.iter().enumerate() {
        if index > 0 {
            encoded.push_str(", ");
        }
        encoded.push_str(capability_debug_name(*capability));
    }
    encoded.push(']');
    encoded
}

/// 返回能力枚举的 v1 Debug 变体名。
///
/// # 参数
/// * `capability` - 商品级能力
///
/// # 返回
/// 返回 `Cancel` / `Refund` / `Logistics`。
///
/// # 错误
/// 无。
fn capability_debug_name(capability: ProductCapability) -> &'static str {
    match capability {
        ProductCapability::Cancel => "Cancel",
        ProductCapability::Refund => "Refund",
        ProductCapability::Logistics => "Logistics",
    }
}

/// 计算 FNV-1a 64 位哈希。
///
/// # 参数
/// * `bytes` - canonical UTF-8 字节
///
/// # 返回
/// 返回 64 位哈希。
///
/// # 错误
/// 无。乘法按 wrapping 溢出，禁止 panic。
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = FNV1A64_OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV1A64_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_v1, capability_debug_name, encode_capabilities, encode_optional_text, encode_optional_unix,
        PublicationContentFingerprint, PublicationContentSnapshot,
    };
    use crate::common::time::Instant;
    use crate::money::{Amount, Quantity, Rate};
    use crate::publication::{ProductCapability, SaleStatus};
    use std::str::FromStr;

    /// 样本修订在 v1 FNV 合同下的 golden 指纹。
    const SAMPLE_V1_GOLDEN: &str = "f2da0094c4fd33b9";

    #[derive(Clone)]
    struct SampleContent {
        name: String,
        specification: Option<String>,
        sales_description: String,
        quantity: String,
        price: String,
        rate: String,
        unit: String,
        region: Option<String>,
        sale_status: SaleStatus,
        capabilities: Vec<ProductCapability>,
        valid_from: i64,
        valid_to: Option<i64>,
    }

    impl SampleContent {
        /// 返回 golden 样本的规范化内容。
        ///
        /// # 返回
        /// 返回与 v1 golden 指纹对应的样本。
        ///
        /// # 错误
        /// 无。
        fn base() -> Self {
            Self {
                name: "福利商城卡".to_string(),
                specification: None,
                sales_description: "员工福利采购".to_string(),
                quantity: "1.000000".to_string(),
                price: "100.00".to_string(),
                rate: "0.130000".to_string(),
                unit: "张".to_string(),
                region: None,
                sale_status: SaleStatus::OnSale,
                capabilities: vec![ProductCapability::Cancel],
                valid_from: 1_700_000_000,
                valid_to: Some(1_800_000_000),
            }
        }

        /// 把样本转成指纹输入快照。
        ///
        /// # 返回
        /// 返回借用自身字段的快照。
        ///
        /// # 错误
        /// 金额或数量文本非法时 panic。
        fn snapshot(&self) -> PublicationContentSnapshot<'_> {
            PublicationContentSnapshot {
                name: &self.name,
                specification: self.specification.as_deref(),
                sales_description: &self.sales_description,
                minimum_purchase_quantity: Quantity::from_str(&self.quantity).unwrap(),
                sales_price_gross: Amount::from_str(&self.price).unwrap(),
                sales_tax_rate: Rate::from_str(&self.rate).unwrap(),
                base_unit_code: &self.unit,
                sales_region: self.region.as_deref(),
                sale_status: self.sale_status,
                product_capabilities: &self.capabilities,
                valid_from: Instant::from_unix_secs(self.valid_from),
                valid_to: self.valid_to.map(Instant::from_unix_secs),
            }
        }

        /// 计算当前样本的 v1 指纹。
        ///
        /// # 返回
        /// 返回 16 位小写十六进制指纹。
        ///
        /// # 错误
        /// 无。
        fn fingerprint(&self) -> String {
            PublicationContentFingerprint::from_snapshot(&self.snapshot()).into_wire()
        }
    }

    #[test]
    fn v1_fingerprint_is_deterministic_hex_and_excludes_placeholders() {
        let sample = SampleContent::base();
        let first = sample.fingerprint();
        let second = PublicationContentFingerprint::from_snapshot(&sample.snapshot());
        assert_eq!(first, second.as_str());
        assert_eq!(first.len(), 16);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(first, first.to_ascii_lowercase());
        assert_ne!(first, "placeholder");
        assert_ne!(first, "pending-safety-pause-hash");
        assert_eq!(
            canonical_v1(&sample.snapshot()),
            "福利商城卡|None|员工福利采购|1.000000|100.00|0.130000|张|None|on_sale|[Cancel]|1700000000|Some(1800000000)"
        );
        assert_eq!(first, SAMPLE_V1_GOLDEN);
    }

    #[test]
    fn each_participating_field_changes_fingerprint() {
        let base = SampleContent::base().fingerprint();
        let mut variants = Vec::new();
        let mut name = SampleContent::base();
        name.name = "另一名称".to_string();
        variants.push(name);
        let mut specification = SampleContent::base();
        specification.specification = Some("100 元面额".to_string());
        variants.push(specification);
        let mut description = SampleContent::base();
        description.sales_description = "另一说明".to_string();
        variants.push(description);
        let mut quantity = SampleContent::base();
        quantity.quantity = "2.000000".to_string();
        variants.push(quantity);
        let mut price = SampleContent::base();
        price.price = "99.00".to_string();
        variants.push(price);
        let mut rate = SampleContent::base();
        rate.rate = "0.060000".to_string();
        variants.push(rate);
        let mut unit = SampleContent::base();
        unit.unit = "件".to_string();
        variants.push(unit);
        let mut region = SampleContent::base();
        region.region = Some("全国".to_string());
        variants.push(region);
        let mut off_sale = SampleContent::base();
        off_sale.sale_status = SaleStatus::OffSale;
        variants.push(off_sale);
        let mut pause = SampleContent::base();
        pause.sale_status = SaleStatus::PauseOrder;
        variants.push(pause);
        let mut refund = SampleContent::base();
        refund.capabilities = vec![ProductCapability::Refund];
        variants.push(refund);
        let mut logistics = SampleContent::base();
        logistics.capabilities = vec![ProductCapability::Logistics];
        variants.push(logistics);
        let mut both = SampleContent::base();
        both.capabilities = vec![ProductCapability::Cancel, ProductCapability::Refund];
        variants.push(both);
        let mut empty = SampleContent::base();
        empty.capabilities.clear();
        variants.push(empty);
        let mut from = SampleContent::base();
        from.valid_from = 1_700_000_001;
        variants.push(from);
        let mut open_ended = SampleContent::base();
        open_ended.valid_to = None;
        variants.push(open_ended);

        for (index, variant) in variants.iter().enumerate() {
            let changed = variant.fingerprint();
            assert_ne!(changed, base, "字段变化 {index} 必须改变指纹");
            assert_eq!(changed.len(), 16);
        }
    }

    #[test]
    fn optional_text_and_capability_encoding_match_v1_debug_contract() {
        assert_eq!(encode_optional_text(None), "None");
        assert_eq!(encode_optional_text(Some("100 元")), "Some(\"100 元\")");
        assert_eq!(encode_optional_unix(None), "None");
        assert_eq!(encode_optional_unix(Some(1_800_000_000)), "Some(1800000000)");
        assert_eq!(encode_capabilities(&[]), "[]");
        assert_eq!(
            encode_capabilities(&[ProductCapability::Cancel, ProductCapability::Refund]),
            "[Cancel, Refund]"
        );
        assert_eq!(capability_debug_name(ProductCapability::Cancel), "Cancel");
        assert_eq!(capability_debug_name(ProductCapability::Refund), "Refund");
        assert_eq!(capability_debug_name(ProductCapability::Logistics), "Logistics");
    }
}
