//! 可复用分类核对记录与不可自动采用的安全建议投影。

use std::collections::HashSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::DictionaryCandidate;
use crate::{Error, ProductCategory, ProductKind, Result};

/// 核对时完整公司分类路径中每一级的稳定身份及版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryHierarchyNode {
    pub id: String,
    pub version: u64,
    pub name: String,
    pub parent_id: Option<String>,
    pub product_kind: ProductKind,
}

impl From<&ProductCategory> for CategoryHierarchyNode {
    fn from(category: &ProductCategory) -> Self {
        Self {
            id: category.base.id.clone(),
            version: category.base.version,
            name: category.name.clone(),
            parent_id: category.parent_category_id.as_ref().map(ToString::to_string),
            product_kind: category.product_kind,
        }
    }
}

/// 一次内部确认事实；只追加历史，不改写已建档商品的分类。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryMappingConfirmation {
    pub draft_id: String,
    pub submission_id: String,
    pub source_original_category_path: String,
    pub category_id: String,
    pub category_version: u64,
    pub category_path: String,
    pub hierarchy: Vec<CategoryHierarchyNode>,
    pub confirmed_by: String,
    pub confirmed_at: Instant,
    pub reason: String,
}

/// 一个供应商、原始完整路径和商品类型下的独立乐观锁映射。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct SupplierCategoryMapping {
    #[serde(flatten)]
    pub base: BaseModel,
    pub supplier_id: String,
    pub original_category_path: String,
    pub product_kind: ProductKind,
    pub confirmations: Vec<CategoryMappingConfirmation>,
}

/// 历史匹配始终需要本次人工核对；路径或版本变化必须重新核对。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoryMappingSuggestionStatus {
    ConfirmationRequired,
    RecheckRequired,
}

/// 门户可见的分类建议；不暴露内部人员、理由、任务或其他供应商事实。
#[derive(Debug, Serialize)]
pub struct CategoryMappingSuggestion {
    pub mapping_id: String,
    pub version: u64,
    pub original_category_path: String,
    pub product_kind: ProductKind,
    pub confirmed_category_id: String,
    pub confirmed_category_version: u64,
    pub confirmed_category_path: String,
    pub category: Option<DictionaryCandidate>,
    pub status: CategoryMappingSuggestionStatus,
    pub requires_confirmation: bool,
}

impl SupplierCategoryMapping {
    /// 根据一次真实内部确认建立独立供应商映射。
    /// # 参数
    /// 稳定标识、服务端供应商、原始路径、商品类型及本次核对事实。
    /// # 返回
    /// 初始版本的独立映射，原始输入仍由提报历史保存。
    /// # 错误
    /// 归属、路径、核对理由或完整分类层级非法时拒绝。
    pub(super) fn new(
        id: String,
        supplier_id: String,
        original_category_path: String,
        product_kind: ProductKind,
        confirmation: CategoryMappingConfirmation,
    ) -> Result<Self> {
        require_text(&id, "分类映射标识", 128)?;
        require_text(&supplier_id, "供应商标识", 128)?;
        let original_category_path = source_path(&original_category_path)?.to_string();
        validate_confirmation(&confirmation, &original_category_path, product_kind)?;
        Ok(Self {
            base: BaseModel::new(id),
            supplier_id,
            original_category_path,
            product_kind,
            confirmations: vec![confirmation],
        })
    }

    /// 在精确乐观锁版本下追加核对，不替换以往核对事实。
    /// # 参数
    /// `expected_version` 为当前映射版本，`confirmation` 为新的真实核对。
    /// # 返回
    /// 追加成功；仓储负责递增持久化版本。
    /// # 错误
    /// 版本冲突、重复提交决定或核对事实不一致时拒绝。
    pub(super) fn confirm(
        &mut self,
        expected_version: u64,
        confirmation: CategoryMappingConfirmation,
    ) -> Result<()> {
        if self.base.is_deleted() || expected_version == 0 || self.base.version != expected_version {
            return Err(Error::ConflictError("供应商分类映射已变化，请重新核对".into()));
        }
        validate_confirmation(&confirmation, &self.original_category_path, self.product_kind)?;
        if self.confirmations.iter().any(|row| row.submission_id == confirmation.submission_id) {
            return Err(Error::ConflictError("本次提交已保存分类核对决定".into()));
        }
        self.confirmations.push(confirmation);
        Ok(())
    }

