//! 商品分类祖先链投影：一次 `$graphLookup` 返回 ID/父 ID 与缺失、成环、截断事实。

use std::collections::{HashMap, HashSet};

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use serde::Deserialize;

use entities::catalog::ProductCategory;
use entities::ids::ProductCategoryId;

use super::super::Repository;
use super::shared::PRODUCT_CATEGORIES;
use crate::executor::Executor;
use crate::Result;

/// 祖先链最大节点数（含起始父节点）。超出即视为异常链并失败关闭。
const PARENT_CHAIN_MAX_NODES: usize = 32;
/// `$graphLookup` 在起始父节点之后继续上溯的最大深度。
const PARENT_CHAIN_GRAPH_LOOKUP_MAX_DEPTH: i32 = 30;

/// 祖先链上的一条 ID / 父 ID 投影。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryParentLink {
    /// 当前节点稳定主键。
    pub id: String,
    /// 父分类稳定主键；空表示该节点为根。
    pub parent_id: Option<String>,
}

/// 分类祖先链持久化事实（不含业务判定）。
///
/// Repository 只投影 ID、父 ID，并标出缺失、成环或深度截断；父节点不存在、
/// 命中自身和成环的错误适配仍由 Service 承担。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryParentChainFact {
    /// 待校验的起始父分类；`None` 表示新节点本身提升为根，无需读取。
    pub start_parent_id: Option<String>,
    /// 从起始父分类沿 `parent_category_id` 上溯得到的投影节点。
    pub links: Vec<CategoryParentLink>,
    /// 被引用但不存在或已软删除的父分类 ID。
    pub missing_parent_id: Option<String>,
    /// 沿父指针回访已出现节点时为 `true`。
    pub cycle_detected: bool,
    /// 达到最大深度仍未落到根、缺失或成环时为 `true`。
    pub truncated: bool,
}

impl CategoryParentChainFact {
    /// 构造根节点事实：新节点没有父分类，不访问数据库。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回空链且无缺失/成环/截断的根事实。
    ///
    /// # 错误
    /// 无。
    pub fn root() -> Self {
        Self::from_projection(None, None, false, false)
    }

    /// 装配已投影的祖先链事实。
    ///
    /// # 参数
    /// * `start_parent_id` - 起始父分类；`None` 表示根
    /// * `missing_parent_id` - 缺失或已软删除的父分类
    /// * `cycle_detected` - 沿父指针回访已出现节点
    /// * `truncated` - 达到最大深度仍未结束
    ///
    /// # 返回
    /// 返回不含业务错误的投影事实；`links` 由仓储装配，调用方可留空。
    ///
    /// # 错误
    /// 无。
    pub fn from_projection(
        start_parent_id: Option<String>,
        missing_parent_id: Option<String>,
        cycle_detected: bool,
        truncated: bool,
    ) -> Self {
        Self {
            start_parent_id,
            links: Vec::new(),
            missing_parent_id,
            cycle_detected,
            truncated,
        }
    }

    /// 判断投影链是否包含指定分类 ID。
    ///
    /// # 参数
    /// * `id` - 待检查的分类稳定主键
    ///
    /// # 返回
    /// 起始父 ID 或任一投影节点 ID 等于该值时返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn hits_id(&self, id: &str) -> bool {
        self.start_parent_id.as_deref() == Some(id) || self.links.iter().any(|link| link.id == id)
    }

    /// 追加一条 ID/父 ID 投影。
    ///
    /// # 参数
    /// * `id` - 节点稳定主键
    /// * `parent_id` - 父分类稳定主键；空表示根
    ///
    /// # 返回
    /// 返回追加投影后的事实。
    ///
    /// # 错误
    /// 无。
    pub fn with_link(mut self, id: impl Into<String>, parent_id: Option<String>) -> Self {
        self.links.push(CategoryParentLink {
            id: id.into(),
            parent_id,
        });
        self
    }
}

/// `$graphLookup` 祖先节点投影。
#[derive(Debug, Deserialize)]
struct ParentChainAncestorRow {
    id: String,
    parent_category_id: Option<String>,
    depth: i32,
}

/// 起始父节点及其祖先投影。
#[derive(Debug, Deserialize)]
struct ParentChainAggregateRow {
    id: String,
    parent_category_id: Option<String>,
    ancestors: Vec<ParentChainAncestorRow>,
}

