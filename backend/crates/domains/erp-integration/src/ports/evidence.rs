//! W29 权威证据事实与调用方事务内查询端口。

use std::future::Future;
use std::pin::Pin;

use persistence_core::Executor;

use crate::Result;
use crate::dto::ControlledEvidenceRef;
use crate::entity::integration_ops::{
    CanonicalEvidenceReference, IntegrationErrorTask, ReconciliationDifference,
};

/// W29 当前业务项的证据上下文；只包含关联校验所需的稳定身份。
#[derive(Debug, Clone)]
pub struct EvidenceSubject {
    /// W29 业务项 ID。
    pub item_id: String,
    /// 错误任务关联的入站消息。
    pub message_id: Option<String>,
    /// 差异对象类型。
    pub business_object_type: Option<String>,
    /// 错误任务或差异关联的业务对象。
    pub business_object_id: Option<String>,
    /// 差异两侧不可变事实引用。
    pub fact_references: Vec<String>,
}

impl EvidenceSubject {
    /// 以必填业务项构造证据上下文；关联维度默认为空。
    ///
    /// # 参数
    /// * `item_id` - W29 业务项 ID
    ///
    /// # 返回
    /// 返回无关联的证据上下文。
    ///
    /// # 错误
    /// 无。
    pub fn new(item_id: String) -> Self {
        Self {
            item_id,
            message_id: None,
            business_object_type: None,
            business_object_id: None,
            fact_references: Vec::new(),
        }
    }

    /// 从错误任务构造证据上下文。
    ///
    /// # 参数
    /// * `task` - 集成错误任务
    ///
    /// # 返回
    /// 业务项 ID 取任务主键；消息与业务对象按任务字段填入，对象类型与事实引用为空。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn error(task: &IntegrationErrorTask) -> Self {
        Self {
            item_id: task.base.id.clone(),
            message_id: task.message_id.as_ref().map(ToString::to_string),
            business_object_type: None,
            business_object_id: task.business_object_id.clone(),
            fact_references: Vec::new(),
        }
    }

    /// 从对账差异构造证据上下文。
    ///
    /// # 参数
    /// * `difference` - 对账差异
    ///
    /// # 返回
    /// 业务项 ID、对象类型与对象 ID 取差异字段；两侧事实引用按左、右顺序收集，缺侧跳过。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn difference(difference: &ReconciliationDifference) -> Self {
        Self {
            item_id: difference.base.id.clone(),
            message_id: None,
            business_object_type: Some(difference.business_object_type.clone()),
            business_object_id: Some(difference.business_object_id.clone()),
            fact_references: [
                difference.left_fact_reference.clone(),
                difference.right_fact_reference.clone(),
            ]
            .into_iter()
            .flatten()
            .collect(),
        }
    }
}

/// 已由权威仓储重验的证据。
#[derive(Debug, Clone)]
pub struct VerifiedEvidence {
    /// 归一化后的客户端证据引用。
    pub reference: ControlledEvidenceRef,
    /// 可写入领域证据字段的稳定引用。
    pub canonical_reference: CanonicalEvidenceReference,
}

/// 查询原结果的服务端事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OriginalResultFact {
    /// 已找到终态或正式修复事实。
    Terminal(String),
    /// 已在注册适配器内确认没有结果，可以安全重放。
    NoResult,
    /// 当前模型无法权威判断。
    Unknown,
}

pub type EvidenceFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// W29 跨域权威证据端口。
///
/// 新对象类型必须在实现中显式注册并校验状态与业务关联；默认分支失败关闭。
pub trait IntegrationEvidenceAuthority: Send + Sync {
    /// 查询原动作的当前结果。
    ///
    /// # 参数
    /// * `subject` - 当前业务项证据上下文
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 成功时返回终态事实、确认无结果或无法判断。
    ///
    /// # 错误
    /// 实现无法完成权威查询时返回对应错误。
    fn query_original<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, OriginalResultFact>;

