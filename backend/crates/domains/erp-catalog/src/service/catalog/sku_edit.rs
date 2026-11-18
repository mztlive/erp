use erp_core::common::time::BusinessDate;
use id_generator::next_id;
use persistence_core::Executor;

use super::CatalogService;
use crate::dto::{ProductSkuInput, SpecEntryInput};
use crate::entity::catalog::sku::{Sku, SkuData, SkuEditAction, SkuEditIdentity, SkuEditIdentityError};
use crate::entity::catalog::sku_revision::{SkuRevision, SkuRevisionData};
use crate::entity::catalog::specification::{SpecSignatureEntry, compute_specification_signature};
use crate::entity::catalog::{
    EnableStatus, ListingStatus, ProductId, SkuId, SkuRevisionId, next_revision_no,
};
use crate::error::{Error, Result};
use crate::repository::CatalogExt;

/// 规格编辑计划中的一行；每种动作都伴随一条新 SKU 修订。
pub(super) struct SkuEditItem {
    /// 编辑动作。
    pub(super) action: SkuEditAction,
    /// 待写入的 SKU。
    pub(super) sku: Sku,
    /// 待写入的 SKU 修订。
    pub(super) revision: SkuRevision,
}

/// 新 SKU 构建上下文。
pub(super) struct NewSkuContext<'a> {
    /// 所属 SPU。
    pub(super) product_id: &'a ProductId,
    /// 生效起始日。
    pub(super) effective_from: BusinessDate,
    /// 生效截止日。
    pub(super) effective_to: Option<BusinessDate>,
    /// 操作人 ID。
    pub(super) created_by: &'a str,
}

impl CatalogService {
    // SKU 编辑确认规范签名从未存在时，核验新建意图、规格及条码占用，构造稳定 SKU 与首个修订。
    // 原执行器用于查询条码；校验失败时不生成编辑项，本步骤不写库。
    pub(super) async fn build_new_sku_item(
        &self,
        ctx: NewSkuContext<'_>,
        input: ProductSkuInput,
        executor: &mut dyn Executor,
    ) -> Result<SkuEditItem> {
        sku_edit_identity(&input, None).ensure_new().map_err(map_sku_edit_error)?;
        let signature = specification_signature_for(&input.spec_entries)?;
        self.ensure_barcode_available(&input.barcode, None, executor).await?;

        let revision =
            sku_revision_for_input(SkuId::new(next_id()), 1, ctx.effective_from, ctx.effective_to, &input)?;
        let mut sku = Sku::new(
            SkuId::new(revision.sku_id.to_string()),
            SkuData {
                sku_no: input.sku_no,
                product_id: ctx.product_id.clone(),
                base_unit_id: input.base_unit_id,
                specification_signature: signature,
                status: EnableStatus::Active,
                listing_status: ListingStatus::Unlisted,
            },
            ctx.created_by,
        )?;
        sku.attach_revision(&revision, ctx.created_by)?;
        Ok(SkuEditItem { action: SkuEditAction::Create, sku, revision })
    }

    // 生成 SKU 修订前，沿用原执行器检查非空条码是否被其他在用 SKU 占用。
    // 归属本 SKU 的占用不冲突；他方占用或仓储读取失败时拒绝。
    pub(super) async fn ensure_barcode_available(
        &self,
        barcode: &Option<String>,
        current_sku_id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let Some(barcode) = barcode.as_deref().map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(());
        };
        let owners = self.db.catalog().barcode_owner_sku_ids(barcode, executor).await?;
        if owners.iter().any(|sku_id| sku_id != current_sku_id.unwrap_or_default()) {
            return Err(Error::BusinessLogicError(format!("条码已被其他在用SKU使用: {barcode}")));
        }
        Ok(())
    }

    // 计算某 SKU 应写入的下一修订序号。
    //
    // 返回从 1 开始、按历史最大修订号递增的下一序号。仓储查询失败或修订序号达到上限时返回错误。
    //
    // 沿用调用方的执行器，不另开事务。
    pub(super) async fn next_sku_revision_no(
        &self,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<u32> {
        let latest = self.db.catalog().latest_sku_revision_no(sku_id, executor).await?;
        Ok(next_revision_no(latest)?)
    }
}

// 新建、保留或重新启用 SKU 时，用本次输入的名称、条码、主图、物流属性和价格构造不可变修订。
// 首修订用新 SKU ID 和序号 1；追加用原 SKU ID 和下一序号。修订标为 `Active`，停用只改稳定 SKU 主表。
pub(super) fn sku_revision_for_input(
    sku_id: SkuId,
    revision_no: u32,
    effective_from: BusinessDate,
    effective_to: Option<BusinessDate>,
    input: &ProductSkuInput,
) -> Result<SkuRevision> {
    Ok(SkuRevision::new(
        SkuRevisionId::new(next_id()),
        SkuRevisionData {
            sku_id,
            revision_no,
            name: input.name.clone(),
            description: None,
            specification: None,
            barcode: input.barcode.clone(),
            source_main_image_asset_id: input.main_image_asset_id.clone(),
            weight_kg: input.weight_kg,
            volume_m3: input.volume_m3,
            sales_visible_price_gross: input.sales_visible_price_gross,
            bulk_price_gross: input.bulk_price_gross,
            bulk_min_quantity: input.bulk_min_quantity,
            market_price: input.market_price,
            status: EnableStatus::Active,
            effective_from,
            effective_to,
        },
    )?)
}

// 根据请求中的 SPU 局部规格名和值计算 SKU 身份签名。
//
// 返回去除首尾空白并按规格名和值排序后的规范化签名。规格名值为空、超长或同一规格名重复时返回领域错误。
pub(super) fn specification_signature_for(entries: &[SpecEntryInput]) -> Result<String> {
    let signature_entries = entries
        .iter()
        .map(|entry| SpecSignatureEntry {
            attribute_code: entry.attribute_code.clone(),
            value_code: entry.attribute_value_code.clone(),
        })
        .collect::<Vec<_>>();
    Ok(compute_specification_signature(&signature_entries)?)
}

// 规划 SKU 编辑时借用请求中的身份字段；新建不传变更原因，编辑沿用请求级原因。
// 身份意图与当前修订是否匹配由 `Sku::classify_edit` 核验。
pub(super) fn sku_edit_identity<'a>(
    input: &'a ProductSkuInput,
    change_reason: Option<&'a str>,
) -> SkuEditIdentity<'a> {
    SkuEditIdentity {
        sku_id: input.sku_id.as_ref(),
        expected_revision_id: input.expected_sku_revision_id.as_ref(),
        sku_no: &input.sku_no,
        base_unit_id: &input.base_unit_id,
        reenable: input.reenable,
        change_reason,
    }
}

// 把 SKU 身份领域错误映射为稳定的 Service 错误语义。
//
// 修订过期映射为 409 冲突，其余输入意图错误映射为参数验证错误。本方法只做错误边界适配。
pub(super) fn map_sku_edit_error(error: SkuEditIdentityError) -> Error {
    match error {
        SkuEditIdentityError::RevisionConflict => Error::ConflictError(error.to_string()),
        _ => Error::ValidationError(error.to_string()),
    }
}
