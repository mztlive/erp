//! 销售选品 P0 强制上限。
//!
//! 这些数字是交付上限，变更必须同步产品文档与验收数据。

/// 每册商品池 SKU 数下限。
pub const POOL_SKU_MIN: usize = 1;
/// 每册商品池 SKU 数上限。
pub const POOL_SKU_MAX: usize = 500;
/// 套餐档位数下限。
pub const TIER_COUNT_MIN: usize = 1;
/// 套餐档位数上限。
pub const TIER_COUNT_MAX: usize = 10;
/// 每档期望套餐数下限。
pub const TIER_PACKAGE_MIN: u32 = 1;
/// 每档期望套餐数上限。
pub const TIER_PACKAGE_MAX: u32 = 20;
/// 每套餐独立 SKU 数下限。
pub const PACKAGE_SKU_MIN: u32 = 2;
/// 每套餐独立 SKU 数上限。
pub const PACKAGE_SKU_MAX: u32 = 8;
/// 单品陈列项上限。
pub const SINGLE_DISPLAY_MAX: usize = 500;
/// 套餐陈列项上限。
pub const PACKAGE_DISPLAY_MAX: usize = 200;
/// 按份采购份数下限。
pub const QUANTITY_MIN: u32 = 1;
/// 按份采购份数上限。
pub const QUANTITY_MAX: u32 = 100_000;
/// 单档最多展开的搜索状态数。
pub const SEARCH_STATE_BUDGET: u64 = 100_000;
/// 单档搜索最长执行时间（秒）。
pub const SEARCH_TIME_BUDGET_SECS: u64 = 5;
/// 一次准备任务期限（秒）。
pub const PREPARE_TASK_DEADLINE_SECS: u64 = 180;
/// 公开链接有效期（天）。
pub const LINK_TTL_DAYS: i64 = 30;
/// 令牌原始字节数（256 位，满足至少 128 位）。
pub const LINK_TOKEN_BYTES: usize = 32;
/// 档位名称最大长度。
pub const TIER_NAME_MAX_LEN: usize = 64;
/// 幂等键最大长度。
pub const IDEMPOTENCY_KEY_MAX_LEN: usize = 128;
/// 套餐图生成实现版本（P0 兜底）。
pub const PACKAGE_IMAGE_FALLBACK_VERSION: &str = "p0-first-member-v1";
/// 组合搜索算法版本。
pub const COMBINATION_ALGORITHM_VERSION: &str = "sales-selection-package-search-v1";
