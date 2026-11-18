//! 补齐演示用的岗位账号、部门、审批流程和默认责任规则。全库重置后重新创建这些；已有岗位账号会把姓名同步为演示人名。

use application_core::AuditActor;
use erp_workflow::ports::WorkflowAuthorizationPort;

use super::{DemoMasterDataProcess, spec};
use crate::{Error, Result};

/// 岗位账号、部门与审批流程的准备结果。
#[derive(Debug, serde::Serialize)]
pub struct DemoFoundationReport {
    /// 本次新建的岗位账号数。
    pub accounts_created: u32,
    /// 已经存在的岗位账号数。
    pub accounts_existing: u32,
    /// 本次新发布的审批流程数。
    pub approvals_published: u32,
    /// 已发布且节点与规格一致的审批流程数。
    pub approvals_existing: u32,
    /// 已发布但节点与规格不一致、因此未改动的审批流程数。
    pub approvals_mismatched: u32,
    /// 本次新建的默认采购或财务责任规则数。
    pub responsibilities_created: u32,
    /// 已有启用规则、因此保留现有负责人的数量。
    pub responsibilities_existing: u32,
    /// 需要告诉操作人的说明。
    pub notices: Vec<String>,
}

impl<A: WorkflowAuthorizationPort> DemoMasterDataProcess<A> {
    /// 为演示业务准备岗位角色、账号、部门、业务范围、审批链和默认责任规则，并返回准备统计。
    /// 已有账号保留密码、同步姓名；已启用的默认采购或财务负责人不替换。
    ///
    /// 必须开启演示配置并具备各正式用例的权限。已发布审批链不匹配时保留原定义并停止；授权或写入失败不继续后续步骤。
    pub async fn ensure_foundation(&self, actor: &AuditActor) -> Result<DemoFoundationReport> {
        self.ensure_enabled()?;
        self.authorize_apply(actor).await?;
        spec::foundation_spec().validate()?;

        let mut report = DemoFoundationReport {
            accounts_created: 0,
            accounts_existing: 0,
            approvals_published: 0,
            approvals_existing: 0,
            approvals_mismatched: 0,
            responsibilities_created: 0,
            responsibilities_existing: 0,
            notices: Vec::new(),
        };

        self.ensure_demo_roles(actor, &mut report).await?;
        let accounts = self.ensure_accounts(actor, &mut report).await?;
        self.validate_demo_permissions(&accounts).await?;

        self.ensure_departments(actor, &accounts.by_login, &mut report.notices).await?;
        self.ensure_person_scopes(actor, &accounts.by_login).await?;

        self.ensure_approvals(actor, &accounts, &mut report).await?;
        if report.approvals_mismatched > 0 {
            return Err(Error::ValidationError(report.notices.join("；")));
        }

        self.ensure_responsibilities(actor, &accounts, &mut report).await?;
        Ok(report)
    }
}