impl<'a> Repository<'a, ProductCategory> {
    /// 投影新父分类的祖先链事实。
    ///
    /// 根节点不访问数据库。非根时以一次 `$match` + `$graphLookup` 取回起始父
    /// 节点及祖先的 ID/父 ID，查询次数不随树深度增长。本方法不开启事务，使用
    /// 调用方执行器。
    ///
    /// # 参数
    /// * `parent_id` - 新父分类；`None` 表示提升为根
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回 ID/父 ID 投影及缺失、成环、截断事实。
    ///
    /// # 错误
    /// MongoDB 聚合或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 单次校验固定 0 或 1 次查询；历史断链、成环或过深必须标出，不得无限遍历。
    pub async fn parent_chain(
        &self,
        parent_id: Option<&ProductCategoryId>,
        executor: &mut dyn Executor,
    ) -> Result<CategoryParentChainFact> {
        let Some(parent_id) = parent_id else {
            return Ok(CategoryParentChainFact::root());
        };
        let start_parent_id = parent_id.as_ref();
        let row = aggregate_parent_chain(&self.collection(), start_parent_id, executor).await?;
        Ok(assemble_parent_chain_fact(start_parent_id, row))
    }

    /// 返回祖先链 `$match` + `$graphLookup` 管道，供 explain 与行为测试共用。
    ///
    /// 调用方不得附加 hint；执行计划必须能命中稳定主键索引。
    ///
    /// # 参数
    /// * `start_parent_id` - 起始父分类稳定主键
    ///
    /// # 返回
    /// 返回与 [`Self::parent_chain`] 相同的聚合管道。
    ///
    /// # 错误
    /// 无。
    pub fn parent_chain_aggregation_pipeline(&self, start_parent_id: &str) -> Vec<Document> {
        parent_chain_pipeline(start_parent_id)
    }
}

