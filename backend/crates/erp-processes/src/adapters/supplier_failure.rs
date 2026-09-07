//! 供应商与集成之间唯一的分类映射；规则仍由各自拥有领域执行。
use erp_integration::entity::integration_ops::ErrorClass;
use erp_supply::entity::failure::SupplierFailureClass;
/// 将供应商失败事实交集成领域执行其权威政策。
pub fn integration_class(class: SupplierFailureClass) -> ErrorClass {
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
/// 将已分类集成结果转换为供应商外部调用事实。
pub fn supplier_class(class: ErrorClass) -> SupplierFailureClass {
    match class {
        ErrorClass::CapabilityGap => SupplierFailureClass::CapabilityGap,
        ErrorClass::MappingError => SupplierFailureClass::MappingError,
        ErrorClass::BusinessRejected => SupplierFailureClass::BusinessRejected,
        ErrorClass::TransientFailure => SupplierFailureClass::TransientFailure,
        ErrorClass::ResultUnknown => SupplierFailureClass::ResultUnknown,
        ErrorClass::AuthSignature => SupplierFailureClass::AuthSignature,
        ErrorClass::RateLimited => SupplierFailureClass::RateLimited,
        ErrorClass::OutOfOrder => SupplierFailureClass::OutOfOrder,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_failure_classes_round_trip_with_identical_wire_codes() {
        for class in [
            SupplierFailureClass::CapabilityGap,
            SupplierFailureClass::MappingError,
            SupplierFailureClass::BusinessRejected,
            SupplierFailureClass::TransientFailure,
            SupplierFailureClass::ResultUnknown,
            SupplierFailureClass::AuthSignature,
            SupplierFailureClass::RateLimited,
            SupplierFailureClass::OutOfOrder,
        ] {
            let mapped = integration_class(class);
            assert_eq!(supplier_class(mapped), class);
            assert_eq!(
                serde_json::to_value(class).unwrap(),
                serde_json::to_value(mapped).unwrap()
            );
        }
    }
}
