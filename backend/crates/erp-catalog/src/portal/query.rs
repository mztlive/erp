//! Independent dictionary candidates and internal authorized duplicate hints.

use std::collections::HashSet;

use application_core::AuditActor;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Pagination, QueryFilter, insert_literal_regex_filter};
use serde::{Deserialize, Serialize};

use super::CatalogPortalService;
use super::category_mapping::{CategoryHierarchyNode, hierarchy_path};
use crate::repository::CatalogExt;
use crate::{
    EnableStatus, Error, Product, ProductBrand, ProductCategory, ProductKind, Result, Sku, UnitOfMeasure,
};

/// Read-only dictionary namespaces offered to a supplier.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictionaryKind {
    Brand,
    Category,
    Unit,
}

/// Necessary dictionary facts only; there are no business aggregates or counts.
#[derive(Debug, Clone, Serialize)]
pub struct DictionaryCandidate {
    pub id: String,
    pub version: u64,
    pub code: String,
    pub name: String,
    pub path: Option<String>,
    pub parent_id: Option<String>,
    pub product_kind: Option<ProductKind>,
    pub quantity_scale: Option<u8>,
    pub hierarchy: Vec<CategoryHierarchyNode>,
}

impl From<ProductBrand> for DictionaryCandidate {
    fn from(row: ProductBrand) -> Self {
        Self {
            id: row.base.id,
            version: row.base.version,
            code: row.brand_code,
            name: row.name,
            path: None,
            parent_id: None,
            product_kind: None,
            quantity_scale: None,
            hierarchy: Vec::new(),
        }
    }
}

impl From<UnitOfMeasure> for DictionaryCandidate {
    fn from(row: UnitOfMeasure) -> Self {
        Self {
            id: row.base.id,
            version: row.base.version,
            code: row.unit_code,
            name: row.name,
            path: None,
            parent_id: None,
            product_kind: None,
            quantity_scale: Some(row.quantity_scale),
            hierarchy: Vec::new(),
        }
    }
}

/// Internal duplicate hint requiring explicit versioned reviewer selection.
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateCandidate {
    pub product_id: String,
    pub version: u64,
    pub revision_id: String,
    pub name: String,
    pub product_kind: ProductKind,
    pub skus: Vec<DuplicateSkuCandidate>,
}

/// Reviewer-readable SKU facts accompanying the explicit frozen identity.
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateSkuCandidate {
    pub sku_id: String,
    pub version: u64,
    pub revision_id: String,
    pub sku_no: String,
    pub name: String,
    pub specification: Option<String>,
    pub unit_id: String,
    pub unit_version: u64,
    pub unit_name: String,
}

struct CandidateFilter {
    query: Option<String>,
    fields: Vec<&'static str>,
    product_kind: Option<ProductKind>,
}

impl QueryFilter for CandidateFilter {
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "status": EnableStatus::Active.as_str() };
        if let Some(kind) = self.product_kind {
            filter.insert("product_kind", kind.as_str());
        }
        if let Some(query) = self.query.as_deref().map(str::trim).filter(|text| !text.is_empty()) {
            let clauses = self
                .fields
                .iter()
                .map(|field| {
                    let mut clause = Document::new();
                    insert_literal_regex_filter(&mut clause, field, Some(query));
                    clause
                })
                .collect::<Vec<_>>();
            filter.insert("$or", clauses);
        }
        filter
    }
}
impl Pagination for CandidateFilter {
    fn page_and_size(&self) -> (u64, u64) {
        (1, 100)
    }
}

