//! 各业务编排与应用装配错误到 HTTP 合同的穷尽转换。

use super::{Error, historical_http_duplicate, wiring_errors};

impl From<erp_supplier_orders::Error> for Error {
    /// 保留编排边界的原错误载荷和 HTTP 分类。
    fn from(err: erp_supplier_orders::Error) -> Self {
        boundary_error!(err, erp_supplier_orders;
            error @ erp_supplier_orders::Error::Rbac(_) => Self::Internal(error.to_string()),
            erp_supplier_orders::Error::Coded(code) => Self::Coded(code),
            erp_supplier_orders::Error::RepositoryError(error) if historical_http_duplicate(&error) => Self::from(error),
        )
    }
}

impl From<erp_file_assets::Error> for Error {
    /// 将实际组合边界错误转换为稳定 HTTP 响应。
    fn from(err: erp_file_assets::Error) -> Self {
        boundary_error!(err, erp_file_assets;
            erp_file_assets::Error::RepositoryError(error) if historical_http_duplicate(&error) => Self::from(error),
        )
    }
}

impl From<erp_approval_runtime::Error> for Error {
    /// 将实际组合边界错误转换为稳定 HTTP 响应。
    fn from(err: erp_approval_runtime::Error) -> Self {
        boundary_error!(err, erp_approval_runtime;
            error @ erp_approval_runtime::Error::Rbac(_) => Self::Internal(error.to_string()),
            erp_approval_runtime::Error::Coded(code) => Self::Coded(code),
            erp_approval_runtime::Error::RepositoryError(error) if historical_http_duplicate(&error) => Self::from(error),
        )
    }
}

impl From<erp_master_data::Error> for Error {
    /// 保留编排边界的原错误载荷和 HTTP 分类。
    fn from(err: erp_master_data::Error) -> Self {
        boundary_error!(err, erp_master_data;
            error @ erp_master_data::Error::Rbac(_) => Self::Internal(error.to_string()),
            erp_master_data::Error::RepositoryError(error) if historical_http_duplicate(&error) => Self::from(error),
        )
    }
}

impl From<erp_order_to_cash::Error> for Error {
    /// 保留编排边界的原错误载荷和 HTTP 分类。
    fn from(err: erp_order_to_cash::Error) -> Self {
        boundary_error!(err, erp_order_to_cash;
            erp_order_to_cash::Error::CommittedResultUnavailable(message) => Self::CommittedResultUnavailable(message),
            error @ erp_order_to_cash::Error::Rbac(_) => Self::Internal(error.to_string()),
            erp_order_to_cash::Error::Coded(code) => Self::Coded(code),
            erp_order_to_cash::Error::RepositoryError(error) if historical_http_duplicate(&error) => Self::from(error),
        )
    }
}

impl From<erp_fulfillment_execution::Error> for Error {
    /// 保留编排边界的原错误载荷和 HTTP 分类。
    fn from(err: erp_fulfillment_execution::Error) -> Self {
        boundary_error!(err, erp_fulfillment_execution;
            error @ erp_fulfillment_execution::Error::Rbac(_) => Self::Internal(error.to_string()),
            erp_fulfillment_execution::Error::Coded(code) => Self::Coded(code),
            erp_fulfillment_execution::Error::RepositoryError(error) if historical_http_duplicate(&error) => Self::from(error),
        )
    }
}

impl From<erp_demo_data::Error> for Error {
    /// 保留编排边界的原错误载荷和 HTTP 分类。
    fn from(err: erp_demo_data::Error) -> Self {
        boundary_error!(err, erp_demo_data;
            error @ erp_demo_data::Error::Rbac(_) => Self::Internal(error.to_string()),
            erp_demo_data::Error::Coded(code) => Self::Coded(code),
            erp_demo_data::Error::RepositoryError(error) if historical_http_duplicate(&error) => Self::from(error),
        )
    }
}

impl From<erp_procure_to_pay::Error> for Error {
    /// 保留编排边界的原错误载荷和 HTTP 分类。
    fn from(err: erp_procure_to_pay::Error) -> Self {
        boundary_error!(err, erp_procure_to_pay;
            error @ erp_procure_to_pay::Error::Rbac(_) => Self::Internal(error.to_string()),
            erp_procure_to_pay::Error::Coded(code) => Self::Coded(code),
            erp_procure_to_pay::Error::RepositoryError(error) if historical_http_duplicate(&error) => Self::from(error),
        )
    }
}

