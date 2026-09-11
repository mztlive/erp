//! 产品报价表导入的 HTTP DTO；Handler 直接复用本文件类型。

use serde::{Deserialize, Serialize};
use validator::Validate;

use application_core::{page_or_default, page_size_or_default};

use super::common::PageParams;
use crate::error::Result;

/// 「对内」工作表名称。
pub const PRODUCT_IMPORT_SHEET_NAME: &str = "对内";
/// 「对内」表头，共 24 列，必须与原模板第 1 行一致。
pub const PRODUCT_IMPORT_HEADERS: &[&str] = &[
    "产品编码",
    "产品条码",
    "产品主图",
    "产品副图",
    "产品副图",
    "品牌",
    "产品类别",
    "产品类别编码",
    "产品名称",
    "产品规格",
    "一件代发成本价（含税运）",
    "集采成本价（含税）",
    "一件代发底价（含税运）",
    "集采底价（含税）",
    "集采起订量",
    "一件代发快递",
    "市场价",
    "供应商名称",
    "供应商编号",
    "食品产品有效期",
    "生产批次",
    "是否自营产品",
    "自营产品库存",
    "商品税率",
];
/// 产品名称列（1 起）。
pub const PRODUCT_IMPORT_NAME_COLUMN: usize = 9;
/// 默认基础单位代码（开发种子 `JIAN` / 「件」）。
pub const PRODUCT_IMPORT_UNIT_CODE: &str = "JIAN";
/// 默认基础单位名称。
pub const PRODUCT_IMPORT_UNIT_NAME: &str = "件";
/// 产品报价表 xlsx 内容类型（直传合并与提交校验共用）。
pub const PRODUCT_IMPORT_XLSX_MIME: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
/// 产品报价表导入文件上限（含内嵌图片），与网关 multipart 上限保持一致。
pub const MAX_PRODUCT_IMPORT_FILE_BYTES: u64 = 700 * 1024 * 1024;
/// 浏览器直传分片大小；S3 要求除最后一片外每片不小于 5 MiB。
pub const PRODUCT_IMPORT_DIRECT_PART_BYTES: u64 = 8 * 1024 * 1024;
/// 分片预签名地址有效期（秒）；慢速网络下按需重新获取，无需一次签发全部。
pub const PRODUCT_IMPORT_DIRECT_PART_URL_TTL_SECS: u64 = 2 * 60 * 60;

/// 导入任务列表查询。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProductImportJobListParams {
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
}

impl ProductImportJobListParams {
    /// 归一化分页参数。
    ///
    /// # 返回
    /// 返回页码与单页条数。
    ///
    /// # 错误
    /// 无；非法值由 `Validate` 拦截。
    pub fn paging(&self) -> PageParams {
        PageParams {
            page: page_or_default(self.page),
            page_size: page_size_or_default(self.page_size),
            sort_by: "created_at",
            sort_dir: super::SortDir::Desc,
        }
    }
}

/// 导入任务逐项列表查询。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProductImportItemListParams {
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
}

impl ProductImportItemListParams {
    /// 归一化分页参数。
    ///
    /// # 返回
    /// 返回页码与单页条数。
    ///
    /// # 错误
    /// 无。
    pub fn paging(&self) -> (u64, u32) {
        (page_or_default(self.page), page_size_or_default(self.page_size))
    }
}

/// 导入任务进度视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductImportJobView {
    /// 后台任务 ID。
    pub id: String,
    /// 任务编号。
    pub job_no: String,
    /// 任务状态。
    pub status: String,
    /// 源文件名。
    pub file_name: Option<String>,
    /// 目标行数。
    pub total_count: u64,
    /// 已处理行数。
    pub processed_count: u64,
    /// 成功行数。
    pub success_count: u64,
    /// 跳过行数。
    pub skipped_count: u64,
    /// 失败行数。
    pub failed_count: u64,
    /// 开始时间（秒）。
    pub started_at: Option<u64>,
    /// 结束时间（秒）。
    pub finished_at: Option<u64>,
    /// 脱敏错误摘要。
    pub error_summary: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒）。
    pub created_at: u64,
}

/// 导入逐项结果视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductImportItemView {
    /// 逐项序号。
    pub item_no: u32,
    /// Excel 行号。
    pub source_row_no: Option<u32>,
    /// 产品名称。
    pub name: Option<String>,
    /// 执行结果；空表示尚未执行。
    pub status: Option<String>,
    /// 脱敏结果说明。
    pub result_summary: Option<String>,
    /// 成功写入的商品 ID。
    pub product_id: Option<String>,
}

