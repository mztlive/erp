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
    shipping_fee: string | null
}>

export type SupplierAddressLevel = "PROVINCE" | "CITY" | "COUNTY" | "TOWN"

export type SupplierAddressCandidate = Readonly<{
    code: string
    name: string
    level: SupplierAddressLevel
    ticket: string
}>

type ConfirmedAddressLevel = Readonly<{ code: string; name: string }>

export type SupplierConfirmedAddress = Readonly<{
    province: ConfirmedAddressLevel
    city: ConfirmedAddressLevel
    county: ConfirmedAddressLevel
    town: ConfirmedAddressLevel
    version: number
}>

export type SupplierExecutionOrderNodeRole =
    | "PARENT"
    | "RESERVATION_ROOT"
    | "LEAF"
    | "UNRESOLVED"

export type SupplierExecutionOrderTree = Readonly<{
    root_order_no: string
    version: number
    pending: boolean
    nodes: readonly Readonly<{
        external_order_no: string
        parent_order_no: string | null
        role: SupplierExecutionOrderNodeRole
        confirmation: "PENDING" | "CONFIRMED" | "REJECTED" | null
        fulfillment: string | null
        cancellation: string | null
        refund: string | null
        lines: readonly Readonly<{
            product_name: string
            sku_code: string
            quantity: string
            unit: string | null
        }>[]
    }>[]
}>

export type SupplierExecutionShipment = Readonly<{
    external_shipment_id: string | null
    source_order_no: string | null
    carrier: string | null
    tracking_no: string | null
    quantity_proven: boolean
    lines: readonly Readonly<{
        product_name: string
        unit: string | null
        sku_code: string
        quantity: string
    }>[]
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
    address_requirement?: "TEXT" | "BD09" | "FOUR_LEVEL" | "UNAVAILABLE"
    supplier_address?: SupplierConfirmedAddress | null
    supplier_address_editable?: boolean
    order_tree?: SupplierExecutionOrderTree | null
    result_query_available?: boolean
    shipments?: readonly SupplierExecutionShipment[]
}>

export type SaveSupplierExecutionSetup = Readonly<{
    expected_version: number
    idempotency_key: string
    geocode?: SupplierDeliveryGeocode
    choice_index?: number
    recipient_phone?: string
    supplier_address_ticket?: string
}>

export function fetchSupplierAddressCandidates(
    orderId: string,
    level: SupplierAddressLevel,
    parentTicket?: string,
    signal?: AbortSignal,
) {
    return apiGet<{ candidates: readonly SupplierAddressCandidate[] }>(
        `/admin/supplier-fulfillment-orders/${encodeURIComponent(orderId)}/execution-address-candidates`,
        { level, parent_ticket: parentTicket },
        { signal },
    )
}

export function querySupplierExecutionResult(
    orderId: string,
    expectedVersion: number,
) {
    return apiPost<SupplierExecutionSetup>(
        `/admin/supplier-fulfillment-orders/${encodeURIComponent(orderId)}/execution-query`,
        { expected_version: expectedVersion },
    )
}

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
