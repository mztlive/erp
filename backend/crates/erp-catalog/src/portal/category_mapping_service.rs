//! 分类映射只由正式内部确认保存，查询结果仅供下一次人工核对。

use persistence_core::Executor;

use super::category_mapping::{hierarchy_path, source_path};
use super::repository::category_mapping_filter;
use super::{
    CatalogPortalExt, CatalogPortalService, CategoryHierarchyNode, CategoryMappingConfirmation,
    CategoryMappingSuggestion, DictionaryCandidate, DraftStatus, NewProductDraft, SupplierCategoryMapping,
};
use crate::{CatalogExt, Error, ProductKind, Result};

impl CatalogPortalService {
    /// 查询当前供应商原始完整分类路径的历史核对建议，绝不自动应用。
    /// # 参数
    /// `supplier_id` 必须来自当前门户身份或已授权申请归属；路径和类型为当前原始输入。
    /// `executor` 为本次读取执行器。
    /// # 返回
    /// 无独立映射时为空；存在时返回历史路径与当前有效候选，始终要求人工确认。
    /// # 错误
    /// 空路径、映射事实损坏或数据库错误时拒绝。
    pub async fn category_mapping_suggestion(
        &self,
        supplier_id: &str,
        original_category_path: &str,
        product_kind: ProductKind,
        executor: &mut dyn Executor,
    ) -> Result<Option<CategoryMappingSuggestion>> {
        if supplier_id.trim().is_empty() {
            return Err(Error::ValidationError("供应商归属缺失".into()));
        }
        let path = source_path(original_category_path)?;
        let mapping = self
            .db
            .supplier_category_mappings()
            .find_one(category_mapping_filter(supplier_id, path, product_kind), executor)
            .await?;
        let Some(mapping) = mapping else { return Ok(None) };
        let confirmed = mapping
            .confirmations
            .last()
            .ok_or_else(|| Error::Internal("供应商分类映射缺少内部核对事实".into()))?;
        let current = self.current_mapping_category(&confirmed.category_id, product_kind, executor).await?;
        Ok(Some(mapping.suggestion(current)?))
    }

    /// 内部生效决定与分类映射共用调用方事务，冻结原始资料保持不变。
    /// # 参数
    /// `draft` 为已通过纯规则核对的本次生效决定；`executor` 为Process事务。
    /// # 返回
    /// 分类核对记录已建立或追加。
    /// # 错误
    /// 冻结资料或决定缺失、字典失效、映射并发冲突或数据库错误时拒绝。
    pub(super) async fn record_category_mapping(
        &self,
        draft: &NewProductDraft,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let confirmation = self.category_confirmation(draft, executor).await?;
        let path = source_path(&confirmation.source_original_category_path)?.to_string();
        let kind = draft
            .frozen_input
            .as_ref()
            .ok_or_else(|| Error::Internal("新品冻结输入缺失".into()))?
            .product_kind;
        let repository = self.db.supplier_category_mappings();
        let filter = category_mapping_filter(&draft.supplier_id, &path, kind);
        match repository.find_one(filter, executor).await? {
            Some(mut mapping) => {
                mapping.confirm(mapping.base.version, confirmation)?;
                repository.update(&mut mapping, executor).await?;
            },
            None => {
                let mapping = SupplierCategoryMapping::new(
                    id_generator::next_id(),
                    draft.supplier_id.clone(),
                    path,
                    kind,
                    confirmation,
                )?;
                repository.create(&mapping, executor).await?;
            },
        }
        Ok(())
    }

    /// 从本次真实生效决定构建分类匹配事实，复验当前完整字典资格。
    async fn category_confirmation(
        &self,
        draft: &NewProductDraft,
        executor: &mut dyn Executor,
    ) -> Result<CategoryMappingConfirmation> {
        let input = draft.frozen_input.as_ref().ok_or_else(|| Error::Internal("新品冻结输入缺失".into()))?;
        let submission = draft
            .submissions
            .iter()
            .find(|submission| Some(submission.id.as_str()) == draft.current_submission_id.as_deref())
            .ok_or_else(|| Error::Internal("新品当前提交关联缺失".into()))?;
        let decision = submission
            .decision
            .as_ref()
            .filter(|decision| {
                decision.status == DraftStatus::Effective && draft.status == DraftStatus::Effective
            })
            .ok_or_else(|| Error::Internal("分类映射必须来自真实新品生效决定".into()))?;
        let normalized =
            decision.normalized_product.as_ref().ok_or_else(|| Error::Internal("新品生效映射缺失".into()))?;
        self.ensure_dictionaries(input, normalized, executor).await?;
        let category = self
            .db
            .product_categories()
            .find_by_id(&normalized.category_id, executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("分类尚未完成有效匹配".into()))?;
        let hierarchy = self.category_hierarchy(&category, executor).await?;
        Ok(CategoryMappingConfirmation {
            draft_id: draft.base.id.clone(),
            submission_id: submission.id.clone(),
            source_original_category_path: input.category.raw_name.clone(),
            category_id: category.base.id,
            category_version: category.base.version,
            category_path: hierarchy_path(&hierarchy),
            hierarchy,
            confirmed_by: decision.decided_by.clone(),
            confirmed_at: decision.decided_at,
            reason: decision.reason.clone().ok_or_else(|| Error::Internal("分类核对理由缺失".into()))?,
        })
    }

    /// 仅投影当前仍有完整有效层级且商品类型一致的公司分类。
    async fn current_mapping_category(
        &self,
        category_id: &str,
        product_kind: ProductKind,
        executor: &mut dyn Executor,
    ) -> Result<Option<(DictionaryCandidate, Vec<CategoryHierarchyNode>)>> {
        let Some(category) = self.db.product_categories().find_by_id(category_id, executor).await? else {
            return Ok(None);
        };
        if !category.is_active() || category.product_kind != product_kind {
            return Ok(None);
        }
        let hierarchy = match self.category_hierarchy(&category, executor).await {
            Ok(hierarchy) => hierarchy,
            Err(Error::BusinessLogicError(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
        let candidate = DictionaryCandidate {
            id: category.base.id,
            version: category.base.version,
            code: category.category_code,
            name: category.name,
            path: Some(hierarchy_path(&hierarchy)),
            parent_id: category.parent_category_id.map(|id| id.to_string()),
            product_kind: Some(category.product_kind),
            quantity_scale: None,
            hierarchy: hierarchy.clone(),
        };
        Ok(Some((candidate, hierarchy)))
    }
}
