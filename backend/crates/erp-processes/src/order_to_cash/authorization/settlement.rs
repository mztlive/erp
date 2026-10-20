//! 销售结算主体资格；写入时重验我方公司、启用状态及当前资料。
use erp_core::ids::PartyId;
use erp_party::{PartyExt, PartyKind};
use persistence_core::Executor;

use super::SalesCommandAccess;
use crate::{Error, Result};

impl SalesCommandAccess {
    /// 校验所选结算主体并读取当前法定名称。
    /// # 参数
    /// `selected` 为所选我方公司主体。
    /// `executor` 沿用调用方事务。
    /// # 返回
    /// 启用的我方企业主体名称。
    /// # 错误
    /// 所选主体不是我方公司、停用、缺失或资料版本无效时拒绝。
    pub(in crate::order_to_cash) async fn settlement_name(
        &self,
        selected: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let selected_company = self
            .db
            .parties()
            .find_by_id(selected.as_ref(), executor)
            .await?
            .filter(|party| party.is_active() && party.company_profile.is_some());
        if selected_company.is_none() {
            return Err(Error::ValidationError("我方结算主体必须是启用的我方公司".into()));
        }
        self.party_name(selected, executor).await
    }

    /// 在已完成身份授权后读取启用主体的现行名称。
    /// # 参数
    /// `id` 为已授权主体，`executor` 为调用方执行器。
    /// # 返回
    /// 归属该主体的有效资料版本名称。
    /// # 错误
    /// 主体不可用、版本缺失或归属不一致时拒绝。
    pub(in crate::order_to_cash) async fn party_name(
        &self,
        id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let party = self
            .db
            .parties()
            .find_by_id(id.as_ref(), executor)
            .await?
            .filter(|party| party.is_active() && party.party_kind == PartyKind::Enterprise)
            .ok_or_else(|| Error::ValidationError("主体不存在或已停用，请重新选择".into()))?;
        let revision_id = party
            .stable
            .current_revision_id
            .as_deref()
            .ok_or_else(|| Error::ConflictError("主体缺少有效资料版本".into()))?;
        let revision = self
            .db
            .party_revisions()
            .find_by_id(revision_id, executor)
            .await?
            .filter(|revision| &revision.party_id == id)
            .ok_or_else(|| Error::ConflictError("主体资料版本不存在或归属不一致".into()))?;
        Ok(revision.legal_name)
    }
}
