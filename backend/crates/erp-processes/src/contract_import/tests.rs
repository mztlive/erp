use super::*;
#[tokio::test]
async fn missing_provider_never_fabricates_success() {
    let attempt = RecognitionProviders::default().run(b"%PDF-1.7", 1).await;
    assert_eq!(attempt.failure.unwrap().code, "OCR_NOT_CONFIGURED");
    assert!(attempt.ocr.is_none());
    assert!(attempt.extraction.is_none());
}

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use erp_contract::entity::recognition::{
    ContractExtraction, ContractField, ExtractedField, ImportFailure, OcrDocument, OcrPage,
};
use erp_contract::ports::recognition::{ContractExtractor, ContractOcr};

struct TestProvider {
    calls: Arc<AtomicUsize>,
    missing_page: bool,
}
fn fields() -> BTreeMap<ContractField, ExtractedField> {
    use ContractField::*;
    [
        (ContractNo, "HT-1"),
        (CustomerName, "客户公司"),
        (CompanyName, "我方公司"),
        (SettlementName, "客户公司"),
        (PaymentTerms, "先款100%"),
        (InvoiceType, "增值税专用发票"),
        (TaxPoint, "13%"),
        (SignedAt, "2026-01-01"),
        (ValidFrom, "2026-01-01"),
        (ValidTo, "长期"),
        (BusinessScope, "年节礼包"),
    ]
    .into_iter()
    .map(|(field, value)| (field, ExtractedField { value: value.into(), quote: value.into(), page: 1 }))
    .collect()
}
#[async_trait]
impl ContractOcr for TestProvider {
    async fn recognize(&self, _: &[u8], _: u32) -> std::result::Result<OcrDocument, ImportFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(OcrDocument {
            provider: "unit-test".into(),
            version: "1".into(),
            pages: if self.missing_page {
                vec![]
            } else {
                vec![OcrPage {
                    number: 1,
                    text: fields().values().map(|f| f.value.as_str()).collect::<Vec<_>>().join("\n"),
                    blank: false,
                    readable: true,
                }]
            },
        })
    }
}
#[async_trait]
impl ContractExtractor for TestProvider {
    async fn extract(&self, _: &OcrDocument) -> std::result::Result<ContractExtraction, ImportFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ContractExtraction {
            provider: "unit-test".into(),
            version: "1".into(),
            fields: fields(),
            conflicts: vec![],
        })
    }
}
#[tokio::test]
async fn executes_real_pipeline_and_stops_before_ai_on_missing_page() {
    for missing_page in [false, true] {
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(TestProvider { calls: calls.clone(), missing_page });
        let providers = RecognitionProviders { ocr: provider.clone(), extractor: provider };
        let result = providers.run(b"pdf", 1).await;
        assert_eq!(calls.load(Ordering::SeqCst), if missing_page { 1 } else { 2 });
        assert_eq!(result.failure.is_some(), missing_page);
        assert_eq!(result.extraction.is_some(), !missing_page);
    }
}
#[tokio::test]
async fn rejects_broken_pdf_before_provider_call() {
    assert!(inspect_pdf(b"%PDF-broken".to_vec()).await.is_err());
}

struct PartialExtractor;
#[async_trait]
impl ContractExtractor for PartialExtractor {
    async fn extract(&self, _: &OcrDocument) -> std::result::Result<ContractExtraction, ImportFailure> {
        let mut fields = fields();
        fields.remove(&ContractField::ContractNo);
        fields.remove(&ContractField::PaymentTerms);
        Ok(ContractExtraction {
            provider: "test".into(),
            version: "1".into(),
            fields,
            conflicts: vec!["payment_terms：第2页存在两种付款约定".into()],
        })
    }
}

#[tokio::test]
async fn partial_extraction_and_conflicts_do_not_fail_recognition() {
    let providers = RecognitionProviders {
        ocr: Arc::new(TestProvider { calls: Arc::new(AtomicUsize::new(0)), missing_page: false }),
        extractor: Arc::new(PartialExtractor),
    };
    let attempt = providers.run(b"pdf", 1).await;
    assert!(attempt.failure.is_none());
    let draft = attempt.extraction.unwrap().draft(&attempt.ocr.unwrap());
    assert!(draft.fields[&ContractField::ContractNo].is_none());
    assert!(draft.fields[&ContractField::PaymentTerms].is_none());
    assert_eq!(draft.fields[&ContractField::CustomerName].as_deref(), Some("客户公司"));
    assert_eq!(draft.warnings.len(), 1);
}
