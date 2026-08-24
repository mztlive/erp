import { apiPost } from "@/lib/api"
import type {
    ResolveProcurementResponsibilityRequestWire,
    ResolveProcurementResponsibilityResponseWire,
} from "@/features/sales-orders/api/procurement-responsibility-wire-types"
import type {
    SalesLineProcurementResponsibility,
    SalesOrderDraftLineInput,
} from "@/features/sales-orders/types"

export async function resolveSalesLineProcurementResponsibilities(
    lines: readonly SalesOrderDraftLineInput[],
): Promise<readonly SalesLineProcurementResponsibility[]> {
    const payload: ResolveProcurementResponsibilityRequestWire = {
        lines: lines.map((line) => ({
            line_key: line.rowKey,
            sku_id: line.sku,
            sku_revision_id: line.skuRevisionId || undefined,
            fulfillment_mode: line.fulfillmentMode || undefined,
        })),
    }
    const response =
        await apiPost<ResolveProcurementResponsibilityResponseWire>(
            "/admin/procurement-responsibility/resolve",
            payload,
        )
    const rows = Array.isArray(response) ? response : response.lines
    return rows.map((line) => {
        const ownerUserId = line.owner_user_id?.trim() || undefined
        const ownerName = line.owner_name?.trim() || undefined
        return {
            rowKey: line.line_key,
            resolved:
                line.resolved && Boolean(ownerUserId) && Boolean(ownerName),
            ownerUserId,
            ownerName,
            matchedRuleType: line.rule_type ?? undefined,
        }
    })
}
