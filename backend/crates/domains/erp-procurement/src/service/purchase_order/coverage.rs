//! 销售当前版本的采购数量覆盖编排。
//!
//! 草稿、旧待财务与审批中采购只读取 `current_submission_id`；生效、部分执行与
//! 已完成采购只读取 `current_revision_id` 及其销售分配。历史提交、历史采购版本
//! 与作废采购单均不进入覆盖量。
//!
//! 事实批量加载由 `ProcurementCoveragePort::load_procurement_coverage_facts`
//! 承担；覆盖聚合、累计、当前行关联、超覆盖拒绝、剩余量与进度计算由
//! `crate::entity::purchase_order::coverage::build_procurement_coverage` 领域构造函数
//! 承担；本模块只负责当前指针解析、仓储调用与领域错误映射。

use erp_core::ids::{SalesOrderId, SalesOrderRevisionId};
use persistence_core::Executor;

use crate::entity::purchase_order::{SalesProcurementCoverage, build_procurement_coverage};
use crate::ports::coverage::ProcurementCoveragePort;
use crate::{Error, Result};

/// 加载销售单当前版本及采购覆盖数量。
///
/// # 参数
/// * `port` - 由组合层装配的唯一覆盖事实提供方
/// * `current_revision_id` - 销售已加载的当前修订指针，缺失保留原首错
/// * `sales_order_id` - 销售稳定身份
/// * `executor` - 数据访问执行器；创建命令必须传入事务会话
///
/// # 返回
/// 返回当前销售版本商品/服务目标行、逐行覆盖与总汇总。
///
/// # 错误
/// 当前版本缺失、当前采购指针缺失、正式分配未绑定销售当前版本行、覆盖超过目标
/// 或仓储查询失败时返回错误。
///
/// # 关键业务约束
/// 只沿销售与采购的当前指针读取，稳定关联键为 `sales_order_line_id`；事务内
/// 调用必须复用调用方 executor，保证与同事务写入的 read-your-writes。
pub async fn load_sales_procurement_coverage(
    port: &dyn ProcurementCoveragePort,
    current_revision_id: Option<&str>,
    sales_order_id: &SalesOrderId,
    executor: &mut dyn Executor,
) -> Result<SalesProcurementCoverage> {
    let revision_id = current_revision_id
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本，无法计算采购剩余量".to_string()))?;
    let facts = port
        .load_procurement_coverage_facts(
            &SalesOrderRevisionId::new(revision_id.to_string()),
            sales_order_id,
            executor,
        )
        .await?;
    build_procurement_coverage(facts).map_err(Error::Logic)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;
    use crate::entity::purchase_order::ProcurementCoverageFacts;
    struct RecordingExecutor {
        marker: u64,
    }
    impl Executor for RecordingExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            self.marker += 1;
            None
        }
    }
    struct FactsPort {
        expected_executor: usize,
        calls: Mutex<Vec<(String, String)>>,
    }
    #[async_trait]
    impl ProcurementCoveragePort for FactsPort {
        async fn load_procurement_coverage_facts(
            &self,
            revision: &SalesOrderRevisionId,
            order: &SalesOrderId,
            executor: &mut dyn Executor,
        ) -> crate::Result<ProcurementCoverageFacts> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.expected_executor);
            self.calls.lock().unwrap().push((revision.to_string(), order.to_string()));
            Err(Error::Internal("coverage provider failed".to_string()))
        }
    }
    /// 当前版本缺失不得触发外域读取，保留原首错。
    #[tokio::test]
    async fn missing_current_revision_stops_before_facts_read() {
        let mut executor = RecordingExecutor { marker: 91 };
        let port = FactsPort {
            expected_executor: (&mut executor as *mut RecordingExecutor) as usize,
            calls: Mutex::new(Vec::new()),
        };
        let result =
            load_sales_procurement_coverage(&port, None, &SalesOrderId::new("so-1"), &mut executor).await;
        assert!(
            matches!(result,Err(Error::BusinessLogicError(message)) if message=="销售单缺少当前版本，无法计算采购剩余量")
        );
        assert!(port.calls.into_inner().unwrap().is_empty());
        assert_eq!(executor.marker, 91);
    }
    /// 覆盖事实必须使用同一非零大小执行器，提供方失败阻止规则构造。
    #[tokio::test]
    async fn coverage_port_preserves_executor_and_provider_failure() {
        let mut executor = RecordingExecutor { marker: 91 };
        let port = FactsPort {
            expected_executor: (&mut executor as *mut RecordingExecutor) as usize,
            calls: Mutex::new(Vec::new()),
        };
        let result = load_sales_procurement_coverage(
            &port,
            Some("revision-1"),
            &SalesOrderId::new("so-1"),
            &mut executor,
        )
        .await;
        assert!(matches!(result,Err(Error::Internal(message)) if message=="coverage provider failed"));
        assert_eq!(port.calls.into_inner().unwrap(), vec![("revision-1".to_string(), "so-1".to_string())]);
        assert_eq!(executor.marker, 91);
    }
}