    /// 沿服务器锁定的入站消息身份重新排队。
    ///
    /// # 参数
    /// * `subject` - 当前业务项证据上下文
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 成功时返回实现给出的重放结果。
    ///
    /// # 错误
    /// 实现无法完成重放时返回对应错误。
    fn replay_original<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, String>;

    /// 验证既有归集事实已经进入终态。
    ///
    /// # 参数
    /// * `subject` - 当前业务项证据上下文
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 成功时返回实现确认终态后的结果。
    ///
    /// # 错误
    /// 实现无法确认归集终态时返回对应错误。
    fn verify_reattribution<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, String>;

    /// 重验单条受控证据的类型、存在性、终态与业务关联。
    ///
    /// # 参数
    /// * `subject` - 当前业务项证据上下文
    /// * `evidence` - 客户端提交的受控证据引用
    /// * `actor_id` - 当前操作人
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 成功时返回归一化引用与可持久化的稳定引用。
    ///
    /// # 错误
    /// 类型、存在性、终态或业务关联不满足时返回对应错误。
    fn verify_evidence<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        evidence: &'a ControlledEvidenceRef,
        actor_id: &'a str,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, VerifiedEvidence>;

    /// 发现当前对象已经存在且可安全投影的权威证据。
    ///
    /// # 参数
    /// * `subject` - 当前业务项证据上下文
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 成功时返回可投影的受控证据引用。
    ///
    /// # 错误
    /// 实现无法完成发现时返回对应错误。
    fn discover_evidence<'a>(
        &'a self,
        subject: &'a EvidenceSubject,
        executor: &'a mut dyn Executor,
    ) -> EvidenceFuture<'a, Vec<ControlledEvidenceRef>>;
}

#[cfg(test)]
mod tests {
    use super::EvidenceSubject;
    use crate::entity::integration_ops::{
        InboxMessageId, ReconciliationDifference, ReconciliationDifferenceId,
    };

    #[test]
    fn error_subject_preserves_optional_message_and_business_identity() {
        let mut task = crate::entity::integration_ops::integration_error_task::tests::task();
        task.message_id = Some(InboxMessageId::new("msg-context"));
        task.business_object_id = Some("business-context".to_string());
        let subject = EvidenceSubject::error(&task);
        assert_eq!(subject.item_id, task.base.id);
        assert_eq!(subject.message_id.as_deref(), Some("msg-context"));
        assert_eq!(subject.business_object_id.as_deref(), Some("business-context"));
        assert!(subject.business_object_type.is_none());
        assert!(subject.fact_references.is_empty());
        task.message_id = None;
        task.business_object_id = None;
        let subject = EvidenceSubject::error(&task);
        assert!(subject.message_id.is_none());
        assert!(subject.business_object_id.is_none());
    }

    #[test]
    fn difference_subject_preserves_fact_order_and_missing_side() {
        let mut difference = ReconciliationDifference::new(
            ReconciliationDifferenceId::new("diff-context"),
            crate::entity::integration_ops::reconciliation_difference::tests::difference_data(),
        )
        .unwrap();
        difference.left_fact_reference = Some("left-fact".to_string());
        difference.right_fact_reference = Some("right-fact".to_string());
        let subject = EvidenceSubject::difference(&difference);
        assert_eq!(subject.item_id, "diff-context");
        assert!(subject.message_id.is_none());
        assert_eq!(subject.business_object_type.as_deref(), Some(difference.business_object_type.as_str()));
        assert_eq!(subject.business_object_id.as_deref(), Some(difference.business_object_id.as_str()));
        assert_eq!(subject.fact_references, ["left-fact", "right-fact"]);
        difference.left_fact_reference = None;
        assert_eq!(EvidenceSubject::difference(&difference).fact_references, ["right-fact"]);
        difference.left_fact_reference = Some("left-fact".to_string());
        difference.right_fact_reference = None;
        assert_eq!(EvidenceSubject::difference(&difference).fact_references, ["left-fact"]);
    }
}
