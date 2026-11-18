//! 申请人隔离、幂等领号和同事务流水分配。

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_core::common::time::BusinessDate;
use erp_identity::AccessTarget;
use erp_permissions::contract;
use persistence_core::{Executor, Transactional};

use super::ContractTemplateService;
use crate::dto::template::{ApplicationView, ApplyTemplateRequest};
use crate::entity::template::{ContractApplication, ContractCounter, text};
use crate::error::{Error, Result};
use crate::repository::ContractExt;
use crate::repository::templates::{ContractTemplateExt, application_by_command, owned_application};

/// 已授权文件读取事实，只供组合层使用，不返回 HTTP。
pub struct TemplateDownloadSource {
    pub object_key: String,
    pub contract_no: String,
}

impl ContractTemplateService {
    /// 销售申请用模板领号时，先规范化用途并在同一事务授权、查申请键、分配公司编号组的年度流水并保存申请和审计。
    /// 同申请人和申请键仅在模板与用途一致时复用原结果，不另占流水。模板或主体停用、键冲突、流水耗尽或并发冲突会阻止本次新写入。
    /// 提交不确定时只读核对原申请；领号不创建已签合同。
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
                    service
                        .authorization
                        .require(
                            &transaction_actor,
                            &contract::TEMPLATES_APPLY,
                            AccessTarget::Operation,
                            executor,
                        )
                        .await?;
                    service.apply_step(transaction_request, &transaction_actor, executor).await
                })
            })
            .await;

        self.finish_apply(result, &request, actor).await
    }

    // 写事务冲突、瞬时失败或提交结果未知后，用新事务只读查证同键的已提交记录，不重放写入。
    // 无匹配时保留原错误；结果未知且查证失败时也保留原错误。
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

    // 只读查证领号结果时在新快照先复核原申请权限，再按当前申请人及原键定位记录；只在模板和用途匹配时返回原申请。
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

    async fn apply_step(
        &self,
        request: ApplyTemplateRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ApplicationView> {
        if let Some(existing) =
            application_by_command(&self.db, actor.id(), &request.command_id, executor).await?
        {
            existing.require_same(&request.template_id, &request.purpose)?;
            return Ok(existing.into());
        }

        let template = self.template(&request.template_id, executor).await?;
        if !template.enabled {
            return Err(Error::BusinessLogicError("模板已停用，请选择其他模板".into()));
        }
        self.active_company(&template.company_id, executor).await?;

        let counter = ContractCounter::initial(template.group, BusinessDate::today().ymd().0)?;
        let contract_no = self.allocate(counter, executor).await?;

        let application = ContractApplication {
            base: BaseModel::new(id_generator::next_id()),
            command_id: request.command_id,
            applicant_id: actor.id().into(),
            template_id: template.base.id,
            template_name: template.name,
            company_name: template.company_name,
            purpose: request.purpose,
            contract_no,
        };

        let audit = self.audit.resource_log(
            actor.clone(),
            "contract_application.apply",
            "contract_application",
            application.base.id.clone(),
        )?;
        self.db.contract_applications().create(&application, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(application.into())
    }

    // 新申请在原事务为公司编号组和业务年份递增流水；已归档合同（含软删除）占用的号码最多尝试跳过 100 次。
    // 只有找到未占用编号才写回计数器；其写入与申请同事务，失败不单独消耗流水。
    async fn allocate(&self, initial: ContractCounter, executor: &mut dyn Executor) -> Result<String> {
        let existing = self.db.contract_counters().find_by_id(&initial.base.id, executor).await?;
        let is_new = existing.is_none();
        let mut counter = existing.unwrap_or(initial);

        for _ in 0..100 {
            let number = counter.allocate()?;
            if self
                .db
                .contracts()
                .find_one_by_field_including_deleted("contract_no", &number, executor)
                .await?
                .is_some()
            {
                continue;
            }

            if is_new {
                self.db.contract_counters().create(&counter, executor).await?;
            } else {
                self.db.contract_counters().update(&mut counter, executor).await?;
            }
            return Ok(number);
        }
        Err(Error::ConflictError("已有合同占用后续号码，请联系管理员校准已用流水".into()))
    }

    /// 正式下载前在同一事务校验下载权限及申请归属，再返回不可变模板对象键和原申请号并登记审计。
    /// 停用模板仍可用于历史申请；不存在和非本人申请返回同一错误，不暴露他人合同号。
    pub async fn download_source(&self, id: &str, actor: &AuditActor) -> Result<TemplateDownloadSource> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service
                        .authorization
                        .require(&actor, &contract::TEMPLATES_DOWNLOAD, AccessTarget::Operation, executor)
                        .await?;

                    let application = owned_application(&service.db, &id, actor.id(), executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("合同申请不存在或无权下载".into()))?;
                    let template = service.template(&application.template_id, executor).await?;

                    let audit = service.audit.resource_log(
                        actor.clone(),
                        "contract_application.download",
                        "contract_application",
                        application.base.id,
                    )?;
                    service.audit.persist(&audit, executor).await?;
                    Ok(TemplateDownloadSource {
                        object_key: template.object_key(),
                        contract_no: application.contract_no,
                    })
                })
            })
            .await
    }

    /// 管理员为核对首页布局下载带 `SAMPLE` 标识的样张，返回模板读取信息，不占年度流水。模板不存在或审计失败时拒绝。
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
