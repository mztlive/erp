import type { SalesOrderProcurementProgress } from "@/features/sales-orders/types"
import { compareDecimal } from "@/lib/fixed-decimal"

/** 页级异常提示与采购页签共用同一判断，避免超采或待提交状态漏标。 */
export function getProcurementAttention(
    progress: SalesOrderProcurementProgress,
) {
    const hasGap = compareDecimal(progress.remainingQuantity, "0", 6) > 0
    const hasExcess = compareDecimal(progress.excessQuantity, "0", 6) > 0
    return {
        hasGap,
        hasExcess,
        needsAttention:
            hasGap ||
            hasExcess ||
            progress.sourceChanged ||
            progress.awaitingSubmissionCount > 0,
    }
}
