//! W17 商城快照重新归集操作。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{MallSalesOrderSnapshotId, MasterMappingTaskId, SalesOrderId, SalesOrderRevisionId};
use crate::validation::{normalize_optional_text, normalize_required_text};

const OPERATION_ID_MAX_LEN: usize = 128;
const HASH_MAX_LEN: usize = 128;
const ACTOR_MAX_LEN: usize = 128;
const RESULT_REFERENCE_MAX_LEN: usize = 256;
const FAILURE_CODE_MAX_LEN: usize = 128;
const FAILURE_MESSAGE_MAX_LEN: usize = 1024;

/// 重新归集操作状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReapplyOperationStatus {
    /// 操作已排队。
    Queued,
    /// 操作执行中。
    Running,
    /// 操作已取得可验证业务结果。
    Succeeded,
    /// 操作已明确失败。
    Failed,
    /// 请求结果暂时无法确认。
    Unknown,
}

impl ReapplyOperationStatus {
    /// 返回持久化与协议共用的稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "QUEUED",
            Self::Running => "RUNNING",
            Self::Succeeded => "SUCCEEDED",
            Self::Failed => "FAILED",
            Self::Unknown => "UNKNOWN",
        }
    }

    /// 判断操作是否已经取得明确终态。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

/// 重新归集操作创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallSnapshotReapplyOperationData {
    /// 映射任务。
    pub mapping_task_id: MasterMappingTaskId,
    /// 沿用的来源快照。
    pub source_snapshot_id: MallSalesOrderSnapshotId,
    /// 幂等键摘要；不持久化调用方原文。
    pub idempotency_key_hash: String,
    /// 完整命令摘要，用于阻断同键异参。
    pub command_fingerprint: String,
    /// 发起人。
    pub requested_by: String,
    /// 发起时间。
    pub requested_at: Instant,
}

/// W17 独立重新归集操作实体。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct MallSnapshotReapplyOperation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 映射任务。
    pub mapping_task_id: MasterMappingTaskId,
    /// 沿用的来源快照。
    pub source_snapshot_id: MallSalesOrderSnapshotId,
    /// 幂等键摘要。
    pub idempotency_key_hash: String,
    /// 命令摘要。
    pub command_fingerprint: String,
    /// 当前状态。
    pub status: ReapplyOperationStatus,
    /// 发起人。
    pub requested_by: String,
    /// 发起时间。
    pub requested_at: Instant,
    /// 最后状态时间。
    pub last_updated_at: Instant,
    /// 形成的销售单。
    pub sales_order_id: Option<SalesOrderId>,
    /// 形成的销售版本。
    pub sales_order_revision_id: Option<SalesOrderRevisionId>,
    /// 适用的应收结果引用。
    pub receivable_result_reference: Option<String>,
    /// 明确失败代码。
    pub failure_code: Option<String>,
    /// 明确失败说明。
    pub failure_message: Option<String>,
}

impl MallSnapshotReapplyOperation {
    /// 创建已排队的重新归集操作。
    ///
    /// # 错误
    /// 操作身份、摘要或发起人为空或超长时返回错误。
    pub fn new(operation_id: String, data: MallSnapshotReapplyOperationData) -> Result<Self> {
        let operation_id = normalize_required_text(
            operation_id,
            "重新归集操作ID不能为空",
            OPERATION_ID_MAX_LEN,
            "重新归集操作ID过长",
        )?;
        let idempotency_key_hash = normalize_required_text(
            data.idempotency_key_hash,
            "幂等摘要不能为空",
            HASH_MAX_LEN,
            "幂等摘要过长",
        )?;
        let command_fingerprint = normalize_required_text(
            data.command_fingerprint,
            "命令摘要不能为空",
            HASH_MAX_LEN,
            "命令摘要过长",
        )?;
        let requested_by =
            normalize_required_text(data.requested_by, "发起人不能为空", ACTOR_MAX_LEN, "发起人过长")?;
        Ok(Self {
            base: BaseModel::new(operation_id),
            mapping_task_id: data.mapping_task_id,
            source_snapshot_id: data.source_snapshot_id,
            idempotency_key_hash,
            command_fingerprint,
            status: ReapplyOperationStatus::Queued,
            requested_by,
            requested_at: data.requested_at,
            last_updated_at: data.requested_at,
            sales_order_id: None,
            sales_order_revision_id: None,
            receivable_result_reference: None,
            failure_code: None,
            failure_message: None,
        })
    }

