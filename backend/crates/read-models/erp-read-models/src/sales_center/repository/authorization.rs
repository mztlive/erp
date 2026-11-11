//! 单个销售事实翻译；不组合权限规则。
use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, QueryModelKey,
    QueryOutput, RelatedObjects, RelationKey, ResourceTargets,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_identity::{Error, Result};
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::repository::DocumentRegistryExt;
use erp_workflow::repository::document_registry::{DocumentParticipantRepositoryExt, participant_access};
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;

/// 将销售行映射为固定资源模型，供组合层登记复用。
pub struct SalesMapping {
    db: Database,
    model: QueryModel,
}
impl SalesMapping {
    /// 构造固定销售行模型。
    /// # 参数
    /// db 为事实数据库，resource 为服务端固定资源，key 为登记模型标识。
    /// # 返回
    /// 不执行查询的 Mapping。
    /// # 错误
    /// 非法模型标识拒绝。
    pub fn new(db: Database, resource: &str, key: &str) -> Result<Self> {
        Ok(Self {
            db,
            model: QueryModel {
                key: QueryModelKey::parse(key)?,
                resource: resource.into(),
                collection: Database::SALES_ORDERS.into(),
                id_field: "id".into(),
                output: QueryOutput::Object,
                facts: [
                    FactKind::Owner,
                    FactKind::Collaborators,
                    FactKind::Organization,
                    FactKind::SettlementParty,
                    FactKind::Warehouse,
                    FactKind::HistoryParticipants,
                ]
                .into(),
            },
        })
    }
}
#[async_trait]
impl AccessMapping for SalesMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, _: &AccessContext, _: &MappingFacts) -> Result<Bson> {
        Ok("$id".into())
    }
    async fn prepare(&self, context: &AccessContext, executor: &mut dyn Executor) -> Result<MappingFacts> {
        let revision = participant_access::revision(&self.db, context.actor.id(), executor)
            .await
            .map_err(workflow_error)?;
        if executor.session().is_none() {
            return Err(Error::ValidationError("销售授权查询必须使用业务事务".into()));
        }
        Ok(MappingFacts { version: Some(format!("sales-participants:{revision}")), valid_until: None })
    }
    fn records(&self, _: &AccessContext, _: &MappingFacts) -> Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at":0}))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: &MappingFacts,
    ) -> Result<QueryFragment> {
        match fact {
            AccessFact::Owned => Ok(QueryFragment::Match(doc! {"sales_owner_user_id":context.actor.id()})),
            AccessFact::Collaborating => Ok(QueryFragment::Match(doc! {"$expr":false})),
            AccessFact::OrganizationIn(ids) => {
                Ok(QueryFragment::Match(doc! {"business_org_unit_id":{"$in":ids.iter().collect::<Vec<_>>()}}))
            },
            AccessFact::SettlementPartyIn(ids) => {
                Ok(QueryFragment::Match(doc! {"settlement_party_id":{"$in":ids.iter().collect::<Vec<_>>()}}))
            },
            AccessFact::WarehouseIn(_) => Ok(QueryFragment::Match(doc! {"$expr":false})),
            AccessFact::HistoryParticipant => Ok(QueryFragment::Exists {
                collection: Database::DOCUMENT_PARTICIPANTS.into(),
                bindings: doc! {"document":"$id"},
                pipeline: vec![
                    doc! {"$match":{"participant_user_id":context.actor.id(),"deleted_at":0,"$expr":{"$eq":["$document_id","$$document"]}}},
                ],
            }),
            _ => Err(Error::ValidationError("销售模型不支持该事实维度".into())),
        }
    }
    fn relation(&self, key: &RelationKey, _: &AccessContext, _: &MappingFacts) -> Result<RelationMapping> {
        if key.as_str() != "sales_source" {
            return Err(Error::ValidationError("未登记的销售关系".into()));
        }
        Ok(RelationMapping {
            expected_ids: doc! {"$cond":[{"$ne":["$id",null]},["$id"],[]]}.into(),
            mapping: Arc::new(Self::new(self.db.clone(), "sales_order", "sales_orders")?),
        })
    }
}
pub(crate) struct SalesFacts(pub Database, pub bool);
#[async_trait]
impl AccessFactsPort for SalesFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let targets = ids.ids().iter().cloned().collect::<Vec<_>>();
        let participants =
            self.0.document_participants().document_ids_in(context.actor.id(), &targets, executor).await?;
        let mut result = Vec::new();
        for id in ids.ids() {
            let Some(order) = self.0.sales_orders().find_by_id(id, executor).await? else { continue };
            let child = ObjectFacts {
                resource: "sales_order".into(),
                id: id.clone(),
                version: order.base.version.to_string(),
                owner_id: FactValue::Known(Some(order.sales_owner_user_id.to_string())),
                org_unit_id: FactValue::Known(Some(order.business_org_unit_id.to_string())),
                settlement_party_id: FactValue::Known(Some(order.settlement_party_id.to_string())),
                warehouse_id: FactValue::Known(None),
                collaborators: FactValue::Known(Default::default()),
                history_participants: FactValue::Known(if participants.contains(id) {
                    [context.actor.id().into()].into()
                } else {
                    Default::default()
                }),
                ..Default::default()
            };
            let mut root = ObjectFacts { resource: context.resource.clone(), ..child.clone() };
            if self.1 {
                root.relations.insert(
                    RelationKey::parse("sales_source")?,
                    RelatedObjects { expected_ids: [id.clone()].into(), objects: vec![child] },
                );
            }
            result.push(root);
        }
        Ok(result)
    }
}

