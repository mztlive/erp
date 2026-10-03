//! 采购关联销售材料二进制读取；只凭本采购详情上下文资格访问精确冻结文件。

use std::result::Result;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::response::Response;
use erp_core::ids::FileAssetId;
use erp_support::FileAssetView;
use erp_workflow::entity::approval_integration::ApprovalMaterialFile;

use super::purchase_reads;
use crate::app_state::AppState;
use crate::core::errors::Error;
use crate::core::handler::file_asset::{asset_download_response, read_asset, revalidate_asset};

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "下载采购关联销售合同及凭证",
    resource = "purchase_order",
    action = "detail"
)]
/// 下载本采购内容精确关联销售版本的合同或建单凭证。
///
/// # 参数
/// * `state` / `actor` - 应用状态与已认证账号。
/// * `order_id` / `asset_id` - 有权查看的采购单与其材料清单文件引用。
/// # 返回
/// 返回经来源、资产内容、治理和审计证明的文件字节，不提供通用销售或合同读取资格。
/// # 错误
/// 采购不可见、跨单据材料、版本变化、内容变化、销毁或隔离均拒绝发送。
pub async fn purchase_sales_material_download(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((order_id, asset_id)): Path<(String, String)>,
) -> Result<Response, Error> {
    let service = purchase_reads(&state);
    let original = service.require_sales_material(&actor, &order_id, &asset_id).await?;
    let (view, bytes) = read_asset(&state, &actor, &asset_id).await?;
    ensure_frozen_asset(&original.file, &view)?;
    let current = service.require_sales_material(&actor, &order_id, &asset_id).await?;
    if current != original {
        return Err(Error::Conflict("采购关联销售资料已变化，请刷新后重试".into()));
    }
    revalidate_asset(&state, &view).await?;
    asset_download_response(&view, bytes)
}

/// 文件领域读取结果必须与采购上下文冻结允许清单完全一致。
fn ensure_frozen_asset(frozen: &ApprovalMaterialFile, view: &FileAssetView) -> Result<(), Error> {
    let current = ApprovalMaterialFile {
        file_asset_id: FileAssetId::new(&view.id),
        file_name: view.file_name.clone(),
        content_type: view.content_type.clone(),
        byte_size: view.byte_size,
        asset_version: view.version,
        content_hmac: view.content_hmac.clone(),
    };
    if !frozen.matches_current(&current) {
        return Err(Error::Conflict("采购关联销售文件已变化，请联系经办人核对".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_support::{FileAsset, RegisterFileAssetRequest, RetentionClass, SensitivityClass};

    use super::*;

    #[test]
    fn file_read_requires_exact_frozen_identity_version_and_content() {
        let request = RegisterFileAssetRequest {
            storage_object_key: "object".into(),
            file_name: "合同.pdf".into(),
            content_type: "application/pdf".into(),
            byte_size: 12,
            content_hmac: "a".repeat(64),
            sensitivity_class: SensitivityClass::Sensitive,
            retention_class: RetentionClass::LongTerm,
            expires_at: None,
        };
        let file = FileAsset::new(FileAssetId::new("file"), request.into_data("actor").unwrap()).unwrap();
        let mut view = FileAssetView::from(file);
        let frozen = ApprovalMaterialFile {
            file_asset_id: FileAssetId::new(&view.id),
            file_name: view.file_name.clone(),
            content_type: view.content_type.clone(),
            byte_size: view.byte_size,
            asset_version: view.version,
            content_hmac: view.content_hmac.clone(),
        };
        assert!(ensure_frozen_asset(&frozen, &view).is_ok());
        view.id = "foreign-file".into();
        assert!(ensure_frozen_asset(&frozen, &view).is_err());
        view.id = frozen.file_asset_id.to_string();
        view.version += 1;
        assert!(ensure_frozen_asset(&frozen, &view).is_err());
        view.version = frozen.asset_version;
        view.content_hmac = "b".repeat(64);
        assert!(ensure_frozen_asset(&frozen, &view).is_err());
    }
}
