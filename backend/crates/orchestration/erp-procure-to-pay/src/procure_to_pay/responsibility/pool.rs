//! 全池成员资格使用调用方原事务核验，询价池使用询价处理权限。
use std::collections::HashSet;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_identity::Permission;
use erp_procurement::dto::procurement_responsibility::ProcurementResponsibilityResolutionView;
use erp_procurement::entity::procurement_responsibility::{
    ProcurementResponsibilityRuleData, ProcurementResponsibilityRuleType, ResolutionPurpose,
};
use persistence_core::{Executor, NoTransaction};

use super::ProcurementResponsibilityProcess;
use super::resolver::{ResolutionInput, load_owner_account};
use crate::procure_to_pay::responsibility::eligibility::ProcurementInquiryEligibility;
use crate::{Error, Result};

impl ProcurementResponsibilityProcess {
    /// 在调用方执行器中重验全部成员的账号；询价池同时重验询价权限和组织。
    /// # 参数
    /// `data` 为待维护的规则，`executor` 为当前执行器。
    /// # 返回
    /// 全部成员满足对应业务资格时返回空值。
    /// # 错误
    /// 任一成员失效、缺少权限或组织时拒绝整个规则维护。
    pub(super) async fn validate_pool(
        &self,
        data: &ProcurementResponsibilityRuleData,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for owner in &data.owner_user_ids {
            if data.rule_type == ProcurementResponsibilityRuleType::Inquiry {
                ProcurementInquiryEligibility::new(self.db.clone(), self.rbac.clone())
                    .assignment_owner(owner, executor)
                    .await?;
            } else {
                load_owner_account(&self.db, owner, executor).await?;
                let actor = AuditActor::new(owner.clone(), owner.clone(), AccountKind::Admin);
                let permission = Permission::parse("purchase_order:create")?;
                if !crate::authorization::features(self.db.clone(), self.rbac.clone())
                    .has_permission(&actor, &permission, executor)
                    .await?
                {
                    return Err(Error::BusinessLogicError(format!(
                        "采购负责人 {owner} 缺少 purchase_order:create 权限"
                    )));
                }
            }
        }
        Ok(())
    }
}

impl ProcurementResponsibilityProcess {
    /// 按指定场景只读预览，询价候选使用询价执行资格。
    /// # 参数
    /// `inputs` 为整单商品；`purpose` 区分普通采购与询价。
    /// # 返回
    /// 返回已检查人员资格的逐行预览，不推进任何池。
    /// # 错误
    /// 配置、目录、账号或业务权限不足时返回对应错误。
    pub(super) async fn preview_batch(
        &self,
        inputs: &[ResolutionInput],
        purpose: ResolutionPurpose,
    ) -> Result<Vec<ProcurementResponsibilityResolutionView>> {
        if purpose == ResolutionPurpose::Procurement {
            return Ok(self.resolve_strict(inputs).await?.views());
        }
        let candidates = self.resolve_inquiry_candidates(inputs, &mut NoTransaction).await?;
        let access = ProcurementInquiryEligibility::new(self.db.clone(), self.rbac.clone());
        let mut checked = HashSet::new();
        for candidate in &candidates {
            if checked.insert(candidate.owner_user_id.as_str()) {
                access.assignment_owner(&candidate.owner_user_id, &mut NoTransaction).await?;
            }
        }
        Ok(candidates
            .into_iter()
            .map(|candidate| ProcurementResponsibilityResolutionView {
                line_key: candidate.line_key,
                owner_user_id: candidate.owner_user_id,
                owner_name: candidate.owner_name,
                rule_id: candidate.rule_id,
                rule_type: candidate.rule_type,
            })
            .collect())
    }
}
