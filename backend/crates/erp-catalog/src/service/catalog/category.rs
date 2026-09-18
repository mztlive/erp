use application_core::AuditActor;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::CatalogService;
use super::support::ensure_version;
use crate::dto::{
    MoveProductCategoryRequest, PageView, ProductCategoryListParams, ProductCategoryView, SortDir,
    UpdateProductCategoryRequest,
};
use crate::entity::catalog::ProductCategoryId;
use crate::entity::catalog::product_category::{ProductCategory, ProductCategoryUpdate};
use crate::error::{Error, Result};
use crate::repository::CatalogExt;
use crate::repository::prelude::*;

/// 商品分类列表筛选条件类型（经 `CatalogExt` 关联类型跨 crate 可达）。
type ProductCategoryFilter = <mongodb::Database as CatalogExt>::ProductCategoryFilter;
/// 分类祖先链投影事实类型（经 `CatalogExt` 关联类型跨 crate 可达）。
type CategoryParentChainFact = <mongodb::Database as CatalogExt>::CategoryParentChainFact;

impl CatalogService {
    // ---------- 商品分类（树形字典） ----------

    /// 分页查询商品分类列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`category_code`/`name`/`parent_category_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn product_category_list(
        &self,
        params: &ProductCategoryListParams,
    ) -> Result<PageView<ProductCategoryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ProductCategoryFilter {
            q: query.q,
            category_code: query.category_code,
            name: query.name,
            parent_category_id: query.parent_category_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page =
            self.db.product_categories().search_product_categories(&filter, &mut NoTransaction).await?;
        let items = page.items.into_iter().map(ProductCategoryView::from).collect();
        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 更新商品分类（乐观锁语义；名称、类型、状态与可选父级变更原子提交）。
    ///
    /// # 参数
    /// * `id` - 分类 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后分类的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 分类不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn product_category_update(
        &self,
        id: &str,
        req: UpdateProductCategoryRequest,
        actor: &AuditActor,
    ) -> Result<ProductCategoryView> {
        req.validate()?;
        let mut category = self.load_category(id).await?;
        ensure_version(category.base.version, req.version)?;
        if let Some(parent_change) = &req.parent_change {
            self.ensure_parent_chain_ok(&category.base.id, parent_change.parent_category_id.as_ref()).await?;
        }
        category.update(
            ProductCategoryUpdate { name: req.name, product_kind: req.product_kind, status: req.status },
            actor.id(),
        )?;
        if let Some(parent_change) = req.parent_change {
            category.set_parent(parent_change.parent_category_id, actor.id())?;
        }
        let audit = self.audit.resource_log(
            actor.clone(),
            "product_category.update",
            "product_category",
            category.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let updated = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    db.product_categories().update(&mut category, executor).await?;
                    audit_port.persist(&audit, executor).await?;
                    Ok::<ProductCategory, crate::error::Error>(category)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 移动商品分类到新父分类（树形维护；成环检测在服务层完成）。
    ///
    /// 沿新父分类的祖先链上溯，命中本节点即拒绝；`None` 表示提升为根分类。
    ///
    /// # 参数
    /// * `id` - 分类 ID
    /// * `req` - 移动请求（含期望版本与新父分类）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回移动后分类的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 分类或新父分类不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    /// * `BusinessLogicError` - 移动将形成环
    pub async fn product_category_move(
        &self,
        id: &str,
        req: MoveProductCategoryRequest,
        actor: &AuditActor,
    ) -> Result<ProductCategoryView> {
        req.validate()?;
        let mut category = self.load_category(id).await?;
        ensure_version(category.base.version, req.version)?;
        self.ensure_parent_chain_ok(&category.base.id, req.parent_category_id.as_ref()).await?;
        category.set_parent(req.parent_category_id, actor.id())?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "product_category.move",
            "product_category",
            category.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let updated = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    db.product_categories().update(&mut category, executor).await?;
                    audit_port.persist(&audit, executor).await?;
                    Ok::<ProductCategory, crate::error::Error>(category)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 删除商品分类（软删除，乐观锁语义）。
    ///
    /// 存在子分类时拒绝删除（数据模型 §6.3：树形维护页不允许留下孤儿子树）。
    ///
    /// # 参数
    /// * `id` - 分类 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回删除结果。
    ///
    /// # 错误
    /// * `NotFound` - 分类不存在
    /// * `BusinessLogicError` - 分类下存在子分类
    /// * `ConflictError` - 并发修改（CAS 冲突）
    pub async fn product_category_delete(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let mut category = self.load_category(id).await?;
        let has_children = self.db.product_categories().has_children(id, &mut NoTransaction).await?;
        if has_children {
            return Err(Error::BusinessLogicError("分类下存在子分类，不能删除".to_string()));
        }
        let audit = self.audit.resource_log(
            actor.clone(),
            "product_category.delete",
            "product_category",
            category.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    db.product_categories().soft_delete(&mut category, executor).await?;
                    audit_port.persist(&audit, executor).await?;
                    Ok::<(), crate::error::Error>(())
                })
            })
            .await
    }

    // ---------- 商品品牌 ----------
    /// 校验新父分类的祖先链不包含本节点（成环检测）。
    ///
    /// 祖先链投影由 Repository 一次取回；本方法只解释缺失、命中自身、成环和截断。
    ///
    /// # 参数
    /// * `id` - 本节点 ID
    /// * `parent_id` - 新父分类（`None` 为根）
    ///
    /// # 返回
    /// 合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 父分类不存在或沿祖先链命中本节点、成环、过深时返回错误。
    pub async fn ensure_parent_chain_ok(
        &self,
        id: &str,
        parent_id: Option<&ProductCategoryId>,
    ) -> Result<()> {
        if parent_id.is_some_and(|parent| parent.as_ref() == id) {
            return Err(Error::BusinessLogicError("父子关系不能形成环".to_string()));
        }
        let fact = self.db.product_categories().parent_chain(parent_id, &mut NoTransaction).await?;
        interpret_parent_chain(id, &fact)
    }
}

