//! 三域申请的固定供应商过滤、联合分页与领域根读取。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_catalog::portal::{CatalogPortalExt, NewProductDraft};
use erp_supplier::portal::{COOPERATION_APPLICATIONS, CooperationApplication, CooperationRepository};
use erp_supply::portal::{OfferingApplication, PortalSupplyExt};
use mongodb::bson::{Document, deserialize_from_document, doc};
use mongodb::{Collection, Database};
use persistence_core::Executor;
use serde::Deserialize;
use serde_json::Value;

use super::super::PortalApplicationView;
use super::super::views::value;
use super::query::{PortalQuery, aggregate};
use crate::{Error, Result};

#[derive(Deserialize)]
struct ApplicationPage {
    items: Vec<ApplicationRecord>,
    count: Vec<ApplicationCount>,
}
#[derive(Deserialize)]
struct ApplicationCount {
    total: i64,
}
#[derive(Deserialize)]
struct ApplicationRecord {
    domain: String,
    record: Document,
}
#[derive(Deserialize)]
struct ApplicationId {
    id: String,
}

pub(in crate::supplier_portal) enum Application {
    Offering(Box<OfferingApplication>),
    NewProduct(Box<NewProductDraft>),
    Cooperation(Box<CooperationApplication>),
}

/// 只读跨域仓储。
pub(in crate::supplier_portal) struct PortalApplicationRepository<'a> {
    db: &'a Database,
}
impl<'a> PortalApplicationRepository<'a> {
    /// 绑定数据库，不缓存账号或对象授权。
    pub(in crate::supplier_portal) fn new(db: &'a Database) -> Self {
        Self { db }
    }
    /// 有界取得待逐申请授权的身份，超限必须收窄。
    pub(in crate::supplier_portal) async fn application_ids(
        &self,
        supplier_id: &str,
        query: &PortalQuery,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let mut pipeline = application_pipeline(supplier_id, query);
        pipeline.push(doc! {"$project":{"_id":0,"id":"$record.id"}});
        pipeline.push(doc! {"$limit":10_001});
        let ids = aggregate::<ApplicationId>(self.application_collection(), pipeline, executor).await?;
        if ids.len() > 10_000 {
            return Err(Error::ValidationError("申请数量超出审核列表范围，请收窄状态或搜索".into()));
        }
        Ok(ids.into_iter().map(|row| row.id).collect())
    }

