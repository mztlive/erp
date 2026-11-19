//! 库存调整普通撤回保留原提交人和运行管理员两个固定分支。
use erp_permissions::{approval_admin, approval_instance, inventory};
use persistence_core::Executor;

use super::AuthorizationService;
use super::targets::TargetFacts;
use super::work_items::exact_source;
use crate::entity::authorization::AccessContext;
use crate::repository::authorization::AccessProof;
use crate::{Error, QualifiedPermissions, Result};

impl AuthorizationService {
    /// 撤回库存调整审批时核实真实调整与仓库。原提交人无需另证库存详情资格；代办管理员须具备同角色管理资格，并独立满足库存详情资格和实际仓库范围。
    pub(super) async fn stock_adjustment_cancel_sources(
        &self,
        proof: &AccessProof,
        qualified: &QualifiedPermissions,
        facts: &TargetFacts,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccessProof>> {
        let mut sources = Vec::new();
        for object in facts.roots() {
            if object.subject_type.get()? != "stock_adjustment" {
                return Err(Error::ValidationError("普通撤回的冻结业务类型未登记".into()));
            }
            let source = exact_source(object, "subject_source", "stock_adjustment")?;
            let warehouse = exact_source(source, "warehouse", "warehouse")?;
            if source.warehouse_id.get()?.as_deref() != Some(warehouse.id.as_str()) {
                return Err(Error::ValidationError("库存调整来源与真实仓库不一致".into()));
            }

            if object.owner_id.get()?.as_deref() == Some(proof.context.actor.id()) {
                continue;
            }
            let required = [
                inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL,
                approval_instance::INSTANCE_LIST,
                approval_admin::STOCK_ADJUSTMENT_RUNTIME,
            ];
            if !qualified.same_role(&required) || !qualified.same_role(&[inventory::STOCK_ADJUSTMENT_DETAIL])
            {
                return Err(Error::Forbidden("撤回需要原提交人或完整运行管理及独立库存详情资格".into()));
            }

            let context = AccessContext {
                permission: inventory::STOCK_ADJUSTMENT_DETAIL,
                resource: "stock_adjustment".into(),
                model: None,
                ..proof.context.clone()
            };
            let access = self.resolve_context(context, None, false, qualified, executor).await?;
            self.check_work_item_source(&access, facts, source, executor).await?;
            sources.push(access);
        }
        Ok(sources)
    }
}
