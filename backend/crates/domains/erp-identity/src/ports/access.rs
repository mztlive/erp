use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use mongodb::bson::{Bson, Document};
use persistence_core::Executor;

use super::{QueryFragment, RelationMapping};
use crate::Result;
use crate::entity::authorization::{
    AccessContext, AccessFact, MappingFacts, ObjectFacts, QueryModel, RelationKey, ResourceTargets,
};

/// 业务字段与关系映射。所有组合规则由身份域编译器执行。
pub trait AccessMapping: Send + Sync {
    /// 返回固定的资源、集合、行身份和输出粒度，供启动校验绑定及查询编译使用。此方法只读取静态描述。
    fn model(&self) -> &QueryModel;

    /// 在启动时列出全部固定来源及映射，供授权服务校验规则依赖。没有来源时返回空映射；声明内容与请求和动态事实无关。
    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        BTreeMap::new()
    }

    /// 提供授权过滤前需要的固定聚合阶段，例如 `$lookup` 或 `$set`。阶段必须保持根记录的成员和数量，且独立于请求和授权结果。
    fn stages(&self) -> Vec<Document> {
        Vec::new()
    }

    /// 按本次授权时点计算静态条件的失效时间，例如下一自然日的起点，用于限制查询凭证期限。无时间约束时返回 `None`。
    fn valid_until(&self, _context: &AccessContext) -> Option<Instant> {
        None
    }

    /// 返回产出标量资源 ID 的表达式，供记录授权和来源关联使用。动态模型只使用本次准备的事实；无法表达资源身份时返回错误。
    fn key(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<Bson>;

    /// 条件编译依赖查询外的动态数据时，提供用于取证的准备端口。静态映射返回 `None`，编译器会跳过准备。
    fn preparation(&self) -> Option<&dyn MappingPreparation> {
        None
    }

    /// 为根查询和来源关联提供记录存在性与软删除条件。无软删除的专用模型可返回空条件；无法确认记录集合时返回错误，业务状态筛选由业务查询负责。
    fn records(&self, context: &AccessContext, prepared: Option<&MappingFacts>) -> Result<QueryFragment>;

    /// 将单条业务事实转换为字段过滤或关联存在性条件。使用本次上下文及准备事实，维度不受支持时返回错误；规则组合由授权编译器负责。
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        prepared: Option<&MappingFacts>,
    ) -> Result<QueryFragment>;

    /// 返回已登记关系的完整来源 ID 表达式及相应映射，供授权编译器核验来源。须保留无权或缺失的引用；关系未知或引用不完整时返回错误。
    fn relation(
        &self,
        relation: &RelationKey,
        context: &AccessContext,
        prepared: Option<&MappingFacts>,
    ) -> Result<RelationMapping>;
}

/// 只有查询外读取动态数据的映射接入本能力；准备结果属于单次调用。
#[async_trait]
pub trait MappingPreparation: Send + Sync {
    /// 查询编译需要外部动态条件时，沿原执行器读取使用的数据、可靠修订号及最早到期时点。读取错误原样传播；非事务查询交付需要可靠修订号。
    async fn prepare(&self, context: &AccessContext, executor: &mut dyn Executor) -> Result<MappingFacts>;
}

/// 拥有领域提供当前对象事实；不得读取其他领域的集合。
#[async_trait]
pub trait AccessFactsPort: Send + Sync {
    /// 沿调用方的执行器按本次身份和授权时点读取指定根对象和真实来源，供对象授权使用。只返回实际读取的对象，让授权服务识别缺失目标；读取错误原样传播。
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>>;
}
