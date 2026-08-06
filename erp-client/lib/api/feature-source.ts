/**
 * Feature 数据源标签（P4 后业务路径均为真实 HTTP）。
 *
 * 保留 API 以免外部引用断裂；`isFeatureReal` 恒为 true。
 * 新代码无需再分支 mock。
 */

/** erp-client/features 下全部 feature 名（28 个）。 */
export type FeatureName =
  | "access-audit"
  | "actual-profit-loss"
  | "card-business-analytics"
  | "card-funds-review"
  | "contracts"
  | "customer-quality"
  | "customer-receivables"
  | "customers"
  | "execution-projections"
  | "fulfillment-operations"
  | "history-backfill"
  | "import-opening"
  | "integration-errors"
  | "inventory"
  | "mall-consumption-orders"
  | "mall-sync"
  | "master-data"
  | "procurement-confirmation"
  | "product-publications"
  | "purchase-orders"
  | "sales-orders"
  | "supplier-api-connections"
  | "supplier-catalog"
  | "supplier-orders"
  | "supplier-payables"
  | "supplier-settlements"
  | "unified-task-queue"
  | "workspace"

/** 历史集合字段：P4 后业务路径均为真实接口，保留为空集仅兼容旧引用。 */
export const REAL_FEATURES: ReadonlySet<FeatureName> = new Set()

/**
 * 判断指定 feature 是否已接入真实接口。
 *
 * @param name feature 名（保留参数兼容调用方）。
 * @returns P4 后恒为 true。
 */
export const isFeatureReal = (name: FeatureName): boolean => {
  void name
  return true
}

/**
 * 返回 feature 当前数据源的标签。
 *
 * @param name feature 名（保留参数兼容调用方）。
 * @returns 恒为 "real"。
 */
export const featureSourceLabel = (name: FeatureName): "real" | "mock" => {
  void name
  return "real"
}