    /// 登记可验证的销售结果。
    ///
    /// # 错误
    /// 操作已取得终态或应收引用超长时返回错误。
    pub fn succeed(
        &mut self,
        sales_order_id: SalesOrderId,
        sales_order_revision_id: SalesOrderRevisionId,
        receivable_result_reference: Option<String>,
        at: Instant,
    ) -> Result<()> {
        self.ensure_mutable()?;
        self.receivable_result_reference = normalize_optional_text(
            receivable_result_reference,
            "应收结果引用",
            RESULT_REFERENCE_MAX_LEN,
        )?;
        self.status = ReapplyOperationStatus::Succeeded;
        self.sales_order_id = Some(sales_order_id);
        self.sales_order_revision_id = Some(sales_order_revision_id);
        self.failure_code = None;
        self.failure_message = None;
        self.last_updated_at = at;
        Ok(())
    }

    /// 登记明确失败结果。
    ///
    /// # 错误
    /// 操作已取得终态，或失败代码/说明为空或超长时返回错误。
    pub fn fail(&mut self, code: String, message: String, at: Instant) -> Result<()> {
        self.ensure_mutable()?;
        self.failure_code = Some(normalize_required_text(
            code,
            "失败代码不能为空",
            FAILURE_CODE_MAX_LEN,
            "失败代码过长",
        )?);
        self.failure_message = Some(normalize_required_text(
            message,
            "失败说明不能为空",
            FAILURE_MESSAGE_MAX_LEN,
            "失败说明过长",
        )?);
        self.status = ReapplyOperationStatus::Failed;
        self.last_updated_at = at;
        Ok(())
    }

    /// 将非终态操作标为结果待确认。
    ///
    /// # 错误
    /// 操作已经取得明确终态时返回错误。
    pub fn mark_unknown(&mut self, at: Instant) -> Result<()> {
        self.ensure_mutable()?;
        self.status = ReapplyOperationStatus::Unknown;
        self.last_updated_at = at;
        Ok(())
    }

    fn ensure_mutable(&self) -> Result<()> {
        if self.status.is_terminal() {
            return Err(Error::from("重新归集操作已取得明确结果"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operation() -> MallSnapshotReapplyOperation {
        MallSnapshotReapplyOperation::new(
            "op-1".to_string(),
            MallSnapshotReapplyOperationData {
                mapping_task_id: MasterMappingTaskId::new("mapping-1"),
                source_snapshot_id: MallSalesOrderSnapshotId::new("snapshot-1"),
                idempotency_key_hash: "a".repeat(64),
                command_fingerprint: "b".repeat(64),
                requested_by: "user-1".to_string(),
                requested_at: Instant::from_unix_secs(100),
            },
        )
        .unwrap()
    }

    #[test]
    fn operation_records_success_once() {
        let mut operation = operation();
        operation
            .succeed(
                SalesOrderId::new("so-1"),
                SalesOrderRevisionId::new("sor-1"),
                Some("ra-1".to_string()),
                Instant::from_unix_secs(101),
            )
            .unwrap();
        assert_eq!(operation.status, ReapplyOperationStatus::Succeeded);
        assert!(operation
            .fail(
                "LATE_FAILURE".to_string(),
                "不得覆盖成功".to_string(),
                Instant::from_unix_secs(102),
            )
            .is_err());
    }

    #[test]
    fn operation_records_queryable_failure() {
        let mut operation = operation();
        operation
            .fail(
                "APPLICATION_NOT_REGISTERED".to_string(),
                "当前快照尚无可执行归集器".to_string(),
                Instant::from_unix_secs(101),
            )
            .unwrap();
        assert_eq!(operation.status, ReapplyOperationStatus::Failed);
        assert_eq!(
            operation.failure_code.as_deref(),
            Some("APPLICATION_NOT_REGISTERED")
        );
    }
}
