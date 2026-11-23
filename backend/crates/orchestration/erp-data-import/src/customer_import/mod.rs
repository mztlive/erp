//! 客户批量建档任务及源行结果。
mod execute;
mod source;

use std::sync::Arc;

use erp_identity::AuthorizationService;
use erp_master_data::CustomerProfileProcess;
use erp_party::SensitiveDataCodec;
use erp_permissions::internal;
use mongodb::Database;
use serde::{Deserialize, Serialize};
use storage::S3Storage;

use crate::background::import_source::ImportSourceCache;
use crate::repository::import_job_access::ImportJobAccess;
use crate::{Error, Result};

const DOMAIN: &str = "CUSTOMER_IMPORT";
const MAX_SOURCE_BYTES: usize = 5 * 1024 * 1024;

/// 客户建档工作簿中的一行。
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomerImportRow {
    /// Excel 原始行号，表头为第一行。
    pub row_number: u32,
    /// 客户法定名称。
    pub legal_name: String,
    /// 客户简称。
    pub short_name: Option<String>,
    /// 统一社会信用代码。
    pub unified_credit_code: Option<String>,
    /// 工作簿单元格无法作为有效输入读取时的说明。
    pub error: Option<String>,
}

/// 客户导入任务的不可变输入。
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomerImportRequest {
    /// 客户端本次提交编号，重试保留原值。
    pub request_id: String,
    /// 用户选择的工作簿文件名。
    pub file_name: String,
    /// 按 Excel 行号升序的源行。
    pub rows: Vec<CustomerImportRow>,
}

impl CustomerImportRequest {
    fn validate(&self) -> Result<()> {
        if self.request_id.is_empty()
            || self.request_id.len() > 64
            || !self.request_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(Error::ValidationError("导入提交编号无效，请重新选择文件".into()));
        }
        if self.file_name.trim().is_empty() || self.file_name.chars().count() > 255 {
            return Err(Error::ValidationError("文件名称无效，请调整后重试".into()));
        }
        if self.rows.is_empty() || self.rows.len() > 1000 {
            return Err(Error::ValidationError("每个文件需包含 1–1000 行客户资料".into()));
        }
        let mut previous = 1;
        for row in &self.rows {
            if row.row_number <= previous || row.row_number > 1_048_576 {
                return Err(Error::ValidationError("Excel 行号无效或重复，请重新选择文件".into()));
            }
            previous = row.row_number;
        }
        Ok(())
    }
}

/// 可导出的一条源行和对应的处理说明。
#[derive(Serialize)]
pub struct CustomerImportResultRow {
    /// 原始工作簿数据。
    #[serde(flatten)]
    pub row: CustomerImportRow,
    /// 处理结果说明。
    pub message: String,
}

/// 装配客户批量导入登记、执行和结果读取。
pub struct CustomerImportProcess {
    db: Database,
    profile: CustomerProfileProcess,
    access: ImportJobAccess,
    storage: S3Storage,
    codec: Arc<SensitiveDataCodec>,
    sources: ImportSourceCache<CustomerImportRequest>,
}

impl CustomerImportProcess {
    /// 绑定同一数据库、客户建档流程、授权服务和加密源存储。
    pub fn new(
        db: Database,
        profile: CustomerProfileProcess,
        authorization: Arc<AuthorizationService>,
        storage: S3Storage,
        codec: Arc<SensitiveDataCodec>,
    ) -> Self {
        let access =
            ImportJobAccess::new(db.clone(), authorization, DOMAIN, internal::CUSTOMER_IMPORT_JOB_READ);
        Self { db, profile, access, storage, codec, sources: ImportSourceCache::new(4 * MAX_SOURCE_BYTES) }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn request(numbers: &[u32]) -> CustomerImportRequest {
        serde_json::from_value(json!({
            "request_id": "request-1",
            "file_name": "客户.xlsx",
            "rows": numbers.iter().map(|n| json!({"row_number": n, "legal_name": "某公司"})).collect::<Vec<_>>()
        })).unwrap()
    }

    #[test]
    fn preserves_sheet_row_numbers_and_rejects_ambiguous_or_out_of_range_rows() {
        let input = request(&[2, 9, 101]);
        assert!(input.validate().is_ok());
        assert_eq!(input.rows[1].row_number, 9);
        for rows in [&[][..], &[1], &[2, 2], &[9, 2], &[1_048_577]] {
            assert!(matches!(request(rows).validate(), Err(Error::ValidationError(_))));
        }
        assert!(request(&(2..=1002).collect::<Vec<_>>()).validate().is_err());
    }
}
