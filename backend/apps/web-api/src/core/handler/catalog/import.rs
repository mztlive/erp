//! 产品报价表异步导入。

use application_core::AuditActor;
use axum::{
    extract::{Json, Multipart, Path, Query, State},
    Extension,
};
use erp_catalog::{
    PageView, ProductImportDirectUploadCompleteRequest, ProductImportDirectUploadInitRequest,
    ProductImportDirectUploadInitView, ProductImportDirectUploadPartView, ProductImportItemListParams,
    ProductImportItemView, ProductImportJobListParams, ProductImportJobView,
};
use erp_support::{RetentionClass, SensitivityClass};
use serde::Deserialize;

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

/// 分片地址查询（`object_key` 与 `request_id` 须绑定）。
#[derive(Debug, Deserialize)]
pub struct DirectUploadPartQuery {
    object_key: String,
    request_id: String,
}

/// 取消直传查询（`object_key` 须与 `request_id` 绑定）。
#[derive(Debug, Deserialize)]
pub struct DirectUploadAbortQuery {
    object_key: String,
    request_id: String,
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "初始化商品直传",
    resource = "product",
    action = "create"
)]
/// 初始化浏览器直传：在对象存储创建分片上传。
///
/// 大文件不再经网关上传：本接口只返回分片上传标识、对象键与分片口径，
/// 浏览器凭分片地址直传对象存储，全程可在前端展示进度。
///
/// # 参数
/// * `state` - 应用状态
/// * `req` - 文件名、总字节数与幂等请求身份
///
/// # 返回
/// 返回分片上传标识、对象键与分片口径。
///
/// # 错误
/// 文件名不是 `.xlsx` 或大小越限时返回错误。
pub async fn product_import_direct_upload_init(
    State(state): State<AppState>,
    Json(req): Json<ProductImportDirectUploadInitRequest>,
) -> Result<ProductImportDirectUploadInitView> {
    let view = state.product_import_process().init_direct_upload(req).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "签发商品直传分片地址",
    resource = "product",
    action = "create"
)]
/// 为单个分片签发浏览器可直接 PUT 的预签名地址。
///
/// 地址按需获取：有效期约 2 小时，慢速网络可在上传该片前重新获取，
/// 无需初始化时一次签发全部地址。
///
/// # 参数
/// * `state` - 应用状态
/// * `upload_id` - 分片上传标识
/// * `part_number` - 分片序号（1 起）
/// * `params` - 对象键与请求身份（须绑定）
///
/// # 返回
/// 返回预签名 PUT 地址。
pub async fn product_import_direct_upload_part_url(
    State(state): State<AppState>,
    Path((upload_id, part_number)): Path<(String, i32)>,
    Query(params): Query<DirectUploadPartQuery>,
) -> Result<ProductImportDirectUploadPartView> {
    let url = state
        .product_import_process()
        .direct_upload_part_url(&params.object_key, &upload_id, part_number, &params.request_id)
        .await?;
    Ok(ApiResponse::ok_with_data(ProductImportDirectUploadPartView {
        url,
        expires_in_secs: erp_catalog::PRODUCT_IMPORT_DIRECT_PART_URL_TTL_SECS,
    }))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "合并商品直传并登记导入任务",
    resource = "product",
    action = "create"
)]
/// 合并浏览器已直传的分片并登记异步导入任务。
///
/// 请求体为小 JSON（对象键、文件名、大小、请求身份与分片 ETag），
/// 不再携带文件内容；合并后复用既有解析与落库链路，进度与结果仍在
/// 「后台任务」查看。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 审计操作人
/// * `upload_id` - 分片上传标识
/// * `req` - 对象键、文件名、总字节数、请求身份与已上传分片
///
/// # 返回
/// 返回导入任务进度视图。
///
/// # 错误
/// 分片缺失、对象损坏或解析失败时返回错误。
pub async fn product_import_direct_upload_complete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(upload_id): Path<String>,
    Json(req): Json<ProductImportDirectUploadCompleteRequest>,
) -> Result<ProductImportJobView> {
    let view = state
        .product_import_process()
        .submit_direct_upload(&upload_id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "商品与仓库",
    group_desc = "公司商品池、商品、类目、供应商与仓库基础资料",
    desc = "取消商品直传",
    resource = "product",
    action = "create"
)]
/// 取消分片上传并清理对象存储侧已上传的分片。
///
/// # 参数
/// * `state` - 应用状态
/// * `upload_id` - 分片上传标识
/// * `params` - 对象键与请求身份（须绑定）
///
/// # 返回
/// 成功无返回体；上传已不存在视为成功。
pub async fn product_import_direct_upload_abort(
    State(state): State<AppState>,
    Path(upload_id): Path<String>,
    Query(params): Query<DirectUploadAbortQuery>,
) -> Result<()> {
    state
        .product_import_process()
        .abort_direct_upload(&params.object_key, &upload_id, &params.request_id)
        .await?;
    Ok(ApiResponse::ok())
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
