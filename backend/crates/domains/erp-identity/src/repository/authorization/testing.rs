//! 公共授权 API 的无数据库查询夹具；不执行真实 MongoDB 请求。
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_permissions::sales_order;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::Executor;
use serde_json::{Value, to_value};

use super::fixtures::execute;
use crate::entity::authorization::{
    AccessContext, AccessFact, AccessRule, FactKind, MappingFacts, QueryModel, QueryModelKey, QueryOutput,
    RelationKey,
};
use crate::ports::{AccessMapping, MappingPreparation, QueryFragment, RelationMapping};
use crate::service::authorization::rules;
use crate::{CompiledAccess, Error, Permission, Result};

/// 公共授权 API 的内存映射替身，可模拟动态修订、到期和违反绑定约束。
pub(crate) struct Mapping {
    /// 固定模型、集合及可用事实维度的测试描述。
    pub model: QueryModel,
    /// 按关系键登记的来源映射实例。
    pub relations: BTreeMap<RelationKey, Arc<dyn AccessMapping>>,
    /// 是否启用动态准备；关闭时模拟静态映射。
    pub dynamic: bool,
    /// 动态准备是否提供可用于非事务复验的可靠版本。
    pub reliable: bool,
    /// 用于模拟底层动态事实变化的修订号。
    pub revision: AtomicU64,
    /// 动态准备调用次数，供缓存及交付复验断言使用。
    pub prepares: AtomicU64,
    /// 动态事实可选的到期边界，供测试推进或修改时限。
    pub expires: Mutex<Option<Instant>>,
    /// 模拟准备过程错误地推进执行器读取阶段。
    pub change_stage: AtomicBool,
    /// 模拟运行时返回与启动登记不同的来源实例。
    pub replace_source: AtomicBool,
}
impl AccessMapping for Mapping {
    // 暴露测试登记的固定模型描述。
    fn model(&self) -> &QueryModel {
        &self.model
    }
    // 仅为启用动态行为的替身提供准备接口。
    fn preparation(&self) -> Option<&dyn MappingPreparation> {
        if self.dynamic { Some(self) } else { None }
    }
    // 返回原登记的来源实例，供启动绑定冻结使用。
    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        self.relations.clone()
    }
    // 使用样本行的 `id` 字段作为授权资源键。
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> Result<Bson> {
        Ok("$id".into())
    }
    // 在所有授权分支下都排除已删除样本。
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at":0}))
    }
    // 将测试支持的所有者、任务关系和仓库事实转换成样本字段条件，未支持的维度返回错误。
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<QueryFragment> {
        Ok(QueryFragment::Match(match fact {
            AccessFact::Owned => doc! {"owner":context.actor.id()},
            AccessFact::HistoryParticipant => doc! {"history":context.actor.id()},
            AccessFact::AssignedTo => doc! {"assignee":context.actor.id()},
            AccessFact::Participant => doc! {"participant":context.actor.id()},
            AccessFact::Completed(completed) => doc! {"completed":completed},
            AccessFact::WarehouseIn(ids) => {
                doc! {"warehouse_id":{"$in":ids.iter().cloned().collect::<Vec<_>>() }}
            },
            _ => return Err(Error::ValidationError("fixture unsupported predicate".into())),
        }))
    }
    // 解析指定关系的样本引用；可替换来源实例以验证冻结映射拒绝漂移。
    fn relation(
        &self,
        key: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<RelationMapping> {
        if self.replace_source.load(Ordering::SeqCst) {
            return Ok(RelationMapping {
                expected_ids: format!("${}", key.as_str()).into(),
                mapping: mapping(&sales_order::DETAIL),
            });
        }
        Ok(RelationMapping {
            expected_ids: format!("${}", key.as_str()).into(),
            mapping: self
                .relations
                .get(key)
                .ok_or_else(|| Error::ValidationError("fixture missing relation".into()))?
                .clone(),
        })
    }
}

#[async_trait]
impl MappingPreparation for Mapping {
    // 记录调用并返回可控事实、修订及期限；需要测试边界失败时推进读取阶段。
    async fn prepare(&self, _: &AccessContext, executor: &mut dyn Executor) -> Result<MappingFacts> {
        self.prepares.fetch_add(1, Ordering::SeqCst);
        if self.change_stage.load(Ordering::SeqCst) {
            executor.next_read();
        }
        let revision = self.revision.load(Ordering::SeqCst);
        Ok(MappingFacts {
            data: serde_json::json!({"revision":revision}),
            version: self.reliable.then(|| revision.to_string()),
            valid_until: *self.expires.lock().unwrap(),
        })
    }
}

