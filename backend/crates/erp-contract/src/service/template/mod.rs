//! 模板是签约前资料；领号不会创建已签合同、审批或销售资格。

use std::sync::Arc;

use application_core::AuditActor;
use entity_core::BaseModel;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use tokio::sync::Semaphore;

use crate::dto::template::{CreateTemplateRequest, TemplateStatusRequest, TemplateView};
use crate::entity::template::{CompanyNumbering, ContractTemplate, text};
use crate::entity::template_docx;
use crate::error::{Error, Result};
use crate::ports::{ContractAuditPort, SigningCompanyPort};
use crate::repository::templates::ContractTemplateExt;

mod application;
mod counter;
mod query;
pub use application::TemplateDownloadSource;

static DOCX_WORKERS: Semaphore = Semaphore::const_new(2);

/// 单域模板管理与领号服务，外域主体及审计通过窄 Port 接入。
#[derive(Clone)]
pub struct ContractTemplateService {
    db: Database,
    audit: Arc<dyn ContractAuditPort>,
    companies: Arc<dyn SigningCompanyPort>,
}

impl ContractTemplateService {
    /// 装配模板服务。
    /// # 参数
    /// * `db` - 合同领域数据库。
    /// * `audit` - 审计写入 Port。
    /// * `companies` - 公司主体事实 Port。
    /// # 返回
    /// 服务实例。
    /// # 错误
    /// 无。
    pub fn new(
        db: Database,
        audit: Arc<dyn ContractAuditPort>,
        companies: Arc<dyn SigningCompanyPort>,
    ) -> Self {
        Self { db, audit, companies }
    }

    /// 校验模板字段并生成不可覆盖的存储身份，尚未写入数据库。
    /// # 参数
    /// * `request` - 名称、主体及编号组。
    /// * `file_name` - 已验证为 DOCX 的展示文件名。
    /// # 返回
    /// 待持久化模板。
    /// # 错误
    /// 非启用公司或输入非法时拒绝。
    pub async fn prepare(&self, request: CreateTemplateRequest, file_name: &str) -> Result<ContractTemplate> {
        let company_id = text(&request.company_id, "公司主体", true)?;
        let company_name = self.active_company(&company_id, &mut NoTransaction).await?;
        Ok(ContractTemplate {
            base: BaseModel::new(id_generator::next_id()),
            name: text(&request.name, "模板名称", true)?,
            company_id,
            company_name,
            group: request.group,
            file_name: text(file_name, "文件名", true)?,
            enabled: true,
        })
    }

    /// 文件写入完成后，原子登记主体编号组、模板及审计。
    /// # 参数
    /// * `template` - 本服务规划的模板。
    /// * `actor` - 当前已认证管理员。
    /// # 返回
    /// 模板公开视图。
    /// # 错误
    /// 主体停用、编号组冲突或事务失败；提交未知时调用方不得删除对象。
    pub async fn create(&self, template: ContractTemplate, actor: &AuditActor) -> Result<TemplateView> {
        let service = self.clone();
        let audit = self.audit.resource_log(
            actor.clone(),
            "contract_template.create",
            "contract_template",
            template.base.id.clone(),
        )?;
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service.active_company(&template.company_id, executor).await?;
                    service.bind_group(&template, executor).await?;
                    service.db.contract_templates().create(&template, executor).await?;
                    service.audit.persist(&audit, executor).await?;
                    Ok(template.into())
                })
            })
            .await
    }

    async fn bind_group(&self, template: &ContractTemplate, executor: &mut dyn Executor) -> Result<()> {
        let repository = self.db.contract_company_numbering();
        if let Some(existing) = repository.find_by_id(&template.company_id, executor).await? {
            if existing.group != template.group {
                return Err(Error::ConflictError("该公司已绑定其他合同编号组，请沿用原编号组".into()));
            }
        } else {
            repository
                .create(
                    &CompanyNumbering {
                        base: BaseModel::new(template.company_id.clone()),
                        group: template.group,
                    },
                    executor,
                )
                .await?;
        }
        Ok(())
    }

    /// 启停模板；文件与主体禁止覆盖，历史申请仍可下载。
    /// # 参数
    /// * `id` - 模板 ID。
    /// * `request` - 期望版本及启停状态。
    /// * `actor` - 管理员。
    /// # 返回
    /// 更新后的模板。
    /// # 错误
    /// 版本不一致或模板不存在时拒绝。
    pub async fn set_status(
        &self,
        id: &str,
        request: TemplateStatusRequest,
        actor: &AuditActor,
    ) -> Result<TemplateView> {
        let service = self.clone();
        let id = id.to_string();
        let audit = self.audit.resource_log(
            actor.clone(),
            "contract_template.status",
            "contract_template",
            id.clone(),
        )?;
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut template = service.template(&id, executor).await?;
                    if template.base.version != request.version {
                        return Err(Error::ConflictError("模板已变化，请刷新后重试".into()));
                    }
                    template.enabled = request.enabled;
                    service.db.contract_templates().update(&mut template, executor).await?;
                    service.audit.persist(&audit, executor).await?;
                    Ok(template.into())
                })
            })
            .await
    }

    /// 在阻塞线程处理 DOCX，不阻塞异步 HTTP 线程。
    /// # 参数
    /// * `bytes` - 原始模板。
    /// * `number` - 合同号或明确标识的样张编号。
    /// # 返回
    /// 已填写合同号的 Word 文件。
    /// # 错误
    /// 模板校验或 Word 处理失败。
    pub async fn render(bytes: Vec<u8>, number: String) -> Result<Vec<u8>> {
        let permit =
            DOCX_WORKERS.acquire().await.map_err(|_| Error::Internal("Word 模板处理暂不可用".into()))?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            template_docx::stamp(&bytes, &number)
        })
        .await
        .map_err(|_| Error::Internal("Word 模板处理失败".into()))?
    }

    async fn template(&self, id: &str, executor: &mut dyn Executor) -> Result<ContractTemplate> {
        self.db
            .contract_templates()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("合同模板不存在".into()))
    }

    async fn active_company(&self, id: &str, executor: &mut dyn Executor) -> Result<String> {
        match self.companies.company(id, executor).await? {
            Some(company) if company.active => Ok(company.name),
            _ => Err(Error::ValidationError("请选择启用的公司主体".into())),
        }
    }
}
