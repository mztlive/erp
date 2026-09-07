//! 履约执行任务的对象标题。
//!
//! 工作台只展示往来单据号与作业类型，不把内部主键或「草稿」状态写进标题。
//! 「草稿」是履约单据未过账的内部状态，对执行人就是待处理任务。

#[cfg(test)]
use super::authority::fulfillment::fulfillment_source_label;
use super::{ObjectKind, WorkbenchObjectFact, WorkbenchObjectFactMap, WorkbenchReadService};
use crate::errors::Result;
use persistence_core::Executor;
use std::collections::HashSet;
impl<A: erp_workflow::WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 复用唯一履约来源读取与权威投影；这四类对象没有额外显示查询。
    pub(super) async fn load_fulfillment_operation_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut loaded = erp_workflow::ports::ObjectFactMap::new();
        self.facts_reader()
            .load_fulfillment_operation_facts(keys, &mut loaded, executor)
            .await?;
        facts.extend(
            loaded
                .into_iter()
                .map(|(key, fact)| (key, WorkbenchObjectFact::from_authority(fact))),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::fulfillment_source_label;

    #[test]
    fn fulfillment_title_uses_source_document_number() {
        assert_eq!(
            fulfillment_source_label("供应商直发", "销售单", Some("SO20260826-000001")),
            "供应商直发 · 销售单 SO20260826-000001"
        );
        assert_eq!(
            fulfillment_source_label("供应商直发", "销售单", Some("  ")),
            "供应商直发"
        );
        assert_eq!(fulfillment_source_label("采购入库", "采购单", None), "采购入库");
    }
}
