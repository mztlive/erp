import { apiGet } from "@/lib/api"
import { compactFixed } from "@/lib/fixed-decimal"

/** 仅消费销售详情的冻结商业快照；不引入销售工作面的查询及依赖。 */
type SalesSourceDto = {
    current_revision_id?: string | null
    owner_user_name?: string | null
    revisions: Array<{
        id: string
        customer_name?: string | null
        project_name?: string | null
        contract_no?: string | null
        commercial_lines?: Array<{
            item_name_snapshot: string
            spec_snapshot?: string | null
            quantity?: string | null
            card_count?: number | null
            unit_snapshot?: string | null
        }>
    }>
}

export type AllocationSource = {
    customerName?: string
    projectName?: string
    contractNo?: string
    ownerName?: string
    products: readonly string[]
}

/** 有应收的销售单只读指定生效修订，不以未通过的工作副本替代。 */
export async function fetchAllocationSource(
    salesOrderId: string,
    revisionId?: string,
): Promise<AllocationSource> {
    const detail = await apiGet<SalesSourceDto>(
        `/admin/sales-orders/${encodeURIComponent(salesOrderId)}`,
    )
    const revision = detail.revisions.find(
        (item) => item.id === (revisionId || detail.current_revision_id),
    )
    return {
        customerName: revision?.customer_name?.trim() || undefined,
        projectName: revision?.project_name?.trim() || undefined,
        contractNo: revision?.contract_no?.trim() || undefined,
        ownerName: detail.owner_user_name?.trim() || undefined,
        products: (revision?.commercial_lines ?? []).map((line) => {
            const quantity = line.quantity ?? line.card_count?.toString()
            const spec = line.spec_snapshot?.trim()
            return [
                line.item_name_snapshot,
                spec && spec !== "无规格" ? `（${spec}）` : "",
                quantity
                    ? ` × ${compactFixed(quantity)}${line.unit_snapshot?.trim() || ""}`
                    : "",
            ].join("")
        }),
    }
}
