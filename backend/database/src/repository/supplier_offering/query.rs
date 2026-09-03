//! 供给聚合通用查询：稳定身份按 ID 读取与商业条款修订查询。

use entities::ids::{SupplierOfferingId, SupplierOfferingRevisionId};
use entities::supplier_offering::{SupplierOffering, SupplierOfferingRevision};
use mongodb::bson::doc;

use super::super::Repository;
use crate::executor::Executor;
use crate::Result;

impl<'a> Repository<'a, SupplierOffering> {
    /// 按稳定 ID 读取未删除的供给身份。
    ///
    /// 发布与安全暂停流程通过供给稳定 ID 引用供给身份，本方法提供该跨域只读
    /// 事实的数据访问能力，不承载业务规则判断。
    ///
    /// # 参数
    /// * `id` - 供给稳定 ID
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界
    ///
    /// # 返回
    /// 返回未删除供给；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 只查询本聚合 `supplier_offerings` 集合，不触碰其他集合。
    pub async fn find_publication_supplier_offering(
        &self,
        id: &SupplierOfferingId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOffering>> {
        self.find_by_id(id.as_ref(), executor).await
    }
}

impl<'a> Repository<'a, SupplierOfferingRevision> {
    /// 按稳定 ID 读取未删除的供给商业条款修订。
    ///
    /// 发布修订通过供给修订稳定 ID 引用不可变商业条款，本方法提供该跨域只读
    /// 事实的数据访问能力，不承载业务规则判断。
    ///
    /// # 参数
    /// * `id` - 供给修订 ID
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界
    ///
    /// # 返回
    /// 返回未删除供给修订；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 只查询本聚合 `supplier_offering_revisions` 集合，不触碰其他集合。
    pub async fn find_publication_offering_revision(
        &self,
        id: &SupplierOfferingRevisionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOfferingRevision>> {
        self.find_by_id(id.as_ref(), executor).await
    }

    /// 列出稳定供给的全部未删除不可变商业条款修订。
    ///
    /// 安全暂停流程需要加载来源供给的全部修订以解析在售发布影响集，本方法
    /// 提供该集合查询能力，不承载业务规则判断。
    ///
    /// # 参数
    /// * `offering_id` - 供应商供给稳定 ID
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界
    ///
    /// # 返回
    /// 返回全部匹配且未删除的供给修订。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 只查询本聚合 `supplier_offering_revisions` 集合，按 `supplier_offering_id`
    /// 精确过滤，不触碰其他集合。
    pub async fn list_publication_offering_revisions(
        &self,
        offering_id: &SupplierOfferingId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOfferingRevision>> {
        self.find_many(doc! { "supplier_offering_id": offering_id.to_string() }, executor)
            .await
    }
}
