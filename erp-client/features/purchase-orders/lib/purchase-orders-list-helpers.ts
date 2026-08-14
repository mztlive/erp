import type { PurchaseOrderListItem } from "@/features/purchase-orders/types"
import { PURCHASE_TYPE_LABEL } from "@/features/purchase-orders/types"

/** 列表单号展示：正式单号 > 草稿标签 > 占位。 */
export function displayPurchaseOrderNo(
    row: PurchaseOrderListItem,
): string {
    return row.purchaseNo ?? row.draftLabel ?? "采购单（未编号）"
}

const CSV_HEADER = "采购单号,状态,供应商,来源销售单,类型,含税金额,付款,履约,负责人"

/** 列表导出 CSV（与页面导出动作一致：成本隐藏时金额列打码）。 */
export function buildPurchaseOrdersCsv(
    rows: readonly PurchaseOrderListItem[],
): string {
    const quote = (value: string) => `"${value.replaceAll('"', '""')}"`
    const body = rows.map((row) =>
        [
            displayPurchaseOrderNo(row),
            row.statusLabel,
            row.supplierName,
            row.salesOrderNo,
            PURCHASE_TYPE_LABEL[row.purchaseType],
            row.costMasked ? "***" : row.grossAmount,
            row.paymentProgress,
            row.fulfillmentProgress,
            row.ownerName,
        ]
            .map((value) => quote(String(value)))
            .join(","),
    )
    return [CSV_HEADER, ...body].join("\n")
}
