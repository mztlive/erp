//! 供应商合作资料及门户会话的最小对外字段。

use erp_identity::{AccessControlExt, PortalActor, PortalRole};
use erp_party::PartyExt;
use erp_supplier::{ReconciliationCycle, SettlementMode, SupplierAccount, SupplierExt};
use persistence_core::{Executor, NoTransaction};
use serde::Serialize;

use super::SupplierPortalReadService;
use crate::{Error, Result};

/// 当前实名登录人与供应商名称，不承载内部角色或组织。
#[derive(Debug, Serialize)]
pub struct PortalSessionView {
    pub account_id: String,
    pub account: String,
    pub name: String,
    pub supplier_id: String,
    pub supplier_name: String,
    pub role: PortalRole,
    pub account_version: u64,
    pub binding_version: u64,
}

/// 本供应商正式付款条件及采购联系人姓名。
#[derive(Debug, Serialize)]
pub struct PortalCooperationView {
    pub supplier_name: String,
    pub name: String,
    pub supplier_no: String,
    pub version: u64,
    pub current_profile_id: Option<String>,
    pub settlement_mode: Option<SettlementMode>,
    pub reconciliation_cycle: Option<ReconciliationCycle>,
    pub payment_term: Option<String>,
    pub procurement_contact_name: Option<String>,
    pub contact_name: Option<String>,
}

impl SupplierPortalReadService {
    /// 读取已验证会话对应的供应商名称。
    ///
    /// # 参数
    /// `actor` 为每次请求重新读取账号与绑定的身份。
    /// # 返回
    /// 只含本账号和本供应商的必要字段。
    /// # 错误
    /// 供应商已停用、删除或查询失败时拒绝。
    pub async fn session(&self, actor: &PortalActor) -> Result<PortalSessionView> {
        let supplier = self.portal_supplier(actor, &mut NoTransaction).await?;
        let supplier_name = self.supplier_name(&supplier, &mut NoTransaction).await?;
        Ok(PortalSessionView {
            account_id: actor.account_id.clone(),
            account: actor.account.clone(),
            name: actor.name.clone(),
            supplier_id: actor.supplier_id.clone(),
            supplier_name,
            role: actor.role,
            account_version: actor.account_version,
            binding_version: actor.binding_version,
        })
    }

    /// 返回本供应商当前正式商务资料，不显示银行和内部评价。
    ///
    /// # 参数
    /// `actor` 为已验证门户账号。
    /// # 返回
    /// 当前付款条件及内部采购联系人姓名。
    /// # 错误
    /// 供应商或商务版本归属失效、查询失败时拒绝。
    pub async fn cooperation(&self, actor: &PortalActor) -> Result<PortalCooperationView> {
        let supplier = self.portal_supplier(actor, &mut NoTransaction).await?;
        let name = self.supplier_name(&supplier, &mut NoTransaction).await?;
        let pointer = supplier.current_commercial_profile_revision_id.as_ref().map(ToString::to_string);
        let profile = match pointer.as_deref() {
            Some(id) => self
                .db
                .supplier_commercial_profile_revisions()
                .find_by_id(id, &mut NoTransaction)
                .await?
                .filter(|profile| profile.supplier_id.as_ref() == supplier.base.id),
            None => None,
        };
        if pointer.is_some() && profile.is_none() {
            return Err(Error::BusinessLogicError("供应商商务档案不可用".into()));
        }
        let contact = self
            .db
            .accounts()
            .find_by_id(&supplier.maintainer_user_id, &mut NoTransaction)
            .await?
            .filter(|account| account.is_active_backoffice())
            .map(|account| account.name);
        Ok(PortalCooperationView {
            supplier_name: name.clone(),
            name,
            supplier_no: supplier.supplier_no,
            version: supplier.base.version,
            current_profile_id: pointer,
            settlement_mode: profile.as_ref().map(|profile| profile.settlement_mode),
            reconciliation_cycle: profile.as_ref().map(|profile| profile.reconciliation_cycle),
            payment_term: profile.map(|profile| profile.effective_payment_term_code()),
            procurement_contact_name: contact.clone(),
            contact_name: contact,
        })
    }

    /// 按服务端绑定取得本供应商，不授予任何内部DataScope。
    pub(super) async fn portal_supplier(
        &self,
        actor: &PortalActor,
        executor: &mut dyn Executor,
    ) -> Result<SupplierAccount> {
        self.db
            .supplier_accounts()
            .find_by_id(&actor.supplier_id, executor)
            .await?
            .filter(|supplier| supplier.is_active())
            .ok_or_else(|| Error::Unauthenticated("认证已失效".into()))
    }

    /// 仅取主体当前名称，隐藏税务、联系人、银行及其他角色资料。
    async fn supplier_name(&self, supplier: &SupplierAccount, executor: &mut dyn Executor) -> Result<String> {
        let names = self
            .db
            .party()
            .current_legal_names_by_party_ids(std::slice::from_ref(&supplier.party_id), executor)
            .await?;
        names
            .get(supplier.party_id.as_ref())
            .cloned()
            .ok_or_else(|| Error::BusinessLogicError("供应商主体资料不可用".into()))
    }
}
