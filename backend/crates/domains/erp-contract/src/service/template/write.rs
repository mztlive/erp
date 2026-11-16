//! 公开事务入口与库单测共用的真实模板命令；资格先于存储、流水和成功审计。
use application_core::AuditActor;
use entity_core::BaseModel;
use erp_core::common::time::BusinessDate;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::contract;
use persistence_core::Executor;

use super::active_company;
use super::store::TemplateStore;
use crate::dto::template::{
    ApplicationView, ApplyTemplateRequest, ConfigureCounterRequest, CounterView, TemplateStatusRequest,
    TemplateView,
};
use crate::entity::template::{CompanyNumbering, ContractApplication, ContractCounter, ContractTemplate};
use crate::ports::{ContractAuditPort, SigningCompanyPort};
use crate::{Error, Result};

pub(super) struct TemplateWriter<'a, S> {
    pub authorization: &'a AuthorizationService,
    pub store: &'a S,
    pub companies: &'a dyn SigningCompanyPort,
    pub audit: &'a dyn ContractAuditPort,
}

impl<S: TemplateStore> TemplateWriter<'_, S> {
    pub(super) async fn create(
        &self,
        template: ContractTemplate,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<TemplateView> {
        self.authorization
            .require(actor, &contract::TEMPLATES_CREATE, AccessTarget::Operation, executor)
            .await?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "contract_template.create",
            "contract_template",
            template.base.id.clone(),
        )?;
        active_company(self.companies, &template.company_id, executor).await?;
        self.bind_group(&template, executor).await?;
        self.store.create_template(&template, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(template.into())
    }

    async fn bind_group(&self, template: &ContractTemplate, executor: &mut dyn Executor) -> Result<()> {
        if let Some(existing) = self.store.numbering(&template.company_id, executor).await? {
            if existing.group != template.group {
                return Err(Error::ConflictError("该公司已绑定其他合同编号组，请沿用原编号组".into()));
            }
        } else {
            self.store
                .create_numbering(
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

    pub(super) async fn set_status(
        &self,
        id: &str,
        request: TemplateStatusRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<TemplateView> {
        self.authorization
            .require(actor, &contract::TEMPLATES_CREATE, AccessTarget::Operation, executor)
            .await?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "contract_template.status",
            "contract_template",
            id.into(),
        )?;
        let mut template = self.template(id, executor).await?;
        if template.base.version != request.version {
            return Err(Error::ConflictError("模板已变化，请刷新后重试".into()));
        }
        template.enabled = request.enabled;
        self.store.update_template(&mut template, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(template.into())
    }

    pub(super) async fn apply(
        &self,
        request: ApplyTemplateRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ApplicationView> {
        self.authorization
            .require(actor, &contract::TEMPLATES_APPLY, AccessTarget::Operation, executor)
            .await?;
        if let Some(existing) = self.store.application(actor.id(), &request.command_id, executor).await? {
            existing.require_same(&request.template_id, &request.purpose)?;
            return Ok(existing.into());
        }
        let template = self.template(&request.template_id, executor).await?;
        if !template.enabled {
            return Err(Error::BusinessLogicError("模板已停用，请选择其他模板".into()));
        }
        active_company(self.companies, &template.company_id, executor).await?;
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
        self.persist_application(application, actor, executor).await
    }

    async fn persist_application(
        &self,
        application: ContractApplication,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ApplicationView> {
        let audit = self.audit.resource_log(
            actor.clone(),
            "contract_application.apply",
            "contract_application",
            application.base.id.clone(),
        )?;
        self.store.create_application(&application, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(application.into())
    }

    async fn allocate(&self, initial: ContractCounter, executor: &mut dyn Executor) -> Result<String> {
        let existing = self.store.counter(&initial.base.id, executor).await?;
        let is_new = existing.is_none();
        let mut counter = existing.unwrap_or(initial);
        for _ in 0..100 {
            let number = counter.allocate()?;
            if self.store.number_exists(&number, executor).await? {
                continue;
            }
            self.store.save_counter(&mut counter, is_new, executor).await?;
            return Ok(number);
        }
        Err(Error::ConflictError("已有合同占用后续号码，请联系管理员校准已用流水".into()))
    }

    pub(super) async fn configure_counter(
        &self,
        request: ConfigureCounterRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CounterView> {
        self.authorization
            .require(actor, &contract::TEMPLATES_CREATE, AccessTarget::Operation, executor)
            .await?;
        let initial = ContractCounter::initial(request.group, request.year)?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "contract_number_counter.configure",
            "contract_number_counter",
            initial.base.id.clone(),
        )?;
        let existing = self.store.counter(&initial.base.id, executor).await?;
        if existing.as_ref().map(|c| c.base.version) != request.version {
            return Err(Error::ConflictError("流水已变化，请刷新后重试".into()));
        }
        let is_new = existing.is_none();
        let mut counter = existing.unwrap_or(initial);
        counter.advance_to(request.last_sequence)?;
        self.store.save_counter(&mut counter, is_new, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(counter.into())
    }

    async fn template(&self, id: &str, executor: &mut dyn Executor) -> Result<ContractTemplate> {
        self.store.template(id, executor).await?.ok_or_else(|| Error::NotFound("合同模板不存在".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_identity::testing::AuthorizationFixture;
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{NoTransaction, ReadExecutor};

    use super::*;
    use crate::entity::template::NumberGroup;
    use crate::ports::{PreparedContractAudit, SigningCompanyFact};

    struct State {
        template: Option<ContractTemplate>,
        numbering: Option<CompanyNumbering>,
        application: Option<ContractApplication>,
        counter: Option<ContractCounter>,
        occupied: BTreeSet<String>,
        active: bool,
        events: Vec<(&'static str, usize)>,
        applicant: Option<(String, String)>,
        audits: Vec<PreparedContractAudit>,
        fail: Option<&'static str>,
    }
    struct Memory(Mutex<State>);
    impl Memory {
        fn new() -> Self {
            Self(Mutex::new(State {
                template: Some(template()),
                numbering: None,
                application: None,
                counter: None,
                occupied: BTreeSet::new(),
                active: true,
                events: vec![],
                applicant: None,
                audits: vec![],
                fail: None,
            }))
        }
        fn record(&self, name: &'static str, executor: &mut dyn Executor) -> Result<()> {
            let mut state = self.0.lock().unwrap();
            state.events.push((name, executor as *mut dyn Executor as *mut () as usize));
            if state.fail == Some(name) {
                return Err(Error::Internal(format!("injected {name}")));
            }
            Ok(())
        }
        fn events(&self) -> Vec<&'static str> {
            self.0.lock().unwrap().events.iter().map(|(name, _)| *name).collect()
        }
        fn assert_executor(&self, executor: &SnapshotExecutor) {
            let pointer = executor as *const SnapshotExecutor as usize;
            assert!(self.0.lock().unwrap().events.iter().all(|(_, actual)| *actual == pointer));
        }
        fn assert_no_effects(&self) {
            let state = self.0.lock().unwrap();
            assert!(state.events.is_empty());
            assert!(state.numbering.is_none() && state.counter.is_none() && state.application.is_none());
            assert!(state.audits.is_empty());
        }
    }

    #[async_trait]
    impl TemplateStore for Memory {
        async fn numbering(
            &self,
            company: &str,
            executor: &mut dyn Executor,
        ) -> Result<Option<CompanyNumbering>> {
            self.record("numbering.read", executor)?;
            Ok(self.0.lock().unwrap().numbering.clone().filter(|row| row.base.id == company))
        }
        async fn create_numbering(
            &self,
            numbering: &CompanyNumbering,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.record("numbering.create", executor)?;
            self.0.lock().unwrap().numbering = Some(numbering.clone());
            Ok(())
        }
        async fn template(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<ContractTemplate>> {
            self.record("template.read", executor)?;
            Ok(self.0.lock().unwrap().template.clone().filter(|row| row.base.id == id))
        }
        async fn create_template(
            &self,
            template: &ContractTemplate,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.record("template.create", executor)?;
            self.0.lock().unwrap().template = Some(template.clone());
            Ok(())
        }
        async fn update_template(
            &self,
            template: &mut ContractTemplate,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.record("template.update", executor)?;
            self.0.lock().unwrap().template = Some(template.clone());
            Ok(())
        }
        async fn application(
            &self,
            applicant: &str,
            command: &str,
            executor: &mut dyn Executor,
        ) -> Result<Option<ContractApplication>> {
            self.record("application.read", executor)?;
            let mut state = self.0.lock().unwrap();
            state.applicant = Some((applicant.into(), command.into()));
            Ok(state
                .application
                .clone()
                .filter(|row| row.applicant_id == applicant && row.command_id == command))
        }
        async fn create_application(
            &self,
            application: &ContractApplication,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.record("application.create", executor)?;
            self.0.lock().unwrap().application = Some(application.clone());
            Ok(())
        }
        async fn counter(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<ContractCounter>> {
            self.record("counter.read", executor)?;
            Ok(self.0.lock().unwrap().counter.clone().filter(|row| row.base.id == id))
        }
        async fn save_counter(
            &self,
            counter: &mut ContractCounter,
            is_new: bool,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.record(if is_new { "counter.create" } else { "counter.update" }, executor)?;
            self.0.lock().unwrap().counter = Some(counter.clone());
            Ok(())
        }
        async fn number_exists(&self, number: &str, executor: &mut dyn Executor) -> Result<bool> {
            self.record("number.exists", executor)?;
            Ok(self.0.lock().unwrap().occupied.contains(number))
        }
    }

    #[async_trait]
    impl SigningCompanyPort for Memory {
        async fn company(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<SigningCompanyFact>> {
            assert_eq!(id, "company");
            self.record("company", executor)?;
            Ok(Some(SigningCompanyFact { name: "公司".into(), active: self.0.lock().unwrap().active }))
        }
    }
    #[async_trait]
    impl ContractAuditPort for Memory {
        fn resource_log(
            &self,
            actor: AuditActor,
            action: &str,
            resource: &str,
            id: String,
        ) -> Result<PreparedContractAudit> {
            PreparedContractAudit::resource(actor, action, resource, id)
        }
        async fn persist(&self, audit: &PreparedContractAudit, executor: &mut dyn Executor) -> Result<()> {
            self.record("audit", executor)?;
            self.0.lock().unwrap().audits.push(audit.clone());
            Ok(())
        }
    }

    fn template() -> ContractTemplate {
        ContractTemplate {
            base: BaseModel { id: "template".into(), version: 7, ..BaseModel::fake() },
            name: "模板".into(),
            company_id: "company".into(),
            company_name: "公司".into(),
            group: NumberGroup::FSY,
            file_name: "contract.docx".into(),
            enabled: true,
        }
    }
    fn actor() -> AuditActor {
        AuditActor::new("user".into(), "login".into(), AccountKind::Admin)
    }
    fn request() -> ApplyTemplateRequest {
        ApplyTemplateRequest {
            command_id: "command".into(),
            template_id: "template".into(),
            purpose: "签约".into(),
        }
    }
    fn configuration() -> ConfigureCounterRequest {
        ConfigureCounterRequest { group: NumberGroup::FSY, year: 2026, last_sequence: 460, version: None }
    }
    fn writer<'a>(authorization: &'a AuthorizationService, memory: &'a Memory) -> TemplateWriter<'a, Memory> {
        TemplateWriter { authorization, store: memory, companies: memory, audit: memory }
    }

    #[tokio::test]
    async fn invalid_audit_resource_is_rejected_before_template_business_reads_or_writes() {
        let authorization = AuthorizationFixture::new(&[("manager", &[contract::TEMPLATES_CREATE])]).build();
        let memory = Memory::new();
        let command = writer(&authorization, &memory);
        let mut invalid = template();
        invalid.base.id.clear();
        assert!(matches!(command.create(invalid, &actor(), &mut SnapshotExecutor::default()).await,
            Err(Error::ValidationError(message)) if message == "资源ID不能为空"));
        assert!(
            matches!(command.set_status("", TemplateStatusRequest { version: 7, enabled: false }, &actor(), &mut SnapshotExecutor::default()).await,
            Err(Error::ValidationError(message)) if message == "资源ID不能为空")
        );
        memory.assert_no_effects();
    }

    #[tokio::test]
    async fn actual_management_steps_reject_missing_or_other_entry_permissions_before_all_effects() {
        for grants in [
            vec![],
            vec![contract::TEMPLATES_LIST],
            vec![contract::TEMPLATES_APPLICATIONS],
            vec![contract::TEMPLATES_DOWNLOAD],
            vec![contract::TEMPLATES_APPLY],
        ] {
            let authorization = AuthorizationFixture::new(&[("other-entry", &grants)]).build();
            let memory = Memory::new();
            let command = writer(&authorization, &memory);
            let mut executor = SnapshotExecutor::default();
            assert!(matches!(
                command.create(template(), &actor(), &mut executor).await,
                Err(Error::Forbidden(_))
            ));
            assert!(matches!(
                command
                    .set_status(
                        "template",
                        TemplateStatusRequest { version: 7, enabled: false },
                        &actor(),
                        &mut executor
                    )
                    .await,
                Err(Error::Forbidden(_))
            ));
            assert!(matches!(
                command.configure_counter(configuration(), &actor(), &mut executor).await,
                Err(Error::Forbidden(_))
            ));
            memory.assert_no_effects();
        }
    }

    #[tokio::test]
    async fn actual_first_application_rejects_other_permissions_before_lookup_allocation_writes_and_audit() {
        for grants in [
            vec![],
            vec![contract::TEMPLATES_LIST],
            vec![contract::TEMPLATES_CREATE],
            vec![contract::TEMPLATES_APPLICATIONS],
            vec![contract::TEMPLATES_DOWNLOAD],
        ] {
            let authorization = AuthorizationFixture::new(&[("other-entry", &grants)]).build();
            let memory = Memory::new();
            assert!(matches!(
                writer(&authorization, &memory)
                    .apply(request(), &actor(), &mut SnapshotExecutor::default())
                    .await,
                Err(Error::Forbidden(_))
            ));
            memory.assert_no_effects();
        }
    }

    #[tokio::test]
    async fn all_four_actual_write_steps_reject_no_transaction_and_read_only_executors_without_effects() {
        let authorization = AuthorizationFixture::new(&[(
            "writer",
            &[contract::TEMPLATES_CREATE, contract::TEMPLATES_APPLY],
        )])
        .build();
        for executor in [&mut NoTransaction as &mut dyn Executor, &mut ReadExecutor::default()] {
            let memory = Memory::new();
            let command = writer(&authorization, &memory);
            assert!(matches!(
                command.create(template(), &actor(), executor).await,
                Err(Error::ValidationError(_))
            ));
            assert!(matches!(
                command
                    .set_status(
                        "template",
                        TemplateStatusRequest { version: 7, enabled: false },
                        &actor(),
                        executor
                    )
                    .await,
                Err(Error::ValidationError(_))
            ));
            assert!(matches!(
                command.configure_counter(configuration(), &actor(), executor).await,
                Err(Error::ValidationError(_))
            ));
            assert!(matches!(
                command.apply(request(), &actor(), executor).await,
                Err(Error::ValidationError(_))
            ));
            memory.assert_no_effects();
        }
    }

    #[tokio::test]
    async fn management_create_binds_group_persists_template_and_audits_on_the_original_executor() {
        let authorization = AuthorizationFixture::new(&[("manager", &[contract::TEMPLATES_CREATE])]).build();
        let memory = Memory::new();
        memory.0.lock().unwrap().template = None;
        let mut executor = SnapshotExecutor::default();
        let view = writer(&authorization, &memory).create(template(), &actor(), &mut executor).await.unwrap();
        assert_eq!(view.id, "template");
        assert_eq!(view.name, "模板");
        assert_eq!(
            memory.events(),
            ["company", "numbering.read", "numbering.create", "template.create", "audit"]
        );
        memory.assert_executor(&executor);
        let state = memory.0.lock().unwrap();
        assert_eq!(state.numbering.as_ref().unwrap().group, NumberGroup::FSY);
        assert_eq!(state.template.as_ref().unwrap().base.id, view.id);
        assert_eq!(state.audits.len(), 1);
        assert_eq!(state.audits[0].actor_id, "user");
        assert_eq!(state.audits[0].action, "contract_template.create");
        assert_eq!(state.audits[0].resource_id.as_deref(), Some("template"));
    }

    #[tokio::test]
    async fn first_application_allocates_once_and_same_key_replays_without_new_allocation_write_or_audit() {
        let authorization = AuthorizationFixture::new(&[("applicant", &[contract::TEMPLATES_APPLY])]).build();
        let memory = Memory::new();
        let mut executor = SnapshotExecutor::default();
        let command = writer(&authorization, &memory);
        let first = command.apply(request(), &actor(), &mut executor).await.unwrap();
        assert_eq!(first.template_name, "模板");
        assert_eq!(first.purpose, "签约");
        assert_eq!(
            memory.events(),
            [
                "application.read",
                "template.read",
                "company",
                "counter.read",
                "number.exists",
                "counter.create",
                "application.create",
                "audit"
            ]
        );
        memory.assert_executor(&executor);
        {
            let mut state = memory.0.lock().unwrap();
            let saved = state.application.as_ref().unwrap();
            assert_eq!(saved.applicant_id, "user");
            assert_eq!(saved.command_id, "command");
            assert_eq!(saved.contract_no, first.contract_no);
            assert_eq!(state.applicant, Some(("user".into(), "command".into())));
            let counter = state.counter.as_ref().unwrap();
            assert_eq!(counter.group.number(counter.year, counter.last_sequence).unwrap(), first.contract_no);
            assert_eq!(state.audits[0].action, "contract_application.apply");
            assert_eq!(state.audits[0].resource_id.as_deref(), Some(first.id.as_str()));
            state.events.clear();
        }
        let replay = command.apply(request(), &actor(), &mut executor).await.unwrap();
        assert_eq!((replay.id, replay.contract_no), (first.id, first.contract_no));
        assert_eq!(memory.events(), ["application.read"]);
        assert_eq!(memory.0.lock().unwrap().audits.len(), 1);
        let mut changed = request();
        changed.purpose = "不同用途".into();
        assert!(matches!(
            command.apply(changed, &actor(), &mut executor).await,
            Err(Error::ConflictError(_))
        ));
        assert_eq!(memory.events(), ["application.read", "application.read"]);
        assert_eq!(memory.0.lock().unwrap().audits.len(), 1);
    }

    #[tokio::test]
    async fn management_version_conflicts_and_counter_forward_only_rules_remain_in_actual_steps() {
        let authorization = AuthorizationFixture::new(&[("manager", &[contract::TEMPLATES_CREATE])]).build();
        let memory = Memory::new();
        let mut executor = SnapshotExecutor::default();
        let command = writer(&authorization, &memory);
        assert!(matches!(
            command
                .set_status(
                    "template",
                    TemplateStatusRequest { version: 6, enabled: false },
                    &actor(),
                    &mut executor
                )
                .await,
            Err(Error::ConflictError(_))
        ));
        assert_eq!(memory.events(), ["template.read"]);
        memory.0.lock().unwrap().events.clear();
        let view = command
            .set_status(
                "template",
                TemplateStatusRequest { version: 7, enabled: false },
                &actor(),
                &mut executor,
            )
            .await
            .unwrap();
        assert!(!view.enabled);
        assert_eq!(memory.events(), ["template.read", "template.update", "audit"]);
        assert!(!memory.0.lock().unwrap().template.as_ref().unwrap().enabled);
        memory.0.lock().unwrap().events.clear();
        let counter = command.configure_counter(configuration(), &actor(), &mut executor).await.unwrap();
        assert_eq!(counter.last_sequence, 460);
        assert_eq!(memory.events(), ["counter.read", "counter.create", "audit"]);
        memory.assert_executor(&executor);
        memory.0.lock().unwrap().events.clear();
        let mut stale = configuration();
        stale.version = Some(99);
        assert!(matches!(
            command.configure_counter(stale, &actor(), &mut executor).await,
            Err(Error::ConflictError(_))
        ));
        let mut backwards = configuration();
        backwards.version = Some(memory.0.lock().unwrap().counter.as_ref().unwrap().base.version);
        backwards.last_sequence = 459;
        assert!(matches!(
            command.configure_counter(backwards, &actor(), &mut executor).await,
            Err(Error::ValidationError(_))
        ));
        assert_eq!(memory.events(), ["counter.read", "counter.read"]);
        assert_eq!(memory.0.lock().unwrap().counter.as_ref().unwrap().last_sequence, 460);
    }

    #[tokio::test]
    async fn qualified_store_failures_keep_the_original_error_and_stop_before_success_audit() {
        let authorization = AuthorizationFixture::new(&[(
            "writer",
            &[contract::TEMPLATES_CREATE, contract::TEMPLATES_APPLY],
        )])
        .build();
        for (failure, application) in [
            ("company", false),
            ("template.create", false),
            ("application.read", true),
            ("template.read", true),
            ("company", true),
            ("counter.create", true),
            ("application.create", true),
        ] {
            let memory = Memory::new();
            memory.0.lock().unwrap().fail = Some(failure);
            let command = writer(&authorization, &memory);
            let mut executor = SnapshotExecutor::default();
            let result = if application {
                command.apply(request(), &actor(), &mut executor).await.map(|_| ())
            } else {
                command.create(template(), &actor(), &mut executor).await.map(|_| ())
            };
            assert!(
                matches!(result, Err(Error::Internal(message)) if message == format!("injected {failure}"))
            );
            assert_eq!(memory.events().last(), Some(&failure));
            assert!(!memory.events().contains(&"audit"));
            assert!(memory.0.lock().unwrap().audits.is_empty());
            memory.assert_executor(&executor);
        }
    }

    #[tokio::test]
    async fn actual_steps_keep_inactive_company_disabled_template_and_numbering_conflicts() {
        let authorization = AuthorizationFixture::new(&[(
            "writer",
            &[contract::TEMPLATES_CREATE, contract::TEMPLATES_APPLY],
        )])
        .build();
        let memory = Memory::new();
        memory.0.lock().unwrap().active = false;
        let command = writer(&authorization, &memory);
        assert!(matches!(
            command.create(template(), &actor(), &mut SnapshotExecutor::default()).await,
            Err(Error::ValidationError(_))
        ));
        assert_eq!(memory.events(), ["company"]);
        memory.0.lock().unwrap().events.clear();
        memory.0.lock().unwrap().template.as_mut().unwrap().enabled = false;
        assert!(matches!(
            command.apply(request(), &actor(), &mut SnapshotExecutor::default()).await,
            Err(Error::BusinessLogicError(_))
        ));
        assert_eq!(memory.events(), ["application.read", "template.read"]);
        let mut state = memory.0.lock().unwrap();
        state.active = true;
        state.events.clear();
        state.numbering = Some(CompanyNumbering {
            base: BaseModel { id: "company".into(), ..BaseModel::fake() },
            group: NumberGroup::GYL,
        });
        drop(state);
        assert!(matches!(
            command.create(template(), &actor(), &mut SnapshotExecutor::default()).await,
            Err(Error::ConflictError(_))
        ));
        assert_eq!(memory.events(), ["company", "numbering.read"]);
        assert!(memory.0.lock().unwrap().audits.is_empty());
    }
}
