//! 产品报价表异步导入。

use application_core::AuditActor;
use axum::{
    extract::{Multipart, Path, Query, State},
    Extension,
};
use erp_catalog::{
    PageView, ProductImportItemListParams, ProductImportItemView, ProductImportJobListParams,
    ProductImportJobView,
};
use erp_support::{RetentionClass, SensitivityClass};

use crate::{
    app_state::AppState,
    core::{
        errors::{Error, Result},
        handler::file_asset::{delete_pending_asset_objects, store_asset_file, AssetFile},
        response::ApiResponse,
        upload,
    },
};

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "导入商品报价表",
    resource = "product",
    action = "create"
)]
/// 上传产品报价表并登记异步导入任务。
///
/// 前台只做任务投递，不直接执行；执行统一由后台任务执行器认领，避免与轮询器并发写入同一任务。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 审计操作人
/// * `multipart` - 含 `file` 与可选 `request_id` 的表单
///
/// # 返回
/// 返回导入任务进度视图，可轮询任务详情与逐项结果查看执行进度。
///
/// # 错误
/// 文件不是原模板、超过大小上限或解析失败时返回错误。
pub async fn product_import_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<ProductImportJobView> {
    let extracted = extract_import_form(&mut multipart).await?;
    let pending = store_asset_file(
        &state,
        extracted.file,
        SensitivityClass::General,
        RetentionClass::LongTerm,
        None,
    )
    .await?;
    let result = state
        .product_import_process()
        .submit(
            extracted.file_name,
            extracted.bytes,
            pending.clone(),
            extracted.request_id,
            &actor,
        )
        .await;
    match result {
        Ok(view) => Ok(ApiResponse::ok_with_data(view)),
        Err(error) => {
            delete_pending_asset_objects(
                &state,
                &[erp_support::PendingFileAssetRequest {
                    reference: "pending-file:import".into(),
                    registration: pending,
                }],
            )
            .await;
            Err(error.into())
        }
    }
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询商品导入任务",
    resource = "product",
    action = "list"
)]
/// 查询当前操作人的商品导入任务。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 审计操作人
/// * `params` - 分页参数
///
/// # 返回
/// 返回任务分页。
pub async fn product_import_job_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<ProductImportJobListParams>,
) -> Result<PageView<ProductImportJobView>> {
    let view = state.product_import_process().job_list(&params, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询商品导入任务详情",
    resource = "product",
    action = "list"
)]
/// 查询单个商品导入任务。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 审计操作人
/// * `id` - 任务 ID
///
/// # 返回
/// 返回任务详情。
pub async fn product_import_job_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<ProductImportJobView> {
    let view = state.product_import_process().job_detail(&id, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "查询商品导入逐项结果",
    resource = "product",
    action = "list"
)]
/// 查询导入任务逐项结果。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 审计操作人
/// * `id` - 任务 ID
/// * `params` - 分页参数
///
/// # 返回
/// 返回逐项分页。
pub async fn product_import_job_items(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<ProductImportItemListParams>,
) -> Result<PageView<ProductImportItemView>> {
    let view = state
        .product_import_process()
        .job_items(&id, &params, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

struct ExtractedImport {
    file_name: String,
    bytes: Vec<u8>,
    file: AssetFile,
    request_id: String,
}

async fn extract_import_form(multipart: &mut Multipart) -> std::result::Result<ExtractedImport, Error> {
    let mut file_name = None;
    let mut bytes = None;
    let mut request_id = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|_| Error::BadRequest("Multipart 表单无效".into()))?
    {
        let name = field.name().unwrap_or_default().to_string();
        if name == "request_id" {
            request_id = Some(
                field
                    .text()
                    .await
                    .map_err(|_| Error::BadRequest("请求身份读取失败".into()))?,
            );
            continue;
        }
        if field.file_name().is_none() {
            continue;
        }
        let uploaded_name = field
            .file_name()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::BadRequest("请选择产品报价表文件".into()))?
            .to_string();
        let mut content = Vec::new();
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|_| Error::BadRequest("上传文件读取失败".into()))?
        {
            if content.len().saturating_add(chunk.len()) > upload::MAX_PRODUCT_IMPORT_XLSX_BYTES {
                return Err(Error::BadRequest("导入文件不能超过 700 MB".into()));
            }
            content.extend_from_slice(&chunk);
        }
        file_name = Some(uploaded_name);
        bytes = Some(content);
    }
    let file_name = file_name.ok_or_else(|| Error::BadRequest("请选择产品报价表文件".into()))?;
    let bytes = bytes.ok_or_else(|| Error::BadRequest("请选择产品报价表文件".into()))?;
    validate_xlsx(&file_name, &bytes)?;
    let content_type = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string();
    let request_id = request_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(id_generator::next_id);
    Ok(ExtractedImport {
        file: AssetFile {
            file_name: file_name.clone(),
            content_type,
            content: bytes.clone(),
        },
        file_name,
        bytes,
        request_id,
    })
}

fn validate_xlsx(file_name: &str, bytes: &[u8]) -> std::result::Result<(), Error> {
    let extension = upload::normalized_extension(file_name).unwrap_or_default();
    if extension != "xlsx" {
        return Err(Error::BadRequest("请上传 .xlsx 产品报价表".into()));
    }
    if bytes.len() < 4 || &bytes[..2] != b"PK" {
        return Err(Error::BadRequest("文件不是有效的 Excel 工作簿".into()));
    }
    Ok(())
}