/// 从生产权限登记的完整对象规则构造测试映射和所需来源。
pub(crate) fn mapping(permission: &Permission) -> Arc<Mapping> {
    let rule = rules::binding_condition(permission, &QueryOutput::Object).unwrap();
    model(permission.resource(), &rule)
}
// 为资源生成内存测试模型，并递归登记规则所需的来源映射。
fn model(resource: &str, rule: &AccessRule) -> Arc<Mapping> {
    let mut relations = BTreeMap::new();
    collect(rule, &mut relations);
    Arc::new(Mapping {
        model: QueryModel {
            key: QueryModelKey::parse(resource).unwrap(),
            resource: resource.into(),
            collection: format!("{resource}_rows"),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [
                FactKind::Owner,
                FactKind::Organization,
                FactKind::Collaborators,
                FactKind::HistoryParticipants,
                FactKind::SettlementParty,
                FactKind::Warehouse,
                FactKind::Assignee,
                FactKind::Participants,
                FactKind::Completed,
            ]
            .into(),
        },
        relations,
        dynamic: false,
        reliable: true,
        revision: AtomicU64::new(1),
        prepares: AtomicU64::new(0),
        expires: Mutex::new(None),
        change_stage: AtomicBool::new(false),
        replace_source: AtomicBool::new(false),
    })
}
// 遍历规则树抽取来源关系，为样本按关系键建立对应子模型。
fn collect(rule: &AccessRule, relations: &mut BTreeMap<RelationKey, Arc<dyn AccessMapping>>) {
    match rule {
        AccessRule::Related { relation, resource, rule, .. } => {
            relations.insert(relation.clone(), model(resource, rule));
        },
        AccessRule::Required { rule, .. } | AccessRule::Independent { rule, .. } => collect(rule, relations),
        AccessRule::All(items) | AccessRule::Any(items) => {
            for rule in items {
                collect(rule, relations);
            }
        },
        _ => {},
    }
}

/// 运行询价授权过滤，返回本人、他人及已删除样本中实际可见的 ID。
pub(crate) fn inquiry_members(access: &CompiledAccess) -> Vec<String> {
    let rows = execute(
        vec![
            doc! {"id":"one", "owner":"user", "deleted_at":0},
            doc! {"id":"other", "owner":"other", "deleted_at":0},
            doc! {"id":"deleted", "owner":"user", "deleted_at":1},
        ],
        access.stages(),
        &BTreeMap::new(),
        &doc! {},
    );
    rows.into_iter().map(|row| row.get_str("id").unwrap().to_owned()).collect()
}

/// 对销售、采购及缺失或无权来源的样本运行生产管理列表计划，返回可见记录 ID。
pub(crate) fn management_members(access: &CompiledAccess) -> Vec<String> {
    let rows = vec![
        doc! {"id":"sales", "deleted_at":0, "management_sales_order_sales_order":["sale"]},
        doc! {"id":"outside", "deleted_at":0, "management_sales_order_sales_order":["outside"]},
        doc! {"id":"missing", "deleted_at":0, "management_sales_order_sales_order":["missing"]},
        doc! {"id":"multiple", "deleted_at":0, "management_sales_order_sales_order":["sale", "other"]},
        doc! {"id":"purchase", "deleted_at":0, "management_purchase_order_purchase_order":["purchase"]},
    ];
    let sources = [
        (
            "sales_order_rows".into(),
            vec![
                doc! {"id":"sale", "owner":"user", "deleted_at":0},
                doc! {"id":"other", "owner":"user", "deleted_at":0},
                doc! {"id":"outside", "owner":"other", "deleted_at":0},
            ],
        ),
        ("purchase_order_rows".into(), vec![doc! {"id":"purchase", "owner":"user", "deleted_at":0}]),
    ]
    .into();
    execute(rows, access.stages(), &sources, &doc! {})
        .into_iter()
        .map(|row| row.get_str("id").unwrap().to_owned())
        .collect()
}

/// 对含整账金额和私有内容的收款样本执行计划，返回序列化投影供断言。
pub(crate) fn receipt_projection(access: &CompiledAccess) -> Value {
    let collections =
        [("sales_order_rows".into(), vec![doc! {"id": "sale", "owner": "user", "deleted_at": 0}])].into();
    let rows = execute(
        vec![doc! {
            "id": "receipt", "deleted_at": 0,
            "sales_source": ["sale"], "purchase_source": [], "settlement_source": [],
            "total_amount": 1000, "amount": 1000, "payload": {"customer_name": "private"}
        }],
        access.stages(),
        &collections,
        &doc! {},
    );
    to_value(rows).unwrap()
}

/// 对指定指派人、完成状态、参与人及采购来源样本执行任务过滤，返回可见任务 ID。
pub(crate) fn task_members(stages: &[Document], tasks: &[(&str, &str, bool, &str)]) -> Vec<String> {
    let rows = tasks
        .iter()
        .map(|(id, assignee, completed, participant)| {
            doc! {
                "id": *id, "assignee": *assignee, "completed": *completed,
                "participant": *participant, "deleted_at": 0, "purchase_source": ["purchase"]
            }
        })
        .collect();
    let sources =
        [("purchase_order_rows".into(), vec![doc! {"id":"purchase", "owner":"user", "deleted_at":0}])].into();
    let rows = execute(rows, stages, &sources, &doc! {});
    rows.into_iter().map(|row| row.get_str("id").unwrap().to_owned()).collect()
}
