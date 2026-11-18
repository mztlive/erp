//! 运行图只采用实例冻结人员，不查询当前负责人。
use super::DefinitionGraph;
use crate::model::{ApprovalInstanceAssignee, ApprovalProcessInstance, ModelError, ModelResult};

impl DefinitionGraph {
    /// 回读运行实例时用实例绑定还原动态节点的审批人、冻结显示名与来源依据，不重新查询当前负责人。
    /// 定义与实例版本不符、节点绑定缺失或重复、绑定未保持冻结事实，或规则、申请人及参与名单不匹配时返回错误。
    pub fn with_frozen_assignees(
        mut self,
        instance: &ApprovalProcessInstance,
        bindings: &[ApprovalInstanceAssignee],
    ) -> ModelResult<Self> {
        if self.definition.base.id != instance.process_definition_id.as_ref()
            || self.definition.definition_version != instance.definition_version
            || bindings.len() != self.nodes.len()
        {
            return Err(ModelError::InvalidField("实例指派与定义不匹配"));
        }

        for node in &mut self.nodes {
            let mut matches = bindings.iter().filter(|binding| binding.node_key == node.node_key);
            let (Some(binding), None) = (matches.next(), matches.next()) else {
                return Err(ModelError::InvalidField("实例节点指派缺失或重复"));
            };
            binding.ensure_unchanged_from_definition()?;
            if binding.process_instance_id != instance.typed_id() {
                return Err(ModelError::InvalidField("实例指派归属不一致"));
            }

            if let Some(rule) = &node.assignee_rule {
                let resolution = binding
                    .resolution
                    .as_ref()
                    .filter(|r| &r.rule == rule && r.subject_id == instance.started_by.as_str())
                    .ok_or(ModelError::InvalidField("动态指派冻结依据缺失或不一致"))?;
                node.assignee_participant_id = Some(binding.current_assignee_participant_id.clone());
                node.assignee_label_snapshot = resolution.participant_name.clone();
                node.resolution = Some(resolution.clone());
            } else if binding.resolution.is_some()
                || node.assignee()? != &binding.current_assignee_participant_id
            {
                return Err(ModelError::InvalidField("指定审批人与实例绑定不一致"));
            }

            if node.additional_assignees != binding.additional_assignees {
                return Err(ModelError::InvalidField("实例审批人集合不一致"));
            }
            node.validate_assignment()?;
        }
        Ok(self)
    }
}
