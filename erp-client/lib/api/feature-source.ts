/**
 * 按 feature 切换 mock / 真实接口的数据源开关。
 *
 * REAL_FEATURES 当前为空集（全部 feature 走 mock）；后续阶段接入真实接口时，
 * 在集合中加入对应 feature 名（如 "mall-sync"）即可切换。
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

/** 已切换到真实接口的 feature 集合（当前为空，全量走 mock）。 */
export const REAL_FEATURES: ReadonlySet<FeatureName> = new Set()

/**
 * 判断指定 feature 是否已接入真实接口（否则走 mock）。
 *
 * @param name feature 名。
 * @returns 已接入真实接口返回 true。
 */
export const isFeatureReal = (name: FeatureName): boolean =>
  REAL_FEATURES.has(name)

/**
 * 返回 feature 当前数据源的标签。
 *
 * @param name feature 名。
 * @returns "real" 表示真实接口，"mock" 表示 Mock 数据。
 */
export const featureSourceLabel = (name: FeatureName): "real" | "mock" =>
  isFeatureReal(name) ? "real" : "mock"
