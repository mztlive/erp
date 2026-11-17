//! 独立字典候选与经授权的内部重复提示。

use std::collections::HashSet;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::AccessTarget;
use erp_permissions::{catalog as catalog_permissions, supplier_portal};
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Pagination, QueryFilter, Transactional, insert_literal_regex_filter};
use serde::{Deserialize, Serialize};

use super::CatalogPortalService;
use super::category_mapping::{CategoryHierarchyNode, hierarchy_path};
use crate::authorization::CompiledScope;
use crate::repository::CatalogExt;
use crate::{
    CatalogAccess, EnableStatus, Error, Product, ProductBrand, ProductCategory, ProductKind, Result, Sku,
    UnitOfMeasure,
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
    /// 管理端审核人员在快照中读取独立字典候选。
    /// # 参数
    /// kind 为字典类型，q 为搜索，product_kind 为分类约束，actor 为当前账号。
    /// # 返回
    /// 已通过审核功能资格检查的安全字典候选。
    /// # 错误
    /// 身份无效、缺少审核权限、搜索非法或数据库失败时返回对应错误。
    pub async fn admin_dictionary_candidates(
        &self,
        kind: DictionaryKind,
        q: Option<&str>,
        product_kind: Option<ProductKind>,
        actor: &AuditActor,
    ) -> Result<Vec<DictionaryCandidate>> {
        let service = Self::new(self.db.clone(), self.catalog.clone());
        let query = q.map(str::to_owned);
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service
                        .review_dictionary_candidates(kind, query.as_deref(), product_kind, &actor, executor)
                        .await
                })
            })
            .await
    }

    /// 管理端资格先于字典输入校验和读取，门户原入口保留自身认证合同。
    async fn review_dictionary_candidates(
        &self,
        kind: DictionaryKind,
        q: Option<&str>,
        product_kind: Option<ProductKind>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DictionaryCandidate>> {
        self.catalog
            .access()
            .authorization
            .require(actor, &supplier_portal::ADMIN_REVIEW, AccessTarget::Operation, executor)
            .await?;
        self.dictionary_candidates(kind, q, product_kind, executor).await
    }

    /// 独立于商品结果列表，查询已启用的字典值。
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

    /// 拼接当前分类从根到自身的路径文本。
    ///
    /// # 参数
    /// * `category` - 目标分类
    /// * `executor` - 当前读取执行器
    ///
    /// # 返回
    /// 返回完整分类路径。
    ///
    /// # 错误
    /// 目标或祖先停用、缺失、类型不一致、路径循环或数据库错误时返回对应错误。
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

    /// 仅在商品对象授权通过后返回内部重复提示。
    /// # 参数
    /// `q` 为商品、型号、SKU规格或条码的字面搜索，`actor` 为当前内部审核人，
    /// `executor` 为覆盖授权和全部候选读取的同一快照事务执行器。
    /// # 返回
    /// 可读取商品及精确SKU版本；供应商不得调用本入口。
    /// # 错误
    /// 非快照执行器、缺少商品读取资格、搜索非法或数据库错误。
    pub async fn duplicate_candidates(
        &self,
        q: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DuplicateCandidate>> {
        read_duplicates(self, &self.catalog.access(), q, actor, executor).await
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
            if sku.is_active() && sku.stable.current_revision_id.as_deref() == Some(revision.base.id.as_str())
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

#[async_trait]
trait DuplicateReader: Sync {
    async fn product_ids(&self, q: &str, executor: &mut dyn Executor) -> Result<Vec<String>>;

    async fn candidate(
        &self,
        product_id: &str,
        scope: &CompiledScope,
        executor: &mut dyn Executor,
    ) -> Result<Option<DuplicateCandidate>>;
}

#[async_trait]
impl DuplicateReader for CatalogPortalService {
    async fn product_ids(&self, q: &str, executor: &mut dyn Executor) -> Result<Vec<String>> {
        self.duplicate_product_ids(q, executor).await
    }

    async fn candidate(
        &self,
        product_id: &str,
        scope: &CompiledScope,
        executor: &mut dyn Executor,
    ) -> Result<Option<DuplicateCandidate>> {
        let Some(product) = self.db.catalog().authorized_product(product_id, scope, executor).await? else {
            return Ok(None);
        };
        if !product.is_active() {
            return Ok(None);
        }
        let revision = self
            .db
            .catalog()
            .current_product_revision(&product, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("候选商品当前资料缺失".into()))?;
        Ok(Some(self.duplicate_candidate(product, revision.base.id, revision.name, executor).await?))
    }
}

/// 在同一快照中完成读取资格、候选读取及交付前复验。
async fn read_duplicates(
    reader: &dyn DuplicateReader,
    access: &CatalogAccess,
    q: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Vec<DuplicateCandidate>> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("商品查重读取必须使用快照事务".into()));
    }
    ensure_query(Some(q))?;
    if q.trim().is_empty() {
        return Err(Error::ValidationError("重复匹配搜索不能为空".into()));
    }
    let scope = access.resolve(actor, &catalog_permissions::PRODUCT_PRODUCT_DETAIL, executor).await?;
    let mut result = Vec::new();
    for product_id in reader.product_ids(q, executor).await? {
        if let Some(candidate) = reader.candidate(&product_id, &scope, executor).await? {
            result.push(candidate);
        }
    }
    scope.validate(executor).await?;
    Ok(result)
}

fn ensure_query(q: Option<&str>) -> Result<()> {
    if q.is_some_and(|text| text.chars().count() > 128) {
        return Err(Error::ValidationError("搜索文本过长".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use erp_core::AccountKind;
    use erp_identity::AuthorizationService;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::{fixed_authorization, unavailable_authorization};
    use erp_permissions::Permission;
    use mongodb::Client;
    use mongodb::error::Error as MongoError;
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{NoTransaction, ReadExecutor};

    use super::*;
    use crate::CatalogService;
    use crate::authorization::bindings;
    use crate::ports::{EmptyFileAssetFacts, FailClosedAuditPort};

    #[derive(Default)]
    struct Reader {
        ids: Vec<String>,
        hidden: HashSet<String>,
        failure: Option<&'static str>,
        calls: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl DuplicateReader for Reader {
        async fn product_ids(&self, _: &str, _: &mut dyn Executor) -> Result<Vec<String>> {
            self.calls.lock().unwrap().push("ids".into());
            Ok(self.ids.clone())
        }

        async fn candidate(
            &self,
            product_id: &str,
            scope: &CompiledScope,
            executor: &mut dyn Executor,
        ) -> Result<Option<DuplicateCandidate>> {
            scope.ensure_current(executor)?;
            self.calls.lock().unwrap().push(product_id.into());
            match self.failure {
                Some("database") => {
                    return Err(Error::RepositoryError(persistence_core::Error::DatabaseError(
                        MongoError::custom("candidate database unavailable"),
                    )));
                },
                Some("authorization") => {
                    return Err(Error::Internal("candidate facility unavailable".into()));
                },
                Some("forbidden") => return Err(Error::Forbidden("facility denied".into())),
                Some("stage") => executor.next_read(),
                _ => {},
            }
            Ok((!self.hidden.contains(product_id)).then(|| DuplicateCandidate {
                product_id: product_id.into(),
                version: 1,
                revision_id: format!("revision-{product_id}"),
                name: format!("商品-{product_id}"),
                product_kind: ProductKind::Physical,
                skus: Vec::new(),
            }))
        }
    }

    async fn database() -> mongodb::Database {
        Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("catalog_duplicate_authorization_unit")
    }

    fn actor() -> AuditActor {
        AuditActor::new("reviewer".into(), "reviewer".into(), AccountKind::Admin)
    }

    async fn access(permissions: &[Permission]) -> CatalogAccess {
        let db = database().await;
        let authorization = fixed_authorization(
            permissions,
            ResolvedScope {
                role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
                user_limit: None,
            },
            Vec::new(),
            vec![(catalog_permissions::PRODUCT_PRODUCT_DETAIL, bindings(db.clone()).unwrap().1)],
        )
        .unwrap();
        CatalogAccess::new(db, authorization)
    }

    async fn portal(authorization: Arc<AuthorizationService>) -> CatalogPortalService {
        let db = database().await;
        let catalog = CatalogService::new(
            db.clone(),
            Arc::new(FailClosedAuditPort),
            Arc::new(EmptyFileAssetFacts),
            CatalogAccess::new(db.clone(), authorization),
        );
        CatalogPortalService::new(db, catalog)
    }

    #[tokio::test]
    async fn admin_dictionaries_reject_missing_review_permission_before_storage() {
        let authorization = fixed_authorization(
            &[],
            ResolvedScope { role_clauses: vec![], user_limit: None },
            vec![],
            vec![],
        )
        .unwrap();
        let service = portal(authorization).await;
        for kind in [DictionaryKind::Brand, DictionaryKind::Unit, DictionaryKind::Category] {
            let error = service
                .review_dictionary_candidates(kind, None, None, &actor(), &mut SnapshotExecutor::default())
                .await
                .unwrap_err();
            assert!(matches!(error, Error::Forbidden(_)));
        }
    }

    #[tokio::test]
    async fn admin_dictionary_qualification_reaches_real_validation_without_storage() {
        let authorization = fixed_authorization(
            &[supplier_portal::ADMIN_REVIEW],
            ResolvedScope { role_clauses: vec![], user_limit: None },
            vec![],
            vec![],
        )
        .unwrap();
        let service = portal(authorization).await;
        let search = "字".repeat(129);
        let error = service
            .review_dictionary_candidates(
                DictionaryKind::Brand,
                Some(&search),
                None,
                &actor(),
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::ValidationError(reason) if reason == "搜索文本过长"));
    }

    #[tokio::test]
    async fn admin_dictionary_identity_failure_stops_before_storage() {
        let service = portal(unavailable_authorization()).await;
        let error = service
            .review_dictionary_candidates(
                DictionaryKind::Brand,
                None,
                None,
                &actor(),
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn duplicate_entry_requires_snapshot_before_search_validation_or_identity() {
        let service = portal(unavailable_authorization()).await;
        for executor in [&mut NoTransaction as &mut dyn Executor, &mut ReadExecutor::default()] {
            let result = service.duplicate_candidates("", &actor(), executor).await;
            assert!(matches!(result, Err(Error::ValidationError(message))
                if message == "商品查重读取必须使用快照事务"));
        }
    }

    #[tokio::test]
    async fn empty_candidates_require_detail_qualification_and_accept_read_only_reviewer() {
        let reader = Reader::default();
        for permissions in [vec![], vec![catalog_permissions::PRODUCT_PRODUCT_UPDATE]] {
            let access = access(&permissions).await;
            let result =
                read_duplicates(&reader, &access, "商品", &actor(), &mut SnapshotExecutor::default()).await;
            assert!(matches!(result, Err(Error::Forbidden(_))));
        }
        assert!(reader.calls.lock().unwrap().is_empty());
        let access = access(&[catalog_permissions::PRODUCT_PRODUCT_DETAIL]).await;
        let result = read_duplicates(&reader, &access, "商品", &actor(), &mut SnapshotExecutor::default())
            .await
            .unwrap();
        assert!(result.is_empty());
        assert_eq!(*reader.calls.lock().unwrap(), ["ids"]);
    }

    #[tokio::test]
    async fn filtered_product_is_skipped_and_remaining_candidates_are_returned() {
        let reader = Reader {
            ids: vec!["first".into(), "outside-scope".into(), "last".into()],
            hidden: ["outside-scope".into()].into(),
            ..Default::default()
        };
        let access = access(&[catalog_permissions::PRODUCT_PRODUCT_DETAIL]).await;
        let result = read_duplicates(&reader, &access, "商品", &actor(), &mut SnapshotExecutor::default())
            .await
            .unwrap();
        assert_eq!(
            result.iter().map(|candidate| candidate.product_id.as_str()).collect::<Vec<_>>(),
            ["first", "last"]
        );
        assert_eq!(*reader.calls.lock().unwrap(), ["ids", "first", "outside-scope", "last"]);
    }

    #[tokio::test]
    async fn identity_failure_stops_before_candidate_search() {
        let reader = Reader::default();
        let access = CatalogAccess::new(database().await, unavailable_authorization());
        let result =
            read_duplicates(&reader, &access, "商品", &actor(), &mut SnapshotExecutor::default()).await;
        assert!(matches!(result, Err(Error::Internal(message)) if message == "测试授权设施不可用"));
        assert!(reader.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn candidate_database_and_facility_failures_are_not_filtered() {
        let access = access(&[catalog_permissions::PRODUCT_PRODUCT_DETAIL]).await;
        for failure in ["database", "authorization", "forbidden"] {
            let reader = Reader {
                ids: vec!["first".into(), "last".into()],
                failure: Some(failure),
                ..Default::default()
            };
            let result =
                read_duplicates(&reader, &access, "商品", &actor(), &mut SnapshotExecutor::default()).await;
            match failure {
                "database" => assert!(matches!(result, Err(Error::RepositoryError(_)))),
                "authorization" => assert!(
                    matches!(result, Err(Error::Internal(message)) if message == "candidate facility unavailable")
                ),
                _ => {
                    assert!(matches!(result, Err(Error::Forbidden(message)) if message == "facility denied"))
                },
            }
            assert_eq!(*reader.calls.lock().unwrap(), ["ids", "first"]);
        }
    }

    #[tokio::test]
    async fn candidate_result_is_rejected_when_reading_stage_changes() {
        let reader = Reader { ids: vec!["first".into()], failure: Some("stage"), ..Default::default() };
        let access = access(&[catalog_permissions::PRODUCT_PRODUCT_DETAIL]).await;
        let result =
            read_duplicates(&reader, &access, "商品", &actor(), &mut SnapshotExecutor::default()).await;
        assert!(matches!(result, Err(Error::ValidationError(_))));
    }
}
