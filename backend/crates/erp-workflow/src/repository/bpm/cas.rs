use bpm::ids::ApprovalNodeExecutionId;
use bpm::model::types::{ApprovalDefinitionStatus, ApprovalNodeExecutionStatus};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessDefinition};
use entity_core::{HasBaseModel, NOT_DELETED_TIMESTAMP_BSON};
use mongodb::bson::{Document, doc, serialize_to_document};
use persistence_core::{Error, Executor, Result, mongo_ops};
use serde::{Deserialize, Serialize};

use super::{
    AssignDocumentNoOutcome, BpmWorkflowRepository, CasReplaceSpec, CasWriteOutcome, DEFINITIONS, EXECUTIONS,
    i64_version, merge_documents,
};

impl<'a> BpmWorkflowRepository<'a> {
    /// 按定义版本和允许状态写入，并使用当前状态分类 CAS 未命中。
    ///
    /// # 参数
    /// * `definition` - 待写回的流程定义。
    /// * `expected_lock_version` - 调用方持有的定义锁版本。
    /// * `required_status` - 允许命中的定义状态。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 命中时返回 [`CasWriteOutcome::Applied`]；未命中时按当前文档返回缺失、版本冲突或状态变化。
    ///
    /// # 错误
    /// 版本无法表示为 BSON 整数、实体无法序列化，或 MongoDB 更新与回读失败时返回错误。
    pub(super) async fn cas_write_definition(
        &self,
        definition: &ApprovalProcessDefinition,
        expected_lock_version: u64,
        required_status: &[ApprovalDefinitionStatus],
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalProcessDefinition>> {
        let filter = draft_or_status_filter(&definition.base.id, expected_lock_version, required_status)?;
        self.cas_replace(
            CasReplaceSpec {
                collection: DEFINITIONS,
                filter,
                entity: definition,
                expected_version: expected_lock_version,
                extra_set: None,
            },
            |current| required_status.contains(&current.status),
            executor,
        )
        .await
    }

    /// 按执行版本和单一要求状态结束节点执行，未命中时分类当前执行。
    ///
    /// # 参数
    /// * `execution` - 待写回的节点执行。
    /// * `expected_execution_version` - 调用方持有的执行版本。
    /// * `required_status` - 允许结束的当前状态。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 命中时返回 [`CasWriteOutcome::Applied`]；未命中时返回缺失、版本冲突或状态变化。
    ///
    /// # 错误
    /// 版本无法表示为 BSON 整数、实体无法序列化，或 MongoDB 更新与回读失败时返回错误。
    pub(super) async fn cas_end_execution(
        &self,
        execution: &ApprovalNodeExecution,
        expected_execution_version: u64,
        required_status: ApprovalNodeExecutionStatus,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalNodeExecution>> {
        let filter = execution_end_filter(&execution.base.id, expected_execution_version, required_status)?;
        self.cas_replace(
            CasReplaceSpec {
                collection: EXECUTIONS,
                filter,
                entity: execution,
                expected_version: expected_execution_version,
                extra_set: None,
            },
            move |current| current.status == required_status,
            executor,
        )
        .await
    }

    /// 按预期版本条件替换文档；未命中时回读并用 `status_matches` 分类。
    ///
    /// # 参数
    /// * `spec` - 集合、过滤条件、实体与预期版本。
    /// * `status_matches` - 判断回读文档的状态是否仍符合写入前提。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 匹配数大于 0 时返回版本已加一的 [`CasWriteOutcome::Applied`]；否则返回 [`classify_cas_miss`] 的分类。
    ///
    /// # 错误
    /// 下一版本越界、实体无法序列化，或 MongoDB 更新与回读失败时返回错误。
    pub(super) async fn cas_replace<T, F>(
        &self,
        spec: CasReplaceSpec<'_, T>,
        status_matches: F,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<T>>
    where
        T: Serialize + for<'de> Deserialize<'de> + HasBaseModel + Clone + Send + Sync,
        F: Fn(&T) -> bool,
    {
        let next_version = next_version_i64(spec.expected_version)?;
        let mut set_doc = serialize_to_document(spec.entity)?;
        set_doc.insert("version", next_version);
        if let Some(extra_set) = spec.extra_set {
            merge_documents(&mut set_doc, extra_set);
        }
        let matched = mongo_ops::update_one(
            &self.db.collection::<T>(spec.collection),
            spec.filter,
            doc! { "$set": set_doc },
            false,
            executor,
        )
        .await?
        .matched_count;
        if matched > 0 {
            let mut applied = spec.entity.clone();
            applied.base_mut().version = spec.expected_version.saturating_add(1);
            return Ok(CasWriteOutcome::Applied(applied));
        }
        let current = mongo_ops::find_one(
            &self.db.collection::<T>(spec.collection),
            doc! { "id": spec.entity.base().id.as_str(), "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await?;
        Ok(classify_cas_miss(current, spec.expected_version, status_matches))
    }
}

/// 构造定义 CAS 过滤：主键、版本、软删除，以及单个或一组允许状态。
///
/// # 参数
/// * `id` - 定义主键。
/// * `expected_version` - 预期锁版本。
/// * `required_status` - 允许的定义状态；多于一个时写成 `$in`。
///
/// # 返回
/// 返回可交给更新条件的查询文档。
///
/// # 错误
/// 版本无法表示为 BSON 整数时返回错误。
pub(super) fn draft_or_status_filter(
    id: &str,
    expected_version: u64,
    required_status: &[ApprovalDefinitionStatus],
) -> Result<Document> {
    let expected = i64_version(expected_version)?;
    let mut filter = doc! {
        "id": id,
        "version": expected,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    };
    match required_status {
        [status] => {
            filter.insert("status", status.as_str());
        },
        statuses => {
            filter.insert(
                "status",
                doc! { "$in": statuses.iter().map(|status| status.as_str()).collect::<Vec<_>>() },
            );
        },
    }
    Ok(filter)
}

/// 构造结束执行的 CAS 过滤：主键、版本、要求状态和软删除。
///
/// # 参数
/// * `id` - 节点执行主键。
/// * `expected_version` - 预期执行版本。
/// * `required_status` - 允许结束的当前状态。
///
/// # 返回
/// 返回可交给更新条件的查询文档。
///
/// # 错误
/// 版本无法表示为 BSON 整数时返回错误。
pub(super) fn execution_end_filter(
    id: &str,
    expected_version: u64,
    required_status: ApprovalNodeExecutionStatus,
) -> Result<Document> {
    Ok(doc! {
        "id": id,
        "version": i64_version(expected_version)?,
        "status": required_status.as_str(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    })
}

/// 按当前文档分类 CAS 未命中：不存在、版本冲突或状态变化。
///
/// 版本仍等于预期且 `status_matches` 仍成立时也返回 [`CasWriteOutcome::VersionConflict`]，
/// 因为条件更新未命中却回读到同一前提。
///
/// # 参数
/// * `current` - 按主键回读到的当前文档；没有时为 `None`。
/// * `expected_version` - 写入时期望的版本。
/// * `status_matches` - 判断当前状态是否仍允许该写入。
///
/// # 返回
/// 无文档时返回 [`CasWriteOutcome::NotFound`]；版本不同，或版本相同且状态谓词仍成立时返回
/// [`CasWriteOutcome::VersionConflict`]；版本相同但状态谓词不成立时返回 [`CasWriteOutcome::StatusChanged`]。
///
/// # 错误
/// 不返回错误。
pub fn classify_cas_miss<T: HasBaseModel>(
    current: Option<T>,
    expected_version: u64,
    status_matches: impl Fn(&T) -> bool,
) -> CasWriteOutcome<T> {
    let Some(current) = current else {
        return CasWriteOutcome::NotFound;
    };
    if current.base().version != expected_version {
        return CasWriteOutcome::VersionConflict(current);
    }
    if status_matches(&current) {
        return CasWriteOutcome::VersionConflict(current);
    }
    CasWriteOutcome::StatusChanged(current)
}

/// 期望版本加一后必须能表示为 BSON `i64`。
fn next_version_i64(expected_version: u64) -> Result<i64> {
    let next = expected_version.checked_add(1).ok_or(Error::EntityMetadataOutOfRange("version"))?;
    i64_version(next)
}

/// 审批任务完成/关闭 CAS 过滤条件。
///
/// # 参数
/// * `id` - 工作项主键。
/// * `expected_task_version` - 加载时的任务版本。
/// * `approval_node_execution_id` - 必须仍绑定的节点执行。
///
/// # 返回
/// 返回同时约束开放状态、版本、执行身份和软删除的查询文档。
///
/// # 错误
/// 版本无法表示为 BSON 整数时返回错误。
pub fn approval_task_cas_filter(
    id: &str,
    expected_task_version: u64,
    approval_node_execution_id: &ApprovalNodeExecutionId,
) -> Result<Document> {
    Ok(doc! {
        "id": id,
        "version": i64_version(expected_task_version)?,
        "status": "OPEN",
        "approval_node_execution_id": approval_node_execution_id.as_ref(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    })
}

/// 一次性编号赋值 CAS 过滤条件。空字符串与 `null` 均视为未分配。
///
/// # 参数
/// * `id` - 单据注册行主键。
/// * `expected_version` - 预期乐观锁版本。
///
/// # 返回
/// 返回只命中未分配编号行的查询文档。
///
/// # 错误
/// 版本无法表示为 BSON 整数时返回错误。
pub fn assign_document_no_filter(id: &str, expected_version: u64) -> Result<Document> {
    Ok(doc! {
        "id": id,
        "version": i64_version(expected_version)?,
        "$or": [
            { "document_no": "" },
            { "document_no": mongodb::bson::Bson::Null },
        ],
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    })
}

/// 按当前事实分类一次性编号赋值未命中。
///
/// # 参数
/// * `current` - 按主键回读到的当前行；没有时为 `None`。
/// * `expected_version` - 赋值时期望的版本。
/// * `requested_document_no` - 本次要写入的正式编号。
/// * `version_of` - 从当前行读取版本。
/// * `document_no_of` - 从当前行读取已有编号。
///
/// # 返回
/// 无行时返回 [`AssignDocumentNoOutcome::NotFound`]；已有相同非空编号时返回
/// [`AssignDocumentNoOutcome::SamePayload`]；已有不同非空编号时返回
/// [`AssignDocumentNoOutcome::NumberConflict`]；编号仍为空时返回
/// [`AssignDocumentNoOutcome::VersionConflict`]。
///
/// # 错误
/// 不返回错误。
pub fn classify_assign_document_no_miss<T>(
    current: Option<T>,
    expected_version: u64,
    requested_document_no: &str,
    version_of: impl Fn(&T) -> u64,
    document_no_of: impl Fn(&T) -> &str,
) -> AssignDocumentNoOutcome<T> {
    let Some(current) = current else {
        return AssignDocumentNoOutcome::NotFound;
    };
    let existing_no = document_no_of(&current);
    if !existing_no.is_empty() && existing_no == requested_document_no {
        return AssignDocumentNoOutcome::SamePayload(current);
    }
    if !existing_no.is_empty() {
        return AssignDocumentNoOutcome::NumberConflict(current);
    }
    if version_of(&current) != expected_version {
        return AssignDocumentNoOutcome::VersionConflict(current);
    }
    AssignDocumentNoOutcome::VersionConflict(current)
}