/// 把祖先链持久化事实适配为分类树业务错误。
///
/// # 参数
/// * `self_id` - 正在创建或移动的分类 ID
/// * `fact` - Repository 返回的 ID/父 ID、缺失、成环与截断事实
///
/// # 返回
/// 根节点或合法多级链返回 `Ok(())`。
///
/// # 错误
/// 命中自身或成环返回 `BusinessLogicError`；父节点缺失返回 `NotFound`；
/// 过深截断失败关闭。
fn interpret_parent_chain(self_id: &str, fact: &CategoryParentChainFact) -> Result<()> {
    if fact.start_parent_id.is_none() {
        return Ok(());
    }
    if fact.hits_id(self_id) || fact.cycle_detected {
        return Err(Error::BusinessLogicError("父子关系不能形成环".to_string()));
    }
    if fact.missing_parent_id.is_some() {
        return Err(Error::NotFound("父分类不存在".to_string()));
    }
    if fact.truncated {
        return Err(Error::BusinessLogicError("分类祖先链过深或存在环，已失败关闭".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(
        start_parent_id: Option<&str>,
        missing_parent_id: Option<&str>,
        cycle_detected: bool,
        truncated: bool,
    ) -> CategoryParentChainFact {
        CategoryParentChainFact::from_projection(
            start_parent_id.map(ToString::to_string),
            missing_parent_id.map(ToString::to_string),
            cycle_detected,
            truncated,
        )
    }

    /// 根节点与正常多级链通过；缺失、命中自身、成环和截断失败关闭。
    #[test]
    fn interpret_parent_chain_covers_root_valid_missing_and_cycles() {
        assert!(interpret_parent_chain("child", &CategoryParentChainFact::root()).is_ok());
        assert!(interpret_parent_chain("child", &fact(Some("parent"), None, false, false)).is_ok());

        let missing = interpret_parent_chain("child", &fact(Some("ghost"), Some("ghost"), false, false));
        assert!(matches!(missing, Err(Error::NotFound(_))));

        let direct = interpret_parent_chain("child", &fact(Some("child"), None, false, false));
        assert!(matches!(direct, Err(Error::BusinessLogicError(_))));

        let ancestor_hit = fact(Some("parent"), None, false, false)
            .with_link("parent".to_string(), Some("child".to_string()))
            .with_link("child".to_string(), None);
        assert!(matches!(interpret_parent_chain("child", &ancestor_hit), Err(Error::BusinessLogicError(_))));

        let indirect = interpret_parent_chain("child", &fact(Some("parent"), None, true, false));
        assert!(matches!(indirect, Err(Error::BusinessLogicError(_))));

        let truncated = interpret_parent_chain("child", &fact(Some("parent"), None, false, true));
        assert!(matches!(truncated, Err(Error::BusinessLogicError(_))));
    }
}
