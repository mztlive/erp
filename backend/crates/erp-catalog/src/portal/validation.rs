//! 新品原始输入、明确审核映射和最终建档结果的纯规则。

use std::collections::HashSet;

use erp_core::common::time::Instant;
use erp_core::validation::non_empty_trimmed;

use super::{
    CatalogDraftResult, CatalogDraftSkuResult, CategoryHierarchyNode, DictionaryInput, DraftDecision,
    DraftSku, DraftStatus, NewProductInput, NormalizedProduct, NormalizedSku, UnitSynonymConfirmation,
};
use crate::entity::catalog::{ListingStatus, SpecSignatureEntry, SpecificationSignatureSet};
use crate::{Error, ProductKind, Result, SpecEntryInput};

const ID_MAX_LEN: usize = 128;
const NAME_MAX_LEN: usize = 128;
const DESCRIPTION_MAX_LEN: usize = 10_000;
const MAX_SKUS: usize = 100;
const MAX_ASSETS: usize = 100;

impl NewProductInput {
    /// 正式提报每条 SKU 必须有自身图片或可共同引用的商品公共图片。
    /// # 参数
    /// 无；原始图片标识保持不变，实际素材归属由文件 Port 重验。
    /// # 返回
    /// 每条 SKU 均有明确可追溯图片来源时成功。
    /// # 错误
    /// 图片标识无效、缺 SKU 或任一 SKU 无自身及公共图时拒绝。
    pub fn ensure_submission_images(&self) -> Result<()> {
        validate_assets(&self.image_asset_ids)?;
        if self.skus.is_empty() {
            return Err(validation("新品至少需要一个 SKU"));
        }
        for sku in &self.skus {
            optional_id(sku.image_asset_id.as_deref(), "SKU 图片")?;
            if self.image_asset_ids.is_empty() && sku.image_asset_id.is_none() {
                return Err(validation(&format!("SKU {} 须补充自身图片或商品公共图片后提交", sku.row_id)));
            }
        }
        Ok(())
    }

    /// 校验提交完整性，允许充分的未匹配字典原始资料。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 完整且无重复规格或订货编码时返回空结果。
    /// # 错误
    /// 缺失、非法、重复或超长资料时返回验证错误；商务条款另由供给 Port 校验。
    pub fn validate_submission(&self) -> Result<()> {
        self.validate_storage()?;
        required(&self.name, "商品名称", NAME_MAX_LEN)?;
        self.brand.validate_required("品牌")?;
        self.category.validate_required("分类")?;
        if self.skus.is_empty() {
            return Err(validation("新品至少需要一个 SKU"));
        }
        let mut signatures = SpecificationSignatureSet::new();
        let mut ordering_codes = HashSet::new();
        for sku in &self.skus {
            sku.validate_submission()?;
            if !ordering_codes.insert(sku.ordering_code.trim()) {
                return Err(validation("SKU 订货编码重复"));
            }
            let entries = signature_entries(&sku.spec_entries);
            signatures.register(&entries).map_err(|error| validation(&error.to_string()))?;
        }
        Ok(())
    }

    /// 核对内部映射与本次原始提交严格对应。
    ///
    /// # 参数
    /// * `normalized` - 内部明确确认的字典及已有 SKU 映射。
    /// # 返回
    /// 每行完整对应且仅整理名称首尾空白时返回空结果。
    /// # 错误
    /// 改变名称含义、遗漏或重复行、替换已选择字典或版本不一致时返回错误。
    pub fn ensure_normalized(&self, normalized: &NormalizedProduct) -> Result<()> {
        self.validate_submission()?;
        ensure_name(&self.name, &normalized.name)?;
        self.brand.ensure_resolved(&normalized.brand_id, normalized.brand_version, "品牌")?;
        self.category.ensure_resolved(&normalized.category_id, normalized.category_version, "分类")?;
        ensure_category_hierarchy(normalized, self.product_kind)?;
        if normalized.sku_mappings.len() != self.skus.len() {
            return Err(validation("审核映射必须完整包含本次全部 SKU"));
        }
        let mut rows = HashSet::new();
        let mut targets = HashSet::new();
        for mapping in &normalized.sku_mappings {
            if !rows.insert(mapping.row_id.as_str()) {
                return Err(validation("审核映射包含重复 SKU 行"));
            }
            let sku = self
                .skus
                .iter()
                .find(|sku| sku.row_id == mapping.row_id)
                .ok_or_else(|| validation("审核映射包含非本次提交的 SKU 行"))?;
            sku.ensure_normalized(mapping)?;
            if let Some(target) = &mapping.target_sku
                && !targets.insert(target.sku_id.as_str())
            {
                return Err(validation("多条提报规格不能重复匹配同一 SKU"));
            }
        }
        Ok(())
    }