/// 执行祖先链聚合并收集至多一行结果。
///
/// # 参数
/// * `collection` - 商品分类集合
/// * `start_parent_id` - 起始父分类稳定主键
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 起始父分类存在时返回其投影行；缺失或已软删除时返回 `None`。
///
/// # 错误
/// MongoDB 聚合、游标读取或反序列化失败时返回错误。
async fn aggregate_parent_chain(
    collection: &mongodb::Collection<ProductCategory>,
    start_parent_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ParentChainAggregateRow>> {
    let pipeline = parent_chain_pipeline(start_parent_id);
    let rows = match executor.session() {
        Some(session) => {
            collection
                .aggregate(pipeline)
                .with_type::<ParentChainAggregateRow>()
                .session(&mut *session)
                .await?
                .stream(session)
                .try_collect::<Vec<_>>()
                .await?
        }
        None => {
            collection
                .aggregate(pipeline)
                .with_type::<ParentChainAggregateRow>()
                .await?
                .try_collect::<Vec<_>>()
                .await?
        }
    };
    Ok(rows.into_iter().next())
}

/// 构造祖先链聚合管道：按稳定主键精确匹配起始父节点，再 `$graphLookup` 上溯。
///
/// # 参数
/// * `start_parent_id` - 起始父分类稳定主键
///
/// # 返回
/// 返回固定两段管道（`$match` + `$graphLookup`/`$project`），查询次数与深度无关。
///
/// # 错误
/// 无。
fn parent_chain_pipeline(start_parent_id: &str) -> Vec<Document> {
    vec![
        doc! {
            "$match": {
                "id": start_parent_id,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            }
        },
        doc! {
            "$graphLookup": {
                "from": PRODUCT_CATEGORIES,
                "startWith": "$parent_category_id",
                "connectFromField": "parent_category_id",
                "connectToField": "id",
                "as": "ancestors",
                "maxDepth": PARENT_CHAIN_GRAPH_LOOKUP_MAX_DEPTH,
                "restrictSearchWithMatch": { "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
                "depthField": "depth",
            }
        },
        doc! {
            "$project": {
                "_id": 0,
                "id": 1,
                "parent_category_id": 1,
                "ancestors": {
                    "$map": {
                        "input": "$ancestors",
                        "as": "node",
                        "in": {
                            "id": "$$node.id",
                            "parent_category_id": "$$node.parent_category_id",
                            "depth": "$$node.depth",
                        }
                    }
                }
            }
        },
    ]
}

/// 把聚合行装配为祖先链事实；空行视为起始父节点缺失。
///
/// # 参数
/// * `start_parent_id` - 起始父分类稳定主键
/// * `row` - 聚合投影行；`None` 表示起始父分类不存在或已软删除
///
/// # 返回
/// 返回 ID/父 ID 链及缺失、成环、截断事实。
///
/// # 错误
/// 无。
fn assemble_parent_chain_fact(
    start_parent_id: &str,
    row: Option<ParentChainAggregateRow>,
) -> CategoryParentChainFact {
    let Some(row) = row else {
        return CategoryParentChainFact {
            start_parent_id: Some(start_parent_id.to_string()),
            links: Vec::new(),
            missing_parent_id: Some(start_parent_id.to_string()),
            cycle_detected: false,
            truncated: false,
        };
    };
    let mut ancestors = row.ancestors;
    ancestors.sort_by_key(|item| item.depth);
    let mut links = Vec::with_capacity(ancestors.len() + 1);
    links.push(CategoryParentLink {
        id: row.id,
        parent_id: row.parent_category_id,
    });
    let graph_lookup_saturated = ancestors
        .iter()
        .any(|item| item.depth >= PARENT_CHAIN_GRAPH_LOOKUP_MAX_DEPTH);
    for ancestor in ancestors {
        links.push(CategoryParentLink {
            id: ancestor.id,
            parent_id: ancestor.parent_category_id,
        });
    }
    inspect_parent_links(start_parent_id, links, graph_lookup_saturated)
}

/// 沿父指针检查缺失、成环和深度截断，不把业务错误下沉到仓储。
///
/// # 参数
/// * `start_parent_id` - 起始父分类稳定主键
/// * `links` - 聚合得到的 ID/父 ID 投影
/// * `graph_lookup_saturated` - 祖先 `$graphLookup` 已达到 `maxDepth`
///
/// # 返回
/// 返回带异常标记的祖先链事实；正常链的 `missing_parent_id` 为空且不成环、不截断。
/// 达到 `maxDepth` 仍有未取回的父指针时标 `truncated`，不得记为缺失父节点。
///
/// # 错误
/// 无。
fn inspect_parent_links(
    start_parent_id: &str,
    links: Vec<CategoryParentLink>,
    graph_lookup_saturated: bool,
) -> CategoryParentChainFact {
    let mut by_id = HashMap::with_capacity(links.len());
    for link in &links {
        by_id.insert(link.id.as_str(), link);
    }
    let mut visited = HashSet::new();
    let mut missing_parent_id = None;
    let mut cycle_detected = false;
    let mut truncated = false;
    let mut cursor = Some(start_parent_id);
    let mut steps = 0_usize;
    while let Some(id) = cursor {
        if !visited.insert(id) {
            cycle_detected = true;
            break;
        }
        steps += 1;
        if steps > PARENT_CHAIN_MAX_NODES {
            truncated = true;
            break;
        }
        let Some(node) = by_id.get(id) else {
            missing_parent_id = Some(id.to_string());
            break;
        };
        cursor = node.parent_id.as_deref();
        let Some(parent_id) = cursor else {
            break;
        };
        if by_id.contains_key(parent_id) || visited.contains(parent_id) {
            continue;
        }
        if graph_lookup_saturated {
            truncated = true;
        } else {
            missing_parent_id = Some(parent_id.to_string());
        }
        break;
    }
    CategoryParentChainFact {
        start_parent_id: Some(start_parent_id.to_string()),
        links,
        missing_parent_id,
        cycle_detected,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ancestor(id: &str, parent_id: Option<&str>, depth: i32) -> ParentChainAncestorRow {
        ParentChainAncestorRow {
            id: id.to_string(),
            parent_category_id: parent_id.map(ToString::to_string),
            depth,
        }
    }

    fn row(
        id: &str,
        parent_id: Option<&str>,
        ancestors: Vec<ParentChainAncestorRow>,
    ) -> ParentChainAggregateRow {
        ParentChainAggregateRow {
            id: id.to_string(),
            parent_category_id: parent_id.map(ToString::to_string),
            ancestors,
        }
    }

    /// 祖先链管道固定一次 `$match` + `$graphLookup`，查询次数不随深度增长。
    #[test]
    fn parent_chain_pipeline_is_single_graph_lookup() {
        let pipeline = parent_chain_pipeline("parent-1");
        let json = format!("{pipeline:?}");

        assert_eq!(pipeline.len(), 3);
        assert!(json.contains("$graphLookup"));
        assert!(json.contains("parent_category_id"));
        assert!(json.contains("connectToField"));
        assert!(json.contains("id"));
        assert!(json.contains("parent-1"));
        assert!(json.contains("deleted_at"));
        assert_eq!(json.matches("$graphLookup").count(), 1);
        assert_eq!(json.matches("$match").count(), 1);
    }

    /// 根节点（无父分类）不产生投影链，也不标记缺失或成环。
    #[test]
    fn root_parent_is_empty_fact() {
        let fact = CategoryParentChainFact::root();

        assert!(fact.start_parent_id.is_none());
        assert!(fact.links.is_empty());
        assert!(fact.missing_parent_id.is_none());
        assert!(!fact.cycle_detected);
        assert!(!fact.truncated);
        assert!(!fact.hits_id("child-1"));
    }

    /// 正常多级链按父指针上溯到根，不标记异常。
    #[test]
    fn normal_multi_level_chain_reaches_root() {
        let fact = assemble_parent_chain_fact(
            "child-parent",
            Some(row(
                "child-parent",
                Some("mid"),
                vec![ancestor("mid", Some("root"), 0), ancestor("root", None, 1)],
            )),
        );

        assert_eq!(fact.links.len(), 3);
        assert!(fact.missing_parent_id.is_none());
        assert!(!fact.cycle_detected);
        assert!(!fact.truncated);
        assert!(fact.hits_id("mid"));
        assert!(!fact.hits_id("moving-node"));
    }

    /// 起始父节点缺失时标出缺失 ID，供 Service 适配 NotFound。
    #[test]
    fn missing_start_parent_is_fail_closed() {
        let fact = assemble_parent_chain_fact("ghost", None);

        assert_eq!(fact.missing_parent_id.as_deref(), Some("ghost"));
        assert!(fact.links.is_empty());
        assert!(!fact.cycle_detected);
        assert!(fact.hits_id("ghost"));
    }

    /// 中段父节点缺失视为历史断链，失败关闭。
    #[test]
    fn broken_mid_chain_is_missing() {
        let fact = assemble_parent_chain_fact(
            "child-parent",
            Some(row("child-parent", Some("ghost"), Vec::new())),
        );

        assert_eq!(fact.missing_parent_id.as_deref(), Some("ghost"));
        assert!(!fact.cycle_detected);
        assert!(!fact.truncated);
    }

    /// 直接自环：父指针指向自身。
    #[test]
    fn direct_cycle_is_detected() {
        let fact = assemble_parent_chain_fact("loop", Some(row("loop", Some("loop"), Vec::new())));

        assert!(fact.cycle_detected);
        assert!(fact.missing_parent_id.is_none());
        assert!(fact.hits_id("loop"));
    }

    /// 间接环：A→B→A，图遍历必须停止且标出成环。
    #[test]
    fn indirect_cycle_is_detected() {
        let fact =
            assemble_parent_chain_fact("a", Some(row("a", Some("b"), vec![ancestor("b", Some("a"), 0)])));

        assert!(fact.cycle_detected);
        assert!(fact.missing_parent_id.is_none());
        assert!(!fact.truncated);
        assert!(fact.hits_id("b"));
    }

    /// 达到最大深度仍未结束时截断，避免无限遍历。
    #[test]
    fn overlong_chain_is_truncated() {
        let mut links = Vec::new();
        for index in 0..=PARENT_CHAIN_MAX_NODES {
            links.push(CategoryParentLink {
                id: format!("n{index}"),
                parent_id: Some(format!("n{}", index + 1)),
            });
        }
        let fact = inspect_parent_links("n0", links, false);

        assert!(fact.truncated);
        assert!(!fact.cycle_detected);
        assert!(fact.missing_parent_id.is_none());
    }

    /// `$graphLookup` 达到 maxDepth 且末端仍指向未取回父节点时标截断，不得记为缺失。
    #[test]
    fn assemble_max_depth_onward_parent_is_truncated_not_missing() {
        let mut ids: Vec<String> = (0..=PARENT_CHAIN_GRAPH_LOOKUP_MAX_DEPTH)
            .map(|depth| format!("n{depth}"))
            .collect();
        ids.push("beyond".to_string());
        let mut ancestors = Vec::new();
        for depth in 0..=PARENT_CHAIN_GRAPH_LOOKUP_MAX_DEPTH {
            let index = depth as usize;
            ancestors.push(ancestor(&ids[index], Some(&ids[index + 1]), depth));
        }
        let fact = assemble_parent_chain_fact("n-start", Some(row("n-start", Some("n0"), ancestors)));

        assert!(fact.truncated);
        assert!(fact.missing_parent_id.is_none());
        assert!(!fact.cycle_detected);
    }
}
