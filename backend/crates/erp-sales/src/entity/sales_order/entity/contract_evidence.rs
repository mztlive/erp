//! 销售建单凭证和后补合同的稳定对象规则。

use std::collections::HashSet;

use erp_core::ids::{ContractId, CustomerAccountId, FileAssetId, PartyId};
use erp_core::{Error, Result};

use super::{CommercialStatus, SalesOrder};

impl SalesOrder {
    /// 检验建单凭证的内容类型、对应扩展名与有效状态。
    ///
    /// # 参数
    /// * `mime` / `name` / `unavailable` / `byte_size` - 服务端读取的资产治理与大小元数据
    /// # 返回
    /// PDF、JPG、PNG 或 WebP 且有效时成功。
    /// # 错误
    /// 类型、扩展名不匹配或已销毁时拒绝。
    pub fn validate_creation_evidence_file(
        mime: &str,
        name: &str,
        unavailable: bool,
        byte_size: u64,
    ) -> Result<()> {
        let extension = name.rsplit('.').next().unwrap_or_default().to_ascii_lowercase();
        let accepted = match mime {
            "application/pdf" => extension == "pdf",
            "image/jpeg" => matches!(extension.as_str(), "jpg" | "jpeg"),
            "image/png" => extension == "png",
            "image/webp" => extension == "webp",
            _ => false,
        };
        let max_bytes = if mime == "application/pdf" { 20 * 1024 * 1024 } else { 5 * 1024 * 1024 };
        if !accepted || unavailable || byte_size == 0 || byte_size > max_bytes {
            return Err(Error::from("销售建单凭证必须为有效 PDF、JPG、PNG 或 WebP 文件"));
        }
        Ok(())
    }

    /// 设置首次建单凭证；无合同 ERP 单必须提供凭证。
    ///
    /// # 参数
    /// * `ids` - 已登记并待跨域校验的文件资产身份
    /// # 返回
    /// 规范化后写入稳定对象。
    /// # 错误
    /// 无合同缺凭证、空身份、重复身份或超过 20 份时拒绝。
    pub fn set_creation_evidence(&mut self, ids: Vec<FileAssetId>) -> Result<()> {
        if self.contract_id.is_none() && ids.is_empty() {
            return Err(Error::from("无合同创建销售单必须上传 PDF 或图片凭证"));
        }
        if ids.len() > 20 || ids.iter().any(|id| id.as_ref().trim().is_empty()) {
            return Err(Error::from("销售建单凭证必须为有效资产，最多 20 份"));
        }
        let mut unique = HashSet::new();
        if ids.iter().any(|id| !unique.insert(id.as_ref())) {
            return Err(Error::from("销售建单凭证不能重复"));
        }
        self.evidence_file_asset_ids = ids;
        Ok(())
    }

    /// 接纳销售编辑命令的合同上下文；无合同原单可首次补录。
    ///
    /// # 参数
    /// * `contract_id` / `customer_id` / `settlement_party_id` - 服务端解析的命令关系
    /// * `actor_id` - 当前编辑人
    /// # 返回
    /// 关系一致时成功；首次绑定时更新稳定合同。
    /// # 错误
    /// 客户、结算主体或已有合同不一致时拒绝。
    pub fn apply_command_contract_context(
        &mut self,
        contract_id: &Option<ContractId>,
        customer_id: &CustomerAccountId,
        settlement_party_id: &PartyId,
        actor_id: &str,
    ) -> Result<()> {
        if self.matches_contract_context(contract_id, customer_id, settlement_party_id) {
            return Ok(());
        }
        if let Some(contract_id) = contract_id.as_ref().filter(|_| self.contract_id.is_none()) {
            return self.bind_contract(contract_id.clone(), customer_id, settlement_party_id, actor_id);
        }
        Err(Error::from("销售单合同归属已变化，请刷新后重试"))
    }

