//! 合同后续归档与终止；授权、原对象、版本、写入及回包共用原事务。
use application_core::AuditActor;
use erp_permissions::{Permission, contract};
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::{ContractService, conflict_if_stale_version, plan_next_revision};
use crate::dto::contract::{ArchiveContractRevisionRequest, ContractDetailView, TerminateContractRequest};
use crate::entity::contract::Contract;
use crate::repository::ContractExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

impl ContractService {
    /// 在原授权事务追加不可变修订并切换当前版本指针。
    /// # 参数
    /// id 为合同身份，req 含已见版本与新 PDF，actor 为当前操作人。
    /// # 返回
    /// 原归档命令事务内生成的完整合同响应。
    /// # 错误
    /// 无权、对象缺失、版本冲突、输入非法或数据库失败时拒绝。
    pub async fn archive_contract_revision(
        &self,
        id: &str,
        req: ArchiveContractRevisionRequest,
        actor: &AuditActor,
    ) -> Result<ContractDetailView> {
        req.validate()?;

        let (service, id, actor) = (self.clone(), id.to_owned(), actor.clone());
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.archive_stage(&id, req, &actor, executor).await })
            })
            .await
    }

    /// 先授权再读取原对象与最大修订序号；写入、审计和完整回包共用同一执行器。
    async fn archive_stage(
        &self,
        id: &str,
        req: ArchiveContractRevisionRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ContractDetailView> {
        let contract = self.command_contract(actor, &contract::ARCHIVE_REVISION, id, executor).await?;
        conflict_if_stale_version(contract.matches_version(req.version))?;

        let revision_no = self
            .db
            .contract_revisions()
            .latest_revision_no(&contract.base.id.clone().into(), executor)
            .await?;
        let mut planned = plan_next_revision(contract, req, revision_no.unwrap_or(0))?;

        let audit =
            self.audit.resource_log(actor.clone(), "contract.archive_revision", "contract", id.into())?;
        self.db
            .contract()
            .archive_contract_revision(&mut planned.contract, &planned.revision, executor)
            .await?;
        self.audit.persist(&audit, executor).await?;

        self.detail_view(planned.contract, executor).await
    }

    /// 在原授权事务终止合同，保持既有销售引用。
    /// # 参数
    /// id 为合同身份，req 含已见版本，actor 为当前操作人。
    /// # 返回
    /// 原终止命令事务内生成的完整合同响应。
    /// # 错误
    /// 无权、对象缺失、版本冲突、状态非法或数据库失败时拒绝。
    pub async fn terminate_contract(
        &self,
        id: &str,
        req: TerminateContractRequest,
        actor: &AuditActor,
    ) -> Result<ContractDetailView> {
        req.validate()?;

        let (service, id, actor) = (self.clone(), id.to_owned(), actor.clone());
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.terminate_stage(&id, req, &actor, executor).await })
            })
            .await
    }

    /// 原事务先授权，再检查版本和状态；命令结果不追加详情菜单资格。
    async fn terminate_stage(
        &self,
        id: &str,
        req: TerminateContractRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ContractDetailView> {
        let mut contract = self.command_contract(actor, &contract::TERMINATE, id, executor).await?;
        conflict_if_stale_version(contract.matches_version(req.version))?;
        contract.terminate(actor.id())?;

        let audit = self.audit.resource_log(actor.clone(), "contract.terminate", "contract", id.into())?;
        self.db.contracts().update(&mut contract, executor).await?;
        self.audit.persist(&audit, executor).await?;

        self.detail_view(contract, executor).await
    }

    /// 命令授权必须先于原对象、版本与业务状态的读取。
    async fn command_contract(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Contract> {
        self.data_scope.require_with(actor.clone(), permission, id, executor).await?;
        self.db
            .contracts()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("合同不存在".into()))
    }
}

#[cfg(test)]
mod tests;
