//! 库存调整撤回命令的不可变业务事实；不从审计文本恢复操作人。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use crate::{CancelStockAdjustmentApprovalRequest, Error, Result};

/// 原审批回执的精确结构化身份。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CancellationCommandIdentity {
    /// 审批回执稳定主键。
    pub receipt_id: String,
    /// 审批命令作用域。
    pub scope_id: String,
    /// 已规范化的幂等键。
    pub idempotency_key: String,
    /// 版本化载荷摘要。
    pub payload_digest: String,
}

/// 同次撤回提交后的历史任务身份与版本。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct StockCancellationTask {
    pub task_id: String,
    pub version: u64,
}

/// 库存调整撤回事实输入；版本均为原命令的期望版本。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockAdjustmentCancellationData {
    pub schema_version: u32,
    pub stock_adjustment_id: String,
    pub instance_id: String,
    pub execution_id: String,
    pub subject_version: u32,
    pub actor_id: String,
    pub reason: String,
    pub command: CancellationCommandIdentity,
    pub audit_event_id: String,
    pub document_version: u64,
    pub instance_version: u64,
    pub execution_version: u64,
    pub task_id: Option<String>,
    pub task_version: Option<u64>,
    pub historical_tasks: Vec<StockCancellationTask>,
    pub blocker_code: Option<String>,
    pub cancelled_at: Instant,
}

/// 同审批回执、单据及终态一起提交的唯一撤回事实。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct StockAdjustmentCancellation {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub data: StockAdjustmentCancellationData,
}

impl StockAdjustmentCancellation {
    /// 构造不可变撤回事实。
    /// # 参数
    /// * `data` - 已授权且计划提交的原命令及取消结果。
    /// # 返回
    /// 返回按审批实例唯一定位的事实。
    /// # 错误
    /// 身份、原因、版本或任务引用不完整。
    pub fn new(data: StockAdjustmentCancellationData) -> Result<Self> {
        let fact = Self { base: BaseModel::new(data.instance_id.clone()), data };
        fact.validate()?;
        Ok(fact)
    }

    /// 校验持久化事实的身份、版本及任务引用完整性。
    /// # 参数
    /// 无。
    /// # 返回
    /// 完整事实返回空值。
    /// # 错误
    /// 损坏或未完成的事实返回冲突，禁止回退审计。
    pub fn validate(&self) -> Result<()> {
        let data = &self.data;
        let texts = [
            &self.base.id,
            &data.stock_adjustment_id,
            &data.instance_id,
            &data.execution_id,
            &data.actor_id,
            &data.reason,
            &data.command.receipt_id,
            &data.command.scope_id,
            &data.command.idempotency_key,
            &data.command.payload_digest,
            &data.audit_event_id,
        ];
        if texts.iter().any(|value| value.trim().is_empty())
            || self.base.id != data.instance_id
            || self.base.is_deleted()
            || data.schema_version != 1
            || data.subject_version == 0
            || [data.document_version, data.instance_version, data.execution_version].contains(&0)
            || data.task_id.is_some() != data.task_version.is_some()
            || data.task_id.as_ref().is_some_and(|id| id.trim().is_empty())
            || data.task_version == Some(0)
            || data.historical_tasks.iter().any(|task| task.task_id.trim().is_empty() || task.version == 0)
            || data.historical_tasks.windows(2).any(|pair| pair[0].task_id >= pair[1].task_id)
        {
            return Err(Error::ConflictError("库存调整撤回结构化事实损坏".into()));
        }
        Ok(())
    }

    /// 精确验证请求是否属于原撤回命令。
    /// # 参数
    /// * `adjustment_id` - 路径中的调整单 ID。
    /// * `request` - 原期望版本及实例身份。
    /// * `actor_id` - 当前认证操作人。
    /// * `reason` - 已规范化原因。
    /// # 返回
    /// 身份和全部请求版本相同返回真。
    /// # 错误
    /// 无；损坏事实返回假。
    pub fn matches_request(
        &self,
        adjustment_id: &str,
        request: &CancelStockAdjustmentApprovalRequest,
        actor_id: &str,
        reason: &str,
    ) -> bool {
        let data = &self.data;
        self.validate().is_ok()
            && data.stock_adjustment_id == adjustment_id
            && data.instance_id == request.approval_process_instance_id
            && data.subject_version == request.expected_subject_version
            && data.document_version == request.expected_version
            && data.instance_version == request.expected_instance_version
            && data.execution_version == request.expected_execution_version
            && data.task_version == request.expected_task_version
            && data.actor_id == actor_id
            && data.reason == reason
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> StockAdjustmentCancellationData {
        StockAdjustmentCancellationData {
            stock_adjustment_id: "adjustment".into(),
            schema_version: 1,
            instance_id: "instance".into(),
            execution_id: "execution".into(),
            subject_version: 1,
            actor_id: "actor".into(),
            reason: "撤回".into(),
            command: CancellationCommandIdentity {
                receipt_id: "receipt".into(),
                scope_id: "scope".into(),
                idempotency_key: "key".into(),
                payload_digest: "digest".into(),
            },
            audit_event_id: "audit".into(),
            document_version: 3,
            instance_version: 4,
            execution_version: 5,
            task_id: Some("task".into()),
            task_version: Some(6),
            historical_tasks: vec![StockCancellationTask { task_id: "task".into(), version: 7 }],
            blocker_code: None,
            cancelled_at: Instant::from_unix_secs(100),
        }
    }

    #[test]
    fn request_proof_requires_original_actor_reason_and_all_versions() {
        let fact = StockAdjustmentCancellation::new(data()).unwrap();
        let request = CancelStockAdjustmentApprovalRequest {
            approval_process_instance_id: "instance".into(),
            expected_subject_version: 1,
            expected_version: 3,
            expected_instance_version: 4,
            expected_execution_version: 5,
            expected_task_version: Some(6),
            reason: "撤回".into(),
            idempotency_key: "key".into(),
        };
        assert!(fact.matches_request("adjustment", &request, "actor", "撤回"));
        assert!(!fact.matches_request("adjustment", &request, "other", "撤回"));
        assert!(!fact.matches_request("adjustment", &request, "actor", "other"));
        let changed = CancelStockAdjustmentApprovalRequest { expected_execution_version: 6, ..request };
        assert!(!fact.matches_request("adjustment", &changed, "actor", "撤回"));
        let mut damaged = fact;
        damaged.data.task_id = None;
        assert!(damaged.validate().is_err());
        assert!(
            StockAdjustmentCancellation::new(StockAdjustmentCancellationData {
                actor_id: " ".into(),
                ..data()
            })
            .is_err()
        );
    }
}
