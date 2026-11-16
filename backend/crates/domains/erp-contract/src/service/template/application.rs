//! 申请人隔离、幂等领号和同事务流水分配。

use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::contract;
use persistence_core::{Executor, Transactional};

use super::ContractTemplateService;
use crate::dto::template::{ApplicationView, ApplyTemplateRequest};
use crate::entity::template::text;
use crate::error::{Error, Result};
use crate::repository::templates::application_by_command;

/// 已授权文件读取事实，只供组合层使用，不返回 HTTP。
pub struct TemplateDownloadSource {
    pub object_key: String,
    pub contract_no: String,
}

impl ContractTemplateService {
    /// 一个申请键原子形成一个编号，网络重试不得多占流水。
    /// # 参数
    /// * `request` - 稳定申请键、模板及用途。
    /// * `actor` - 申请销售。
    /// # 返回
    /// 已分配编号；同内容重试返回原记录。
    /// # 错误
    /// 模板停用、主体停用、键冲突、流水耗尽或并发冲突。
    pub async fn apply(
        &self,
        mut request: ApplyTemplateRequest,
        actor: &AuditActor,
    ) -> Result<ApplicationView> {
        request.purpose = text(&request.purpose, "申请用途", false)?;
        if request.command_id.is_empty()
            || request.command_id.len() > 64
            || !request.command_id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        {
            return Err(Error::ValidationError("申请信息无效，请刷新页面重试".into()));
        }
        let service = self.clone();
        let transaction_actor = actor.clone();
        let transaction_request = request.clone();
        let result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service.writer().apply(transaction_request, &transaction_actor, executor).await
                })
            })
            .await;
        self.finish_apply(result, &request, actor).await
    }

    /// 写事务退出后只读查证胜者；不自动重放写入，未知提交保留原始错误。
    async fn finish_apply(
        &self,
        result: Result<ApplicationView>,
        request: &ApplyTemplateRequest,
        actor: &AuditActor,
    ) -> Result<ApplicationView> {
        let original = match result {
            Ok(view) => return Ok(view),
            Err(error) => error,
        };
        if !matches!(
            original,
            Error::ConflictError(_) | Error::TransientTransaction(_) | Error::OutcomeUnknown(_)
        ) {
            return Err(original);
        }
        let service = self.clone();
        let actor = actor.clone();
        let request = request.clone();
        let replay = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.application_replay(&request, &actor, executor).await })
            })
            .await;
        match replay {
            Ok(Some(view)) => Ok(view),
            Ok(None) => Err(original),
            Err(_) if matches!(original, Error::OutcomeUnknown(_)) => Err(original),
            Err(error) => Err(error),
        }
    }

    /// 在新快照先核验原申请资格，再查证同申请人和幂等键的已提交结果。
    async fn application_replay(
        &self,
        request: &ApplyTemplateRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApplicationView>> {
        self.authorization
            .require(actor, &contract::TEMPLATES_APPLY, AccessTarget::Operation, executor)
            .await?;
        let Some(existing) =
            application_by_command(&self.db, actor.id(), &request.command_id, executor).await?
        else {
            return Ok(None);
        };
        existing.require_same(&request.template_id, &request.purpose)?;
        Ok(Some(existing.into()))
    }

    /// 每次下载都按申请人重新授权，停用模板不影响历史申请。
    /// # 参数
    /// * `id` - 申请记录 ID。
    /// * `actor` - 当前销售。
    /// # 返回
    /// 不可变模板存储身份及原合同号。
    /// # 错误
    /// 非本人申请统一返回不存在，避免暴露他人合同号。
    pub async fn download_source(&self, id: &str, actor: &AuditActor) -> Result<TemplateDownloadSource> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let source = super::query::read_download(&service, &id, &actor, executor).await?;
                    let audit = service.audit.resource_log(
                        actor.clone(),
                        "contract_application.download",
                        "contract_application",
                        id,
                    )?;
                    service.audit.persist(&audit, executor).await?;
                    Ok(source)
                })
            })
            .await
    }

    /// 管理员下载样张核对首页布局；不占用任何流水。
    /// # 参数
    /// * `id` - 模板 ID。
    /// * `actor` - 管理员。
    /// # 返回
    /// 使用 SAMPLE 标识的样张读取信息。
    /// # 错误
    /// 模板不存在或审计失败。
    pub async fn sample_source(&self, id: &str, actor: &AuditActor) -> Result<TemplateDownloadSource> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service
                        .authorization
                        .require(&actor, &contract::TEMPLATES_CREATE, AccessTarget::Operation, executor)
                        .await?;

                    let template = service.template(&id, executor).await?;
                    let audit = service.audit.resource_log(
                        actor.clone(),
                        "contract_template.sample",
                        "contract_template",
                        id,
                    )?;
                    service.audit.persist(&audit, executor).await?;
                    Ok(TemplateDownloadSource {
                        object_key: template.object_key(),
                        contract_no: format!("{}-S-SAMPLE", template.group.prefix()),
                    })
                })
            })
            .await
    }
}

#[cfg(test)]
mod authorization_tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::ports::{FailClosedAuditPort, SigningCompanyFact, SigningCompanyPort};

    struct NoCompanies;
    #[async_trait]
    impl SigningCompanyPort for NoCompanies {
        async fn company(&self, _: &str, _: &mut dyn Executor) -> Result<Option<SigningCompanyFact>> {
            panic!("申请结果查证不得读取公司或重放申请写入")
        }
    }

    #[tokio::test]
    async fn application_replay_requires_original_qualification_in_its_new_snapshot() {
        let service = ContractTemplateService::new(
            Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
                .await
                .unwrap()
                .database("application_replay_unit"),
            Arc::new(FailClosedAuditPort),
            Arc::new(NoCompanies),
            unavailable_authorization(),
        );
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let request = ApplyTemplateRequest {
            command_id: "command".into(),
            template_id: "template".into(),
            purpose: "purpose".into(),
        };
        assert!(matches!(
            service.application_replay(&request, &actor, &mut NoTransaction).await,
            Err(Error::ValidationError(_))
        ));
        assert!(matches!(
            service.application_replay(&request, &actor, &mut SnapshotExecutor::default()).await,
            Err(Error::Internal(message)) if message == "测试授权设施不可用"
        ));
    }
}
