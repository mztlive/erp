//! 审批启动回放所需的冻结运行身份。

use bpm::model::{ApprovalProcessInstance, ProcessKind, SubjectRef};

use crate::entity::document_registry::business_document::ApprovalDefinitionBinding;

/// 启动人、主题与定义绑定共同确定的预期审批身份。
pub struct ApprovalStartIdentity<'a> {
    /// 入口按单据类型确定的流程种类。
    pub process_kind: ProcessKind,
    /// 已冻结的审批主题。
    pub subject: &'a SubjectRef,
    /// 已冻结的主题版本。
    pub subject_version: u32,
    /// 已冻结的审批定义绑定。
    pub binding: &'a ApprovalDefinitionBinding,
    /// 原启动人账号 ID。
    pub actor_id: &'a str,
}

impl ApprovalStartIdentity<'_> {
    /// 判断回执指定的精确实例是否保留预期冻结身份。
    ///
    /// # 参数
    /// * `instance` - 按回执结果主键读取的实例。
    /// * `result_ref` - 回执保存的实例主键。
    ///
    /// # 返回
    /// 主键、流程、主题、版本、启动人与定义绑定全部相同时返回 `true`。
    ///
    /// # 错误
    /// 不返回错误；实例当前状态不参与启动身份判定。
    pub fn matches_instance(&self, instance: &ApprovalProcessInstance, result_ref: &str) -> bool {
        instance.base.id == result_ref
            && instance.process_kind == self.process_kind
            && instance.subject.subject_kind() == self.subject.subject_kind()
            && instance.subject.subject_id() == self.subject.subject_id()
            && instance.subject_version == self.subject_version
            && instance.started_by.as_str() == self.actor_id
            && instance.process_definition_id == self.binding.approval_process_definition_id
            && instance.definition_version == self.binding.approval_definition_version
    }
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::ApprovalProcessInstanceStatus;
    use bpm::model::{NewProcessInstance, ParticipantId, Timestamp};
    use erp_core::common::time::Instant;

    use super::*;

    fn binding() -> ApprovalDefinitionBinding {
        ApprovalDefinitionBinding {
            approval_process_definition_id: ApprovalProcessDefinitionId::new("definition-1"),
            approval_definition_version: 2,
            approval_binding_version: 1,
            approval_definition_bound_at: Instant::from_unix_secs(1),
        }
    }

    fn instance() -> ApprovalProcessInstance {
        ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("instance-1"),
            process_definition_id: ApprovalProcessDefinitionId::new("definition-1"),
            definition_version: 2,
            process_kind: ProcessKind::SalesOrder,
            subject: SubjectRef::new("sales_order", "order-1").unwrap(),
            subject_version: 3,
            started_by: ParticipantId::new("actor-1").unwrap(),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap()
    }

    #[test]
    fn matches_original_identity_after_instance_has_ended() {
        let binding = binding();
        let subject = SubjectRef::new("sales_order", "order-1").unwrap();
        let expected = ApprovalStartIdentity {
            process_kind: ProcessKind::SalesOrder,
            subject: &subject,
            subject_version: 3,
            binding: &binding,
            actor_id: "actor-1",
        };
        let mut instance = instance();
        instance.status = ApprovalProcessInstanceStatus::Approved;
        assert!(expected.matches_instance(&instance, "instance-1"));
    }

    #[test]
    fn rejects_every_frozen_identity_mismatch() {
        let binding = binding();
        let subject = SubjectRef::new("sales_order", "order-1").unwrap();
        let expected = ApprovalStartIdentity {
            process_kind: ProcessKind::SalesOrder,
            subject: &subject,
            subject_version: 3,
            binding: &binding,
            actor_id: "actor-1",
        };
        let mutations: [fn(&mut ApprovalProcessInstance); 8] = [
            |value| value.base.id = "instance-2".into(),
            |value| value.process_kind = ProcessKind::VoucherSalesOrder,
            |value| value.subject = SubjectRef::new("voucher_sales_order", "order-1").unwrap(),
            |value| value.subject = SubjectRef::new("sales_order", "order-2").unwrap(),
            |value| value.subject_version += 1,
            |value| value.started_by = ParticipantId::new("actor-2").unwrap(),
            |value| value.process_definition_id = ApprovalProcessDefinitionId::new("definition-2"),
            |value| value.definition_version += 1,
        ];
        for mutate in mutations {
            let mut instance = instance();
            mutate(&mut instance);
            assert!(!expected.matches_instance(&instance, "instance-1"));
        }
    }
}
