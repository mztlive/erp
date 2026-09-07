//! 稳定销售行集合的采购任务责任键。

use erp_core::{Error, Result};
use sha2::{Digest, Sha256};

/// 为稳定销售行集合构造采购任务责任键。
///
/// # 参数
/// * `line_ids` - 已按稳定身份排序去重的销售行 ID
///
/// # 返回
/// 返回长度边界安全的 `sales-lines:<sha256>` 责任键。
///
/// # 错误
/// 行集合为空时返回领域错误。
pub fn procurement_responsibility_key(line_ids: &[String]) -> Result<String> {
    if line_ids.is_empty() {
        return Err(Error::from("供给分配任务责任行不能为空"));
    }
    let mut digest = Sha256::new();
    for line_id in line_ids {
        digest.update((line_id.len() as u64).to_be_bytes());
        digest.update(line_id.as_bytes());
    }
    let digest = digest.finalize();
    let encoded = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("sales-lines:{encoded}"))
}

#[cfg(test)]
mod tests {
    use super::procurement_responsibility_key;

    #[test]
    fn responsibility_key_is_stable_and_boundary_safe() {
        let first = procurement_responsibility_key(&["line-1".to_string(), "line-23".to_string()]).unwrap();
        let repeated =
            procurement_responsibility_key(&["line-1".to_string(), "line-23".to_string()]).unwrap();
        let different =
            procurement_responsibility_key(&["line-12".to_string(), "line-3".to_string()]).unwrap();
        assert_eq!(first, repeated);
        assert_ne!(first, different);
        assert!(first.starts_with("sales-lines:"));
        assert!(procurement_responsibility_key(&[]).is_err());
    }
}
