use database::{CatalogExt, NoTransaction};
use entities::catalog::sku::{Sku, SkuData};
use entities::catalog::sku_revision::{SkuRevision, SkuRevisionData};
use entities::catalog::specification::{compute_specification_signature, SpecSignatureEntry};
use entities::catalog::{EnableStatus, ListingStatus, ProductId, SkuId, SkuRevisionId};
use entities::common::time::BusinessDate;
use id_generator::next_id;
use mongodb::bson::doc;

use super::CatalogService;
use crate::catalog::dto::{ProductSkuInput, SpecEntryInput};
use crate::errors::{Error, Result};

/// 规格编辑动作（数据模型 §6.3：保留/新增/重新启用/移除）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SkuEditAction {
    /// 全新签名：分配新 SKU 身份并写首个修订。
    Create,
    /// 签名未变：沿用原 `sku_id` 并追加修订。
    Keep,
    /// 历史停用签名再次出现：复用原 `sku_id`、追加修订并显式重新启用。
    Reactivate,
}

/// 规格编辑计划中的一行（`Create`/`Keep`/`Reactivate` 都伴随一条新 SKU 修订）。
pub(super) struct SkuEditItem {
    /// 编辑动作。
    pub(super) action: SkuEditAction,
    /// 待写入的 SKU（新增时为新建；重新启用时为已置 `Active` 的既有实体）。
    pub(super) sku: Sku,
    /// 待写入的 SKU 修订。
    pub(super) revision: SkuRevision,
}

/// 新 SKU 构建上下文（所属 SPU、名称快照、生效区间、操作人）。
pub(super) struct NewSkuContext<'a> {
    /// 所属 SPU。
    pub(super) product_id: &'a ProductId,
    /// 商品名称（作为 SKU 修订名称快照）。
    pub(super) product_name: &'a str,
    /// 生效起始日。
    pub(super) effective_from: BusinessDate,
    /// 生效截止日。
    pub(super) effective_to: Option<BusinessDate>,
    /// 操作人 ID。
    pub(super) created_by: &'a str,
}

impl CatalogService {
    /// 构造新 SKU 行（规范化自由规格 → 计算签名 → 生成 SKU 身份 + 首个修订）。
    ///
    /// # 参数
    /// * `ctx` - 新 SKU 上下文（所属 SPU、名称快照、生效区间、操作人）
    /// * `input` - SKU 输入行
    ///
    /// # 返回
    /// 返回 `Create` 动作的规格编辑行。
    ///
    /// # 错误
    /// 规格名/值非法、签名冲突、条码冲突时返回错误。
    pub(super) async fn build_new_sku_item(
        &self,
        ctx: NewSkuContext<'_>,
        input: ProductSkuInput,
    ) -> Result<SkuEditItem> {
        let signature = specification_signature_for(&input.spec_entries)?;
        self.ensure_barcode_available(&input.barcode, None).await?;
        let sku_id = SkuId::new(next_id());
        let revision_id = SkuRevisionId::new(next_id());
        let revision = SkuRevision::new(
            revision_id.clone(),
            SkuRevisionData {
                sku_id: sku_id.clone(),
                revision_no: 1,
                name: ctx.product_name.to_string(),
                description: None,
                specification: None,
                barcode: input.barcode,
                source_main_image_asset_id: input.main_image_asset_id.clone(),
                weight_kg: input.weight_kg,
                volume_m3: input.volume_m3,
                sales_visible_price_gross: input.sales_visible_price_gross,
                market_price: input.market_price,
                status: EnableStatus::Active,
                effective_from: ctx.effective_from,
                effective_to: ctx.effective_to,
            },
        )?;
        let mut sku = Sku::new(
            sku_id,
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
        sku.stable.current_revision_id = Some(revision.base.id.clone());
        Ok(SkuEditItem {
            action: SkuEditAction::Create,
            sku,
            revision,
        })
    }

    /// 校验条码未被其他在用 SKU 使用（数据模型 §6.3：冲突转人工，不自动合并）。
    ///
    /// # 参数
    /// * `barcode` - 条码原值
    /// * `current_sku_id` - 本次写入归属的 SKU（同 SKU 自身修订不视为冲突）
    ///
    /// # 返回
    /// 可用时返回 `Ok(())`。
    ///
    /// # 错误
    /// 条码已被其他在用 SKU 使用时返回 `BusinessLogicError`。
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
        let active = self
            .db
            .sku_revisions()
            .find_active_by_barcode(barcode, &mut NoTransaction)
            .await?;
        if active
            .iter()
            .any(|revision| revision.sku_id.as_ref() != current_sku_id.unwrap_or_default())
        {
            return Err(Error::BusinessLogicError(format!(
                "条码已被其他在用SKU使用: {barcode}"
            )));
        }
        Ok(())
    }

    /// 计算某 SKU 已有修订的最大序号 + 1（唯一索引兜底并发）。
    ///
    /// # 参数
    /// * `sku_id` - SKU
    ///
    /// # 返回
    /// 返回下一个修订序号。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    pub(super) async fn next_sku_revision_no(&self, sku_id: &str) -> Result<u32> {
        let revisions = self
            .db
            .sku_revisions()
            .find_many(doc! { "sku_id": sku_id }, &mut NoTransaction)
            .await?;
        Ok(revisions
            .iter()
            .map(|revision| revision.revision.revision_no)
            .max()
            .unwrap_or(0)
            + 1)
    }
}