    /// 草稿存储校验只检查边界，允许尚未填写的必填资料。
    pub(super) fn validate_storage(&self) -> Result<()> {
        bounded(&self.name, "商品名称", NAME_MAX_LEN)?;
        self.brand.validate_storage("品牌")?;
        self.category.validate_storage("分类")?;
        optional_text(self.model.as_deref(), "型号", NAME_MAX_LEN)?;
        optional_text(self.description.as_deref(), "描述", 512)?;
        validate_assets(&self.image_asset_ids)?;
        validate_assets(&self.file_asset_ids)?;
        if self.skus.len() > MAX_SKUS {
            return Err(validation("一张新品提报最多包含 100 个 SKU"));
        }
        let mut rows = HashSet::new();
        for sku in &self.skus {
            sku.validate_storage()?;
            if !rows.insert(sku.row_id.as_str()) {
                return Err(validation("SKU 行标识重复"));
            }
        }
        Ok(())
    }
}

impl DictionaryInput {
    /// 所选 ID 与版本必须同时存在，原稿不做 trim 写回。
    pub(super) fn validate_storage(&self, field: &str) -> Result<()> {
        bounded(&self.raw_name, field, DESCRIPTION_MAX_LEN)?;
        match (&self.selected_id, self.expected_version) {
            (Some(id), Some(version)) if version > 0 => validate_id(id, field),
            (None, None) => Ok(()),
            _ => Err(validation(&format!("{field}标识与有效版本必须成对提供"))),
        }
    }

    /// 未匹配字典使用明确原始资料提交，不默认成未知或无品牌。
    fn validate_required(&self, field: &str) -> Result<()> {
        self.validate_storage(field)?;
        required(&self.raw_name, field, DESCRIPTION_MAX_LEN)?;
        Ok(())
    }

    /// 明确选择的字典不得通过改映射绕过提交时版本。
    fn ensure_resolved(&self, id: &str, version: u64, field: &str) -> Result<()> {
        validate_id(id, field)?;
        if version == 0 {
            return Err(validation(&format!("{field}审核版本必须为正数")));
        }
        if let Some(selected) = &self.selected_id
            && (selected != id || self.expected_version != Some(version))
        {
            return Err(Error::ConflictError(format!("{field}所选对象或版本已变化，请重新核对")));
        }
        Ok(())
    }
}

impl DraftSku {
    /// 保存时只验证输入尺寸及显式引用的结构。
    pub(super) fn validate_storage(&self) -> Result<()> {
        validate_id(&self.row_id, "SKU 行标识")?;
        bounded(&self.name, "SKU 名称", NAME_MAX_LEN)?;
        self.unit.validate_storage("单位")?;
        optional_text(self.barcode.as_deref(), "条码", NAME_MAX_LEN)?;
        optional_id(self.image_asset_id.as_deref(), "SKU 图片")?;
        bounded(&self.ordering_code, "订货编码", ID_MAX_LEN)?;
        optional_text(self.quote_basis.as_deref(), "原始报价口径", 512)?;
        if let Some(packaging) = &self.packaging {
            packaging.validate_storage()?;
        }
        if self.spec_entries.len() > 100 {
            return Err(validation("SKU 规格属性过多"));
        }
        for entry in &self.spec_entries {
            bounded(&entry.attribute_code, "规格名", 64)?;
            bounded(&entry.attribute_value_code, "规格值", 64)?;
        }
        if self.available_quantity.is_some_and(|quantity| quantity.to_decimal().is_sign_negative()) {
            return Err(validation("可供数量不能为负数"));
        }
        if self.reported_at.unix_secs() < 0 {
            return Err(validation("供应商填报时间无效"));
        }
        Ok(())
    }

