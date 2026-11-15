//! 连接合同本域实现与真实工作流参与端口，不读取合同或解释责任口径。
use std::sync::Arc;

use async_trait::async_trait;
use erp_contract::repository::authorization;
use erp_contract::{ContractParticipantPort, Error as ContractError, Result as ContractResult};
use erp_identity::{AccessFactsPort, AccessMapping, Result};
use erp_workflow::repository::document_registry::participant_access;
use erp_workflow::repository::prelude::*;
use erp_workflow::{DocumentRegistryExt, Error as WorkflowError};
use mongodb::Database;
use persistence_core::Executor;

/// 将真实工作流参与来源注入合同拥有域的 Facts 和 Mapping。
/// # 参数
/// db 为共享业务库，端口和静态来源引用同一工作流集合。
/// # 返回
/// 合同拥有域构造的共享事实和映射。
/// # 错误
/// 拥有域拒绝模型或来源描述时原样返回，不构造占位实现。
pub(super) fn bindings(db: Database) -> Result<(Arc<dyn AccessFactsPort>, Arc<dyn AccessMapping>)> {
    let participants = Arc::new(ContractParticipants(db.clone()));
    authorization::bindings(db, participants, Database::DOCUMENT_PARTICIPANTS)
}

struct ContractParticipants(Database);

#[async_trait]
impl ContractParticipantPort for ContractParticipants {
    async fn document_ids_in(
        &self,
        user: &str,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> ContractResult<Vec<String>> {
        Ok(self.0.document_participants().document_ids_in(user, ids, executor).await?)
    }

    async fn revision(&self, user: &str, executor: &mut dyn Executor) -> ContractResult<i64> {
        participant_access::revision(&self.0, user, executor).await.map_err(participant_error)
    }
}

/// 保留工作流仓储和内部失败分类，不把取证失败当空参与集合。
fn participant_error(error: WorkflowError) -> ContractError {
    match error {
        WorkflowError::RepositoryError(error) => ContractError::from(error),
        WorkflowError::Internal(message) => ContractError::Internal(message),
        error => ContractError::Internal(error.to_string()),
    }
}
