import type { StatusTone } from "@/components/ui/status-badge"
import type { PurchaseSourceSalesOrderView } from "@/features/purchase-orders/types"
import type { BackendPurchaseSourceSalesOrder } from "./purchase-order-wire-types"

const SALES_STATUS: Record<string, { label: string; tone: StatusTone }> = {
    DRAFT: { label: "草稿", tone: "neutral" },
    PENDING_REVIEW: { label: "审批中", tone: "info" },
    EFFECTIVE: { label: "已生效", tone: "success" },
    VOIDED: { label: "已作废", tone: "void" },
}

/** 保留采购接口指定的销售版本和成交价，不补读当前销售单。 */
export function mapPurchaseSourceSalesOrder(
    source?: BackendPurchaseSourceSalesOrder | null,
): PurchaseSourceSalesOrderView | undefined {
    if (!source) return undefined
    const status = SALES_STATUS[source.status]
    return {
        salesOrderId: source.sales_order_id,
        salesOrderNo: source.sales_order_no,
        statusLabel: status?.label ?? "状态待确认",
        statusTone: status?.tone ?? "neutral",
        revisionId: source.revision_id,
        revisionNo: source.revision_no,
        customerName: source.customer_name,
        salesOwnerName: source.sales_owner_name ?? undefined,
        contractNo: source.contract_no ?? undefined,
        totals: source.totals,
        lines: source.lines.map((line) => ({
            salesOrderRevisionLineId: line.sales_order_revision_line_id,
            salesOrderLineId: line.sales_order_line_id,
            lineNo: line.line_no,
            itemName: line.item_name,
            specification: line.specification ?? undefined,
            quantity: line.quantity,
            unit: line.unit,
            unitPriceGross: line.unit_price_gross,
            grossAmount: line.gross_amount,
        })),
        materials: source.materials.map((file) => ({
            fileAssetId: file.file_asset_id,
            kind: file.kind,
            fileName: file.file_name,
            contentType: file.content_type,
            byteSize: file.byte_size,
        })),
        materialsUnavailable: source.materials_unavailable ?? false,
    }
}
