//! 能力事实读取、领域变更计划与修订准备。

use std::collections::HashMap;

use erp_core::field_update::FieldUpdate;
use erp_core::ids::{SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId};
use erp_supplier::entity::supplier::profile_change::CapabilityToggle;
use erp_supplier::{
    SaveSupplierProfileRequest, SupplierCapability, SupplierCapabilityUpdate, SupplierExt, profile_change,
};
use id_generator::next_id;
use persistence_core::NoTransaction;

use super::super::SupplierProfileProcess;
use super::persist::CapabilityChanges;
use crate::{Error, Result};

impl SupplierProfileProcess {
    /// 将能力代码集合解析为新增、启停与不可变快照。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `req` - 已校验的根资料命令
    /// * `actor_id` - 执行变更的账号 ID
    ///
    /// # 返回
    /// 返回待事务持久化的能力实体、修订和稳定 ID 映射。
    ///
    /// # 错误
    /// 仓储查询、状态迁移、修订号生成或实体构造失败时返回错误。
    pub(super) async fn prepare_capability_changes(
        &self,
        supplier_id: &SupplierAccountId,
        req: &SaveSupplierProfileRequest,
        actor_id: &str,
    ) -> Result<CapabilityChanges> {
        let existing = self.db.supplier().list_capabilities(supplier_id, &mut NoTransaction).await?;
        let plan = profile_change::SupplierProfileChangePlan::from_loaded(
            &existing,
            &[],
            &HashMap::new(),
            &HashMap::new(),
            &req.capability_codes,
            &[],
        )
        .map_err(|e| Error::ValidationError(e.to_string()))?;
        let mut changes = CapabilityChanges::default();
        let mut existing_by_code: HashMap<String, SupplierCapability> =
            existing.into_iter().map(|cap| (cap.capability_code.as_str().to_string(), cap)).collect();
        for cap in existing_by_code.values() {
            changes
                .ids
                .insert(cap.capability_code.as_str().to_string(), SupplierCapabilityId::new(&cap.base.id));
        }
        self.prepare_capability_toggles(
            plan.capability_toggles,
            &mut existing_by_code,
            &mut changes,
            actor_id,
        )
        .await?;
        for code in plan.capability_creates {
            let capability_id = SupplierCapabilityId::new(next_id());
            let revision_id = SupplierCapabilityRevisionId::new(next_id());
            let owner = req
                .capability_owners
                .iter()
                .find(|item| item.capability_code == code && !item.owner_user_id.trim().is_empty())
                .map(|item| item.owner_user_id.trim().to_string())
                .ok_or_else(|| Error::ValidationError("供给能力负责人必须显式指定".to_string()))?;
            let (capability, revision) = profile_change::new_capability(
                supplier_id,
                code,
                req.effective_from,
                &owner,
                actor_id,
                capability_id.clone(),
                revision_id,
            )
            .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
            changes.ids.insert(code.as_str().to_string(), SupplierCapabilityId::new(&capability.base.id));
            changes.created.push(capability);
            changes.revisions.push(revision);
        }
        Ok(changes)
    }

    /// 逐能力生成启停修订；修订号读取失败后不再准备后续能力。
    async fn prepare_capability_toggles(
        &self,
        toggles: Vec<CapabilityToggle>,
        existing_by_code: &mut HashMap<String, SupplierCapability>,
        changes: &mut CapabilityChanges,
        actor_id: &str,
    ) -> Result<()> {
        for toggle in toggles {
            let mut capability = existing_by_code
                .remove(toggle.code.as_str())
                .ok_or_else(|| Error::Internal("能力计划与已加载事实不一致".to_string()))?;
            capability
                .update(
                    SupplierCapabilityUpdate {
                        service_region: FieldUpdate::Unchanged,
                        owner_user_id: None,
                        fulfillment_note: FieldUpdate::Unchanged,
                        valid_to: FieldUpdate::Unchanged,
                        status: Some(toggle.target_status),
                    },
                    actor_id,
                )
                .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
            let revision_no = self
                .db
                .supplier()
                .next_capability_revision_no(
                    &capability.supplier_id,
                    capability.capability_code,
                    &mut NoTransaction,
                )
                .await?;
            let revision_id = SupplierCapabilityRevisionId::new(next_id());
            capability.stable.current_revision_id = Some(revision_id.to_string());
            let revision = capability
                .snapshot_revision(revision_id, revision_no)
                .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
            changes.updated.push(capability);
            changes.revisions.push(revision);
        }
        Ok(())
    }
}
