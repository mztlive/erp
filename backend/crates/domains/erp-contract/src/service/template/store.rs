//! 模板命令的私有持久化边界；不解释资格、幂等、流水或状态规则。
use async_trait::async_trait;
use persistence_core::Executor;

use super::ContractTemplateService;
use crate::Result;
use crate::entity::template::{CompanyNumbering, ContractApplication, ContractCounter, ContractTemplate};
use crate::repository::ContractExt;
use crate::repository::templates::{ContractTemplateExt, application_by_command};

#[async_trait]
pub(super) trait TemplateStore: Sync {
    async fn numbering(&self, company: &str, executor: &mut dyn Executor)
    -> Result<Option<CompanyNumbering>>;
    async fn create_numbering(&self, numbering: &CompanyNumbering, executor: &mut dyn Executor)
    -> Result<()>;
    async fn template(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<ContractTemplate>>;
    async fn create_template(&self, template: &ContractTemplate, executor: &mut dyn Executor) -> Result<()>;
    async fn update_template(
        &self,
        template: &mut ContractTemplate,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn application(
        &self,
        applicant: &str,
        command: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ContractApplication>>;
    async fn create_application(
        &self,
        application: &ContractApplication,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn counter(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<ContractCounter>>;
    async fn save_counter(
        &self,
        counter: &mut ContractCounter,
        is_new: bool,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn number_exists(&self, number: &str, executor: &mut dyn Executor) -> Result<bool>;
}

#[async_trait]
impl TemplateStore for ContractTemplateService {
    async fn numbering(
        &self,
        company: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CompanyNumbering>> {
        Ok(self.db.contract_company_numbering().find_by_id(company, executor).await?)
    }
    async fn create_numbering(
        &self,
        numbering: &CompanyNumbering,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self.db.contract_company_numbering().create(numbering, executor).await?)
    }
    async fn template(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<ContractTemplate>> {
        Ok(self.db.contract_templates().find_by_id(id, executor).await?)
    }
    async fn create_template(&self, template: &ContractTemplate, executor: &mut dyn Executor) -> Result<()> {
        Ok(self.db.contract_templates().create(template, executor).await?)
    }
    async fn update_template(
        &self,
        template: &mut ContractTemplate,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self.db.contract_templates().update(template, executor).await?)
    }
    async fn application(
        &self,
        applicant: &str,
        command: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ContractApplication>> {
        application_by_command(&self.db, applicant, command, executor).await
    }
    async fn create_application(
        &self,
        application: &ContractApplication,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self.db.contract_applications().create(application, executor).await?)
    }
    async fn counter(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<ContractCounter>> {
        Ok(self.db.contract_counters().find_by_id(id, executor).await?)
    }
    async fn save_counter(
        &self,
        counter: &mut ContractCounter,
        is_new: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if is_new {
            self.db.contract_counters().create(counter, executor).await?;
        } else {
            self.db.contract_counters().update(counter, executor).await?;
        }
        Ok(())
    }
    async fn number_exists(&self, number: &str, executor: &mut dyn Executor) -> Result<bool> {
        Ok(self
            .db
            .contracts()
            .find_one_by_field_including_deleted("contract_no", number, executor)
            .await?
            .is_some())
    }
}
