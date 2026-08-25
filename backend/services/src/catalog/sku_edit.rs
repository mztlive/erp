use database::{CatalogExt, NoTransaction};
use entities::catalog::sku::{Sku, SkuData, SkuEditAction, SkuEditIdentity, SkuEditIdentityError};
use entities::catalog::sku_revision::{SkuRevision, SkuRevisionData};
use entities::catalog::specification::{compute_specification_signature, SpecSignatureEntry};
use entities::catalog::{next_revision_no, EnableStatus, ListingStatus, ProductId, SkuId, SkuRevisionId};
use entities::common::time::BusinessDate;
use id_generator::next_id;

use super::CatalogService;
use crate::catalog::dto::{ProductSkuInput, SpecEntryInput};
use crate::errors::{Error, Result};

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
    /// 构造新 SKU 行。
    ///
    /// 规范化自由规格、校验新身份意图与条码占用，并创建稳定 SKU 及首个修订。
    ///
    /// # 参数
    /// * `ctx` - 新 SKU 所属商品、生效区间与创建人
    /// * `input` - SKU 输入行
    ///
    /// # 返回
    /// 返回 `Create` 动作的规格编辑行。
    ///
    /// # 错误
    /// 身份、规格、条码或实体字段违反规则，以及仓储查询失败时返回错误。
    pub(super) async fn build_new_sku_item(
        &self,
        ctx: NewSkuContext<'_>,
        input: ProductSkuInput,
    ) -> Result<SkuEditItem> {
        new_sku_edit_identity(&input)
            .ensure_new()
            .map_err(map_sku_edit_error)?;
        let signature = specification_signature_for(&input.spec_entries)?;
        self.ensure_barcode_available(&input.barcode, None).await?;
        let revision = build_initial_sku_revision(&input, ctx.effective_from, ctx.effective_to)?;
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
        Ok(SkuEditItem {
            action: SkuEditAction::Create,
            sku,
            revision,
        })
    }

    /// 校验条码未被其他在用 SKU 使用。
    ///
    /// # 参数
    /// * `barcode` - 条码原值
    /// * `current_sku_id` - 本次写入归属 SKU；同一 SKU 自身修订不视为冲突
    ///
    /// # 返回
    /// 条码为空或未被其他 SKU 占用时返回 `Ok(())`。
    ///
    /// # 错误
    /// 条码已被其他在用 SKU 使用，或仓储查询失败时返回错误。
    pub(super) async fn ensure_barcode_available(
        &self,
        barcode: &Option<String>,
        current_sku_id: Option<&str>,
    ) -> Result<()> {
        let Some(barcode) = barcode
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(());
        };
        let owners = self
            .db
            .catalog()
            .barcode_owner_sku_ids(barcode, &mut NoTransaction)
            .await?;
        if owners
            .iter()
            .any(|sku_id| sku_id != current_sku_id.unwrap_or_default())
        {
            return Err(Error::BusinessLogicError(format!(
                "条码已被其他在用SKU使用: {barcode}"
            )));
        }
        Ok(())
    }

    /// 计算某 SKU 应写入的下一修订序号。
    ///
    /// # 参数
    /// * `sku_id` - SKU 稳定 ID
    ///
    /// # 返回
    /// 返回从 1 开始、按历史最大修订号递增的下一序号。
    ///
    /// # 错误
    /// 仓储查询失败或修订序号达到上限时返回错误。
    pub(super) async fn next_sku_revision_no(&self, sku_id: &SkuId) -> Result<u32> {
        let latest = self
            .db
            .catalog()
            .latest_sku_revision_no(sku_id, &mut NoTransaction)
            .await?;
        Ok(next_revision_no(latest)?)
    }
}

/// 构造新 SKU 的首个修订。
///
/// # 参数
/// * `input` - 新 SKU 输入
/// * `effective_from` / `effective_to` - 首个修订生效区间
///
/// # 返回
/// 返回使用新稳定 SKU ID、修订号 1 的不可变 SKU 修订。
///
/// # 错误
/// SKU 修订字段违反实体不变式时返回错误。
fn build_initial_sku_revision(
    input: &ProductSkuInput,
    effective_from: BusinessDate,
    effective_to: Option<BusinessDate>,
) -> Result<SkuRevision> {
    let sku_id = SkuId::new(next_id());
    Ok(SkuRevision::new(
        SkuRevisionId::new(next_id()),
        SkuRevisionData {
            sku_id,
            revision_no: 1,
            name: input.name.clone(),
            description: None,
            specification: None,
            barcode: input.barcode.clone(),
            source_main_image_asset_id: input.main_image_asset_id.clone(),
            weight_kg: input.weight_kg,
            volume_m3: input.volume_m3,
            sales_visible_price_gross: input.sales_visible_price_gross,
            market_price: input.market_price,
            status: EnableStatus::Active,
            effective_from,
            effective_to,
        },
    )?)
}

/// 根据请求中的 SPU 局部规格名和值计算 SKU 身份签名。
///
/// # 参数
/// * `entries` - 一个 SKU 的全部规格名和值
///
/// # 返回
/// 返回去除首尾空白并按规格名和值排序后的规范化签名。
///
/// # 错误
/// 规格名值为空、超长或同一规格名重复时返回领域错误。
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

/// 构造既有 SKU 编辑身份值对象。
///
/// # 参数
/// * `input` - 客户端提交的 SKU 行
/// * `change_reason` - 请求级变更原因
///
/// # 返回
/// 返回借用输入字段的 SKU 编辑身份快照。
///
/// # 错误
/// 无；具体身份规则由 [`Sku::classify_edit`] 校验。
pub(super) fn existing_sku_edit_identity<'a>(
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

/// 构造新 SKU 身份值对象。
///
/// # 参数
/// * `input` - 客户端提交的新 SKU 行
///
/// # 返回
/// 返回用于校验“不得猜测既有身份”的借用快照。
///
/// # 错误
/// 无；具体身份规则由 [`SkuEditIdentity::ensure_new`] 校验。
fn new_sku_edit_identity(input: &ProductSkuInput) -> SkuEditIdentity<'_> {
    SkuEditIdentity {
        sku_id: input.sku_id.as_ref(),
        expected_revision_id: input.expected_sku_revision_id.as_ref(),
        sku_no: &input.sku_no,
        base_unit_id: &input.base_unit_id,
        reenable: input.reenable,
        change_reason: None,
    }
}

/// 把 SKU 身份领域错误映射为稳定的 Service 错误语义。
///
/// # 参数
/// * `error` - SKU 身份规则错误
///
/// # 返回
/// 修订过期映射为 409 冲突，其余输入意图错误映射为参数验证错误。
///
/// # 错误
/// 无；本方法只做错误边界适配。
pub(super) fn map_sku_edit_error(error: SkuEditIdentityError) -> Error {
    match error {
        SkuEditIdentityError::RevisionConflict => Error::ConflictError(error.to_string()),
        _ => Error::ValidationError(error.to_string()),
    }
}