    /// 条款仅检查为对象，其商务校验归供给领域。
    fn validate_submission(&self) -> Result<()> {
        required(&self.name, "SKU 名称", NAME_MAX_LEN)?;
        self.unit.validate_required("单位")?;
        required(&self.ordering_code, "订货编码", ID_MAX_LEN)?;
        for entry in &self.spec_entries {
            if entry.attribute_code.contains(['|', '=']) || entry.attribute_value_code.contains('|') {
                return Err(validation(&format!(
                    "SKU {} 规格名不得包含 | 或 =，规格值不得包含 |",
                    self.row_id
                )));
            }
        }
        if let Some(quote_basis) = &self.quote_basis {
            required(quote_basis, "原始报价口径", 512)?;
        }
        if let Some(packaging) = &self.packaging {
            packaging.validate_submission(&self.unit.raw_name)?;
        }
        if !self.supply_terms.is_object() {
            return Err(validation("SKU 首次供给条款不能为空且必须为对象"));
        }
        Ok(())
    }

    /// 映射不能替代原始规格、单位含义或商务输入。
    fn ensure_normalized(&self, mapping: &NormalizedSku) -> Result<()> {
        ensure_name(&self.name, &mapping.name)?;
        self.unit.ensure_resolved(&mapping.unit_id, mapping.unit_version, "单位")?;
        if let Some(confirmation) = &mapping.unit_synonym_confirmation {
            confirmation.ensure_for(&self.unit.raw_name)?;
        }
        if let Some(target) = &mapping.target_sku {
            validate_id(&target.sku_id, "匹配 SKU")?;
            validate_id(&target.revision_id, "匹配 SKU 修订")?;
            if target.version == 0 {
                return Err(validation("匹配 SKU 版本必须为正数"));
            }
        }
        Ok(())
    }
}

impl UnitSynonymConfirmation {
    /// 同义核对只接受完整原稿的明确确认，不允许换算或改写原文。
    /// # 参数
    /// `original_unit` 为本行供应商原始基础单位文本。
    /// # 返回
    /// 明确同义确认和完整核对依据有效时成功。
    /// # 错误
    /// 原文变化、未明确确认或核对依据为空及过长时拒绝。
    pub(crate) fn ensure_for(&self, original_unit: &str) -> Result<()> {
        if self.original_unit != original_unit || !self.same_unit_meaning_confirmed {
            return Err(validation("单位同义映射须明确确认本行原始基础单位含义"));
        }
        required(&self.reason, "单位同义核对依据", 500)?;
        Ok(())
    }
}

/// 审核证据必须包含从根到选中叶的完整同类型链。
fn ensure_category_hierarchy(normalized: &NormalizedProduct, kind: ProductKind) -> Result<()> {
    let nodes = &normalized.category_hierarchy;
    if nodes.is_empty() || nodes.len() > 32 {
        return Err(validation("分类映射须冻结完整根到叶路径"));
    }
    let mut seen = HashSet::new();
    let mut parent_id = None;
    for node in nodes {
        ensure_category_node(node, kind)?;
        if !seen.insert(node.id.as_str()) || node.parent_id.as_deref() != parent_id {
            return Err(validation("分类映射冻结路径不完整或存在循环"));
        }
        parent_id = Some(node.id.as_str());
    }
    let leaf = nodes.last().ok_or_else(|| validation("分类映射路径缺失"))?;
    if leaf.id != normalized.category_id || leaf.version != normalized.category_version {
        return Err(validation("分类映射路径与所选叶分类不一致"));
    }
    Ok(())
}

/// 每级节点均须是可追溯且类型一致的核对事实。
fn ensure_category_node(node: &CategoryHierarchyNode, kind: ProductKind) -> Result<()> {
    validate_id(&node.id, "分类路径节点")?;
    required(&node.name, "分类路径名称", NAME_MAX_LEN)?;
    if node.version == 0 || node.product_kind != kind {
        return Err(validation("分类路径节点版本或商品类型无效"));
    }
    Ok(())
}

/// 核对最终结果与全部映射行及复用目标一致。
pub(super) fn ensure_result(normalized: &NormalizedProduct, result: &CatalogDraftResult) -> Result<()> {
    validate_id(&result.product_id, "结果商品")?;
    if result.skus.len() != normalized.sku_mappings.len() {
        return Err(validation("新品生效结果必须完整包含全部 SKU"));
    }
    let mut rows = HashSet::new();
    let mut sku_ids = HashSet::new();
    for sku in &result.skus {
        if !rows.insert(sku.row_id.as_str()) || !sku_ids.insert(sku.sku_id.as_str()) {
            return Err(validation("新品生效结果包含重复 SKU"));
        }
        validate_id(&sku.sku_id, "结果 SKU")?;
        validate_id(&sku.revision_id, "结果 SKU 修订")?;
        let offering_id = sku.offering_id.as_deref().ok_or_else(|| validation("新品生效结果缺少供给"))?;
        validate_id(offering_id, "结果供给")?;
        let mapping = normalized
            .sku_mappings
            .iter()
            .find(|mapping| mapping.row_id == sku.row_id)
            .ok_or_else(|| validation("新品生效结果包含非本次提交的 SKU"))?;
        ensure_sku_result(mapping, sku, result.product_created)?;
    }
    Ok(())
}

