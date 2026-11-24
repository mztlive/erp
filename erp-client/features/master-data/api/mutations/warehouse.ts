import { apiPost } from "@/lib/api"
import { formatIsoDate, todayDate } from "@/lib/datetime"

import type { WarehouseDto } from "../contracts"

export type CreateWarehouseInput = Readonly<{
    warehouseCode: string
    name: string
    regionCode: string
    address: string
    contact: string
    inboundHandlerUserId: string
    outboundHandlerUserId: string
    changeReason: string
}>

export function createWarehouse(input: CreateWarehouseInput) {
    return apiPost<WarehouseDto>("/admin/warehouses", {
        warehouse_code: input.warehouseCode.trim(),
        name: input.name.trim(),
        region_code: input.regionCode,
        address: input.address.trim(),
        contact: input.contact.trim(),
        inbound_handler_user_id: input.inboundHandlerUserId,
        outbound_handler_user_id: input.outboundHandlerUserId,
        effective_from:
            formatIsoDate(new Date().toISOString(), {
                timeZone: "Asia/Shanghai",
            }) ?? todayDate(),
        change_reason: input.changeReason.trim(),
    })
}