impl From<erp_supply_settlement::Error> for Error {
    /// 保留编排边界的原错误载荷和 HTTP 分类。
    fn from(err: erp_supply_settlement::Error) -> Self {
        boundary_error!(err, erp_supply_settlement;
            error @ erp_supply_settlement::Error::Rbac(_) => Self::Internal(error.to_string()),
            erp_supply_settlement::Error::Coded(code) => Self::Coded(code),
            erp_supply_settlement::Error::RepositoryError(error) if historical_http_duplicate(&error) => Self::from(error),
        )
    }
}

impl From<erp_finance_posting::Error> for Error {
    /// 保留编排边界的原错误载荷和 HTTP 分类。
    fn from(err: erp_finance_posting::Error) -> Self {
        boundary_error!(err, erp_finance_posting;
            erp_finance_posting::Error::CommittedResultUnavailable(message) => Self::CommittedResultUnavailable(message),
            error @ erp_finance_posting::Error::Rbac(_) => Self::Internal(error.to_string()),
            erp_finance_posting::Error::Coded(code) => Self::Coded(code),
            erp_finance_posting::Error::RepositoryError(error) if historical_http_duplicate(&error) => Self::from(error),
        )
    }
}

impl From<erp_reverse_flow::Error> for Error {
    /// 保留编排边界的原错误载荷和 HTTP 分类。
    fn from(err: erp_reverse_flow::Error) -> Self {
        boundary_error!(err, erp_reverse_flow;
            error @ erp_reverse_flow::Error::Rbac(_) => Self::Internal(error.to_string()),
            erp_reverse_flow::Error::Coded(code) => Self::Coded(code),
            erp_reverse_flow::Error::RepositoryError(error) if historical_http_duplicate(&error) => Self::from(error),
        )
    }
}

impl From<erp_data_import::Error> for Error {
    /// 保留编排边界的原错误载荷和 HTTP 分类。
    fn from(err: erp_data_import::Error) -> Self {
        boundary_error!(err, erp_data_import;
            erp_data_import::Error::CommittedResultUnavailable(message) => Self::CommittedResultUnavailable(message),
            error @ erp_data_import::Error::Rbac(_) => Self::Internal(error.to_string()),
            erp_data_import::Error::Coded(code) => Self::Coded(code),
            erp_data_import::Error::RepositoryError(error) if historical_http_duplicate(&error) => Self::from(error),
        )
    }
}

impl From<erp_supplier_catalog::Error> for Error {
    /// 保留编排边界的原错误载荷和 HTTP 分类。
    fn from(err: erp_supplier_catalog::Error) -> Self {
        boundary_error!(err, erp_supplier_catalog;
            error @ erp_supplier_catalog::Error::Rbac(_) => Self::Internal(error.to_string()),
            erp_supplier_catalog::Error::Coded(code) => Self::Coded(code),
            erp_supplier_catalog::Error::RepositoryError(error) if historical_http_duplicate(&error) => Self::from(error),
        )
    }
}

impl From<wiring_errors::Error> for Error {
    /// 将实际组合边界错误转换为稳定 HTTP 响应。
    fn from(err: wiring_errors::Error) -> Self {
        boundary_error!(err, wiring_errors;
            wiring_errors::Error::CommittedResultUnavailable(message) => Self::CommittedResultUnavailable(message),
            error @ wiring_errors::Error::Rbac(_) => Self::Internal(error.to_string()),
            wiring_errors::Error::Coded(code) => Self::Coded(code),
            wiring_errors::Error::RepositoryError(error) if historical_http_duplicate(&error) => Self::from(error),
        )
    }
}

impl From<erp_read_models::Error> for Error {
    /// 将实际组合边界错误转换为稳定 HTTP 响应。
    fn from(err: erp_read_models::Error) -> Self {
        boundary_error!(err, erp_read_models;
            error @ erp_read_models::Error::Rbac(_) => Self::Internal(error.to_string()),
            erp_read_models::Error::Coded(code) => Self::Coded(code),
            erp_read_models::Error::RepositoryError(error) if historical_http_duplicate(&error) => Self::from(error),
        )
    }
}