/// 根据请求中的 SPU 局部规格名和值计算 SKU 身份签名。
///
/// # 参数
/// * `entries` - 一个 SKU 的全部规格名和值
///
/// # 返回
/// 返回去除首尾空白、按规格名和值排序后的规范化签名。
///
/// # 错误
/// 规格名/值为空、超长或同一规格名重复时返回验证错误。
pub(super) fn specification_signature_for(entries: &[SpecEntryInput]) -> Result<String> {
    let signature_entries = entries
        .iter()
        .map(|entry| SpecSignatureEntry {
            // 字段名为兼容既有 HTTP 契约保留；业务含义是 SPU 局部规格名和值。
            attribute_code: entry.attribute_code.clone(),
            value_code: entry.attribute_value_code.clone(),
        })
        .collect::<Vec<_>>();
    Ok(compute_specification_signature(&signature_entries)?)
}

/// 校验既有规格行的稳定身份、期望修订与显式重新启用意图。
///
/// # 参数
/// * `existing` - 规范化签名命中的既有稳定 SKU
/// * `input` - 客户端提交的 SKU 行
/// * `reactivating` - 该 SKU 当前是否处于停用状态
/// * `change_reason` - 请求级变更原因
///
/// # 返回
/// 身份、修订与重新启用意图一致时返回 `Ok(())`。
///
/// # 错误
/// 身份缺失/不匹配、期望修订过期、稳定字段被改动，或停用 SKU 未明确重新
/// 启用并填写原因时返回验证或冲突错误。
pub(super) fn ensure_existing_sku_identity(
    existing: &Sku,
    input: &ProductSkuInput,
    reactivating: bool,
    change_reason: Option<&str>,
) -> Result<()> {
    if input.sku_id.as_ref().map(ToString::to_string).as_deref() != Some(existing.base.id.as_str()) {
        return Err(Error::ValidationError(
            "既有规格行必须携带匹配的稳定 sku_id".to_string(),
        ));
    }
    let expected_revision = input.expected_sku_revision_id.as_ref().map(ToString::to_string);
    if expected_revision.as_deref() != existing.stable.current_revision_id.as_deref() {
        return Err(Error::ConflictError(
            "SKU 修订已变化，请刷新商品后重试".to_string(),
        ));
    }
    if input.sku_no.trim() != existing.sku_no || input.base_unit_id != existing.base_unit_id {
        return Err(Error::ValidationError(
            "SKU 编码和基础单位为稳定身份字段，编辑时不得修改".to_string(),
        ));
    }
    if reactivating && (!input.reenable || change_reason.is_none_or(str::is_empty)) {
        return Err(Error::ValidationError(
            "重新启用历史停用 SKU 必须明确 reenable=true 并填写 change_reason".to_string(),
        ));
    }
    if !reactivating && input.reenable {
        return Err(Error::ValidationError(
            "当前启用 SKU 不得提交重新启用意图".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use entities::ids::UnitOfMeasureId;

    fn existing_sku(status: EnableStatus) -> Sku {
        let mut sku = Sku::new(
            SkuId::new("sku-1"),
            SkuData {
                sku_no: "SKU-001".to_string(),
                product_id: ProductId::new("product-1"),
                base_unit_id: UnitOfMeasureId::new("unit-1"),
                specification_signature: "color=red".to_string(),
                status,
                listing_status: ListingStatus::Unlisted,
            },
            "tester",
        )
        .unwrap();
        sku.stable.current_revision_id = Some("sku-rev-2".to_string());
        sku
    }

    fn existing_input(reenable: bool) -> ProductSkuInput {
        ProductSkuInput {
            sku_id: Some(SkuId::new("sku-1")),
            expected_sku_revision_id: Some(SkuRevisionId::new("sku-rev-2")),
            reenable,
            sku_no: "SKU-001".to_string(),
            base_unit_id: UnitOfMeasureId::new("unit-1"),
            barcode: None,
            main_image_asset_id: None,
            weight_kg: None,
            volume_m3: None,
            sales_visible_price_gross: None,
            market_price: None,
            spec_entries: Vec::new(),
        }
    }

    #[test]
    fn existing_sku_requires_exact_current_revision() {
        let sku = existing_sku(EnableStatus::Active);
        let mut input = existing_input(false);
        input.expected_sku_revision_id = Some(SkuRevisionId::new("stale-revision"));

        assert!(ensure_existing_sku_identity(&sku, &input, false, None).is_err());
    }

    #[test]
    fn disabled_sku_requires_explicit_reenable_and_reason() {
        let sku = existing_sku(EnableStatus::Disabled);

        assert!(ensure_existing_sku_identity(&sku, &existing_input(false), true, None).is_err());
        assert!(ensure_existing_sku_identity(&sku, &existing_input(true), true, Some("恢复销售")).is_ok());
    }

    /// 商品规格直接使用请求中的 SPU 局部名称和值，不查询全局规格字典。
    #[test]
    fn specification_signature_uses_spu_local_spec_text() {
        let entries = vec![
            SpecEntryInput {
                attribute_code: "颜色".to_string(),
                attribute_value_code: "红色".to_string(),
            },
            SpecEntryInput {
                attribute_code: "尺码".to_string(),
                attribute_value_code: "L".to_string(),
            },
        ];

        assert_eq!(specification_signature_for(&entries).unwrap(), "尺码=L|颜色=红色");
    }
}
