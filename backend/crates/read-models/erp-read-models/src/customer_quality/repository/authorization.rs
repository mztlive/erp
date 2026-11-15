//! 现任质量查询复用销售根和客户真实来源的固定映射。
use std::collections::BTreeMap;
use std::sync::Arc;

use erp_identity::entity::authorization::{AccessContext, AccessFact, MappingFacts, QueryModel, RelationKey};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{AccessMapping, Error, Result};
use mongodb::bson::{Bson, doc};

/// 在销售订单上保留真实客户关系，不预筛除缺失或无权客户。
pub struct QualitySalesMapping {
    sales: Arc<dyn AccessMapping>,
    customer: Arc<dyn AccessMapping>,
}
impl QualitySalesMapping {
    /// 连接共享销售及客户字段映射。
    /// # 参数
    /// sales 为完整销售模型，customer 为完整客户模型。
    /// # 返回
    /// 供固定现任质量查询入口复用的映射。
    /// # 错误
    /// 根或来源资源不匹配时拒绝。
    pub fn new(sales: Arc<dyn AccessMapping>, customer: Arc<dyn AccessMapping>) -> Result<Self> {
        if sales.model().resource != "sales_order" || customer.model().resource != "customer" {
            return Err(Error::ValidationError("客户质量查询资源错配".into()));
        }
        Ok(Self { sales, customer })
    }
}
impl AccessMapping for QualitySalesMapping {
    fn model(&self) -> &QueryModel {
        self.sales.model()
    }
    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        [(RelationKey::parse("customer").expect("固定客户关系"), self.customer.clone())].into()
    }
    fn key(&self, context: &AccessContext, facts: Option<&MappingFacts>) -> Result<Bson> {
        self.sales.key(context, facts)
    }
    fn records(&self, context: &AccessContext, facts: Option<&MappingFacts>) -> Result<QueryFragment> {
        self.sales.records(context, facts)
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        facts: Option<&MappingFacts>,
    ) -> Result<QueryFragment> {
        self.sales.predicate(fact, context, facts)
    }
    fn relation(
        &self,
        relation: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<RelationMapping> {
        if relation.as_str() != "customer" {
            return Err(Error::ValidationError("未知客户质量关系".into()));
        }
        Ok(RelationMapping {
            expected_ids:
                doc! { "$cond": [{ "$ne": [{ "$ifNull": ["$customer_id", ""] }, ""] }, ["$customer_id"], []] }
                    .into(),
            mapping: self.customer.clone(),
        })
    }
}
