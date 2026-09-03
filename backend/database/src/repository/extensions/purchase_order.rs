//! 域 D15 `purchase_order` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as PurchaseOrderExt>::PURCHASE_ORDERS` 等值。

use entities::ids::{SalesOrderId, SalesOrderRevisionId, SkuId};
use entities::purchase_order::{CreationBasisFacts, ProcurementCoverageFacts};
use entities::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeSubmission, PurchaseChangeSubmissionLine, PurchaseLineSalesAllocation,
    PurchaseOrder, PurchaseOrderRevision, PurchaseOrderRevisionLine, PurchaseOrderSubmission,
    PurchaseOrderSubmissionLine,
};
use mongodb::Database;

use super::super::purchase_order::{
    PurchaseOrderFilter, PurchaseOrderRepository, PurchaseOrderSubmissionFilter,
};
use crate::executor::Executor;
use crate::Repository;
use crate::Result;

/// 域 D15 仓储访问器。
#[allow(async_fn_in_trait)]
pub trait PurchaseOrderExt: Sized {
    /// `purchase_order` 集合名。
    const PURCHASE_ORDERS: &'static str = "purchase_orders";
    /// `purchase_order_submission` 集合名。
    const PURCHASE_ORDER_SUBMISSIONS: &'static str = "purchase_order_submissions";
    /// `purchase_order_submission_line` 集合名。
    const PURCHASE_ORDER_SUBMISSION_LINES: &'static str = "purchase_order_submission_lines";
    /// `purchase_order_revision` 集合名。
    const PURCHASE_ORDER_REVISIONS: &'static str = "purchase_order_revisions";
    /// `purchase_order_revision_line` 集合名。
    const PURCHASE_ORDER_REVISION_LINES: &'static str = "purchase_order_revision_lines";
    /// `purchase_line_sales_allocation` 集合名。
    const PURCHASE_LINE_SALES_ALLOCATIONS: &'static str = "purchase_line_sales_allocations";
    /// `purchase_change_order` 集合名。
    const PURCHASE_CHANGE_ORDERS: &'static str = "purchase_change_orders";
    /// `purchase_change_submission` 集合名。
    const PURCHASE_CHANGE_SUBMISSIONS: &'static str = "purchase_change_submissions";
    /// `purchase_change_submission_line` 集合名。
    const PURCHASE_CHANGE_SUBMISSION_LINES: &'static str = "purchase_change_submission_lines";

    /// 采购单列表筛选条件类型（定义见 `repository::purchase_order`）。
    type PurchaseOrderFilter;

    /// 采购提交列表筛选条件类型（定义见 `repository::purchase_order`）。
    type PurchaseOrderSubmissionFilter;

    /// 获取 `purchase_order` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseOrder>`。
    fn purchase_orders(&self) -> Repository<'_, PurchaseOrder>;

    /// 获取 `purchase_order_submission` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseOrderSubmission>`。
    fn purchase_order_submissions(&self) -> Repository<'_, PurchaseOrderSubmission>;

    /// 获取 `purchase_order_submission_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseOrderSubmissionLine>`。
    fn purchase_order_submission_lines(&self) -> Repository<'_, PurchaseOrderSubmissionLine>;

    /// 获取 `purchase_order_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseOrderRevision>`。
    fn purchase_order_revisions(&self) -> Repository<'_, PurchaseOrderRevision>;

    /// 获取 `purchase_order_revision_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseOrderRevisionLine>`。
    fn purchase_order_revision_lines(&self) -> Repository<'_, PurchaseOrderRevisionLine>;

    /// 获取 `purchase_line_sales_allocation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseLineSalesAllocation>`。
    fn purchase_line_sales_allocations(&self) -> Repository<'_, PurchaseLineSalesAllocation>;

    /// 获取 `purchase_change_order` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseChangeOrder>`。
    fn purchase_change_orders(&self) -> Repository<'_, PurchaseChangeOrder>;

    /// 获取 `purchase_change_submission` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseChangeSubmission>`。
    fn purchase_change_submissions(&self) -> Repository<'_, PurchaseChangeSubmission>;

    /// 获取 `purchase_change_submission_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseChangeSubmissionLine>`。
    fn purchase_change_submission_lines(&self) -> Repository<'_, PurchaseChangeSubmissionLine>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `PurchaseOrderRepository` 实例。
    fn purchase_order(&self) -> PurchaseOrderRepository<'_>;

