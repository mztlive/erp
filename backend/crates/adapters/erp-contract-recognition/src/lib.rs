//! 合同识别供应商适配；应用装配本 crate，流程仅持有合同领域 Port。
mod aliyun;
mod openai;

pub use aliyun::AliyunContractOcr;
pub use openai::OpenAiContractExtractor;
