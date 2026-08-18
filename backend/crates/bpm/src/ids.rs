//! BPM 核心主键 ID。
//!
//! 值一律由调用方生成并传入；本模块不依赖 ID 生成器，也不读取系统时钟。

use entity_macros::id_type;

id_type!(ApprovalProcessDefinitionId);
id_type!(ApprovalNodeDefinitionId);
id_type!(ApprovalTransitionDefinitionId);
id_type!(ApprovalProcessInstanceId);
id_type!(ApprovalNodeExecutionId);
id_type!(ApprovalInstanceAssigneeId);
id_type!(ApprovalCommandReceiptId);

#[cfg(test)]
mod tests {
    use super::{
        ApprovalCommandReceiptId, ApprovalInstanceAssigneeId, ApprovalNodeDefinitionId,
        ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId,
        ApprovalTransitionDefinitionId,
    };

    /// 断言过程宏展开后的 `new` / Deref / Display / 透明 serde。
    macro_rules! assert_expanded_id {
        ($ty:ty, $value:expr) => {{
            let value = $value;
            let id = <$ty>::new(value);
            assert_eq!(id.as_ref(), value);
            assert_eq!(&*id, value);
            assert_eq!(id.to_string(), value);
            assert_eq!(format!("{id}"), value);

            let json = serde_json::to_string(&id).unwrap();
            assert_eq!(json, format!("\"{value}\""));
            let back: $ty = serde_json::from_str(&json).unwrap();
            assert_eq!(back, id);
            assert_eq!(back.as_ref(), value);
        }};
    }

    /// 7 个 BPM Core ID 均透明序列化，且彼此类型隔离。
    #[test]
    fn bpm_core_ids_are_transparent_and_type_isolated() {
        let value = "00112233445566778899aabbccddeeff";
        assert_expanded_id!(ApprovalProcessDefinitionId, value);
        assert_expanded_id!(ApprovalNodeDefinitionId, value);
        assert_expanded_id!(ApprovalTransitionDefinitionId, value);
        assert_expanded_id!(ApprovalProcessInstanceId, value);
        assert_expanded_id!(ApprovalNodeExecutionId, value);
        assert_expanded_id!(ApprovalInstanceAssigneeId, value);
        assert_expanded_id!(ApprovalCommandReceiptId, value);

        let names = [
            std::any::type_name::<ApprovalProcessDefinitionId>(),
            std::any::type_name::<ApprovalNodeDefinitionId>(),
            std::any::type_name::<ApprovalTransitionDefinitionId>(),
            std::any::type_name::<ApprovalProcessInstanceId>(),
            std::any::type_name::<ApprovalNodeExecutionId>(),
            std::any::type_name::<ApprovalInstanceAssigneeId>(),
            std::any::type_name::<ApprovalCommandReceiptId>(),
        ];
        for (index, name) in names.iter().enumerate() {
            assert!(
                names.iter().filter(|other| *other == name).count() == 1,
                "BPM ID 类型名必须唯一: index={index} name={name}"
            );
        }
        assert_ne!(
            ApprovalProcessDefinitionId::new(value),
            ApprovalProcessDefinitionId::new("other")
        );
    }
}
