//! 模板相关集合访问与有界分页；全部操作使用调用方执行器。

use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, mongo_ops};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::PageView;
use crate::dto::template::TemplateListParams;
use crate::entity::template::{CompanyNumbering, ContractApplication, ContractCounter, ContractTemplate};
use crate::error::Result;

pub const TEMPLATES: &str = "contract_templates";
pub const COUNTERS: &str = "contract_number_counters";
pub const APPLICATIONS: &str = "contract_applications";
pub const COMPANY_NUMBERING: &str = "contract_company_numbering";

/// 模板相关集合只由合同领域访问。
pub trait ContractTemplateExt {
    /// 获取模板仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 集合仓储。
    /// # 错误
    /// 无。
    fn contract_templates(&self) -> Repository<'_, ContractTemplate>;
    /// 获取年度流水仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 集合仓储。
    /// # 错误
    /// 无。
    fn contract_counters(&self) -> Repository<'_, ContractCounter>;
    /// 获取申请仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 集合仓储。
    /// # 错误
    /// 无。
    fn contract_applications(&self) -> Repository<'_, ContractApplication>;
    /// 获取主体编号绑定仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 集合仓储。
    /// # 错误
    /// 无。
    fn contract_company_numbering(&self) -> Repository<'_, CompanyNumbering>;
}

impl ContractTemplateExt for Database {
    fn contract_templates(&self) -> Repository<'_, ContractTemplate> {
        Repository::new(self, TEMPLATES)
    }
    fn contract_counters(&self) -> Repository<'_, ContractCounter> {
        Repository::new(self, COUNTERS)
    }
    fn contract_applications(&self) -> Repository<'_, ContractApplication> {
        Repository::new(self, APPLICATIONS)
    }
    fn contract_company_numbering(&self) -> Repository<'_, CompanyNumbering> {
        Repository::new(self, COMPANY_NUMBERING)
    }
}

/// 按申请人与稳定请求键读取原申请。
/// # 参数
/// * `db` - 合同库。
/// * `applicant` - 申请人。
/// * `command` - 稳定请求键。
/// * `executor` - 执行器。
/// # 返回
/// 原记录或 None。
/// # 错误
/// 查询失败。
pub(crate) async fn application_by_command(
    db: &Database,
    applicant: &str,
    command: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ContractApplication>> {
    Ok(db
        .contract_applications()
        .find_one(doc! { "applicant_id": applicant, "command_id": command }, executor)
        .await?)
}

/// 只读取当前申请人的指定记录。
/// # 参数
/// * `db` - 合同库。
/// * `id` - 申请 ID。
/// * `applicant` - 申请人。
/// * `executor` - 执行器。
/// # 返回
/// 本人记录或 None。
/// # 错误
/// 查询失败。
pub(crate) async fn owned_application(
    db: &Database,
    id: &str,
    applicant: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ContractApplication>> {
    Ok(db.contract_applications().find_one(doc! { "id": id, "applicant_id": applicant }, executor).await?)
}

/// 按目录条件在数据库分页。
/// # 参数
/// * `db` - 合同库。
/// * `params` - 分页与启停条件。
/// * `executor` - 执行器。
/// # 返回
/// 模板页。
/// # 错误
/// 查询失败。
pub(crate) async fn templates_page(
    db: &Database,
    params: &TemplateListParams,
    executor: &mut dyn Executor,
) -> Result<PageView<ContractTemplate>> {
    let filter = if params.include_disabled == Some(true) {
        doc! {}
    } else {
        doc! { "enabled": true }
    };
    page(db.contract_templates(), filter, params.pagination(), executor).await
}

/// 按本人范围在数据库分页。
/// # 参数
/// * `db` - 合同库。
/// * `applicant` - 申请人。
/// * `params` - 分页。
/// * `executor` - 执行器。
/// # 返回
/// 本人申请页。
/// # 错误
/// 查询失败。
pub(crate) async fn applications_page(
    db: &Database,
    applicant: &str,
    params: &TemplateListParams,
    executor: &mut dyn Executor,
) -> Result<PageView<ContractApplication>> {
    page(db.contract_applications(), doc! { "applicant_id": applicant }, params.pagination(), executor).await
}

/// 稳定排序后在数据库分页，禁止无界加载。
/// # 参数
/// * `repository` - 集合仓储。
/// * `filter` - 业务及申请人限制。
/// * `pagination` - 已规范化的页码与容量。
/// * `executor` - 调用方执行器。
/// # 返回
/// 当前页与匹配总数。
/// # 错误
/// 查询或反序列化失败。
pub(crate) async fn page<T>(
    repository: Repository<'_, T>,
    mut filter: Document,
    pagination: (u64, u32),
    executor: &mut dyn Executor,
) -> Result<PageView<T>>
where
    T: Serialize + DeserializeOwned + Send + Sync,
{
    let (page, page_size) = pagination;
    filter.insert("deleted_at", 0_i64);
    let collection = repository.collection();
    let options = FindOptions::builder()
        .sort(doc! { "created_at": -1, "id": -1 })
        .skip((page - 1) * u64::from(page_size))
        .limit(i64::from(page_size))
        .build();
    let items = mongo_ops::find_many(&collection, filter.clone(), options, &mut *executor).await?;
    let total = mongo_ops::count_documents(&collection, filter, executor).await?;
    Ok(PageView { items, total: i64::try_from(total).unwrap_or(i64::MAX), page, page_size })
}