    /// 固定供应商与申请身份，未命中各域均保持同一不可见语义。
    pub(in crate::supplier_portal) async fn scoped_application(
        &self,
        supplier_id: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Application> {
        let filter = doc! {"id":id,"supplier_id":supplier_id};
        if let Some(app) = self.db.portal_applications().find_one(filter.clone(), executor).await? {
            return Ok(Application::Offering(Box::new(app)));
        }
        if let Some(app) = self.db.new_product_drafts().find_one(filter, executor).await? {
            return Ok(Application::NewProduct(Box::new(app)));
        }
        if let Some(app) = CooperationRepository::new(self.db).get(id, supplier_id, executor).await? {
            return Ok(Application::Cooperation(Box::new(app)));
        }
        Err(hidden_target())
    }

    /// 内部授权通过后才读取完整申请。
    pub(in crate::supplier_portal) async fn any_application(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Application> {
        if let Some(app) = self.db.portal_applications().find_by_id(id, executor).await? {
            return Ok(Application::Offering(Box::new(app)));
        }
        if let Some(app) = self.db.new_product_drafts().find_by_id(id, executor).await? {
            return Ok(Application::NewProduct(Box::new(app)));
        }
        if let Some(app) = CooperationRepository::new(self.db).find_any(id, executor).await? {
            return Ok(Application::Cooperation(Box::new(app)));
        }
        Err(hidden_target())
    }

    /// 同一阶段过滤集合再分页，授权后页总数保持一致。
    pub(in crate::supplier_portal) async fn application_page(
        &self,
        supplier_id: &str,
        query: &PortalQuery,
        allowed: Option<&[String]>,
        executor: &mut dyn Executor,
    ) -> Result<(Vec<Application>, i64)> {
        let mut pipeline = application_pipeline(supplier_id, query);
        if let Some(ids) = allowed {
            pipeline.push(doc! {"$match":{"record.id":{"$in":ids}}});
        }
        pipeline.push(doc! {"$sort":{"sort_at":-1,"sort_id":-1,"domain":1}});
        pipeline.push(doc! {"$facet":{"items":[{"$skip":query.skip},{"$limit":i64::from(query.page_size)}],"count":[{"$count":"total"}]}});
        let page = aggregate::<ApplicationPage>(self.application_collection(), pipeline, executor)
            .await?
            .into_iter()
            .next();
        let (rows, total) = page
            .map(|page| (page.items, page.count.first().map_or(0, |count| count.total)))
            .unwrap_or_default();
        let items = rows.into_iter().map(Application::from_record).collect::<Result<Vec<_>>>()?;
        Ok((items, total))
    }

    /// 从供给领域公开仓储取得集合，其他域由公开集合名参与只读联合。
    fn application_collection(&self) -> Collection<Document> {
        self.db.portal_applications().collection().clone_with_type::<Document>()
    }
}

impl Application {
    /// 将所属领域根记录还原为真实领域类型。
    fn from_record(row: ApplicationRecord) -> Result<Self> {
        match row.domain.as_str() {
            "offering" => deserialize_from_document(row.record).map(Self::Offering),
            "new_product" => deserialize_from_document(row.record).map(Self::NewProduct),
            "cooperation" => deserialize_from_document(row.record).map(Self::Cooperation),
            _ => return Err(Error::Internal("门户申请类型非法".into())),
        }
        .map_err(|error| Error::Internal(error.to_string()))
    }

    /// 外部仅序列化声明允许的字段。
    pub(in crate::supplier_portal) fn external(&self) -> Result<PortalApplicationView> {
        match self {
            Self::Offering(app) => PortalApplicationView::from_offering(app),
            Self::NewProduct(app) => PortalApplicationView::from_new_product(app),
            Self::Cooperation(app) => PortalApplicationView::from_cooperation(app),
        }
    }

    /// 内部审核读取保留任务和规范化历史，并加入统一协议字段。
    pub(in crate::supplier_portal) fn internal(&self) -> Result<Value> {
        let mut result = match self {
            Self::Offering(app) => value(app)?,
            Self::NewProduct(app) => value(app)?,
            Self::Cooperation(app) => value(app)?,
        };
        let external = self.external()?;
        let object = result.as_object_mut().ok_or_else(|| Error::Internal("申请记录不是对象".into()))?;
        object.insert("kind".into(), Value::String(external.kind));
        object.insert("status".into(), Value::String(external.status));
        object.insert("input".into(), external.input);
        object.insert("version".into(), Value::from(external.version));
        if let Self::NewProduct(app) = self {
            object.insert("work_item_id".into(), value(&app.task_id)?);
            object.insert("current_submission_id".into(), value(&app.current_submission_id)?);
        }
        if let Self::Cooperation(app) = self
            && let Some(submission) = app.submissions.last()
        {
            object.insert("work_item_id".into(), value(&submission.task_id)?);
            object.insert("handler_id".into(), value(&submission.procurement_owner_id)?);
        }
        if let Self::Offering(app) = self
            && let Some(submission) = app.submissions.last()
        {
            object.insert("work_item_id".into(), value(&submission.work_item_id)?);
            object.insert("work_item_version".into(), value(submission.work_item_version)?);
            object.insert("handler_id".into(), value(&submission.handler_id)?);
        }
        Ok(result)
    }
}

/// 对每个所属集合先应用相同的供应商边界，不扫描全库请求。
fn application_pipeline(supplier_id: &str, query: &PortalQuery) -> Vec<Document> {
    let mut pipeline = domain_pipeline(supplier_id, "offering");
    pipeline.push(doc! {"$unionWith":{"coll":<Database as CatalogPortalExt>::NEW_PRODUCT_DRAFTS,"pipeline":domain_pipeline(supplier_id,"new_product")}});
    pipeline.push(doc! {"$unionWith":{"coll":COOPERATION_APPLICATIONS,"pipeline":domain_pipeline(supplier_id,"cooperation")}});
    if let Some(status) = &query.status {
        pipeline.push(doc! {"$match":{"status":status}});
    }
    if let Some(q) = &query.q {
        let regex = regex::escape(q);
        pipeline.push(doc! {"$match":{"$or":[{"record.id":{"$regex":&regex,"$options":"i"}},{"record.reason":{"$regex":&regex,"$options":"i"}},{"record.draft.name":{"$regex":&regex,"$options":"i"}},{"record.proposal.reason":{"$regex":&regex,"$options":"i"}},{"record.snapshot.supplier_sku_code":{"$regex":regex,"$options":"i"}}]}});
    }
    pipeline
}

/// 保存完整领域记录于内部载荷，统一状态只用于过滤和排序。
fn domain_pipeline(supplier_id: &str, domain: &str) -> Vec<Document> {
    vec![
        doc! {"$match":{"supplier_id":supplier_id,"deleted_at":NOT_DELETED_TIMESTAMP_BSON}},
        doc! {"$project":{"_id":0,"domain":{"$literal":domain},"record":"$$ROOT","sort_at":"$created_at","sort_id":"$id","status":{"$cond":[{"$eq":[{"$toUpper":"$status"},"PENDING"]},"SUBMITTED",{"$toUpper":"$status"}]}}},
    ]
}

/// 未知、已删除与范围外目标共用错误，不暴露对象存在性差别。
fn hidden_target() -> Error {
    Error::NotFound("申请不存在或无权查看".into())
}
