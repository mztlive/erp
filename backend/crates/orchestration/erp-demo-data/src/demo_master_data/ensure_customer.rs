//! 生成或恢复演示客户。

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_customer::repository::customer::{
    CustomerAssignmentRepositoryExt, CustomerProfileCommandRepositoryExt,
};
use erp_customer::{
    AssignmentAction, AssignmentRole, CustomerAssignmentRequest, CustomerExt, SaveCustomerProfileRequest,
};
use erp_workflow::ports::WorkflowAuthorizationPort;
use persistence_core::ReadExecutor;

use super::ensure_dictionary::EnsureOutcome;
use super::plan::DemoStep;
use super::{DemoMasterDataProcess, lifecycle, record, spec};
use crate::{Error, Result};

impl<A: WorkflowAuthorizationPort> DemoMasterDataProcess<A> {
    // 按种子键优先恢复原客户命令；无回执时以启用的销售岗位创建客户，并登记客户与主体 ID。
    // 缺少岗位或业务拒绝时返回提示，回执失效、写入或登记失败时停止。
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
        let view = match self.ports.customers.create(request.clone(), &owner).await.map_err(Error::from) {
            Ok(view) => view,
            Err(Error::ValidationError(message) | Error::Forbidden(message)) => {
                return Ok(EnsureOutcome::Notice(format!("未生成客户：{message}")));
            },
            Err(error) => return Err(error),
        };

        let account = self
            .db
            .customer_accounts()
            .find_by_id_including_deleted(&view.customer_id, &mut ReadExecutor::default())
            .await?
            .ok_or_else(|| Error::NotFound("演示客户创建后未能读回".to_string()))?;
        let created = account.base.deleted_at == entity_core::NOT_DELETED_TIMESTAMP;
        lifecycle::restore_party(&self.db, self.authorization.clone(), actor, account.party_id.as_ref())
            .await?;
        if !created {
            lifecycle::restore_customer(&self.db, self.authorization.clone(), actor, &view.customer_id)
                .await?;
        }

        record::save(&self.db, step, view.customer_id, vec![view.party_id]).await?;
        Ok(if created { EnsureOutcome::Created } else { EnsureOutcome::Restored })
    }

    // 恢复原命令绑定的客户和主体，按演示销售岗位对齐归属后登记真实主键，避免重试创建第二个客户。
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
            .find_by_id_including_deleted(customer_id, &mut ReadExecutor::default())
            .await?
            .ok_or_else(|| Error::NotFound("演示客户命令缺少客户".to_string()))?;
        let party_id = account.party_id.to_string();
        let live = account.base.deleted_at == entity_core::NOT_DELETED_TIMESTAMP;

        lifecycle::restore_party(&self.db, self.authorization.clone(), actor, &party_id).await?;
        if !live {
            lifecycle::restore_customer(&self.db, self.authorization.clone(), actor, customer_id).await?;
        }

        self.align_customer_owner(actor, customer_id, notices).await?;
        record::save(&self.db, step, customer_id.to_string(), vec![party_id]).await?;
        Ok(if live { EnsureOutcome::Skipped } else { EnsureOutcome::Restored })
    }

    // 用稳定种子幂等键查客户创建回执，让重试继续使用首次命令绑定的客户，而不是按名称认领记录。
    async fn customer_command(&self, key: &str) -> Result<Option<String>> {
        let command = self
            .db
            .customer_profile_commands()
            .find_by_idempotency_key(key, &mut ReadExecutor::default())
            .await?;
        Ok(command.map(|command| command.customer_id))
    }

    // 通过正式客户分配用例对齐销售负责人，使恢复的客户可由演示销售岗位继续经办。
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
            .current_owners(
                Some(&[customer_id.to_string()]),
                None,
                BusinessDate::today(),
                &mut ReadExecutor::default(),
            )
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
        if let Err(error) =
            self.ports.customer_assignments.apply_assignment(customer_id, request, actor).await
        {
            push_unique(notices, &format!("演示客户未能交给销售账号：{error}"));
        }
        Ok(())
    }
}

// 将客户准备提示追加到批次报告，同一原因只记一次，便于定位真正阻断生成的问题。
fn push_unique(notices: &mut Vec<String>, text: &str) {
    if !notices.iter().any(|item| item == text) {
        notices.push(text.to_string());
    }
}
