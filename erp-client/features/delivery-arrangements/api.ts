import { apiGet, apiPut } from "@/lib/api"
export type ArrangementScope = "sales-orders" | "deliveries"
export type AddressAllocation = {
    recipient: string
    phone: string
    region_code: string
    region_name: string
    address: string
    quantity: string
}
export type ArrangementLine = {
    line_id: string
    allocations: AddressAllocation[]
}
export type ArrangementView = {
    version: string
    source_changed: boolean
    blocked_reason?: string | null
    registered_packages: number
    lines: Array<
        ArrangementLine & {
            item_name: string
            quantity: string
            unit: string
            shipped_addresses: Array<{
                key: string
                recipient: string
                phone: string
                address: string
                quantity: string
            }>
        }
    >
}
export type SaveArrangement = {
    expected_version: string
    lines: ArrangementLine[]
    reason: string
    reset_packages: boolean
}
const path = (scope: ArrangementScope, id: string) =>
    `/admin/${scope}/${encodeURIComponent(id)}/delivery-arrangements`
export const getArrangement = (scope: ArrangementScope, id: string) =>
    apiGet<ArrangementView>(path(scope, id))
export const saveArrangement = (
    scope: ArrangementScope,
    id: string,
    request: SaveArrangement,
) => apiPut<void>(path(scope, id), request)