/// 校验导入表头是否为「对内」原模板。
///
/// # 参数
/// * `headers` - 第 1 行单元格
///
/// # 返回
/// 表头完全匹配时返回 `Ok(())`。
///
/// # 错误
/// 列数或列名不一致时返回 `ValidationError`。
pub fn ensure_product_import_headers(headers: &[String]) -> Result<()> {
    if headers.len() < PRODUCT_IMPORT_HEADERS.len() {
        return Err(crate::error::Error::ValidationError(
            "未找到产品报价表「对内」模板，请保留第 1 行原表头".into(),
        ));
    }
    for (index, expected) in PRODUCT_IMPORT_HEADERS.iter().enumerate() {
        let actual = headers.get(index).map(|value| value.trim()).unwrap_or("");
        if actual != *expected {
            return Err(crate::error::Error::ValidationError(format!(
                "第 {} 列应为「{expected}」，请使用原产品报价表模板",
                index + 1
            )));
        }
    }
    Ok(())
}

/// 浏览器直传初始化请求（HTTP 契约：`{ file_name, byte_size, request_id }`）。
///
/// 大文件不再经网关上传：服务端只初始化对象存储分片上传，
/// 浏览器拿分片地址直传对象存储，最后调用合并接口登记导入任务。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProductImportDirectUploadInitRequest {
    /// 展示文件名（须为 `.xlsx`）。
    #[validate(length(min = 1, max = 256, message = "文件名不能为空或过长"))]
    pub file_name: String,
    /// 文件总字节数（须大于 0 且不超过导入上限）。
    #[validate(range(min = 1, message = "文件大小必须大于 0"))]
    pub byte_size: u64,
    /// 幂等请求身份（与导入任务编号绑定）。
    #[validate(length(min = 1, max = 64, message = "请求身份不能为空或过长"))]
    pub request_id: String,
}

/// 浏览器直传初始化视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductImportDirectUploadInitView {
    /// 对象存储分片上传标识。
    pub upload_id: String,
    /// 相对对象键（分片地址与合并接口共用）。
    pub object_key: String,
    /// 每片字节数（最后一片除外）。
    pub part_size: u64,
    /// 总分片数。
    pub total_parts: u32,
    /// 分片地址有效期（秒）；过期前按需重新获取。
    pub part_url_ttl_secs: u64,
}

/// 分片预签名地址视图。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductImportDirectUploadPartView {
    /// 分片 PUT 预签名地址（不携带应用鉴权头）。
    pub url: String,
    /// 地址有效期（秒）。
    pub expires_in_secs: u64,
}

/// 已直传分片（HTTP 契约：`{ part_number, etag }`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate, PartialEq, Eq)]
pub struct ProductImportDirectUploadedPart {
    /// 分片序号（1 起）。
    #[validate(range(min = 1, max = 10000, message = "分片序号必须在 1-10000 之间"))]
    pub part_number: i32,
    /// 分片 PUT 响应 `ETag` 头。
    #[validate(length(min = 1, message = "分片 ETag 不能为空"))]
    pub etag: String,
}

/// 浏览器直传合并请求（HTTP 契约：`{ object_key, file_name, byte_size, request_id, parts }`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProductImportDirectUploadCompleteRequest {
    /// 相对对象键（须与请求身份绑定）。
    #[validate(length(min = 1, message = "对象键不能为空"))]
    pub object_key: String,
    /// 展示文件名（须为 `.xlsx`）。
    #[validate(length(min = 1, max = 256, message = "文件名不能为空或过长"))]
    pub file_name: String,
    /// 文件总字节数（须与对象实际大小一致）。
    #[validate(range(min = 1, message = "文件大小必须大于 0"))]
    pub byte_size: u64,
    /// 幂等请求身份（与导入任务编号绑定）。
    #[validate(length(min = 1, max = 64, message = "请求身份不能为空或过长"))]
    pub request_id: String,
    /// 已直传分片列表。
    #[validate(length(min = 1, max = 10000, message = "分片列表不能为空或过多"))]
    #[validate(nested)]
    pub parts: Vec<ProductImportDirectUploadedPart>,
}

#[cfg(test)]
mod tests {
    use super::{ensure_product_import_headers, PRODUCT_IMPORT_HEADERS};

    #[test]
    fn headers_must_match_internal_sheet() {
        let headers = PRODUCT_IMPORT_HEADERS
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>();
        assert!(ensure_product_import_headers(&headers).is_ok());
        let mut wrong = headers.clone();
        wrong[0] = "编码".into();
        assert!(ensure_product_import_headers(&wrong).is_err());
    }
}