    /// 批量加载采购覆盖计算所需的最小持久化事实。
    ///
    /// # 参数
    /// * `revision_id` - 销售单当前版本身份（Service 已校验指针存在）
    /// * `sales_order_id` - 来源销售单稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
    ///
    /// # 返回
    /// 返回包含当前销售目标行与全部当前覆盖来源的事实集合；当前版本文档缺失时
    /// 返回空事实集合，由 Entity 层校验。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误；不负责缺失校验，软删除与作废采购单
    /// 已通过 Repository 查询过滤。
    ///
    /// # 约束
    /// 查询次数与输入规模无关：销售版本行、商品子类型行、SKU、商品、覆盖采购单、
    /// 当前提交行、当前版本行、正式分配与现有库存预占各一次批量读取，不得出现
    /// 逐行 N+1。草稿类状态只沿当前提交指针、正式状态只沿当前版本指针读取。
    async fn load_procurement_coverage_facts(
        &self,
        revision_id: &SalesOrderRevisionId,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<ProcurementCoverageFacts>;

    /// 批量加载采购创建依据计算所需的最小持久化事实。
    ///
    /// # 参数
    /// * `sku_ids` - 任务责任范围内仍有剩余量的销售目标行 SKU 集合
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
    ///
    /// # 返回
    /// 返回 ACTIVE 供给、供给当前修订、实时可供投影、供应商角色、当前商务资料
    /// 修订与当前法定名称；SKU 集合为空时返回空事实集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误；修订、可供投影或供应商关联缺失
    /// 以缺键形式表达，由 Service 按合格性语义解释。
    ///
    /// # 约束
    /// 查询次数与输入规模无关：供给、修订、可供投影、供应商、商务资料修订与
    /// 法定名称各一次批量读取，不得出现逐行 N+1；供给只读取 ACTIVE 且未删除行，
    /// 并按 SKU、供应商与供给 ID 稳定排序。
    async fn load_creation_basis_facts(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<CreationBasisFacts>;

    /// 批量加载采购单列表页与关联事实。
    ///
    /// # 参数
    /// * `filter` - 采购单列表筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
    ///
    /// # 返回
    /// 返回当前页投影行、总数与同一 executor 下批量取回的关联事实；关联缺失
    /// 以缺键形式表达，由 Service 按完整性错误或约定回退解释。
    ///
    /// # 错误
    /// MongoDB 查询、计数或反序列化失败时返回错误；不负责缺失校验。
    ///
    /// # 约束
    /// 查询次数与页大小无关：列表分页、供应商名称、销售单、负责人、当前提交
    /// 与当前版本各一次批量读取，不得出现逐行 N+1。只沿当前指针读取表头。
    async fn load_purchase_order_list_page(
        &self,
        filter: &PurchaseOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<(
        crate::repository::PageResult<super::super::purchase_order::PurchaseOrderRow>,
        super::super::purchase_order::PurchaseOrderListFacts,
    )>;

    /// 批量加载采购单对象中心事实。
    ///
    /// # 参数
    /// * `order_id` - 采购单主键
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
    ///
    /// # 返回
    /// 返回单个采购单的全部当前指针事实；采购单不存在时 `order` 为空，由
    /// Service 映射为 `NotFound`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误；不负责缺失校验。
    ///
    /// # 约束
    /// 查询次数固定有界，不随行数增长；只沿当前提交或当前版本指针读取，不读
    /// 历史提交与历史版本；不读取审批运行时，不做事务或审批政策判断。
    async fn load_purchase_order_center_facts(
        &self,
        order_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<super::super::purchase_order::PurchaseOrderCenterFacts>;
}

impl PurchaseOrderExt for Database {
    type PurchaseOrderFilter = PurchaseOrderFilter;
    type PurchaseOrderSubmissionFilter = PurchaseOrderSubmissionFilter;

    fn purchase_orders(&self) -> Repository<'_, PurchaseOrder> {
        Repository::new(self, Self::PURCHASE_ORDERS)
    }

    fn purchase_order_submissions(&self) -> Repository<'_, PurchaseOrderSubmission> {
        Repository::new(self, Self::PURCHASE_ORDER_SUBMISSIONS)
    }

    fn purchase_order_submission_lines(&self) -> Repository<'_, PurchaseOrderSubmissionLine> {
        Repository::new(self, Self::PURCHASE_ORDER_SUBMISSION_LINES)
    }

    fn purchase_order_revisions(&self) -> Repository<'_, PurchaseOrderRevision> {
        Repository::new(self, Self::PURCHASE_ORDER_REVISIONS)
    }

