//! 生成或恢复演示客户。

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_customer::repository::customer::{
    CustomerAssignmentRepositoryExt, CustomerProfileCommandRepositoryExt,
};
use erp_customer::{
    AssignmentAction, AssignmentRole, CustomerAssignmentRequest, CustomerExt, SaveCustomerProfileRequest,
};
use persistence_core::NoTransaction;

use super::ensure_dictionary::{EnsureOutcome, step_label};
use super::plan::DemoStep;
use super::record::{self, DemoMasterRecord};
use super::{DemoMasterDataService, lifecycle, spec};
use crate::adapters::scoped_customer_assignment_service;
use crate::{Error, Result};

impl DemoMasterDataService {
    /// 以销售岗位读取并执行客户种子。
    ///
    /// # 参数
    /// `actor` - 操作人；`step` - 种子身份；`request` - 客户创建输入；`notices` - 跳过原因集合。
    ///
    /// # 返回
    /// 返回客户创建、恢复或跳过结果。
    ///
    /// # 错误
    /// 命令回执失效、领域写入或清单登记失败时返回错误。
    pub(super) async fn ensure_customer(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        request: &SaveCustomerProfileRequest,
        notices: &mut Vec<String>,
    ) -> Result<EnsureOutcome> {
        if let Some(existing) = self.customer_command(&step.key).await? {
            return self.adopt_customer(actor, step, &existing, notices).await;
        }
        let Some(owner) = self.role_actor(&spec::foundation_spec().customer_owner_account).await? else {
            return Ok(EnsureOutcome::Notice("没有可用的销售账号，未生成客户".to_string()));
        };
        let view = match self.customers().create(request.clone(), &owner).await {
            Ok(view) => view,
            Err(Error::ValidationError(message) | Error::Forbidden(message)) => {
                return Ok(EnsureOutcome::Notice(format!("未生成客户：{message}")));
            },
            Err(error) => return Err(error),
        };
        let account = self
            .db
            .customer_accounts()
            .find_by_id_including_deleted(&view.customer_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("演示客户创建后未能读回".to_string()))?;
        let created = account.base.deleted_at == entity_core::NOT_DELETED_TIMESTAMP;
        if !created {
            lifecycle::restore_party(&self.db, actor, &view.party_id).await?;
            lifecycle::restore_customer(&self.db, actor, &view.customer_id).await?;
        }
        record::save(&self.db, &customer_record(step, &view.customer_id, &view.party_id)).await?;
        Ok(if created { EnsureOutcome::Created } else { EnsureOutcome::Restored })
    }

    /// 恢复命令回执绑定的客户并对齐销售负责人。
    async fn adopt_customer(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        customer_id: &str,
        notices: &mut Vec<String>,
    ) -> Result<EnsureOutcome> {
        let account = self
            .db
            .customer_accounts()
            .find_by_id_including_deleted(customer_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("演示客户命令缺少客户".to_string()))?;
        let party_id = account.party_id.to_string();
        let live = account.base.deleted_at == entity_core::NOT_DELETED_TIMESTAMP;
        if !live {
            restore_optional(lifecycle::restore_party(&self.db, actor, &party_id).await)?;
            lifecycle::restore_customer(&self.db, actor, customer_id).await?;
        }
        self.align_customer_owner(actor, customer_id, notices).await?;
        record::save(&self.db, &customer_record(step, customer_id, &party_id)).await?;
        Ok(if live { EnsureOutcome::Skipped } else { EnsureOutcome::Restored })
    }

    /// 按稳定幂等键定位此前创建的客户。
    async fn customer_command(&self, key: &str) -> Result<Option<String>> {
        let command =
            self.db.customer_profile_commands().find_by_idempotency_key(key, &mut NoTransaction).await?;
        Ok(command.map(|command| command.customer_id))
    }

    /// 通过领域分配入口对齐演示客户的销售负责人。
    async fn align_customer_owner(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        notices: &mut Vec<String>,
    ) -> Result<()> {
        let Some(owner) = self.role_actor(&spec::foundation_spec().customer_owner_account).await? else {
            push_unique(notices, "没有可用的销售账号，演示客户仍由原负责人维护");
            return Ok(());
        };
        let owners = self
            .db
            .customer_assignments()
            .current_owners(Some(&[customer_id.to_string()]), None, BusinessDate::today(), &mut NoTransaction)
            .await?;
        if owners.iter().any(|row| row.user_id == owner.id()) {
            return Ok(());
        }
        let request = CustomerAssignmentRequest {
            action: AssignmentAction::Assign,
            user_id: Some(owner.id().to_string()),
            assignment_role: Some(AssignmentRole::Owner),
            valid_from: Some(BusinessDate::today()),
            valid_to: None,
            assignment_id: None,
            change_reason: "演示客户交给销售账号".to_string(),
            version: None,
        };
        if let Err(error) = scoped_customer_assignment_service(self.db.clone(), self.rbac.clone())
            .apply_assignment(customer_id, request, actor)
            .await
        {
            push_unique(notices, &format!("演示客户未能交给销售账号：{error}"));
        }
        Ok(())
    }
}

/// 构造客户与主体实际 ID 的登记记录。
fn customer_record(step: &DemoStep, customer_id: &str, party_id: &str) -> DemoMasterRecord {
    DemoMasterRecord {
        key: step.key.clone(),
        kind: step.kind.as_str().to_string(),
        entity_id: customer_id.to_string(),
        related_ids: vec![party_id.to_string()],
        label: step_label(step),
        removed: false,
    }
}

/// 将已不存在的可选主体视为无需恢复。
fn restore_optional(result: Result<()>) -> Result<()> {
    match result {
        Ok(()) | Err(Error::NotFound(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

/// 合并不重复的业务提示。
fn push_unique(notices: &mut Vec<String>, text: &str) {
    if !notices.iter().any(|item| item == text) {
        notices.push(text.to_string());
    }
}
