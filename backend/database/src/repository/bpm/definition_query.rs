use std::collections::{HashMap, HashSet};

use bpm::ids::ApprovalProcessDefinitionId;
use bpm::model::types::ApprovalDefinitionStatus;
use bpm::model::{ApprovalNodeDefinition, ApprovalProcessDefinition, ApprovalTransitionDefinition};
use bpm::ProcessKind;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use super::{
    clamp_limit, find_limited, BpmWorkflowRepository, DefinitionCatalogRow, DefinitionCatalogStatusFact,
    DefinitionGraph, LatestDefinitionVersionProjection, DEFINITIONS, MAX_CATALOG_STATUS_ROWS,
    MAX_DEFINITION_GRAPH_DOCS, MAX_DEFINITION_VERSIONS, NODE_DEFINITIONS, TRANSITION_DEFINITIONS,
};
use crate::executor::Executor;
use crate::{mongo_ops, Error, Result};

impl<'a> BpmWorkflowRepository<'a> {
    /// 查询同一流程种类当前唯一已发布定义。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_published_by_process_kind(
        &self,
        process_kind: ProcessKind,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessDefinition>> {
        self.definitions()
            .find_one(published_kind_filter(process_kind), executor)
            .await
    }

    /// 读取同一流程种类未删除定义的最高持久化业务版本。
    ///
    /// # 参数
    /// * `process_kind` - 需要读取版本事实的流程种类
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按业务版本倒序命中的首个版本；没有历史定义时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或投影反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 查询只投影版本字段并限制一条，不分配或递增版本，且必须保留软删除过滤。
    pub async fn latest_definition_version(
        &self,
        process_kind: ProcessKind,
        executor: &mut dyn Executor,
    ) -> Result<Option<u32>> {
        let options = latest_definition_version_options();
        let rows = mongo_ops::find_many(
            &self
                .db
                .collection::<LatestDefinitionVersionProjection>(DEFINITIONS),
            definition_versions_filter(process_kind),
            options,
            executor,
        )
        .await?;
        Ok(latest_definition_version_from_rows(rows))
    }

    /// 列出同一流程种类的历史定义版本，按业务版本倒序且有上限。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_definition_versions(
        &self,
        process_kind: ProcessKind,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalProcessDefinition>> {
        find_limited(
            &self.db.collection(DEFINITIONS),
            definition_versions_filter(process_kind),
            definition_versions_sort(),
            definition_versions_limit(MAX_DEFINITION_VERSIONS as u32),
            executor,
        )
        .await
    }

    /// 查询同一流程种类当前活动草稿。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_active_draft(
        &self,
        process_kind: ProcessKind,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessDefinition>> {
        self.definitions()
            .find_one(active_draft_filter(process_kind), executor)
            .await
    }

    /// 批量读取定义及其节点、连线，禁止按节点 N+1。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn load_definition_graph(
        &self,
        definition_id: &ApprovalProcessDefinitionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<DefinitionGraph>> {
        let Some(definition) = self
            .definitions()
            .find_by_id(definition_id.as_ref(), executor)
            .await?
        else {
            return Ok(None);
        };
        let nodes = self.load_definition_nodes(definition_id, executor).await?;
        let transitions = self.load_definition_transitions(definition_id, executor).await?;
        Ok(Some(DefinitionGraph {
            definition,
            nodes,
            transitions,
        }))
    }