fn workflow_error(error: erp_workflow::Error) -> Error {
    match error {
        erp_workflow::Error::RepositoryError(error) => Error::RepositoryError(error),
        erp_workflow::Error::ConflictError(message) => Error::ConflictError(message),
        _ => Error::Internal(error.to_string()),
    }
}

pub(crate) struct LoadedSales(pub SalesOrder);
#[async_trait]
impl AccessFactsPort for LoadedSales {
    async fn load(
        &self,
        context: &AccessContext,
        _: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        Ok(vec![ObjectFacts {
            resource: context.resource.clone(),
            id: self.0.base.id.clone(),
            version: self.0.base.version.to_string(),
            owner_id: FactValue::Known(Some(self.0.sales_owner_user_id.to_string())),
            org_unit_id: FactValue::Known(Some(self.0.business_org_unit_id.to_string())),
            settlement_party_id: FactValue::Known(Some(self.0.settlement_party_id.to_string())),
            warehouse_id: FactValue::Known(None),
            collaborators: FactValue::Known(Default::default()),
            history_participants: FactValue::Known(Default::default()),
            ..Default::default()
        }])
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    struct RegisteredModel(QueryModel);

    #[async_trait]
    impl AccessMapping for RegisteredModel {
        fn model(&self) -> &QueryModel {
            &self.0
        }
        fn key(&self, _: &AccessContext, _: &MappingFacts) -> Result<Bson> {
            panic!("登记校验不得执行查询")
        }
        async fn prepare(&self, _: &AccessContext, _: &mut dyn Executor) -> Result<MappingFacts> {
            panic!("登记校验不得准备查询事实")
        }
        fn records(&self, _: &AccessContext, _: &MappingFacts) -> Result<QueryFragment> {
            panic!("登记校验不得读取记录")
        }
        fn predicate(&self, _: &AccessFact, _: &AccessContext, _: &MappingFacts) -> Result<QueryFragment> {
            panic!("登记校验不得编译条件")
        }
        fn relation(&self, _: &RelationKey, _: &AccessContext, _: &MappingFacts) -> Result<RelationMapping> {
            panic!("登记校验不得展开来源")
        }
    }

    struct RegisteredFacts;

    #[async_trait]
    impl AccessFactsPort for RegisteredFacts {
        async fn load(
            &self,
            _: &AccessContext,
            _: &ResourceTargets,
            _: &mut dyn Executor,
        ) -> Result<Vec<ObjectFacts>> {
            panic!("登记校验不得读取对象事实")
        }
    }

    /// 构造不执行查询的固定销售模型，供真实登记校验使用。
    /// # 参数
    /// resource 为测试消费入口的资源。
    /// # 返回
    /// 只允许读取模型声明的 Mapping。
    /// # 错误
    /// 不返回错误；执行查询时测试失败。
    pub(crate) fn mapping(resource: &str) -> Arc<dyn AccessMapping> {
        Arc::new(RegisteredModel(QueryModel {
            key: QueryModelKey::parse("sales_orders").unwrap(),
            resource: resource.into(),
            collection: "sales_orders".into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Owner, FactKind::Organization].into(),
        }))
    }

    /// 构造不会读取数据库的对象事实替身。
    /// # 参数
    /// 无。
    /// # 返回
    /// 仅供登记形态校验使用的事实端口。
    /// # 错误
    /// 不返回错误；读取事实时测试失败。
    pub(crate) fn facts() -> Arc<dyn AccessFactsPort> {
        Arc::new(RegisteredFacts)
    }
}
