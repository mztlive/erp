//! 已授权单据的绑定路线摘要，不暴露定义管理字段。
use bpm::model::{ApprovalNodeDefinition, ModelError};
use erp_workflow::BpmExt;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use mongodb::Database;
use persistence_core::Executor;
use serde::Serialize;

use crate::{Error, Result};

pub(crate) fn invalid_approval_steps(error: ModelError) -> Error {
    tracing::warn!(%error, "审批环节数量或顺序异常");
    Error::ValidationError("审批流程的环节数量或顺序有误，请联系管理员核对流程设置".into())
}

/// 提交确认和绑定卡共用的节点摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalNodeView {
    pub key: String,
    pub name: String,
    pub assignee_source: Option<String>,
    pub assignee_name: Option<String>,
}

impl DocumentApprovalNodeView {
    /// 依据定义或运行图投影来源和完整人员显示名。
    ///
    /// # 参数
    /// 已授权节点。
    ///
    /// # 返回
    /// 可读节点摘要；未解析来源显示配置标签。
    pub fn from_node(node: &ApprovalNodeDefinition) -> Self {
        Self {
            key: node.node_key.clone(),
            name: node.node_name.clone(),
            assignee_source: node.assignee_rule.clone(),
            assignee_name: Some(if node.assignee_rule.is_some() {
                node.assignee_label_snapshot.clone()
            } else {
                node.participants().into_iter().map(|p| p.name).collect::<Vec<_>>().join("、")
            }),
        }
    }
}

/// 处理审批绑定路线时，读取单据绑定的精确版本及来源规则；调用方先完成单据读取授权。
///
/// # 参数
/// 数据库、单据绑定及原执行器。
///
/// # 返回
/// 流程名称及按顺序排列的节点。
///
/// # 错误
/// 版本漂移、定义缺失或数据库错误时拒绝，不补空路线。
pub async fn binding_route(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    ex: &mut dyn Executor,
) -> Result<(String, Vec<DocumentApprovalNodeView>)> {
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&binding.approval_process_definition_id, ex)
        .await?
        .filter(|graph| graph.definition.definition_version == binding.approval_definition_version)
        .ok_or_else(|| {
            Error::ConflictError("找不到这张单据提交时所用的审批流程，请联系管理员核对流程设置".into())
        })?;

    let nodes = graph
        .ordered_nodes()
        .map_err(invalid_approval_steps)?
        .into_iter()
        .map(DocumentApprovalNodeView::from_node)
        .collect();
    Ok((graph.definition.name, nodes))
}