    /// 按流程种类列表一次读取发布与草稿版本目录投影。
    ///
    /// # 参数
    /// * `process_kinds` - 调用方已去权后的流程种类；可含重复
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 按去重后的输入顺序返回每种流程的发布/草稿版本；缺失种类两个版本均为空。
    ///
    /// # 错误
    /// 同一流程同一状态出现多条未删除定义，或 MongoDB 查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 查询次数固定为 0（空输入）或 1，不随种类数量线性增长；退役定义不进入投影；
    /// 历史重复发布/草稿状态必须失败关闭，不得取第一条。
    pub async fn definition_catalog_facts(
        &self,
        process_kinds: &[ProcessKind],
        executor: &mut dyn Executor,
    ) -> Result<Vec<DefinitionCatalogStatusFact>> {
        let unique_kinds = unique_process_kinds(process_kinds);
        if unique_kinds.is_empty() {
            return Ok(Vec::new());
        }
        let options = definition_catalog_options(unique_kinds.len());
        let rows = mongo_ops::find_many(
            &self.db.collection::<DefinitionCatalogRow>(DEFINITIONS),
            definition_catalog_filter(&unique_kinds),
            options,
            executor,
        )
        .await?;
        group_definition_catalog_rows(&unique_kinds, rows)
    }

    /// 读取某流程种类当前唯一已发布定义及其节点、连线，定义文档只读一次。
    ///
    /// # 参数
    /// * `process_kind` - 需要绑定或复制的流程种类
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 存在唯一已发布定义时返回完整图；无发布时返回 `None`。
    ///
    /// # 错误
    /// 同一流程出现多条已发布定义，或 MongoDB 查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 草稿与退役不得命中；重复已发布必须失败关闭；本方法不把状态解释为可绑定结论。
    pub async fn load_published_definition_graph(
        &self,
        process_kind: ProcessKind,
        executor: &mut dyn Executor,
    ) -> Result<Option<DefinitionGraph>> {
        let options = FindOptions::builder()
            .limit(2)
            .sort(doc! { "definition_version": -1, "id": 1 })
            .build();
        let rows = mongo_ops::find_many(
            &self.db.collection::<ApprovalProcessDefinition>(DEFINITIONS),
            published_kind_docs_filter(process_kind),
            options,
            executor,
        )
        .await?;
        let Some(definition) = unique_published_definition(rows)? else {
            return Ok(None);
        };
        let definition_id = ApprovalProcessDefinitionId::new(definition.base.id.clone());
        let nodes = self.load_definition_nodes(&definition_id, executor).await?;
        let transitions = self.load_definition_transitions(&definition_id, executor).await?;
        Ok(Some(DefinitionGraph {
            definition,
            nodes,
            transitions,
        }))
    }