    /// 首次绑定合同，保留客户、结算主体及全部商业内容。
    ///
    /// # 参数
    /// * `contract_id` - 已验证当前有效修订的合同
    /// * `customer_id` / `settlement_party_id` - 合同权威关系
    /// * `actor_id` - 已认证修改人
    /// # 返回
    /// 更新稳定合同关系和修改人。
    /// # 错误
    /// 原单作废、已有合同或合同归属不一致时拒绝。
    pub fn bind_contract(
        &mut self,
        contract_id: ContractId,
        customer_id: &CustomerAccountId,
        settlement_party_id: &PartyId,
        actor_id: &str,
    ) -> Result<()> {
        if self.commercial_status == CommercialStatus::Voided {
            return Err(Error::from("已作废销售单不能补合同"));
        }
        if self.contract_id.is_some() {
            return Err(Error::from("销售单已有关联合同，不允许替换"));
        }
        if &self.customer_id != customer_id || &self.settlement_party_id != settlement_party_id {
            return Err(Error::from("合同客户及结算主体必须与原销售单一致"));
        }
        self.contract_id = Some(contract_id);
        self.stable.touch(actor_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::SalesOrderId;

    use super::*;
    use crate::entity::sales_order::{BusinessType, OriginSystem, SalesOrderData};

    fn order() -> SalesOrder {
        SalesOrder::new(
            SalesOrderId::new("o-1"),
            SalesOrderData {
                business_org_unit_id: "org-1".into(),
                sales_owner_user_id: "sales-1".into(),
                order_no: "SO-1".into(),
                business_type: BusinessType::GoodsService,
                origin_system: OriginSystem::Erp,
                source_identity_id: None,
                customer_id: CustomerAccountId::new("customer-1"),
                contract_id: None,
                settlement_party_id: PartyId::new("party-1"),
                source_status_code: None,
            },
            "sales-1",
        )
        .unwrap()
    }

    #[test]
    fn accepts_recognized_contract_pdf_but_keeps_image_limit() {
        assert!(
            SalesOrder::validate_creation_evidence_file(
                "application/pdf",
                "合同.pdf",
                false,
                20 * 1024 * 1024
            )
            .is_ok()
        );
        assert!(
            SalesOrder::validate_creation_evidence_file(
                "application/pdf",
                "合同.pdf",
                false,
                20 * 1024 * 1024 + 1
            )
            .is_err()
        );
        assert!(
            SalesOrder::validate_creation_evidence_file("image/png", "凭证.png", false, 5 * 1024 * 1024 + 1)
                .is_err()
        );
    }

    #[test]
    fn no_contract_creation_requires_unique_evidence() {
        let mut order = order();
        assert!(order.set_creation_evidence(vec![]).is_err());
        assert!(order.set_creation_evidence(vec![FileAssetId::new("f"), FileAssetId::new("f")]).is_err());
        order.set_creation_evidence(vec![FileAssetId::new("f")]).unwrap();
        assert_eq!(order.evidence_file_asset_ids, vec![FileAssetId::new("f")]);
    }

    #[test]
    fn bind_contract_preserves_content_and_rejects_replacement_or_mismatch() {
        let mut order = order();
        let original = order.clone();
        assert!(
            order
                .bind_contract(
                    ContractId::new("c"),
                    &CustomerAccountId::new("other"),
                    &PartyId::new("party-1"),
                    "sales"
                )
                .is_err()
        );
        assert_eq!(order, original);
        order.commercial_status = CommercialStatus::Effective;
        order
            .bind_contract(
                ContractId::new("c"),
                &CustomerAccountId::new("customer-1"),
                &PartyId::new("party-1"),
                "sales",
            )
            .unwrap();
        assert_eq!(order.commercial_status, CommercialStatus::Effective);
        assert_eq!(order.order_no, original.order_no);
        assert!(
            order
                .bind_contract(
                    ContractId::new("replacement"),
                    &CustomerAccountId::new("customer-1"),
                    &PartyId::new("party-1"),
                    "sales"
                )
                .is_err()
        );
    }
    #[test]
    fn accepts_pdf_and_images_with_matching_extensions() {
        for (mime, name) in [
            ("application/pdf", "凭证.PDF"),
            ("image/jpeg", "a.jpg"),
            ("image/png", "a.png"),
            ("image/webp", "a.webp"),
        ] {
            assert!(SalesOrder::validate_creation_evidence_file(mime, name, false, 1).is_ok());
        }
        assert!(SalesOrder::validate_creation_evidence_file("application/pdf", "a.exe", false, 1).is_err());
        assert!(SalesOrder::validate_creation_evidence_file("image/png", "a.png", true, 1).is_err());
        assert!(SalesOrder::validate_creation_evidence_file("image/svg+xml", "a.svg", false, 1).is_err());
    }
    #[test]
    fn invalid_file_size_and_void_contract_binding_are_rejected() {
        assert!(SalesOrder::validate_creation_evidence_file("application/pdf", "a.pdf", false, 0).is_err());
        assert!(
            SalesOrder::validate_creation_evidence_file(
                "application/pdf",
                "a.pdf",
                false,
                20 * 1024 * 1024 + 1
            )
            .is_err()
        );
        let mut order = order();
        order.commercial_status = CommercialStatus::Voided;
        let before = order.clone();
        assert!(
            order
                .bind_contract(
                    ContractId::new("c"),
                    &CustomerAccountId::new("customer-1"),
                    &PartyId::new("party-1"),
                    "sales"
                )
                .is_err()
        );
        assert_eq!(order, before);
    }

    #[test]
    fn editing_accepts_first_contract_binding_and_prevents_removal() {
        let mut order = order();
        order
            .apply_command_contract_context(
                &Some(ContractId::new("c")),
                &CustomerAccountId::new("customer-1"),
                &PartyId::new("party-1"),
                "sales",
            )
            .unwrap();
        assert_eq!(order.contract_id.as_deref(), Some("c"));
        assert!(
            order
                .apply_command_contract_context(
                    &None,
                    &CustomerAccountId::new("customer-1"),
                    &PartyId::new("party-1"),
                    "sales"
                )
                .is_err()
        );
    }

    #[test]
    fn historical_documents_default_to_no_creation_evidence() {
        let mut value = serde_json::to_value(order()).unwrap();
        value.as_object_mut().unwrap().remove("evidence_file_asset_ids");
        let historical: SalesOrder = serde_json::from_value(value).unwrap();
        assert!(historical.evidence_file_asset_ids.is_empty());
    }
}
