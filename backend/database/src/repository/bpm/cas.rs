use bpm::ids::ApprovalNodeExecutionId;
use bpm::model::types::{ApprovalDefinitionStatus, ApprovalNodeExecutionStatus};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessDefinition};
use entity_core::{HasBaseModel, NOT_DELETED_TIMESTAMP_BSON};
use mongodb::bson::{doc, serialize_to_document, Document};
use serde::{Deserialize, Serialize};

use super::{
    i64_version, merge_documents, AssignDocumentNoOutcome, BpmWorkflowRepository, CasReplaceSpec,
    CasWriteOutcome, DEFINITIONS, EXECUTIONS,
};
use crate::executor::Executor;
use crate::{mongo_ops, Error, Result};

impl<'a> BpmWorkflowRepository<'a> {
    pub(super) async fn cas_write_definition(
        &self,
        definition: &ApprovalProcessDefinition,
        expected_lock_version: u64,
        required_status: &[ApprovalDefinitionStatus],
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalProcessDefinition>> {
        let filter = draft_or_status_filter(&definition.base.id, expected_lock_version, required_status)?;
        let required = required_status.to_vec();
        self.cas_replace(
            CasReplaceSpec {
                collection: DEFINITIONS,
                filter,
                entity: definition,
                expected_version: expected_lock_version,
                extra_set: None,
            },
            move |current| required.contains(&current.status),
            executor,
        )
        .await
    }

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
        }
        statuses => {
            filter.insert(
                "status",
                doc! { "$in": statuses.iter().map(|status| status.as_str()).collect::<Vec<_>>() },
            );
        }
    }
    Ok(filter)
}

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

fn next_version_i64(expected_version: u64) -> Result<i64> {
    let next = expected_version
        .checked_add(1)
        .ok_or(Error::EntityMetadataOutOfRange("version"))?;
    i64_version(next)
}

/// 审批任务完成/关闭 CAS 过滤条件。
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
