import { apiGet } from "@/lib/api"
import { compactFixed } from "@/lib/fixed-decimal"

type AllocationSourceDto = {
    customer_name: string
    project_name?: string | null
    contract_no?: string | null
    owner_name?: string | null
    products: Array<{
        name: string
        spec?: string | null
        quantity?: string | null
        unit?: string | null
    }>
}

export type AllocationSource = {
    customerName?: string
    projectName?: string
    contractNo?: string
    ownerName?: string
    products: readonly string[]
}

/** 从财务接口读取应收绑定的生效资料；来源版本由服务端确定。 */
export async function fetchAllocationSource(
    targetKind: "receivable_entry" | "receivable_account",
    targetId: string,
): Promise<AllocationSource> {
    const source = await apiGet<AllocationSourceDto>(
        `/admin/receivable-accounts/sources/${targetKind}/${encodeURIComponent(targetId)}`,
    )
    return {
        customerName: source.customer_name.trim() || undefined,
        projectName: source.project_name?.trim() || undefined,
        contractNo: source.contract_no?.trim() || undefined,
        ownerName: source.owner_name?.trim() || undefined,
        products: source.products.map((line) =>
            [
                line.name,
                line.spec && line.spec !== "无规格" ? `（${line.spec}）` : "",
                line.quantity
                    ? ` × ${compactFixed(line.quantity)}${line.unit?.trim() || ""}`
                    : "",
            ].join(""),
        ),
    }
}
