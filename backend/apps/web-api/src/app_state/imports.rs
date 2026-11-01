//! 导入与识别业务所消费服务的应用装配。

use erp_data_import::import_apply::{ImportWorkItemAccess, ImportWorkItems};

use super::{
    AliyunContractOcr, AppState, Arc, ContractImportProcess, ContractImportServices, MongoCatalogSupplyQuery,
    OpenAiContractExtractor, ProductImportServices, RecognitionProviders, SalesInquiryImportProcess,
};

impl AppState {
    /// 产品报价表异步导入流程。
    ///
    /// # 返回
    /// 返回绑定当前数据库、对象存储与内容指纹密钥的导入流程。
    ///
    /// # 错误
    /// 无。
    pub fn product_import_process(&self) -> erp_data_import::ProductImportProcess {
        let services = ProductImportServices {
            catalog: catalog_service(self.db()),
            scoped_catalog: scoped_catalog_service(self.db(), self.rbac()),
            catalog_access: catalog_access(self.db(), self.rbac()),
            offerings: Arc::new(scoped_offering_service(self.db(), self.rbac())),
            offering_access: offering_access(self.db(), self.rbac()),
        };
        erp_data_import::ProductImportProcess::new(
            self.db(),
            services,
            self.storage().clone(),
            self.config_snapshot().app.secret.as_bytes(),
        )
    }

    /// 绑定供应商后台导入所需数据库、授权源、对象存储和密文编解码器。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回导入流程；不执行 I/O。
    ///
    /// # 错误
    /// 无。
    pub fn supplier_import_process(&self) -> erp_data_import::SupplierImportProcess {
        erp_data_import::SupplierImportProcess::new(
            self.db(),
            Arc::new(self.supplier_profile_process()),
            self.storage().clone(),
            self.sensitive_data(),
        )
    }

    /// 按当前配置快照装配阿里云 OCR 和 OpenAI 兼容提取端。
    /// # 参数
    /// 无。
    /// # 返回
    /// 供应商无关导入流程。
    /// # 错误
    /// 无；配置缺省或识别失败由任务记录，不返回模拟识别结果。
    pub fn contract_import_process(&self) -> ContractImportProcess {
        let mut providers = RecognitionProviders::default();
        let snapshot = self.config.snapshot();
        if let Some(ocr) = snapshot.aliyun_ocr {
            providers.ocr = Arc::new(AliyunContractOcr::new(ocr.credentials, ocr.pdftoppm_path));
        }
        if let Some(ai) = snapshot.contract_ai {
            providers.extractor = Arc::new(OpenAiContractExtractor::new(
                ai.provider_id,
                ai.base_url,
                ai.api_key,
                ai.model,
                ai.timeout_seconds,
                ai.max_output_tokens,
            ));
        }
        let services = ContractImportServices {
            contracts: Arc::new(self.contract_service()),
            contract_access: contract_access(self.db(), self.rbac()),
            customer_access: customer_access(self.db(), self.rbac()),
        };
        ContractImportProcess::new(self.db(), services, providers)
    }

    /// 装配导入确认查询和每次独立身份的完成、重放资格重验。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回持有原跨域事务边界的导入流程。
    /// # 错误
    /// 不执行 I/O，不返回错误。
    pub fn import_apply_process(&self) -> erp_data_import::ImportApplyProcess {
        let decision_db = self.db();
        let work_items = ImportWorkItemAccess::new(
            Arc::new(work_item_service(self.db(), self.rbac())),
            Arc::new(move || -> Arc<dyn ImportWorkItems> {
                Arc::new(work_item_service(decision_db.clone(), shared_rbac_service(decision_db.clone())))
            }),
        );
        erp_data_import::ImportApplyProcess::new(self.db(), Arc::new(work_items))
    }

    /// 装配询价文件导入，沿用实时身份及精确可售 SKU 查询能力。
    /// # 参数
    /// 无。
    /// # 返回
    /// 未执行 I/O 的询价文件导入流程。
    /// # 错误
    /// 不返回错误。
    pub fn inquiry_import_process(&self) -> SalesInquiryImportProcess {
        SalesInquiryImportProcess::new(
            self.db(),
            self.rbac(),
            Arc::new(MongoCatalogSupplyQuery::new(self.db())),
            Arc::new(self.sales_inquiry_process()),
        )
    }
}

use crate::wiring::identity::shared_rbac_service;
use crate::wiring::workflow::work_item_service;
use crate::wiring::{
    catalog_access, catalog_service, contract_access, customer_access, offering_access,
    scoped_catalog_service, scoped_offering_service,
};