    fn purchase_order_revision_lines(&self) -> Repository<'_, PurchaseOrderRevisionLine> {
        Repository::new(self, Self::PURCHASE_ORDER_REVISION_LINES)
    }

    fn purchase_line_sales_allocations(&self) -> Repository<'_, PurchaseLineSalesAllocation> {
        Repository::new(self, Self::PURCHASE_LINE_SALES_ALLOCATIONS)
    }

    fn purchase_change_orders(&self) -> Repository<'_, PurchaseChangeOrder> {
        Repository::new(self, Self::PURCHASE_CHANGE_ORDERS)
    }

    fn purchase_change_submissions(&self) -> Repository<'_, PurchaseChangeSubmission> {
        Repository::new(self, Self::PURCHASE_CHANGE_SUBMISSIONS)
    }

    fn purchase_change_submission_lines(&self) -> Repository<'_, PurchaseChangeSubmissionLine> {
        Repository::new(self, Self::PURCHASE_CHANGE_SUBMISSION_LINES)
    }

    fn purchase_order(&self) -> PurchaseOrderRepository<'_> {
        PurchaseOrderRepository::new(self)
    }

    /// 批量加载采购覆盖计算所需的最小持久化事实。
    ///
    /// # 参数
    /// * `revision_id` - 销售单当前版本身份（Service 已校验指针存在）
    /// * `sales_order_id` - 来源销售单稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
    ///
    /// # 返回
    /// 返回包含当前销售目标行与全部当前覆盖来源的事实集合；当前版本文档缺失时
    /// 返回空事实集合，由 Entity 层校验。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误；不负责缺失校验，软删除与作废采购单
    /// 已通过 Repository 查询过滤。
    ///
    /// # 约束
    /// 查询次数与输入规模无关：销售版本行、商品子类型行、SKU、商品、覆盖采购单、
    /// 当前提交行、当前版本行、正式分配与现有库存预占各一次批量读取，不得出现
    /// 逐行 N+1。草稿类状态只沿当前提交指针、正式状态只沿当前版本指针读取。
    async fn load_procurement_coverage_facts(
        &self,
        revision_id: &SalesOrderRevisionId,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<ProcurementCoverageFacts> {
        super::super::purchase_order::load_procurement_coverage_facts(
            self,
            revision_id,
            sales_order_id,
            executor,
        )
        .await
    }

    /// 批量加载采购创建依据计算所需的最小持久化事实。
    ///
    /// # 参数
    /// * `sku_ids` - 任务责任范围内仍有剩余量的销售目标行 SKU 集合
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
    ///
    /// # 返回
    /// 返回 ACTIVE 供给、供给当前修订、实时可供投影、供应商角色、当前商务资料
    /// 修订与当前法定名称；SKU 集合为空时返回空事实集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误；修订、可供投影或供应商关联缺失
    /// 以缺键形式表达，由 Service 按合格性语义解释。
    ///
    /// # 约束
    /// 查询次数与输入规模无关：供给、修订、可供投影、供应商、商务资料修订与
    /// 法定名称各一次批量读取，不得出现逐行 N+1。
    async fn load_creation_basis_facts(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<CreationBasisFacts> {
        super::super::purchase_order::load_creation_basis_facts(self, sku_ids, executor).await
    }

    /// 批量加载采购单列表页与关联事实。
    ///
    /// # 参数
    /// * `filter` - 采购单列表筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界
    ///
    /// # 返回
    /// 返回当前页投影行、总数与关联事实；关联缺失以缺键表达。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 查询次数与页大小无关，不得出现逐行 N+1；只沿当前指针读取表头。
    async fn load_purchase_order_list_page(
        &self,
        filter: &PurchaseOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<(
        crate::repository::PageResult<super::super::purchase_order::PurchaseOrderRow>,
        super::super::purchase_order::PurchaseOrderListFacts,
    )> {
        super::super::purchase_order::load_purchase_order_list_page(self, filter, executor).await
    }

    /// 批量加载采购单对象中心事实。
    ///
    /// # 参数
    /// * `order_id` - 采购单主键
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界
    ///
    /// # 返回
    /// 返回单个采购单的全部当前指针事实；不存在时 `order` 为空。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 查询次数固定有界；只沿当前指针读取；不读取审批运行时。
    async fn load_purchase_order_center_facts(
        &self,
        order_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<super::super::purchase_order::PurchaseOrderCenterFacts> {
        super::super::purchase_order::load_purchase_order_center_facts(self, order_id, executor).await
    }
}