    async fn load_definition_nodes(
        &self,
        definition_id: &ApprovalProcessDefinitionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalNodeDefinition>> {
        find_limited(
            &self.db.collection(NODE_DEFINITIONS),
            definition_child_filter(definition_id),
            doc! { "display_order": 1 },
            MAX_DEFINITION_GRAPH_DOCS,
            executor,
        )
        .await
    }

    async fn load_definition_transitions(
        &self,
        definition_id: &ApprovalProcessDefinitionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalTransitionDefinition>> {
        find_limited(
            &self.db.collection(TRANSITION_DEFINITIONS),
            definition_child_filter(definition_id),
            doc! { "from_node_key": 1, "event": 1 },
            definition_graph_transition_limit(),
            executor,
        )
        .await
    }
}

fn published_kind_filter(process_kind: ProcessKind) -> Document {
    doc! {
        "process_kind": process_kind.as_str(),
        "status": ApprovalDefinitionStatus::Published.as_str(),
    }
}

/// 构造直接 `find_many` 使用的已发布定义过滤，含软删除约束。
///
/// # 参数
/// * `process_kind` - 流程种类
///
/// # 返回
/// 返回 `PUBLISHED`、流程种类与 `deleted_at` 条件。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// `Repository::find_one` 会自行补软删除条件；`mongo_ops::find_many` 必须显式带上。
pub(super) fn published_kind_docs_filter(process_kind: ProcessKind) -> Document {
    let mut filter = published_kind_filter(process_kind);
    filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
    filter
}

fn active_draft_filter(process_kind: ProcessKind) -> Document {
    doc! {
        "process_kind": process_kind.as_str(),
        "status": ApprovalDefinitionStatus::Draft.as_str(),
    }
}

pub(super) fn definition_child_filter(definition_id: &ApprovalProcessDefinitionId) -> Document {
    doc! {
        "process_definition_id": definition_id.as_ref(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 构造定义历史查询条件，前缀对齐 `idx_approval_process_definitions_history`。
///
/// # 参数
/// * `process_kind` - 流程种类
///
/// # 返回
/// 返回含 `process_kind` 与软删除约束的查询文档。
pub(super) fn definition_versions_filter(process_kind: ProcessKind) -> Document {
    doc! {
        "process_kind": process_kind.as_str(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 返回定义历史固定排序文档。
///
/// # 返回
/// 返回 `{ definition_version: -1 }`。
fn definition_versions_sort() -> Document {
    doc! { "definition_version": -1 }
}

/// 构建最高定义业务版本查询的限制与最小投影。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回按业务版本降序、限制一条且只读取版本字段的 MongoDB 查询选项。
///
/// # 错误
/// 不返回错误。
///
/// # 关键业务约束
/// 过滤条件由调用点单独提供；本选项不得读取完整定义或分配下一版本。
pub(super) fn latest_definition_version_options() -> FindOptions {
    FindOptions::builder()
        .sort(definition_versions_sort())
        .limit(1)
        .projection(doc! { "definition_version": 1, "_id": 0 })
        .build()
}

/// 从已按最高版本查询返回的投影行中读取首个业务版本。
///
/// # 参数
/// * `rows` - MongoDB 按查询选项返回的零或一条版本投影
///
/// # 返回
/// 有结果时返回首条业务版本；无历史定义时返回 `None`。
///
/// # 错误
/// 不返回错误。
///
/// # 关键业务约束
/// 不在内存重新排序或选择最大值，保持 Repository 查询契约单一。
pub(super) fn latest_definition_version_from_rows(
    rows: Vec<LatestDefinitionVersionProjection>,
) -> Option<u32> {
    rows.into_iter().next().map(|row| row.definition_version)
}

/// 去除重复流程种类并保持首次出现顺序。
///
/// # 参数
/// * `process_kinds` - 调用方提供的流程种类，可为空或含重复
///
/// # 返回
/// 返回去重后的种类列表。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 空输入必须得到空列表，查询次数不得因此变为按种类循环。
pub(super) fn unique_process_kinds(process_kinds: &[ProcessKind]) -> Vec<ProcessKind> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for kind in process_kinds {
        if seen.insert(*kind) {
            unique.push(*kind);
        }
    }
    unique
}

/// 构造目录批量查询过滤：指定种类的未删除草稿或已发布定义。
///
/// # 参数
/// * `process_kinds` - 已去重的流程种类
///
/// # 返回
/// 返回含 `$in`、状态集合与软删除约束的查询文档。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 退役定义不得命中；软删除过滤必须保留。
pub(super) fn definition_catalog_filter(process_kinds: &[ProcessKind]) -> Document {
    doc! {
        "process_kind": {
            "$in": process_kinds.iter().map(|kind| kind.as_str()).collect::<Vec<_>>()
        },
        "status": {
            "$in": [
                ApprovalDefinitionStatus::Draft.as_str(),
                ApprovalDefinitionStatus::Published.as_str(),
            ]
        },
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 构造目录批量查询选项：最小投影、稳定排序与固定上限。
///
/// # 参数
/// * `kind_count` - 去重后的种类数
///
/// # 返回
/// 返回只读取种类/状态/版本的查询选项。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 上限必须能发现重复状态，不得用 `limit=种类数` 掩盖脏数据。
pub(super) fn definition_catalog_options(kind_count: usize) -> FindOptions {
    let limit = i64::try_from(kind_count.saturating_mul(4))
        .unwrap_or(MAX_CATALOG_STATUS_ROWS)
        .clamp(1, MAX_CATALOG_STATUS_ROWS);
    FindOptions::builder()
        .sort(doc! { "process_kind": 1, "status": 1, "id": 1 })
        .limit(limit)
        .projection(doc! {
            "process_kind": 1,
            "status": 1,
            "definition_version": 1,
            "_id": 0,
        })
        .build()
}

/// 把目录投影行归组为每种流程至多一条发布与一条草稿事实。
///
/// # 参数
/// * `process_kinds` - 已去重的请求种类，决定输出顺序
/// * `rows` - 仓储一次查询返回的草稿/发布投影
///
/// # 返回
/// 返回与请求种类等长的事实列表；缺失种类两个版本均为空。
///
/// # 错误
/// 同一流程同一状态重复，或投影含退役状态时返回元数据越界。
///
/// # 关键业务约束
/// 不得取第一条掩盖重复；退役不得被解释为发布或草稿。
pub(super) fn group_definition_catalog_rows(
    process_kinds: &[ProcessKind],
    rows: Vec<DefinitionCatalogRow>,
) -> Result<Vec<DefinitionCatalogStatusFact>> {
    let requested: HashSet<ProcessKind> = process_kinds.iter().copied().collect();
    let mut by_kind: HashMap<ProcessKind, (Option<u32>, Option<u32>)> = HashMap::new();
    for row in rows {
        if !requested.contains(&row.process_kind) {
            continue;
        }
        let slot = by_kind.entry(row.process_kind).or_insert((None, None));
        match row.status {
            ApprovalDefinitionStatus::Published => {
                if slot.0.replace(row.definition_version).is_some() {
                    return Err(Error::EntityMetadataOutOfRange(
                        "duplicate published definition catalog status",
                    ));
                }
            }
            ApprovalDefinitionStatus::Draft => {
                if slot.1.replace(row.definition_version).is_some() {
                    return Err(Error::EntityMetadataOutOfRange(
                        "duplicate draft definition catalog status",
                    ));
                }
            }
            ApprovalDefinitionStatus::Retired => {
                return Err(Error::EntityMetadataOutOfRange(
                    "retired definition in catalog projection",
                ));
            }
        }
    }
    Ok(process_kinds
        .iter()
        .map(|process_kind| {
            let (published_version, draft_version) = by_kind.remove(process_kind).unwrap_or((None, None));
            DefinitionCatalogStatusFact {
                process_kind: *process_kind,
                published_version,
                draft_version,
            }
        })
        .collect())
}

/// 从已发布查询结果取出唯一发布定义。
///
/// # 参数
/// * `rows` - 按发布过滤返回的定义
///
/// # 返回
/// 无命中返回 `None`；恰好一条已发布定义时返回该定义。
///
/// # 错误
/// 命中多于一条时返回元数据越界。
///
/// # 关键业务约束
/// 不得取第一条掩盖历史重复发布。
pub(super) fn unique_published_definition(
    rows: Vec<ApprovalProcessDefinition>,
) -> Result<Option<ApprovalProcessDefinition>> {
    match rows.len() {
        0 => Ok(None),
        1 => Ok(rows.into_iter().next()),
        _ => Err(Error::EntityMetadataOutOfRange("duplicate published definition")),
    }
}

/// 将定义历史请求页大小夹紧到 `[1, MAX_DEFINITION_VERSIONS]`。
///
/// # 参数
/// * `limit` - 调用方请求条数
///
/// # 返回
/// 返回可交给 MongoDB `limit` 的有界整数。
fn definition_versions_limit(limit: u32) -> i64 {
    clamp_limit(limit, MAX_DEFINITION_VERSIONS)
}

/// 返回定义连线一次批量读取上限（节点上限的两倍）。
///
/// # 返回
/// 返回 `MAX_DEFINITION_GRAPH_DOCS.saturating_mul(2)`。
pub(super) fn definition_graph_transition_limit() -> i64 {
    MAX_DEFINITION_GRAPH_DOCS.saturating_mul(2)
}
