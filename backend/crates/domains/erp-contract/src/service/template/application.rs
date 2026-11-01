//! 申请人隔离、幂等领号和同事务流水分配。

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_core::common::time::BusinessDate;
use persistence_core::{Executor, NoTransaction, Transactional};

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
                    service.apply_step(transaction_request, &transaction_actor, executor).await
                })
            })
            .await;
        if matches!(
            &result,
            Err(Error::ConflictError(_) | Error::TransientTransaction(_) | Error::OutcomeUnknown(_))
        ) {
            // 只核对已提交的同键记录，不自动重放任何写入。
            if let Some(existing) =
                application_by_command(&self.db, actor.id(), &request.command_id, &mut NoTransaction).await?
            {
                existing.require_same(&request.template_id, &request.purpose)?;
                return Ok(existing.into());
            }
        }
        result
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

    /// 已签档案占号时有界跳过；失败整事务回滚，不消耗流水。
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

    /// 每次下载都按申请人重新授权，停用模板不影响历史申请。
    /// # 参数
    /// * `id` - 申请记录 ID。
    /// * `actor` - 当前销售。
    /// # 返回
    /// 不可变模板存储身份及原合同号。
    /// # 错误
    /// 非本人申请统一返回不存在，避免暴露他人合同号。
    pub async fn download_source(&self, id: &str, actor: &AuditActor) -> Result<TemplateDownloadSource> {
        let application = owned_application(&self.db, id, actor.id(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("合同申请不存在或无权下载".into()))?;
        let template = self.template(&application.template_id, &mut NoTransaction).await?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "contract_application.download",
            "contract_application",
            application.base.id,
        )?;
        self.audit.persist(&audit, &mut NoTransaction).await?;
        Ok(TemplateDownloadSource { object_key: template.object_key(), contract_no: application.contract_no })
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
        let template = self.template(id, &mut NoTransaction).await?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "contract_template.sample",
            "contract_template",
            id.into(),
        )?;
        self.audit.persist(&audit, &mut NoTransaction).await?;
        Ok(TemplateDownloadSource {
            object_key: template.object_key(),
            contract_no: format!("{}-S-SAMPLE", template.group.prefix()),
        })
    }
}
