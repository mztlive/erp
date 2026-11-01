//! 列表筛选的请求内分类事实缓存；只复用同一执行器下的成功读取。

use std::collections::{HashMap, HashSet};

use erp_catalog::CatalogExt;
use erp_core::ids::ProductCategoryId;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

const CATEGORY_CHAIN_LIMIT: usize = 32;

/// 单次负责人筛选内的父分类事实，禁止跨请求或授权快照复用。
pub(crate) struct CategoryChainCache<'a> {
    db: &'a Database,
    parents: HashMap<String, Option<String>>,
}

impl<'a> CategoryChainCache<'a> {
    /// 为当前执行器下的一次筛选创建空缓存。
    ///
    /// # 参数
    /// * `db` - 分类所属数据库
    ///
    /// # 返回
    /// 返回尚未读取分类的缓存。
    ///
    /// # 错误
    /// 无。
    pub(crate) fn new(db: &'a Database) -> Self {
        Self { db, parents: HashMap::new() }
    }

    /// 按层批量预读候选分类祖先，只缓存成功取得或已确认缺失的节点。
    ///
    /// # 参数
    /// * `category_ids` - 候选商品当前修订所引用的分类
    /// * `executor` - 本次筛选的同一执行器
    ///
    /// # 返回
    /// 成功时缓存最多 32 层父指针；异常链仍由 `ids` 验证并拒绝。
    ///
    /// # 错误
    /// 某层数据库读取失败时返回原错误，失败层不入缓存，调用方可按原策略重试。
    pub(crate) async fn prefetch<'id>(
        &mut self,
        category_ids: impl IntoIterator<Item = &'id str> + Send,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut pending = category_ids.into_iter().map(ToString::to_string).collect::<HashSet<_>>();
        for _ in 0..CATEGORY_CHAIN_LIMIT {
            pending.retain(|id| !self.parents.contains_key(id));
            if pending.is_empty() {
                break;
            }
            let ids = pending.into_iter().collect::<Vec<_>>();
            let rows = self.db.catalog().procurement_categories(&ids, executor).await?;
            let loaded =
                rows.into_iter().map(|row| (row.id, row.parent_category_id)).collect::<HashMap<_, _>>();
            pending = cache_parent_batch(&mut self.parents, &ids, loaded);
        }
        Ok(())
    }

    /// 按原顺序读取分类链，并记住成功取得的父节点或终止事实。
    ///
    /// # 参数
    /// * `category_id` - 商品当前修订的分类 ID
    /// * `executor` - 本次筛选的执行器，整次筛选必须保持相同
    ///
    /// # 返回
    /// 返回从当前分类到根的有序链；缺失分类仍作为末节点保留。
    ///
    /// # 错误
    /// 数据库读取失败、成环或超过 32 层时返回原错误；失败读取不入缓存。
    pub(crate) async fn ids(
        &mut self,
        category_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductCategoryId>> {
        let mut chain = CategoryChain::new(category_id);
        while let Some(id) = chain.advance(&self.parents)? {
            let parent = self
                .db
                .product_categories()
                .find_by_id(&id, executor)
                .await?
                .and_then(|category| category.parent_category_id.map(|parent| parent.to_string()));
            self.parents.insert(id, parent);
        }
        Ok(chain.ids)
    }
}

/// 缺失节点保留为空父引用，并只安排尚未缓存的父分类进入下一层批量读取。
fn cache_parent_batch(
    parents: &mut HashMap<String, Option<String>>,
    ids: &[String],
    mut loaded: HashMap<String, Option<String>>,
) -> HashSet<String> {
    let mut pending = HashSet::new();
    for id in ids {
        let parent = loaded.remove(id).flatten();
        if let Some(parent) = &parent {
            pending.insert(parent.clone());
        }
        parents.insert(id.clone(), parent);
    }
    pending
}

/// 分类链的读取游标；缓存命中时推进，未加载的节点留给拥有领域仓储读取。
struct CategoryChain {
    ids: Vec<ProductCategoryId>,
    seen: HashSet<String>,
    current: Option<String>,
}

impl CategoryChain {
    /// 从商品修订引用的分类开始构造有序链。
    fn new(category_id: &str) -> Self {
        Self { ids: Vec::new(), seen: HashSet::new(), current: Some(category_id.to_string()) }
    }

