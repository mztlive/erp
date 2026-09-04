//! 导入行批量工厂适配器（INT-E24：行级规范化与批内唯一归属领域层）。
//!
//! Service 只注入批次 ID 与预分配行 ID，本模块独占字符串规范化、
//! 批内 `(object_type, row_key)` 唯一判定与行实体装配。无 I/O、时钟、
//! ID 生成器或密钥。

use std::collections::HashSet;

use crate::errors::{Error, Result};
use crate::ids::{LegacyImportBatchId, LegacyImportRowId};

use super::{LegacyImportRow, LegacyImportRowData};

/// 单行导入行装配规格（已由 DTO 校验形态，领域层再做规范化与唯一判定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRowSpec {
    /// 行实体主键（由 Service 预分配并注入）。
    pub row_id: LegacyImportRowId,
    /// 来源对象类型。
    pub source_object_type: String,
    /// 批次内来源行身份。
    pub source_row_key: String,
    /// 规范化载荷引用。
    pub normalized_payload_reference: String,
}

/// 按批次装配导入行实体（INT-E24 行工厂适配器）。
///
/// 逐行经 [`LegacyImportRow::new`] 规范化文本并拒绝空/超长；批内
/// `(source_object_type, source_row_key)` 重复失败关闭，保证唯一索引
/// 冲突不在写入时才暴露。空规格返回空集合。
///
/// # 参数
/// * `batch_id` - 所属导入批次
/// * `specs` - 行装配规格（保持调用方顺序）
///
/// # 返回
/// 返回与规格顺序一致的行实体。
///
/// # 错误
/// 当任一规格非法或批内来源身份重复时返回错误。
///
/// # 约束
/// 纯内存装配；不访问 MongoDB、时钟、ID 生成器或密钥。
pub fn build_import_rows(
    batch_id: &LegacyImportBatchId,
    specs: Vec<ImportRowSpec>,
) -> Result<Vec<LegacyImportRow>> {
    let mut seen = HashSet::new();
    let mut rows = Vec::with_capacity(specs.len());
    for spec in specs {
        let key = (
            spec.source_object_type.trim().to_string(),
            spec.source_row_key.trim().to_string(),
        );
        if !seen.insert(key) {
            return Err(Error::from("同一批次内来源行身份重复"));
        }
        rows.push(LegacyImportRow::new(
            spec.row_id,
            LegacyImportRowData {
                batch_id: batch_id.clone(),
                source_object_type: spec.source_object_type,
                source_row_key: spec.source_row_key,
                normalized_payload_reference: spec.normalized_payload_reference,
            },
        )?);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::{build_import_rows, ImportRowSpec};
    use crate::ids::{LegacyImportBatchId, LegacyImportRowId};

    fn spec(id: &str, object: &str, key: &str) -> ImportRowSpec {
        ImportRowSpec {
            row_id: LegacyImportRowId::new(id),
            source_object_type: object.to_string(),
            source_row_key: key.to_string(),
            normalized_payload_reference: format!("payload:{id}"),
        }
    }

    #[test]
    fn trims_and_rejects_blank_or_duplicate_rows() {
        let batch = LegacyImportBatchId::new("batch-1");
        let rows = build_import_rows(&batch, vec![spec("row-1", " CUSTOMER ", " key-1 ")]).unwrap();
        assert_eq!(rows[0].source_object_type, "CUSTOMER");
        assert_eq!(rows[0].source_row_key, "key-1");

        assert!(build_import_rows(&batch, vec![spec("row-1", "CUSTOMER", "   ")]).is_err());
        assert!(build_import_rows(
            &batch,
            vec![
                spec("row-1", "CUSTOMER", "key-1"),
                spec("row-2", "CUSTOMER", "key-1"),
            ],
        )
        .is_err());
        assert!(build_import_rows(&batch, vec![]).unwrap().is_empty());
    }

    #[test]
    fn same_key_across_object_types_is_distinct() {
        let batch = LegacyImportBatchId::new("batch-1");
        let rows = build_import_rows(
            &batch,
            vec![
                spec("row-1", "CUSTOMER", "key-1"),
                spec("row-2", "CONTRACT", "key-1"),
            ],
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
    }
}
