import { apiGet, apiPut } from "@/lib/api"

import type { WarehouseDto } from "./contracts"

export type WarehouseFulfillmentHandlerOption = Readonly<{
    user_id: string
    display_name: string
    account: string
    inbound_eligible: boolean
    outbound_eligible: boolean
}>

export type UpdateWarehouseFulfillmentHandlersInput = Readonly<{
    warehouseId: string
    version: number
    inboundHandlerUserId: string
    outboundHandlerUserId: string
}>

/** 查询仓库更新权限范围内的收发经办人候选。 */
export function fetchWarehouseFulfillmentHandlerOptions(): Promise<
    WarehouseFulfillmentHandlerOption[]
> {
    return apiGet<WarehouseFulfillmentHandlerOption[]>(
        "/admin/warehouse-fulfillment-handler-options",
    )
}

/** 只更新仓库收发责任，不改写仓库修订与已有履约任务。 */
export function updateWarehouseFulfillmentHandlers(
    input: UpdateWarehouseFulfillmentHandlersInput,
): Promise<WarehouseDto> {
    return apiPut<WarehouseDto>(
        `/admin/warehouses/${encodeURIComponent(input.warehouseId)}/fulfillment-handlers`,
        {
            version: input.version,
            inbound_handler_user_id: input.inboundHandlerUserId,
            outbound_handler_user_id: input.outboundHandlerUserId,
        },
    )
}