/// 新建 SKU 明确未上架，已存在 SKU 仅能返回明确匹配的身份。
fn ensure_sku_result(
    mapping: &NormalizedSku,
    result: &CatalogDraftSkuResult,
    product_created: bool,
) -> Result<()> {
    if result.sku_created && result.listing_status != ListingStatus::Unlisted {
        return Err(validation("本次新建 SKU 必须保持未上架"));
    }
    match &mapping.target_sku {
        Some(target)
            if product_created
                || result.sku_created
                || result.sku_id != target.sku_id
                || result.revision_id != target.revision_id =>
        {
            Err(validation("新品结果与明确匹配的已有 SKU 不一致"))
        },
        None if !result.sku_created => Err(validation("复用已有 SKU 必须显式提供匹配决定")),
        _ => Ok(()),
    }
}

/// 构造决定时验证实际操作人，禁止失败后才发现缺失事实。
pub(super) fn decision(
    status: DraftStatus,
    reason: Option<String>,
    actor_id: &str,
    at: Instant,
) -> Result<DraftDecision> {
    validate_id(actor_id, "决定人")?;
    Ok(DraftDecision {
        status,
        reason,
        decided_by: actor_id.to_string(),
        decided_at: at,
        normalized_product: None,
        result: None,
    })
}

/// 重用公司商品规格的唯一实现，不另建门户签名算法。
fn signature_entries(entries: &[SpecEntryInput]) -> Vec<SpecSignatureEntry> {
    entries
        .iter()
        .map(|entry| SpecSignatureEntry {
            attribute_code: entry.attribute_code.clone(),
            value_code: entry.attribute_value_code.clone(),
        })
        .collect()
}

/// 名称格式整理仅去首尾空白，实质名称修改须重新提交。
fn ensure_name(raw: &str, normalized: &str) -> Result<()> {
    if raw.trim() != normalized || normalized.is_empty() {
        return Err(validation("内部名称只允许整理首尾空白，实质变化须退回供应商确认"));
    }
    Ok(())
}

/// 必填文本只返回规范值供判断，不改写供应商原稿。
pub(super) fn required(value: &str, field: &str, max_len: usize) -> Result<String> {
    let normalized = non_empty_trimmed(value, &format!("{field}不能为空"))
        .map_err(|error| validation(&error.to_string()))?;
    bounded(&normalized, field, max_len)?;
    Ok(normalized)
}

/// 统一文本长度检查。
pub(super) fn bounded(value: &str, field: &str, max_len: usize) -> Result<()> {
    if value.chars().count() > max_len {
        return Err(validation(&format!("{field}过长")));
    }
    Ok(())
}

/// 可空文本仍受明确的长度约束。
fn optional_text(value: Option<&str>, field: &str, max_len: usize) -> Result<()> {
    if let Some(value) = value {
        bounded(value, field, max_len)?;
    }
    Ok(())
}

/// 稳定标识非空、长度有限且不包含首尾空白。
pub(super) fn validate_id(value: &str, field: &str) -> Result<()> {
    let normalized = required(value, field, ID_MAX_LEN)?;
    if normalized != value {
        return Err(validation(&format!("{field}标识不能含首尾空白")));
    }
    Ok(())
}

/// 可选关联存在时仍必须提供有效标识。
fn optional_id(value: Option<&str>, field: &str) -> Result<()> {
    if let Some(value) = value {
        validate_id(value, field)?;
    }
    Ok(())
}

/// 一组媒体引用不能重复或超过允许数量；访问权由文件 Port 重验。
fn validate_assets(ids: &[String]) -> Result<()> {
    if ids.len() > MAX_ASSETS {
        return Err(validation("新品图片或附件数量过多"));
    }
    let mut seen = HashSet::new();
    for id in ids {
        validate_id(id, "图片或附件")?;
        if !seen.insert(id) {
            return Err(validation("新品图片或附件引用重复"));
        }
    }
    Ok(())
}

/// 门户纯规则错误使用验证分类，不按内部错误返回。
pub(super) fn validation(message: &str) -> Error {
    Error::ValidationError(message.to_string())
}