impl CatalogPortalService {
    /// Query enabled dictionary values independently from any product result list.
    /// # 参数
    /// `kind` 为字典类型，`q` 为字面搜索；分类可按商品类型收窄。
    /// # 返回
    /// 至多100个不含内部经营事实的候选。
    /// # 错误
    /// 搜索过长、分类树非法或数据库错误。
    pub async fn dictionary_candidates(
        &self,
        kind: DictionaryKind,
        q: Option<&str>,
        product_kind: Option<ProductKind>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DictionaryCandidate>> {
        ensure_query(q)?;
        let mut filter =
            CandidateFilter { query: q.map(str::to_string), fields: vec!["name"], product_kind: None };
        match kind {
            DictionaryKind::Brand => {
                filter.fields.push("brand_code");
                Ok(self
                    .db
                    .product_brands()
                    .search(&filter, executor)
                    .await?
                    .items
                    .into_iter()
                    .map(DictionaryCandidate::from)
                    .collect())
            },
            DictionaryKind::Unit => {
                filter.fields.push("unit_code");
                Ok(self
                    .db
                    .unit_of_measures()
                    .search(&filter, executor)
                    .await?
                    .items
                    .into_iter()
                    .map(DictionaryCandidate::from)
                    .collect())
            },
            DictionaryKind::Category => {
                filter.fields.push("category_code");
                filter.product_kind = product_kind;
                let rows = self.db.product_categories().search(&filter, executor).await?.items;
                self.category_candidates(rows, executor).await
            },
        }
    }

    async fn category_candidates(
        &self,
        rows: Vec<ProductCategory>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DictionaryCandidate>> {
        let mut candidates = Vec::with_capacity(rows.len());
        for row in rows {
            let hierarchy = self.category_hierarchy(&row, executor).await?;
            let path = hierarchy_path(&hierarchy);
            candidates.push(DictionaryCandidate {
                id: row.base.id,
                version: row.base.version,
                code: row.category_code,
                name: row.name,
                path: Some(path),
                parent_id: row.parent_category_id.map(|id| id.to_string()),
                product_kind: Some(row.product_kind),
                quantity_scale: None,
                hierarchy,
            });
        }
        Ok(candidates)
    }

    pub(super) async fn category_path(
        &self,
        category: &ProductCategory,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        Ok(hierarchy_path(&self.category_hierarchy(category, executor).await?))
    }

    /// 读取当前完整有效分类链，保留每一级身份、名称、父级和版本。
    /// # 参数
    /// `category` 为目标分类，`executor` 为当前读取执行器。
    /// # 返回
    /// 从根分类到目标的完整核对链。
    /// # 错误
    /// 目标或祖先停用、缺失、类型不一致、路径循环或数据库错误时拒绝。
    pub(super) async fn category_hierarchy(
        &self,
        category: &ProductCategory,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CategoryHierarchyNode>> {
        if !category.is_active() {
            return Err(Error::BusinessLogicError("商品分类已停用".into()));
        }
        let mut nodes = vec![CategoryHierarchyNode::from(category)];
        let mut parent_id = category.parent_category_id.clone();
        let mut seen = HashSet::from([category.base.id.clone()]);
        while let Some(id) = parent_id {
            if nodes.len() >= 32 || !seen.insert(id.to_string()) {
                return Err(Error::BusinessLogicError("商品分类路径非法".into()));
            }
            let parent = self
                .db
                .product_categories()
                .find_by_id(id.as_ref(), executor)
                .await?
                .ok_or_else(|| Error::BusinessLogicError("商品分类上级不存在".into()))?;
            if !parent.is_active() {
                return Err(Error::BusinessLogicError("商品分类上级已停用".into()));
            }
            if parent.product_kind != category.product_kind {
                return Err(Error::BusinessLogicError("商品分类路径与商品类型不兼容".into()));
            }
            nodes.push(CategoryHierarchyNode::from(&parent));
            parent_id = parent.parent_category_id;
        }
        nodes.reverse();
        Ok(nodes)
    }

    /// Return internal duplicate hints only after catalog object authorization.
    /// # 参数
    /// `q` 为商品、型号、SKU规格或条码的字面搜索，`actor` 为当前内部审核人。
    /// # 返回
    /// 可维护商品及精确SKU版本；供应商不得调用本入口。
    /// # 错误
    /// 缺权限、搜索非法或数据库错误。
    pub async fn duplicate_candidates(
        &self,
        q: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DuplicateCandidate>> {
        ensure_query(Some(q))?;
        if q.trim().is_empty() {
            return Err(Error::ValidationError("重复匹配搜索不能为空".into()));
        }
        self.catalog.access().resolve(actor, "update", executor).await?;
        let mut result = Vec::new();
        for product_id in self.duplicate_product_ids(q, executor).await? {
            let product =
                match self.catalog.access().require_product(actor, "update", &product_id, executor).await {
                    Ok(product) => product,
                    Err(Error::NotFound(_)) => continue,
                    Err(error) => return Err(error),
                };
            if !product.is_active() {
                continue;
            }
            let revision = self
                .db
                .catalog()
                .current_product_revision(&product, executor)
                .await?
                .ok_or_else(|| Error::ConflictError("候选商品当前资料缺失".into()))?;
            result.push(self.duplicate_candidate(product, revision.base.id, revision.name, executor).await?);
        }
        Ok(result)
    }

    async fn duplicate_product_ids(&self, q: &str, executor: &mut dyn Executor) -> Result<Vec<String>> {
        let mut filter = CandidateFilter {
            query: Some(q.into()),
            fields: vec!["name", "specification"],
            product_kind: None,
        };
        let mut ids = Vec::new();
        let mut seen = HashSet::new();
        for revision in self.db.product_revisions().search(&filter, executor).await?.items {
            let id = revision.product_id.to_string();
            if seen.insert(id.clone()) {
                ids.push(id);
            }
        }
        filter.fields = vec!["product_no"];
        for product in self.db.products().search(&filter, executor).await?.items {
            if seen.insert(product.base.id.clone()) {
                ids.push(product.base.id);
            }
        }
        filter.fields = vec!["name", "specification", "barcode"];
        for revision in self.db.sku_revisions().search(&filter, executor).await?.items {
            let Some(sku) = self.db.skus().find_by_id(revision.sku_id.as_ref(), executor).await? else {
                continue;
            };
            if sku.is_active()
                && sku.stable.current_revision_id.as_ref().map(ToString::to_string).as_deref()
                    == Some(&revision.base.id)
            {
                let id = sku.product_id.to_string();
                if seen.insert(id.clone()) {
                    ids.push(id);
                }
            }
        }
        filter.fields = vec!["sku_no", "specification_signature"];
        for sku in self.db.skus().search(&filter, executor).await?.items {
            let id = sku.product_id.to_string();
            if seen.insert(id.clone()) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    async fn duplicate_candidate(
        &self,
        product: Product,
        revision_id: String,
        name: String,
        executor: &mut dyn Executor,
    ) -> Result<DuplicateCandidate> {
        let skus = self
            .db
            .skus()
            .find_many(
                doc! { "product_id": &product.base.id, "status": EnableStatus::Active.as_str() },
                executor,
            )
            .await?;
        let mut candidates = Vec::with_capacity(skus.len());
        for sku in skus {
            candidates.push(self.duplicate_sku_candidate(sku, executor).await?);
        }
        Ok(DuplicateCandidate {
            product_id: product.base.id,
            version: product.base.version,
            revision_id,
            name,
            product_kind: product.product_kind,
            skus: candidates,
        })
    }

    async fn duplicate_sku_candidate(
        &self,
        sku: Sku,
        executor: &mut dyn Executor,
    ) -> Result<DuplicateSkuCandidate> {
        let revision_id = sku
            .stable
            .current_revision_id
            .as_ref()
            .ok_or_else(|| Error::ConflictError("候选SKU当前资料缺失".into()))?;
        let revision = self
            .db
            .sku_revisions()
            .find_by_id(revision_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("候选SKU当前资料缺失".into()))?;
        let unit = self
            .db
            .unit_of_measures()
            .find_by_id(sku.base_unit_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("候选SKU基础单位缺失".into()))?;
        let specification = revision.specification.or_else(|| {
            (!sku.specification_signature.is_empty()).then(|| sku.specification_signature.clone())
        });
        Ok(DuplicateSkuCandidate {
            sku_id: sku.base.id,
            version: sku.base.version,
            revision_id: revision_id.to_string(),
            sku_no: sku.sku_no,
            name: revision.name,
            specification,
            unit_id: unit.base.id,
            unit_version: unit.base.version,
            unit_name: unit.name,
        })
    }
}

fn ensure_query(q: Option<&str>) -> Result<()> {
    if q.is_some_and(|text| text.chars().count() > 128) {
        return Err(Error::ValidationError("搜索文本过长".into()));
    }
    Ok(())
}
