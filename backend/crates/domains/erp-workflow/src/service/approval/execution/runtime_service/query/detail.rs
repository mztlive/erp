//! 已授权单实例详情的 DTO 转换。

use super::super::hidden_not_found;
use super::super::read_auth::{RuntimeReadSubject, current_execution_matches_instance};
use super::RuntimeInstanceListItem;
use crate::error::Result;

impl RuntimeInstanceListItem {
    /// 从已授权主体与精确提交快照构造独立详情。
    ///
    /// # 参数
    /// * `subject` - 已完成实例读取授权的主体、执行及冻结快照。
    ///
    /// # 返回
    /// 返回可独立展示的单据身份、版本、金额、当前责任和运行状态。
    ///
    /// # 错误
    /// 快照或当前执行与实例身份不一致时隐藏为不存在。
    pub(in super::super) fn from_read_subject(subject: &RuntimeReadSubject) -> Result<Self> {
        let instance = &subject.instance;
        let snapshot = &subject.snapshot;
        if snapshot.approval_process_instance_id.as_ref() != instance.base.id
            || snapshot
                .ensure_matches_runtime_subject(
                    subject.document_type,
                    instance.subject.subject_id(),
                    instance.subject_version,
                )
                .is_err()
        {
            return Err(hidden_not_found());
        }
        let execution = subject.current_execution.as_ref();
        if !current_execution_matches_instance(instance, execution) {
            return Err(hidden_not_found());
        }
        Ok(Self {
            instance_id: instance.base.id.clone(),
            status: instance.status.as_str().to_string(),
            current_round_no: instance.current_round_no,
            current_node_key: execution.map(|value| value.node_key.clone()),
            current_node_name: execution.map(|value| value.node_name.clone()),
            current_assignee_participant_id: execution
                .map(|value| value.assignee_participant_id.as_str().to_string()),
            current_assignee_name: execution.map(|value| value.assignee_display_name()),
            document_type: Some(subject.document_type.as_str().to_string()),
            document_id: Some(instance.subject.subject_id().to_string()),
            document_label: Some(snapshot.payload.document_no.clone()),
            subject_version: Some(instance.subject_version),
            total_amount: snapshot.payload.total_amount,
            process_version: Some(instance.definition_version),
            started_at: Some(instance.started_at.unix_secs()),
            latest_rejection_summary: None,
        })
    }
}
