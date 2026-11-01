//! 只供受控演示数据入口调用的初始化与清理编排。
mod errors;
pub use errors::{Error, Result};
pub mod demo_master_data;
pub mod runtime_ports;
pub use demo_master_data::{
    ApplyDemoMasterDataRequest, DemoChunkReport, DemoFoundationReport, DemoMasterDataProcess,
    DemoResetReport, DemoStatus, ensure_indexes,
};
