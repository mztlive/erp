//! 进度读取复用跨域 ReadModel，协议快照只在本流程解密。
use erp_read_models::supplier_center::execution_progress::{
    ExecutionOrderTreeView, ExecutionShipmentView, SupplierExecutionProgressReadModel,
};
use erp_supply::api_order::ApiOrderExecution;
use persistence_core::Executor;

use super::SupplierApiOrderProcess;
use crate::Result;

impl SupplierApiOrderProcess {
    pub(super) async fn execution_progress(
        &self,
        execution: &ApiOrderExecution,
        executor: &mut dyn Executor,
    ) -> Result<(Option<ExecutionOrderTreeView>, Vec<ExecutionShipmentView>)> {
        let snapshot = execution.request_encrypted.as_ref().map(|_| self.decode(execution)).transpose()?;
        let skus = snapshot
            .into_iter()
            .flat_map(|snapshot| snapshot.lines)
            .map(|line| (line.line_id, line.sku.spec_id))
            .collect();
        Ok(SupplierExecutionProgressReadModel::new(self.db.clone())
            .for_execution(execution, &skus, executor)
            .await?)
    }
}
