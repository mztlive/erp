use async_trait::async_trait;
use mongodb::bson::Bson;
use persistence_core::Executor;

use super::{QueryFragment, RelationMapping};
use crate::Result;
use crate::entity::authorization::{
    AccessContext, AccessFact, MappingFacts, ObjectFacts, ProposedFacts, QueryModel, RelationKey,
    ResourceTargets,
};

/// 业务字段与关系映射。所有组合规则由身份域编译器执行。
#[async_trait]
pub trait AccessMapping: Send + Sync {
    /// 取得固定查询模型。
    /// # 参数
    /// 无。
    /// # 返回
    /// 根资源、集合和记录身份。
    /// # 错误
    /// 不执行 I/O。
    fn model(&self) -> &QueryModel;

    /// 返回当前记录的资源主键表达式。
    /// # 参数
    /// context 为授权上下文，prepared 为同次准备事实。
    /// # 返回
    /// MongoDB 标量表达式，支持字段、类型转换及计算主键。
    /// # 错误
    /// 无法表达该资源身份时返回接入错误。
    fn key(&self, context: &AccessContext, prepared: &MappingFacts) -> Result<Bson>;

    /// 准备当前阶段的业务事实和版本。
    /// # 参数
    /// context 为授权身份和时点；executor 为原执行器。
    /// # 返回
    /// 有界事实、修订号及有效期。
    /// # 错误
    /// 事实不可取得时失败，不返回空集合代替错误。
    async fn prepare(&self, context: &AccessContext, executor: &mut dyn Executor) -> Result<MappingFacts>;

    /// 限定模型中实际存在且未删除的记录，根查询与来源关联均应用。
    /// # 参数
    /// context 为本次身份与时点；prepared 为同次准备事实。
    /// # 返回
    /// 行存在性约束；没有软删除的专用模型可返回空字段条件。
    /// # 错误
    /// 无法确定有效记录集合时拒绝，禁止用对象状态替代权限检查。
    fn records(&self, context: &AccessContext, prepared: &MappingFacts) -> Result<QueryFragment>;

    /// 翻译一个业务事实，不能改变上层与或关系。
    /// # 参数
    /// fact 为待翻译语义；context 和 prepared 为同次准备事实。
    /// # 返回
    /// 根条件或存在性查询。
    /// # 错误
    /// 未支持的事实必须拒绝。
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        prepared: &MappingFacts,
    ) -> Result<QueryFragment>;

    /// 提供完整来源关系，不筛除无权或缺失的引用。
    /// # 参数
    /// relation 为登记关系；context 和 prepared 为同次准备事实。
    /// # 返回
    /// 实际引用 ID 表达式和来源映射。
    /// # 错误
    /// 未登记关系或无法证明完整性时拒绝。
    fn relation(
        &self,
        relation: &RelationKey,
        context: &AccessContext,
        prepared: &MappingFacts,
    ) -> Result<RelationMapping>;
}

/// 拥有领域提供当前对象事实；不得读取其他领域的集合。
#[async_trait]
pub trait AccessFactsPort: Send + Sync {
    /// 读取本次根目标和完整来源关系。
    /// # 参数
    /// context 为身份及解释时点；ids 为完整根集合；executor 为原执行器。
    /// # 返回
    /// 实际读取的对象，缺失对象不能用虚构空对象替代。
    /// # 错误
    /// 持久化及事实读取错误原样传播。
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>>;

    /// 核实拟写入责任和实际来源。
    /// # 参数
    /// context 为身份及解释时点；facts 为业务校验后的输入；executor 为原执行器。
    /// # 返回
    /// 拟创建对象的有效责任事实。
    /// # 错误
    /// 责任、关联或版本不成立时拒绝。
    async fn proposed(
        &self,
        context: &AccessContext,
        facts: &ProposedFacts,
        executor: &mut dyn Executor,
    ) -> Result<ObjectFacts>;
}
