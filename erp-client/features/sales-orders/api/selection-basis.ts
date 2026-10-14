import { apiGet } from "@/lib/api"
import type { SalesOrderDraftLineInput } from "@/features/sales-orders/types"

export type SalesSelectionSource = { proposal_id: string; proposal_no: string }
export type SalesOrderSelectionBasis = {
    source: SalesSelectionSource
    customerId: string
    customerName: string
    lineItems: SalesOrderDraftLineInput[]
}

type BackendSelectionBasis = {
    source: SalesSelectionSource
    customer_id: string
    customer_name: string
    lines: Array<{
        id: string
        sku_id: string
        sku_revision_id: string
        name: string
        specification: { name: string; value: string }[]
        unit: string
        quantity: number
        unit_price: string
    }>
}

export async function fetchSelectionOrderBasis(
    id: string,
): Promise<SalesOrderSelectionBasis> {
    const basis = await apiGet<BackendSelectionBasis>(
        `/admin/sales-orders/selection-proposals/${encodeURIComponent(id)}/basis`,
    )
    return {
        source: basis.source,
        customerId: basis.customer_id,
        customerName: basis.customer_name,
        lineItems: basis.lines.map((line) => ({
            rowKey: line.id,
            name: line.name,
            sku: line.sku_id,
            skuRevisionId: line.sku_revision_id,
            specification: line.specification
                .map((item) => `${item.name}：${item.value}`)
                .join(" / "),
            unit: line.unit,
            quantity: String(line.quantity),
            unitPriceGross: "",
            pricingMode: "MANUAL",
            procurementSupplyPrice: "",
            dueDate: "",
            faceValue: "",
            giftRate: "",
            cardForm: "",
        })),
    }
}
