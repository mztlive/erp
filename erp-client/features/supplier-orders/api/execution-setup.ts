import { apiGet, apiPost } from "@/lib/api"
import { isApiError } from "@/lib/api/errors"

export type SupplierDeliveryGeocode = Readonly<{
    city_name: string
    district: string
    latitude: string
    longitude: string
    coordinate_system: "BD09"
}>

export type SupplierDeliveryChoice = Readonly<{
    index: number
    method: "COURIER" | "LOCAL_DELIVERY" | "PICKUP"
    date: string | null
    slot: string | null
    store_id: string | null
    shipping_fee: string
}>

export type SupplierExecutionSetup = Readonly<{
    execution_version: number
    stage: string
    blocker: string | null
    geocode_editable: boolean
    choice_editable: boolean
    recipient_phone_required: boolean
    geocode: SupplierDeliveryGeocode | null
    choices: readonly SupplierDeliveryChoice[]
    selected_choice: number | null
}>

export type SaveSupplierExecutionSetup = Readonly<{
    expected_version: number
    idempotency_key: string
    geocode?: SupplierDeliveryGeocode
    choice_index?: number
    recipient_phone?: string
}>

export async function fetchSupplierExecutionSetup(orderId: string) {
    try {
        return await apiGet<SupplierExecutionSetup>(
            `/admin/supplier-fulfillment-orders/${encodeURIComponent(orderId)}/execution-setup`,
        )
    } catch (error) {
        // 既有非 API 自动订单没有执行资料；真实鉴权及网络错误不能吞掉。
        if (isApiError(error) && error.status === 404) return null
        throw error
    }
}

export function saveSupplierExecutionSetup(
    orderId: string,
    request: SaveSupplierExecutionSetup,
) {
    return apiPost<SupplierExecutionSetup>(
        `/admin/supplier-fulfillment-orders/${encodeURIComponent(orderId)}/execution-setup`,
        request,
    )
}
