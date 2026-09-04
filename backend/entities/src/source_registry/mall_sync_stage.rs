//! 商城同步阶段准入：来源商城映射写入前的纯实体状态规则。
//!
//! 数据库装载仍由调用方完成；本域只判定已装载来源是否允许映射写入，
//! 并以强类型失配返回原因，传输错误类别由调用方映射。

use super::{MallSyncStage, SourceSystem, SourceSystemStatus, SourceSystemType};

/// 映射阶段准入失配的强类型原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MallSyncStageMismatch {
    /// 来源不是商城系统或已停用。
    NotMallOrInactive,
    /// 商城来源未配置同步阶段。
    StageNotConfigured,
    /// 客户端冻结阶段与服务端当前阶段不一致。
    StageChanged,
    /// 当前阶段不是一期商城主导，映射写入已封存。
    NotFirstPhase,
}

impl SourceSystem {
    /// 校验映射写入的阶段准入并返回强类型失配。
    ///
    /// # 参数
    /// * `expected_stage` - 客户端冻结的期望阶段；`None` 表示只校验准入
    ///
    /// # 返回
    /// 准入通过时返回 `Ok(())`。
    ///
    /// # 错误
    /// 非商城/停用、未配置阶段、阶段已变化或非一期主导时
    /// 返回对应的 `MallSyncStageMismatch`。
    ///
    /// # 约束
    /// 纯实体状态规则，不访问数据库；缺失来源的 `NotFound` 仍由调用方判定。
    pub fn check_mapping_stage(
        &self,
        expected_stage: Option<MallSyncStage>,
    ) -> std::result::Result<(), MallSyncStageMismatch> {
        if self.system_type != SourceSystemType::Mall || self.stable.status != SourceSystemStatus::Active {
            return Err(MallSyncStageMismatch::NotMallOrInactive);
        }
        let Some(current_stage) = self.mall_sync_stage else {
            return Err(MallSyncStageMismatch::StageNotConfigured);
        };
        if expected_stage.is_some_and(|expected| expected != current_stage) {
            return Err(MallSyncStageMismatch::StageChanged);
        }
        if current_stage != MallSyncStage::FirstPhaseMallOwned {
            return Err(MallSyncStageMismatch::NotFirstPhase);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{MallSyncStage, MallSyncStageMismatch};
    use crate::common::stable::StableBase;
    use crate::ids::SourceSystemId;
    use crate::source_registry::{SourceSystem, SourceSystemData, SourceSystemStatus, SourceSystemType};
    use entity_core::BaseModel;

    fn mall_system(stage: Option<MallSyncStage>) -> SourceSystem {
        match stage {
            Some(stage) => SourceSystem::new(
                SourceSystemId::new("mall-1"),
                SourceSystemData {
                    code: "MALL-1".to_string(),
                    system_type: SourceSystemType::Mall,
                    name: "商城".to_string(),
                    status: SourceSystemStatus::Active,
                    mall_sync_stage: Some(stage),
                },
                "admin-1",
            )
            .unwrap(),
            // 历史脏数据可能缺失阶段配置，构造器禁止新建但读取必须失败关闭。
            None => SourceSystem {
                base: BaseModel::new("mall-legacy".to_string()),
                stable: StableBase::new(SourceSystemStatus::Active, "admin-1"),
                code: "MALL-LEGACY".to_string(),
                system_type: SourceSystemType::Mall,
                name: "商城".to_string(),
                mall_sync_stage: None,
            },
        }
    }

    #[test]
    fn first_phase_mall_owned_passes_with_matching_or_absent_expectation() {
        let system = mall_system(Some(MallSyncStage::FirstPhaseMallOwned));
        assert!(system.check_mapping_stage(None).is_ok());
        assert!(system
            .check_mapping_stage(Some(MallSyncStage::FirstPhaseMallOwned))
            .is_ok());
    }

    #[test]
    fn inactive_or_non_mall_source_is_rejected() {
        let disabled = SourceSystem::new(
            SourceSystemId::new("mall-2"),
            SourceSystemData {
                code: "MALL-2".to_string(),
                system_type: SourceSystemType::Mall,
                name: "商城".to_string(),
                status: SourceSystemStatus::Disabled,
                mall_sync_stage: Some(MallSyncStage::FirstPhaseMallOwned),
            },
            "admin-1",
        )
        .unwrap();
        assert_eq!(
            disabled.check_mapping_stage(None),
            Err(MallSyncStageMismatch::NotMallOrInactive)
        );
        let erp = SourceSystem::new(
            SourceSystemId::new("erp-1"),
            SourceSystemData {
                code: "ERP".to_string(),
                system_type: SourceSystemType::Erp,
                name: "ERP".to_string(),
                status: SourceSystemStatus::Active,
                mall_sync_stage: None,
            },
            "admin-1",
        )
        .unwrap();
        assert_eq!(
            erp.check_mapping_stage(None),
            Err(MallSyncStageMismatch::NotMallOrInactive)
        );
    }

    #[test]
    fn missing_stage_changed_stage_and_archived_are_distinguished() {
        assert_eq!(
            mall_system(None).check_mapping_stage(None),
            Err(MallSyncStageMismatch::StageNotConfigured)
        );
        assert_eq!(
            mall_system(Some(MallSyncStage::FirstPhaseMallOwned))
                .check_mapping_stage(Some(MallSyncStage::Archived)),
            Err(MallSyncStageMismatch::StageChanged)
        );
        assert_eq!(
            mall_system(Some(MallSyncStage::Archived)).check_mapping_stage(None),
            Err(MallSyncStageMismatch::NotFirstPhase)
        );
        assert_eq!(
            mall_system(Some(MallSyncStage::Archived)).check_mapping_stage(Some(MallSyncStage::Archived)),
            Err(MallSyncStageMismatch::NotFirstPhase)
        );
    }
}