    /// 消费已加载父节点，返回仍需读取的首个分类；非法链在发起读取前拒绝。
    fn advance(&mut self, parents: &HashMap<String, Option<String>>) -> Result<Option<String>> {
        while let Some(id) = self.current.as_ref() {
            if self.seen.contains(id) || self.ids.len() >= CATEGORY_CHAIN_LIMIT {
                return Err(Error::ValidationError("商品分类链非法".into()));
            }
            let Some(parent) = parents.get(id) else {
                return Ok(Some(id.clone()));
            };
            self.seen.insert(id.clone());
            self.ids.push(ProductCategoryId::new(id.clone()));
            self.current = parent.clone();
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造无需数据库的父分类读取结果。
    fn parents(rows: &[(&str, Option<&str>)]) -> HashMap<String, Option<String>> {
        rows.iter().map(|(id, parent)| (id.to_string(), parent.map(ToString::to_string))).collect()
    }

    #[test]
    /// 同层多叶节点共用一个父节点，缺失末节点保留，并继续按原有序链解析。
    fn category_parent_batch_deduplicates_ancestors_and_keeps_missing_nodes() {
        let mut known = HashMap::new();
        let pending = cache_parent_batch(
            &mut known,
            &["leaf-a".into(), "leaf-b".into(), "missing".into()],
            parents(&[("leaf-a", Some("parent")), ("leaf-b", Some("parent"))]),
        );
        assert_eq!(pending, HashSet::from(["parent".to_string()]));
        assert_eq!(known.get("missing"), Some(&None));
        let pending = cache_parent_batch(&mut known, &["parent".into()], parents(&[("parent", None)]));
        assert!(pending.is_empty());
        for leaf in ["leaf-a", "leaf-b"] {
            let mut chain = CategoryChain::new(leaf);
            assert_eq!(chain.advance(&known).unwrap(), None);
            assert_eq!(chain.ids, [leaf, "parent"].map(ProductCategoryId::new));
        }
    }

    #[test]
    /// 复用成功节点且保持叶分类到根的顺序。
    fn category_chain_cache_reuses_successes_and_preserves_leaf_to_root_order() {
        let mut known = parents(&[("leaf", Some("parent"))]);
        let mut first = CategoryChain::new("leaf");
        assert_eq!(first.advance(&known).unwrap(), Some("parent".to_string()));
        known.insert("parent".to_string(), Some("root".to_string()));
        assert_eq!(first.advance(&known).unwrap(), Some("root".to_string()));
        known.insert("root".to_string(), None);
        assert_eq!(first.advance(&known).unwrap(), None);
        assert_eq!(first.ids, ["leaf", "parent", "root"].map(ProductCategoryId::new));

        let mut next_sku = CategoryChain::new("leaf");
        assert_eq!(next_sku.advance(&known).unwrap(), None);
        assert_eq!(next_sku.ids, first.ids);
    }

    #[test]
    /// 未读取节点继续请求读取，仓储成功确认缺失的末节点保留。
    fn category_chain_cache_keeps_missing_terminal_and_retries_unread_facts() {
        let mut known = parents(&[("leaf", Some("missing"))]);
        let mut first = CategoryChain::new("leaf");
        assert_eq!(first.advance(&known).unwrap(), Some("missing".to_string()));
        let mut next_sku = CategoryChain::new("leaf");
        assert_eq!(next_sku.advance(&known).unwrap(), Some("missing".to_string()));
        known.insert("missing".to_string(), None);
        assert_eq!(next_sku.advance(&known).unwrap(), None);
        assert_eq!(next_sku.ids, ["leaf", "missing"].map(ProductCategoryId::new));
        let mut missing_leaf = CategoryChain::new("missing");
        assert_eq!(missing_leaf.advance(&known).unwrap(), None);
        assert_eq!(missing_leaf.ids, [ProductCategoryId::new("missing")]);
    }

    #[test]
    /// 成环与超过 32 层均拒绝，正好 32 层仍允许。
    fn category_chain_cache_rejects_cycles_and_more_than_32_nodes() {
        let cyclic = parents(&[("leaf", Some("parent")), ("parent", Some("leaf"))]);
        assert!(matches!(CategoryChain::new("leaf").advance(&cyclic), Err(Error::ValidationError(_))));
        let self_cycle = parents(&[("leaf", Some("leaf"))]);
        assert!(matches!(CategoryChain::new("leaf").advance(&self_cycle), Err(Error::ValidationError(_))));
        let mut known: HashMap<String, Option<String>> = (0..CATEGORY_CHAIN_LIMIT)
            .map(|index| {
                let parent = (index + 1 < CATEGORY_CHAIN_LIMIT).then(|| (index + 1).to_string());
                (index.to_string(), parent)
            })
            .collect();
        let mut at_limit = CategoryChain::new("0");
        assert_eq!(at_limit.advance(&known).unwrap(), None);
        assert_eq!(at_limit.ids.len(), CATEGORY_CHAIN_LIMIT);
        known.insert((CATEGORY_CHAIN_LIMIT - 1).to_string(), Some("too-deep".to_string()));
        assert!(matches!(CategoryChain::new("0").advance(&known), Err(Error::ValidationError(_))));
    }
}
