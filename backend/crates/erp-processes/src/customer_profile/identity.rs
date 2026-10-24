//! 客户建档身份解析：在原事务中查重，并按明确选择复用已有企业。

use application_core::AuditActor;
use erp_core::ids::{PartyId, PartyRevisionId};
use erp_customer::repository::prelude::*;
use erp_customer::{CustomerExt, SaveCustomerProfileRequest};
use erp_party::repository::exact_identity::ContractIdentityCandidate;
use erp_party::repository::identity_guard::guarded_candidates;
use erp_party::{Party, PartyExt, PartyRevision};
use id_generator::next_id;
use persistence_core::Executor;

use super::CustomerProfileProcess;
use super::create::create_party;
use crate::{Error, Result};

/// 已确认的新主体或可复用主体；复用时只创建客户角色。
pub(super) struct CreationIdentity {
    pub(super) party: Party,
    pub(super) revision: PartyRevision,
    pub(super) reused: bool,
}

impl CustomerProfileProcess {
    /// 在写事务内锁定名称、查重并解析创建身份。
    /// # 参数
    /// * `req` / `actor` / `executor` - 创建请求、操作人及原事务执行器。
    /// # 返回
    /// 返回新主体，或经用户确认且尚无客户角色的现有主体。
    /// # 错误
    /// 身份歧义、已建客户、未确认复用、主体不可用或数据库失败时拒绝。
    pub(super) async fn creation_identity(
        &self,
        req: &SaveCustomerProfileRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CreationIdentity> {
        let (party, revision) =
            create_party(req, &PartyId::new(next_id()), &PartyRevisionId::new(next_id()), actor.id())?;
        let candidates = guarded_candidates(
            &self.db,
            &revision.legal_name,
            party.unified_credit_code.as_deref(),
            executor,
        )
        .await?;
        if candidates.is_empty() {
            return Ok(CreationIdentity { party, revision, reused: false });
        }
        let [candidate] = <[ContractIdentityCandidate; 1]>::try_from(candidates)
            .map_err(|_| Error::ConflictError("名称或信用代码对应多个主体，请先核对并处理重复主体".into()))?;
        self.reuse_identity(
            candidate.party,
            &revision.legal_name,
            party.unified_credit_code.as_deref(),
            req.reuse_existing_party,
            executor,
        )
        .await
    }

    /// 复用必须保留已有主体资料，并且不能隐式加入已有客户的销售范围。
    async fn reuse_identity(
        &self,
        party: Party,
        name: &str,
        credit: Option<&str>,
        confirmed: bool,
        executor: &mut dyn Executor,
    ) -> Result<CreationIdentity> {
        let existing = self
            .db
            .customer_accounts()
            .find_by_party_including_deleted(&PartyId::new(&party.base.id), executor)
            .await?;
        if existing.is_some() {
            return Err(Error::ConflictError(
                "该企业已建立客户档案，请联系有权限的人员添加负责销售；停用或删除的客户须先恢复".into(),
            ));
        }
        let revision_id = party
            .stable
            .current_revision_id
            .as_deref()
            .ok_or_else(|| Error::ConflictError("已有主体的当前资料不完整，请先修复主体资料".into()))?;
        let revision = self
            .db
            .party_revisions()
            .find_by_id(revision_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("已有主体的当前资料不完整，请先修复主体资料".into()))?;
        party
            .ensure_reusable_identity(&revision, name, credit)
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        if !confirmed {
            return Err(Error::ConflictError("该企业已有主体资料但尚未建立客户档案。请勾选“复用已有企业资料”后提交，原有名称、信用代码和联系方式等资料将予以保留".into()));
        }
        Ok(CreationIdentity { party, revision, reused: true })
    }
}
