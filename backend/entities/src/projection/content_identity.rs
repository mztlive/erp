//! 执行投影版本内容身份：canonical 编码与 FNV-1a 64 指纹。
//!
//! 保持既有持久化合同：对固定字段顺序的 UTF-8 canonical 文本做 FNV-1a 64，
//! 写入 16 位小写十六进制，不带算法前缀。字段顺序冻结；算法或编码变更必须
//! 提供兼容读取，不得破坏已持久化幂等语义。

use crate::common::time::Instant;
use crate::money::Amount;

use super::revision::{CardForm, ProjectionSource};

/// v1 内容指纹的持久化长度（16 位小写十六进制）。
const V1_WIRE_LEN: usize = 16;
/// FNV-1a 64 偏移基数。
const FNV1A64_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64 质数。
const FNV1A64_PRIME: u64 = 0x0000_0100_0000_01b3;

/// 参与投影内容指纹的规范化快照。
///
/// 调用方必须先完成商城标识规范化；本结构只负责稳定编码，不再次清洗。
/// 不包含修订号、投影 ID、生效时间或占位指纹。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionContentSnapshot<'a> {
    /// 投影来源。
    pub projection_source: ProjectionSource,
    /// ERP 销售版本稳定身份。
    pub sales_order_revision_id: &'a str,
    /// 已规范化的商城客户标识。
    pub customer_external_identity: &'a str,
    /// 已规范化的商城卡券类目标识。
    pub voucher_category_external_identity: &'a str,
    /// 表头履约期限。
    pub voucher_expiry_at: Instant,
    /// 卡券面额。
    pub face_value: Amount,
    /// 卡张数。
    pub card_count: u32,
    /// 电子卡或实体卡。
    pub card_form: CardForm,
}

/// 投影版本内容指纹（v1：FNV-1a 64 的 16 位小写十六进制）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectionContentFingerprint(String);

impl ProjectionContentFingerprint {
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
    /// 不得包含修订号、投影 ID、生效时间或占位值。
    pub fn from_snapshot(snapshot: &ProjectionContentSnapshot<'_>) -> Self {
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
fn canonical_v1(snapshot: &ProjectionContentSnapshot<'_>) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        snapshot.projection_source.as_str(),
        snapshot.sales_order_revision_id,
        snapshot.customer_external_identity,
        snapshot.voucher_category_external_identity,
        snapshot.voucher_expiry_at.unix_secs(),
        snapshot.face_value,
        snapshot.card_count,
        snapshot.card_form.as_str(),
    )
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
    use super::super::revision::{CardForm, ProjectionSource};
    use super::{canonical_v1, ProjectionContentFingerprint, ProjectionContentSnapshot};
    use crate::common::time::Instant;
    use crate::money::Amount;
    use std::str::FromStr;

    /// 样本修订在 v1 FNV 合同下的 golden 指纹。
    const SAMPLE_V1_GOLDEN: &str = "f5629b90e1e80d37";
    /// 与既有 Service helper 一致的 v1 canonical 文本。
    const SAMPLE_V1_CANONICAL: &str =
        "cutover_snapshot|so-rev-1|mall-customer-001|mall-voucher-001|1800000000|100.00|100|electronic";

    fn sample_snapshot() -> ProjectionContentSnapshot<'static> {
        ProjectionContentSnapshot {
            projection_source: ProjectionSource::CutoverSnapshot,
            sales_order_revision_id: "so-rev-1",
            customer_external_identity: "mall-customer-001",
            voucher_category_external_identity: "mall-voucher-001",
            voucher_expiry_at: Instant::from_unix_secs(1_800_000_000),
            face_value: Amount::from_str("100.00").unwrap(),
            card_count: 100,
            card_form: CardForm::Electronic,
        }
    }

    #[test]
    fn v1_fingerprint_matches_golden_canonical_and_excludes_placeholders() {
        let snapshot = sample_snapshot();
        let first = ProjectionContentFingerprint::from_snapshot(&snapshot);
        let second = ProjectionContentFingerprint::from_snapshot(&snapshot);
        assert_eq!(canonical_v1(&snapshot), SAMPLE_V1_CANONICAL);
        assert_eq!(first.as_str(), second.as_str());
        assert_eq!(first.as_str().len(), 16);
        assert!(first.as_str().bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(first.as_str(), first.as_str().to_ascii_lowercase());
        assert_ne!(first.as_str(), "placeholder");
        assert_ne!(first.as_str(), "pending");
        assert_eq!(first.into_wire(), SAMPLE_V1_GOLDEN);
    }

    #[test]
    fn each_content_field_changes_fingerprint() {
        let base = ProjectionContentFingerprint::from_snapshot(&sample_snapshot()).into_wire();
        let mut variants = Vec::new();

        let mut source = sample_snapshot();
        source.projection_source = ProjectionSource::ErpRevision;
        variants.push(source);

        let mut sales_revision = sample_snapshot();
        sales_revision.sales_order_revision_id = "so-rev-2";
        variants.push(sales_revision);

        let mut customer = sample_snapshot();
        customer.customer_external_identity = "mall-customer-002";
        variants.push(customer);

        let mut category = sample_snapshot();
        category.voucher_category_external_identity = "mall-voucher-002";
        variants.push(category);

        let mut expiry = sample_snapshot();
        expiry.voucher_expiry_at = Instant::from_unix_secs(1_800_000_001);
        variants.push(expiry);

        let mut face = sample_snapshot();
        face.face_value = Amount::from_str("99.00").unwrap();
        variants.push(face);

        let mut count = sample_snapshot();
        count.card_count = 101;
        variants.push(count);

        let mut form = sample_snapshot();
        form.card_form = CardForm::Physical;
        variants.push(form);

        for (index, variant) in variants.iter().enumerate() {
            let changed = ProjectionContentFingerprint::from_snapshot(variant).into_wire();
            assert_ne!(changed, base, "内容字段变化 {index} 必须改变指纹");
            assert_eq!(changed.len(), 16);
        }
    }
}
