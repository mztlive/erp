//! 供应执行消费的集成错误分类转换。
use erp_integration::entity::integration_ops::ErrorClass;
use erp_supply::entity::failure::SupplierFailureClass;

pub(super) fn integration_class(class: SupplierFailureClass) -> ErrorClass {
    match class {
        SupplierFailureClass::CapabilityGap => ErrorClass::CapabilityGap,
        SupplierFailureClass::MappingError => ErrorClass::MappingError,
        SupplierFailureClass::BusinessRejected => ErrorClass::BusinessRejected,
        SupplierFailureClass::TransientFailure => ErrorClass::TransientFailure,
        SupplierFailureClass::ResultUnknown => ErrorClass::ResultUnknown,
        SupplierFailureClass::AuthSignature => ErrorClass::AuthSignature,
        SupplierFailureClass::RateLimited => ErrorClass::RateLimited,
        SupplierFailureClass::OutOfOrder => ErrorClass::OutOfOrder,
    }
}
