import type {
    OfferingStatus,
    SupplierOfferingView,
} from "@/features/supplier-offerings/types"

/** 行上关系状态动作：提交时仍走条款修订并写入新版本。 */
export type OfferingStatusIntent = Readonly<{
    nextStatus: OfferingStatus
    label: string
    title: string
    submitLabel: string
    defaultReason: string
    actionId: string
    destructive: boolean
}>

export const PAUSE_OFFERING_INTENT: OfferingStatusIntent = {
    nextStatus: "PAUSED",
    label: "暂停",
    title: "暂停供给关系",
    submitLabel: "暂停并保存新版本",
    defaultReason: "暂停供给关系",
    actionId: "pause",
    destructive: false,
}

export const RESUME_OFFERING_INTENT: OfferingStatusIntent = {
    nextStatus: "ACTIVE",
    label: "启用",
    title: "启用供给关系",
    submitLabel: "启用并保存新版本",
    defaultReason: "启用供给关系",
    actionId: "enable",
    destructive: false,
}

export const STOP_OFFERING_INTENT: OfferingStatusIntent = {
    nextStatus: "STOPPED",
    label: "停止",
    title: "停止供给关系",
    submitLabel: "停止并保存新版本",
    defaultReason: "停止供给关系",
    actionId: "stop",
    destructive: true,
}

/** 按当前关系状态给出行上可执行的状态修订。 */
export function statusIntentsFor(
    status: OfferingStatus,
): readonly OfferingStatusIntent[] {
    if (status === "ACTIVE") {
        return [PAUSE_OFFERING_INTENT, STOP_OFFERING_INTENT]
    }
    if (status === "PAUSED") {
        return [RESUME_OFFERING_INTENT, STOP_OFFERING_INTENT]
    }
    return [RESUME_OFFERING_INTENT]
}

/** 条款缺失或看不到价格时，不能把当前条款原样写入新版本。 */
export function statusRevisionBlocker(
    offering: SupplierOfferingView,
): string | null {
    if (
        offering.current_revision_no == null ||
        offering.current_revision_no < 1
    ) {
        return "当前没有可修订的条款版本，请先修订条款。"
    }
    if (
        !offering.dropship_supply_price_gross?.trim() ||
        !offering.bulk_supply_price_gross?.trim() ||
        !offering.input_tax_rate?.trim() ||
        !offering.bulk_minimum_order_quantity?.trim() ||
        offering.supply_region.length === 0 ||
        !offering.valid_from?.trim()
    ) {
        return "当前条款不完整或看不到价格，请先修订条款。"
    }
    return null
}
