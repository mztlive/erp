//! 品牌导入协议；持久化整份输入，重试保留 request_id 与原始行号。
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::entity::catalog::ProductBrand;
use crate::entity::catalog::brand_import::BrandImportKey;
use crate::error::{Error, Result};

/// 后台任务至多 1000 行，整份输入一次提交。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrandImportJobRequest {
    pub request_id: String,
    pub file_name: String,
    pub rows: Vec<BrandImportRow>,
}

impl BrandImportJobRequest {
    /// 校验批次边界并解析稳定提交身份。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已验证的提交身份。
    /// # 错误
    /// 批次为空、超量、行号重复或提交身份非法。
    pub fn key(&self) -> Result<BrandImportKey> {
        let key = BrandImportKey::new(&self.request_id)?;
        if self.file_name.len() > 255 || !self.file_name.to_ascii_lowercase().ends_with(".xlsx") {
            return Err(Error::ValidationError("请选择品牌 Excel 文件（.xlsx）".into()));
        }

        let mut numbers = HashSet::new();
        if self.rows.is_empty() || self.rows.len() > 1000 {
            return Err(Error::ValidationError("每次应包含 1–1000 行品牌".into()));
        }

        for row in &self.rows {
            if !(2..=10001).contains(&row.row_number) || !numbers.insert(row.row_number) {
                return Err(Error::ValidationError("Excel 行号无效或重复".into()));
            }
        }
        Ok(key)
    }
}

/// Excel 原始行；名称必填，代码留空时自动取号，统一新建为启用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrandImportRow {
    pub row_number: u32,
    pub name: String,
    #[serde(default)]
    pub brand_code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 单行执行结果。
#[derive(Debug, Serialize)]
pub struct BrandImportResult {
    pub row_number: u32,
    pub status: BrandImportStatus,
    pub message: String,
    pub brand_code: Option<String>,
    pub brand_id: Option<String>,
}

/// 原提交人可下载的未成功源行。
#[derive(Debug, Serialize)]
pub struct BrandImportFailures {
    pub rows: Vec<BrandImportRow>,
    pub results: Vec<BrandImportResult>,
}

/// 导入结果状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrandImportStatus {
    Created,
    Skipped,
    Failed,
}

impl BrandImportResult {
    /// 构造确定失败的行结果。
    /// # 参数
    /// * `row_number` - 原 Excel 行号
    /// * `message` - 可向用户展示的原因
    /// # 返回
    /// 无写入结果引用的失败行。
    /// # 错误
    /// 无。
    pub fn failed(row_number: u32, message: String) -> Self {
        Self { row_number, status: BrandImportStatus::Failed, message, brand_code: None, brand_id: None }
    }

    /// 从已新建或已存在品牌构造导入结果。
    /// # 参数
    /// * `row_number` - 原始行号
    /// * `status` - 新建或跳过状态
    /// * `brand` - 本行品牌
    /// # 返回
    /// 带品牌引用的结果。
    /// # 错误
    /// 无。
    pub(crate) fn from_brand(row_number: u32, status: BrandImportStatus, brand: ProductBrand) -> Self {
        let message =
            if status == BrandImportStatus::Created { "已新建" } else { "已存在，保留原资料" };
        Self {
            row_number,
            status,
            message: format!("{message}，品牌代码：{}", brand.brand_code),
            brand_code: Some(brand.brand_code),
            brand_id: Some(brand.base.id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_accepts_rows_and_rejects_invalid_boundaries() {
        let mut request: BrandImportJobRequest = serde_json::from_value(serde_json::json!({
            "request_id": "12345678-1234-4234-8234-123456789abc",
            "file_name": "品牌.xlsx",
            "rows": [{"row_number": 2, "name": "品牌"}]
        }))
        .unwrap();
        assert!(request.key().is_ok());

        request.rows.push(request.rows[0].clone());
        assert!(request.key().is_err());
        request.rows.clear();
        assert!(request.key().is_err());

        request.rows = (2..1003)
            .map(|row_number| BrandImportRow {
                row_number,
                name: "品牌".into(),
                brand_code: String::new(),
                error: None,
            })
            .collect();
        assert!(request.key().is_err());

        request.rows.truncate(1000);
        assert!(request.key().is_ok());
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(serde_json::from_str::<BrandImportJobRequest>(&json).unwrap().rows.len(), 1000);

        request.file_name = "品牌.csv".into();
        assert!(request.key().is_err());

        request.file_name = "品牌.xlsx".into();
        request.rows.truncate(1);
        request.rows[0].row_number = 1;
        assert!(request.key().is_err());
    }
}
