//! 询价责任采购的账号、同角色权限与有效主属组织资格。

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_identity::service::business_ownership::required_business_org;
use erp_identity::{Permission, SharedRbacService};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 采购责任消费的询价执行资格，不持有销售询价流程。
pub struct ProcurementInquiryEligibility {
    db: Database,
    rbac: SharedRbacService,
}

impl ProcurementInquiryEligibility {
    /// 绑定当前账号事实与共享授权源。
    ///
    /// # 参数
    /// `db` 为账号数据库，`rbac` 为当前授权服务。
    /// # 返回
    /// 不缓存资格的读取器。
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 重新证明目标采购账号、组织与全部询价处理权限。
    ///
    /// # 参数
    /// `owner` 为目标账号，`executor` 为当前写事务。
    /// # 返回
    /// 目标账号当前有效主属组织。
    /// # 错误
    /// 账号、组织或必要执行权限失效时拒绝，不以销售权限代替。
    pub async fn assignment_owner(&self, owner: &str, executor: &mut dyn Executor) -> Result<String> {
        let actor = AuditActor::new(owner.into(), owner.into(), AccountKind::Admin);
        let permissions = ["detail", "save_feedback", "publish"]
            .into_iter()
            .map(|action| Permission::parse(format!("procurement_inquiry:{action}")))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let qualified = crate::authorization::features(self.db.clone(), self.rbac.clone())
            .permissions(&actor, &permissions, executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("询价责任采购账号已失效".into()))?;
        if !qualified.same_role(&permissions) {
            let account = qualified.account();
            return Err(Error::BusinessLogicError(format!(
                "责任采购「{}（{}）」缺少有效的询价处理权限，请管理员检查其采购角色的查看、保存反馈和发布权限后重试",
                account.display_name, account.login_account
            )));
        }
        required_business_org(&self.db, owner, executor).await.map_err(Error::from)
    }

    /// 在同一执行器证明当前询价处理动作权限。
    ///
    /// # 参数
    /// `actor`、`action` 为当前身份及询价动作，`executor` 为原事务。
    /// # 返回
    /// 返回当前有效策略版本。
    /// # 错误
    /// 账号、同角色动作权限或策略失效时拒绝。
    pub async fn ensure_permission(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        let permissions = [Permission::parse(format!("procurement_inquiry:{action}"))?];
        let qualified = crate::authorization::features(self.db.clone(), self.rbac.clone())
            .permissions(actor, &permissions, executor)
            .await?
            .filter(|value| value.same_role(&permissions))
            .ok_or_else(|| Error::Forbidden("当前账号缺少完整询价操作资格".into()))?;
        Ok(qualified.policy_revision())
    }
}
