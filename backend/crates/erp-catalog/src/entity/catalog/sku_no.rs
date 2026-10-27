//! 商品内 SKU 编号序列：商品的 `SPU` 前缀替换为 `SKU`，再追加 `-NN`。
//!
//! 内部建档、规格编辑与产品报价表导入共用同一编号规则；`sku_no` 全局唯一
//! 由唯一索引兜底，本序列只负责在所属商品内跳过已占用编号。

use std::collections::HashSet;

/// 商品内 SKU 编号序列。
///
/// 商品编号以 `SPU` 开头时替换为 `SKU`，其他商品编号加 `SKU-` 前缀。
/// 追加至少两位的规格序号，超过 99 时展开位数；新旧格式占用的序号均须跳过。
#[derive(Debug, Clone)]
pub struct SkuNoSequence {
    /// 所属商品编号。
    product_no: String,
    /// 新 SKU 编号的商品前缀。
    sku_prefix: String,
    /// 已占用的 SKU 编号（含本序列已分配的编号）。
    taken: HashSet<String>,
    /// 下一个候选序号。
    next_index: u32,
}

impl SkuNoSequence {
    /// 创建 SKU 编号序列。
    ///
    /// # 参数
    /// * `product_no` - 所属商品编号
    /// * `taken` - 已占用的 SKU 编号（既有 SKU 与本次显式填写的编号）
    ///
    /// # 返回
    /// 返回从 `-01` 开始取号的序列。
    ///
    /// # 错误
    /// 无。
    pub fn new(product_no: &str, taken: impl IntoIterator<Item = String>) -> Self {
        let product_no = product_no.trim();
        let sku_prefix = match product_no.strip_prefix("SPU") {
            Some(suffix) => format!("SKU{suffix}"),
            None => format!("SKU-{product_no}"),
        };
        Self {
            product_no: product_no.to_string(),
            sku_prefix,
            taken: taken.into_iter().map(|sku_no| sku_no.trim().to_string()).collect(),
            next_index: 1,
        }
    }

    /// 取下一个未占用编号，并登记为已占用。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回以 `SKU` 开头、以 `-NN` 规格序号结尾的编号。
    ///
    /// # 错误
    /// 无。
    pub fn next_no(&mut self) -> String {
        loop {
            let candidate = format!("{}-{:02}", self.sku_prefix, self.next_index);
            let legacy = format!("{}-{:02}", self.product_no, self.next_index);
            self.next_index = self.next_index.saturating_add(1);
            if !self.taken.contains(&legacy) && self.taken.insert(candidate.clone()) {
                return candidate;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SkuNoSequence;

    #[test]
    fn next_no_starts_from_01_and_increments() {
        let mut sequence = SkuNoSequence::new("SPU20261013-000001", Vec::new());

        assert_eq!(sequence.next_no(), "SKU20261013-000001-01");
        assert_eq!(sequence.next_no(), "SKU20261013-000001-02");
    }

    #[test]
    fn next_no_skips_taken_codes_after_trimming() {
        let mut sequence =
            SkuNoSequence::new(" FSY-1 ", vec![" FSY-1-01 ".to_string(), "FSY-1-03".to_string()]);

        assert_eq!(sequence.next_no(), "SKU-FSY-1-02");
        assert_eq!(sequence.next_no(), "SKU-FSY-1-04");
    }

    #[test]
    fn next_no_continues_product_sequence_with_legacy_source_codes_present() {
        let mut sequence = SkuNoSequence::new(
            " SPU20261027-000002 ",
            vec![
                "SKU-legacy-internal-id".into(),
                " SPU20261027-000002-01 ".into(),
                "SKU20261027-000002-03".into(),
            ],
        );
        assert_eq!(sequence.next_no(), "SKU20261027-000002-02");
        assert_eq!(sequence.next_no(), "SKU20261027-000002-04");
    }

    #[test]
    fn next_no_expands_beyond_two_digits() {
        let taken = (1..=99).map(|index| format!("SKU20261027-000004-{index:02}"));
        let mut sequence = SkuNoSequence::new("SPU20261027-000004", taken);

        assert_eq!(sequence.next_no(), "SKU20261027-000004-100");
    }
}
