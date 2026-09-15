//! 供应商后台导入的提交与失败清单合同。
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::import::{SupplierImportResult, SupplierImportRow};
use crate::{Error, Result};

/// 同一请求身份只允许提交同一份文件内容。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplierImportJobRequest {
    pub request_id: String,
    pub file_name: String,
    pub rows: Vec<SupplierImportRow>,
}

impl SupplierImportJobRequest {
    /// 校验批次边界，行内业务错误留给后台逐行处理。
    ///
    /// 返回校验结果；空批次、重复行号或超限输入返回校验错误。
    pub fn validate(&self) -> Result<()> {
        if self.request_id.is_empty()
            || self.request_id.len() > 128
            || !self.request_id.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        {
            return Err(Error::ValidationError("导入请求标识无效".into()));
        }
        if self.file_name.len() > 255 || !self.file_name.to_ascii_lowercase().ends_with(".xlsx") {
            return Err(Error::ValidationError("请选择供应商 Excel 文件（.xlsx）".into()));
        }
        if self.rows.is_empty() || self.rows.len() > 500 {
            return Err(Error::ValidationError("每次导入应为 1–500 行".into()));
        }
        let mut numbers = HashSet::new();
        if self.rows.iter().any(|row| row.row_number < 2 || !numbers.insert(row.row_number)) {
            return Err(Error::ValidationError("Excel 行号无效或重复".into()));
        }
        Ok(())
    }
}

/// 仅向原提交人返回待修正行；明文不得进入任务列表或日志。
#[derive(Debug, Serialize)]
pub struct SupplierImportFailures {
    pub rows: Vec<SupplierImportRow>,
    pub results: Vec<SupplierImportResult>,
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;

    use super::*;
    fn request() -> SupplierImportJobRequest {
        SupplierImportJobRequest {
            request_id: "req-1".into(),
            file_name: "供应商.xlsx".into(),
            rows: vec![SupplierImportRow {
                row_number: 2,
                cells: vec![String::new(); 23],
                party_no: "PTY-1".into(),
                supplier_no: "SUP-1".into(),
                effective_from: BusinessDate::from_ymd(2026, 9, 11).unwrap(),
                parse_errors: vec![],
            }],
        }
    }
    #[test]
    fn accepts_incomplete_rows_for_background_validation_and_roundtrips() {
        let req = request();
        assert!(req.validate().is_ok());
        let restored: SupplierImportJobRequest =
            serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        assert_eq!(restored.rows[0].cells.len(), 23);
    }
    #[test]
    fn rejects_empty_oversized_duplicate_rows_and_invalid_identity() {
        let mut req = request();
        req.rows.clear();
        assert!(req.validate().is_err());
        req = request();
        req.rows.push(req.rows[0].clone());
        assert!(req.validate().is_err());
        req = request();
        req.rows = vec![req.rows[0].clone(); 501];
        assert!(req.validate().is_err());
        req = request();
        req.request_id = "../key".into();
        assert!(req.validate().is_err());
        req = request();
        req.file_name = "supplier.csv".into();
        assert!(req.validate().is_err());
        req = request();
        req.rows[0].row_number = 1;
        assert!(req.validate().is_err());
    }
}