    /// 将历史核对与当前完整有效路径逐级比较，始终返回待人工核对的建议。
    /// # 参数
    /// `current` 为当前仍有效的完整分类链及最小候选；不可用时为空。
    /// # 返回
    /// 供应商安全投影，必须再次人工确认。
    /// # 错误
    /// 映射缺失内部核对历史时拒绝。
    pub(super) fn suggestion(
        &self,
        current: Option<(DictionaryCandidate, Vec<CategoryHierarchyNode>)>,
    ) -> Result<CategoryMappingSuggestion> {
        let confirmed = self
            .confirmations
            .last()
            .ok_or_else(|| Error::Internal("供应商分类映射缺少内部核对事实".into()))?;
        let status = if current
            .as_ref()
            .is_some_and(|(_, nodes)| nodes.as_slice() == confirmed.hierarchy.as_slice())
        {
            CategoryMappingSuggestionStatus::ConfirmationRequired
        } else {
            CategoryMappingSuggestionStatus::RecheckRequired
        };
        Ok(CategoryMappingSuggestion {
            mapping_id: self.base.id.clone(),
            version: self.base.version,
            original_category_path: self.original_category_path.clone(),
            product_kind: self.product_kind,
            confirmed_category_id: confirmed.category_id.clone(),
            confirmed_category_version: confirmed.category_version,
            confirmed_category_path: confirmed.category_path.clone(),
            category: current.map(|(candidate, _)| candidate),
            status,
            requires_confirmation: true,
        })
    }
}

/// 原始完整路径只去首尾空白，不合并名称、父级、分隔符或供应商。
/// # 参数
/// `path` 为供应商原始完整路径。
/// # 返回
/// 保留原含义的去首尾空白借用。
/// # 错误
/// 路径为空或过长时拒绝。
pub(super) fn source_path(path: &str) -> Result<&str> {
    require_text(path, "供应商原始完整分类路径", 10_000)?;
    Ok(path.trim())
}

/// 用已核对的逐级名称生成展示路径，标识与版本另行保留。
/// # 参数
/// `hierarchy` 为从根到目标的分类链。
/// # 返回
/// 完整展示路径。
/// # 错误
/// 无；分类链资格由调用方复验。
pub(super) fn hierarchy_path(hierarchy: &[CategoryHierarchyNode]) -> String {
    hierarchy.iter().map(|node| node.name.as_str()).collect::<Vec<_>>().join(" / ")
}

/// 核对事实必须指向原始分类含义与完整有效公司分类层级。
fn validate_confirmation(
    confirmation: &CategoryMappingConfirmation,
    original_category_path: &str,
    kind: ProductKind,
) -> Result<()> {
    require_text(&confirmation.draft_id, "新品提报标识", 128)?;
    require_text(&confirmation.submission_id, "新品提交标识", 128)?;
    require_text(&confirmation.confirmed_by, "内部核对人", 128)?;
    require_text(&confirmation.reason, "匹配与建档核对说明", 500)?;
    if source_path(&confirmation.source_original_category_path)? != original_category_path {
        return Err(Error::ValidationError("分类核对不得改写原始完整路径".into()));
    }
    validate_hierarchy(&confirmation.hierarchy, kind)?;
    let leaf = confirmation.hierarchy.last().ok_or_else(|| Error::ValidationError("分类层级缺失".into()))?;
    if leaf.id != confirmation.category_id
        || leaf.version != confirmation.category_version
        || hierarchy_path(&confirmation.hierarchy) != confirmation.category_path
    {
        return Err(Error::ValidationError("分类核对目标与完整路径版本不一致".into()));
    }
    if confirmation.confirmed_at.unix_secs() < 0 {
        return Err(Error::ValidationError("分类核对时间非法".into()));
    }
    Ok(())
}

/// 分类链须从根开始，类型一致，身份无环且每一级版本明确。
fn validate_hierarchy(hierarchy: &[CategoryHierarchyNode], kind: ProductKind) -> Result<()> {
    if hierarchy.is_empty() || hierarchy.len() > 32 {
        return Err(Error::ValidationError("分类完整层级非法".into()));
    }
    let mut seen = HashSet::new();
    for (index, node) in hierarchy.iter().enumerate() {
        require_text(&node.id, "分类标识", 128)?;
        require_text(&node.name, "分类名称", 128)?;
        let expected_parent = index.checked_sub(1).map(|parent| hierarchy[parent].id.as_str());
        if node.version == 0
            || node.product_kind != kind
            || node.parent_id.as_deref() != expected_parent
            || !seen.insert(node.id.as_str())
        {
            return Err(Error::ValidationError("分类完整层级身份或版本非法".into()));
        }
    }
    Ok(())
}

/// 校验需要人工确认的必要文本，不修改原始资料。
fn require_text(value: &str, label: &str, limit: usize) -> Result<()> {
    if value.trim().is_empty() || value.trim().chars().count() > limit {
        return Err(Error::ValidationError(format!("{label}为空或过长")));
    }
    Ok(())
}

#[cfg(test)]
#[path = "category_mapping_tests.rs"]
mod tests;
