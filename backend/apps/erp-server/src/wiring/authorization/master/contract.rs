//! 连接合同本域实现与真实工作流参与端口，不读取合同或解释责任口径。
use std::sync::Arc;

use async_trait::async_trait;
use erp_contract::repository::authorization;
use erp_contract::{ContractParticipantPort, Result as ContractResult};
use erp_identity::{AccessFactsPort, AccessMapping, Result};
use erp_workflow::DocumentRegistryExt;
use erp_workflow::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;

// 将真实工作流参与来源注入合同拥有域的 Facts 和 Mapping。
// db 为共享业务库，端口和静态来源引用同一工作流集合。
// 合同拥有域构造的共享事实和映射。
// 拥有域拒绝模型或来源描述时原样返回，不构造占位实现。
pub(in crate::wiring::authorization) fn bindings(
    db: Database,
) -> Result<(Arc<dyn AccessFactsPort>, Arc<dyn AccessMapping>)> {
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
}
