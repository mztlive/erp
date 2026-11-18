//! 连接查询只消费统一授权条件；供应商来源映射由组合根注入。
use std::collections::BTreeMap;
use std::sync::Arc;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, MappingFacts, QueryModel, QueryModelKey, QueryOutput, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{AccessMapping, CompiledAccess, Error as IdentityError, Result as IdentityResult};
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, aggregate};

use super::SUPPLIER_API_CONNECTIONS;
use crate::entity::supplier_api::SupplierApiConnection;
use crate::{Error, Result};

struct ConnectionMapping {
    model: QueryModel,
    supplier: Arc<dyn AccessMapping>,
}

/// 描述连接主键和真实供应商引用，不声明访问政策。
/// # 参数
/// supplier 为组合根复用的供应商事实映射。
/// # 返回
/// 无动态准备的完整连接对象映射。
/// # 错误
/// 固定模型校验失败时拒绝。
pub fn connection_mapping(supplier: Arc<dyn AccessMapping>) -> IdentityResult<Arc<dyn AccessMapping>> {
    let model = QueryModel {
        key: QueryModelKey::parse("supplier_api_connection")?,
        resource: "supplier_api_connection".into(),
        collection: SUPPLIER_API_CONNECTIONS.into(),
        id_field: "id".into(),
        output: QueryOutput::Object,
        facts: Default::default(),
    };
    model.validate()?;
    Ok(Arc::new(ConnectionMapping { model, supplier }))
}

impl AccessMapping for ConnectionMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        [(RelationKey::parse("supplier").expect("固定供应商关系"), self.supplier.clone())].into()
    }
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at": NOT_DELETED_TIMESTAMP_BSON}))
    }
    fn predicate(
        &self,
        _: &AccessFact,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> IdentityResult<QueryFragment> {
        Err(IdentityError::ValidationError("连接根没有负责人或组织事实".into()))
    }
    fn relation(
        &self,
        key: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> IdentityResult<RelationMapping> {
        if key.as_str() != "supplier" {
            return Err(IdentityError::ValidationError("连接来源关系未登记".into()));
        }
        Ok(RelationMapping {
            expected_ids: Bson::Array(vec!["$supplier_id".into()]),
            mapping: self.supplier.clone(),
        })
    }
}

/// 检查连接仓储的固定模型、完整对象粒度和原读取阶段。
/// # 参数
/// access 为统一查询凭证，executor 为原执行器。
/// # 返回
/// 连接对象条件可在当前读取阶段执行时成功。
/// # 错误
/// 资源、集合、粒度或读取边界不匹配时拒绝。
pub(super) fn ensure_connection(access: &CompiledAccess, executor: &mut dyn Executor) -> Result<()> {
    let model = access.model();
    if model.resource != "supplier_api_connection"
        || model.collection != SUPPLIER_API_CONNECTIONS
        || model.key.as_str() != "supplier_api_connection"
        || model.id_field != "id"
        || model.output != QueryOutput::Object
    {
        return Err(Error::ValidationError("连接查询授权模型不匹配".into()));
    }
    access.ensure_current(executor)?;
    Ok(())
}

/// 在完整授权过滤中读取指定连接，不能凭列表份额放行整对象。
/// # 参数
/// db、id 为本域连接身份；access、executor 为当前查询凭证和原执行器。
/// # 返回
/// 当前授权条件内的连接。
/// # 错误
/// 无权或不存在时统一返回不存在；模型、边界和数据库错误原样返回。
pub async fn authorized_connection(
    db: &Database,
    id: &str,
    access: &CompiledAccess,
    executor: &mut dyn Executor,
) -> Result<SupplierApiConnection> {
    ensure_connection(access, executor)?;

    let collection = db.collection::<SupplierApiConnection>(SUPPLIER_API_CONNECTIONS);
    let rows = aggregate::find(
        &collection,
        doc! {"id": id},
        FindOptions::builder().limit(1).build(),
        access.stages().to_vec(),
        executor,
    )
    .await?;

    ensure_connection(access, executor)?;
    rows.into_iter().next().ok_or_else(|| Error::NotFound("连接不存在".into()))
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_identity::entity::authorization::FactKind;
    use erp_permissions::supplier_api;

    use super::*;

    struct SupplierMapping(QueryModel);
    impl AccessMapping for SupplierMapping {
        fn model(&self) -> &QueryModel {
            &self.0
        }
        fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<Bson> {
            Ok("$id".into())
        }
        fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<QueryFragment> {
            Ok(QueryFragment::Match(doc! {"deleted_at": 0}))
        }
        fn predicate(
            &self,
            _: &AccessFact,
            _: &AccessContext,
            _: Option<&MappingFacts>,
        ) -> IdentityResult<QueryFragment> {
            Err(IdentityError::ValidationError("测试来源不执行字段条件".into()))
        }
        fn relation(
            &self,
            _: &RelationKey,
            _: &AccessContext,
            _: Option<&MappingFacts>,
        ) -> IdentityResult<RelationMapping> {
            Err(IdentityError::ValidationError("测试来源没有下级关系".into()))
        }
    }

    #[test]
    fn connection_mapping_preserves_the_complete_supplier_reference_and_shared_source_instance() {
        let supplier = Arc::new(SupplierMapping(QueryModel {
            key: QueryModelKey::parse("supplier").unwrap(),
            resource: "supplier".into(),
            collection: "suppliers".into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Owner, FactKind::Organization].into(),
        })) as Arc<dyn AccessMapping>;
        let mapping = connection_mapping(supplier.clone()).unwrap();
        let context = AccessContext {
            actor: AuditActor::new("buyer".into(), "采购".into(), AccountKind::Admin),
            permission: supplier_api::CONNECTION_DETAIL,
            resource: "supplier_api_connection".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        };

        let key = RelationKey::parse("supplier").unwrap();
        let relation = mapping.relation(&key, &context, None).unwrap();
        assert_eq!(relation.expected_ids, Bson::Array(vec!["$supplier_id".into()]));
        assert!(Arc::ptr_eq(&relation.mapping, &supplier));
        assert!(Arc::ptr_eq(&mapping.relations()[&key], &supplier));

        assert!(mapping.relation(&RelationKey::parse("other").unwrap(), &context, None).is_err());
        assert!(mapping.predicate(&AccessFact::Owned, &context, None).is_err());
        assert!(mapping.preparation().is_none());
    }
}
