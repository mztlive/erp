//! 模板目录和当前申请人记录的数据库分页。

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::contract;
use persistence_core::{Executor, Transactional};

use super::{ContractTemplateService, TemplateDownloadSource};
use crate::PageView;
use crate::dto::template::{ApplicationView, TemplateListParams, TemplateView};
use crate::entity::template::{ContractApplication, ContractTemplate};
use crate::error::{Error, Result};
use crate::repository::templates::{applications_page, owned_application, templates_page};

/// 模板读取的存储边界；资格与本人身份仍由下方实际用例固定。
#[async_trait]
trait TemplateRead: Sync {
    async fn load_templates(
        &self,
        params: &TemplateListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<ContractTemplate>>;
    async fn load_applications(
        &self,
        applicant: &str,
        params: &TemplateListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<ContractApplication>>;
    async fn load_application(
        &self,
        id: &str,
        applicant: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ContractApplication>>;
    async fn load_download(
        &self,
        application: ContractApplication,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<TemplateDownloadSource>;
}

#[async_trait]
impl TemplateRead for ContractTemplateService {
    async fn load_templates(
        &self,
        params: &TemplateListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<ContractTemplate>> {
        templates_page(&self.db, params, executor).await
    }
    async fn load_applications(
        &self,
        applicant: &str,
        params: &TemplateListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<ContractApplication>> {
        applications_page(&self.db, applicant, params, executor).await
    }
    async fn load_application(
        &self,
        id: &str,
        applicant: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ContractApplication>> {
        owned_application(&self.db, id, applicant, executor).await
    }
    async fn load_download(
        &self,
        application: ContractApplication,
        _actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<TemplateDownloadSource> {
        let template = self.template(&application.template_id, executor).await?;
        Ok(TemplateDownloadSource { object_key: template.object_key(), contract_no: application.contract_no })
    }
}

async fn read_templates(
    authorization: &AuthorizationService,
    port: &impl TemplateRead,
    actor: &AuditActor,
    params: &TemplateListParams,
    executor: &mut dyn Executor,
) -> Result<PageView<TemplateView>> {
    authorization.require(actor, &contract::TEMPLATES_LIST, AccessTarget::Operation, executor).await?;
    let result = port.load_templates(params, executor).await?;
    Ok(PageView {
        items: result.items.into_iter().map(Into::into).collect(),
        total: result.total,
        page: result.page,
        page_size: result.page_size,
    })
}

async fn read_applications(
    authorization: &AuthorizationService,
    port: &impl TemplateRead,
    actor: &AuditActor,
    params: &TemplateListParams,
    executor: &mut dyn Executor,
) -> Result<PageView<ApplicationView>> {
    authorization
        .require(actor, &contract::TEMPLATES_APPLICATIONS, AccessTarget::Operation, executor)
        .await?;
    let result = port.load_applications(actor.id(), params, executor).await?;
    Ok(PageView {
        items: result.items.into_iter().map(Into::into).collect(),
        total: result.total,
        page: result.page,
        page_size: result.page_size,
    })
}

pub(super) async fn read_download(
    service: &ContractTemplateService,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<TemplateDownloadSource> {
    download(&service.authorization, service, id, actor, executor).await
}

async fn download(
    authorization: &AuthorizationService,
    port: &impl TemplateRead,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<TemplateDownloadSource> {
    authorization.require(actor, &contract::TEMPLATES_DOWNLOAD, AccessTarget::Operation, executor).await?;
    let application = port
        .load_application(id, actor.id(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("合同申请不存在或无权下载".into()))?;
    port.load_download(application, actor, executor).await
}

impl ContractTemplateService {
    /// 查询共享模板目录，已停用模板只用于维护回显。
    /// # 参数
    /// * `params` - 分页与停用回显选项。
    /// # 返回
    /// 稳定排序的模板公开视图。
    /// # 错误
    /// 数据读取失败。
    pub async fn templates(
        &self,
        actor: &AuditActor,
        params: &TemplateListParams,
    ) -> Result<PageView<TemplateView>> {
        let service = self.clone();
        let actor = actor.clone();
        let params = params.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.templates_snapshot(&actor, &params, executor).await })
            })
            .await
    }

    /// 在同一授权快照读取业务数据；执行器由公开入口提供。
    async fn templates_snapshot(
        &self,
        actor: &AuditActor,
        params: &TemplateListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<TemplateView>> {
        read_templates(&self.authorization, self, actor, params, executor).await
    }

    /// 查询本人合同申请，不能通过查询参数指定他人。
    /// # 参数
    /// * `params` - 分页参数。
    /// * `actor` - 当前销售。
    /// # 返回
    /// 本人的申请记录。
    /// # 错误
    /// 数据读取失败。
    pub async fn applications(
        &self,
        params: &TemplateListParams,
        actor: &AuditActor,
    ) -> Result<PageView<ApplicationView>> {
        let service = self.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.applications_snapshot(&params, &actor, executor).await })
            })
            .await
    }

    /// 在同一授权快照读取业务数据；执行器由公开入口提供。
    async fn applications_snapshot(
        &self,
        params: &TemplateListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PageView<ApplicationView>> {
        read_applications(&self.authorization, self, actor, params, executor).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use entity_core::BaseModel;
    use erp_core::AccountKind;
    use erp_identity::testing::AuthorizationFixture;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::entity::template::NumberGroup;

    struct Reads {
        fail: bool,
        calls: Mutex<Vec<(String, usize)>>,
    }

    impl Reads {
        fn record(&self, operation: &str, executor: &mut dyn Executor) -> Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push((operation.into(), executor as *mut dyn Executor as *mut () as usize));
            if self.fail {
                return Err(Error::Internal("template store unavailable".into()));
            }
            Ok(())
        }
    }

    fn application(id: &str, applicant: &str) -> ContractApplication {
        ContractApplication {
            base: BaseModel { id: id.into(), ..BaseModel::fake() },
            command_id: format!("command-{id}"),
            applicant_id: applicant.into(),
            template_id: "template".into(),
            template_name: "模板".into(),
            company_name: "公司".into(),
            purpose: "签约".into(),
            contract_no: format!("HT-{id}"),
        }
    }

    #[async_trait]
    impl TemplateRead for Reads {
        async fn load_templates(
            &self,
            params: &TemplateListParams,
            executor: &mut dyn Executor,
        ) -> Result<PageView<ContractTemplate>> {
            self.record("templates", executor)?;
            let (page, page_size) = params.pagination();
            Ok(PageView {
                items: vec![ContractTemplate {
                    base: BaseModel { id: "template".into(), ..BaseModel::fake() },
                    name: "模板".into(),
                    company_id: "company".into(),
                    company_name: "公司".into(),
                    group: NumberGroup::FSY,
                    file_name: "template.docx".into(),
                    enabled: true,
                }],
                total: 1,
                page,
                page_size,
            })
        }
        async fn load_applications(
            &self,
            applicant: &str,
            params: &TemplateListParams,
            executor: &mut dyn Executor,
        ) -> Result<PageView<ContractApplication>> {
            self.record(&format!("applications:{applicant}"), executor)?;
            let items = [application("mine", "user"), application("theirs", "other")]
                .into_iter()
                .filter(|row| row.applicant_id == applicant)
                .collect::<Vec<_>>();
            let (page, page_size) = params.pagination();
            Ok(PageView { total: i64::try_from(items.len()).unwrap(), items, page, page_size })
        }
        async fn load_application(
            &self,
            id: &str,
            applicant: &str,
            executor: &mut dyn Executor,
        ) -> Result<Option<ContractApplication>> {
            self.record(&format!("application:{applicant}:{id}"), executor)?;
            Ok([application("mine", "user"), application("theirs", "other")]
                .into_iter()
                .find(|row| row.base.id == id && row.applicant_id == applicant))
        }
        async fn load_download(
            &self,
            application: ContractApplication,
            actor: &AuditActor,
            executor: &mut dyn Executor,
        ) -> Result<TemplateDownloadSource> {
            assert_eq!(application.applicant_id, actor.id());
            self.record("download", executor)?;
            Ok(TemplateDownloadSource {
                object_key: "immutable-template.docx".into(),
                contract_no: application.contract_no,
            })
        }
    }

    fn actor() -> AuditActor {
        AuditActor::new("user".into(), "login".into(), AccountKind::Admin)
    }

    fn reads(fail: bool) -> Reads {
        Reads { fail, calls: Mutex::new(vec![]) }
    }

    #[tokio::test]
    async fn template_list_application_list_and_download_permissions_are_not_interchangeable() {
        for granted in
            [contract::TEMPLATES_LIST, contract::TEMPLATES_APPLICATIONS, contract::TEMPLATES_DOWNLOAD]
        {
            let authorization =
                AuthorizationFixture::new(&[("single-entry", std::slice::from_ref(&granted))]).build();
            let port = reads(false);
            let mut executor = SnapshotExecutor::default();
            let params = TemplateListParams::default();
            let templates = read_templates(&authorization, &port, &actor(), &params, &mut executor).await;
            if granted == contract::TEMPLATES_LIST {
                assert_eq!(templates.unwrap().items[0].id, "template");
            } else {
                assert!(matches!(templates, Err(Error::Forbidden(_))));
            }
            let applications =
                read_applications(&authorization, &port, &actor(), &params, &mut executor).await;
            if granted == contract::TEMPLATES_APPLICATIONS {
                assert_eq!(applications.unwrap().items[0].id, "mine");
            } else {
                assert!(matches!(applications, Err(Error::Forbidden(_))));
            }
            let source = download(&authorization, &port, "mine", &actor(), &mut executor).await;
            if granted == contract::TEMPLATES_DOWNLOAD {
                assert_eq!(source.unwrap().contract_no, "HT-mine");
            } else {
                assert!(matches!(source, Err(Error::Forbidden(_))));
            }
        }
    }

    #[tokio::test]
    async fn allowed_template_and_personal_application_reads_use_the_original_executor() {
        let authorization = AuthorizationFixture::new(&[(
            "reader",
            &[contract::TEMPLATES_LIST, contract::TEMPLATES_APPLICATIONS],
        )])
        .build();
        let port = reads(false);
        let params = TemplateListParams { page_size: Some(5), ..Default::default() };
        let mut executor = SnapshotExecutor::default();
        let pointer = &mut executor as *mut SnapshotExecutor as usize;
        let templates =
            read_templates(&authorization, &port, &actor(), &params, &mut executor).await.unwrap();
        assert_eq!(templates.items[0].name, "模板");
        assert_eq!((templates.total, templates.page, templates.page_size), (1, 1, 5));
        let applications =
            read_applications(&authorization, &port, &actor(), &params, &mut executor).await.unwrap();
        assert_eq!(applications.items.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(), ["mine"]);
        assert_eq!(applications.total, 1);
        assert_eq!(
            *port.calls.lock().unwrap(),
            [("templates".into(), pointer), ("applications:user".into(), pointer)]
        );
    }

    #[tokio::test]
    async fn template_entry_qualification_and_snapshot_failures_stop_before_any_storage_reads() {
        let params = TemplateListParams::default();
        for grants in [vec![], vec![contract::TEMPLATES_CREATE], vec![contract::DETAIL]] {
            let authorization = AuthorizationFixture::new(&[("wrong-entry", &grants)]).build();
            let port = reads(false);
            let mut executor = SnapshotExecutor::default();
            assert!(matches!(
                read_templates(&authorization, &port, &actor(), &params, &mut executor).await,
                Err(Error::Forbidden(_))
            ));
            assert!(matches!(
                read_applications(&authorization, &port, &actor(), &params, &mut executor).await,
                Err(Error::Forbidden(_))
            ));
            assert!(matches!(
                download(&authorization, &port, "mine", &actor(), &mut executor).await,
                Err(Error::Forbidden(_))
            ));
            assert!(port.calls.lock().unwrap().is_empty());
        }
        let authorization = AuthorizationFixture::new(&[(
            "reader",
            &[contract::TEMPLATES_LIST, contract::TEMPLATES_APPLICATIONS, contract::TEMPLATES_DOWNLOAD],
        )])
        .build();
        let port = reads(false);
        assert!(matches!(
            read_templates(&authorization, &port, &actor(), &params, &mut NoTransaction).await,
            Err(Error::ValidationError(_))
        ));
        assert!(matches!(
            read_applications(&authorization, &port, &actor(), &params, &mut NoTransaction).await,
            Err(Error::ValidationError(_))
        ));
        assert!(matches!(
            download(&authorization, &port, "mine", &actor(), &mut NoTransaction).await,
            Err(Error::ValidationError(_))
        ));
        assert!(port.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn download_uses_the_current_applicant_and_never_loads_a_foreign_template() {
        let authorization =
            AuthorizationFixture::new(&[("downloader", &[contract::TEMPLATES_DOWNLOAD])]).build();
        let port = reads(false);
        let mut executor = SnapshotExecutor::default();
        let pointer = &mut executor as *mut SnapshotExecutor as usize;
        let source = download(&authorization, &port, "mine", &actor(), &mut executor).await.unwrap();
        assert_eq!(source.contract_no, "HT-mine");
        assert_eq!(source.object_key, "immutable-template.docx");
        assert_eq!(
            *port.calls.lock().unwrap(),
            [("application:user:mine".into(), pointer), ("download".into(), pointer)]
        );
        port.calls.lock().unwrap().clear();
        for id in ["theirs", "missing"] {
            assert!(matches!(download(&authorization, &port, id, &actor(), &mut executor).await,
                Err(Error::NotFound(message)) if message == "合同申请不存在或无权下载"));
        }
        assert_eq!(
            *port.calls.lock().unwrap(),
            [("application:user:theirs".into(), pointer), ("application:user:missing".into(), pointer)]
        );
    }

    #[tokio::test]
    async fn template_storage_failure_is_preserved_after_successful_qualification() {
        let authorization = AuthorizationFixture::new(&[(
            "reader",
            &[contract::TEMPLATES_LIST, contract::TEMPLATES_APPLICATIONS, contract::TEMPLATES_DOWNLOAD],
        )])
        .build();
        let port = reads(true);
        let params = TemplateListParams::default();
        let mut executor = SnapshotExecutor::default();
        assert!(matches!(read_templates(&authorization, &port, &actor(), &params, &mut executor).await,
            Err(Error::Internal(message)) if message == "template store unavailable"));
        assert!(matches!(read_applications(&authorization, &port, &actor(), &params, &mut executor).await,
            Err(Error::Internal(message)) if message == "template store unavailable"));
        assert!(matches!(download(&authorization, &port, "mine", &actor(), &mut executor).await,
            Err(Error::Internal(message)) if message == "template store unavailable"));
        assert_eq!(port.calls.lock().unwrap().len(), 3);
    }
}
