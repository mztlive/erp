import { commandFailureDisposition } from "@/lib/api/command-recovery"
import type { PortalOffering, PortalTerms } from "../types"
export const relationLabels: Record<string, string> = {
    ACTIVE: "合作中",
    PAUSED: "采购已暂停",
    STOPPED: "停止供应",
}
export const availabilityLabels: Record<string, string> = {
    AVAILABLE: "有货",
    OUT_OF_STOCK: "临时缺货",
    UNAVAILABLE: "临时缺货",
    STOPPED: "停止供应",
    STALE: "待重新核对",
}
export const statusLabels: Record<string, string> = {
    draft: "草稿",
    pending: "待采购确认",
    returned: "已退回",
    withdrawn: "已撤回",
    effective: "已生效",
}
export const kindLabels: Record<string, string> = {
    quote: "已有商品报价",
    terms: "供给条款变更",
    stop: "停止供应申请",
    cooperation: "付款条件变更",
    new_product: "新品提报",
}
export const commandKey = (purpose: string) =>
    `supplier-portal:${purpose}:${crypto.randomUUID()}`
export const offeringName = (offering: PortalOffering) =>
    offering.name ?? offering.sku_name ?? "商品规格"
export function offeringTerms(offering: PortalOffering): PortalTerms {
    return (
        offering.terms ?? {
            dropship_supply_price_gross:
                offering.dropship_supply_price_gross ?? "",
            bulk_supply_price_gross: offering.bulk_supply_price_gross ?? "",
            input_tax_rate: offering.input_tax_rate ?? "",
            bulk_minimum_order_quantity:
                offering.bulk_minimum_order_quantity ?? "",
            supply_region: offering.supply_region ?? [],
            product_capabilities: offering.product_capabilities ?? [],
            valid_from: offering.valid_from ?? "",
            valid_to: offering.valid_to ?? null,
            freight_amount: offering.freight_amount ?? null,
            service_fee_amount: offering.service_fee_amount ?? null,
            dropship_express: offering.dropship_express ?? null,
        }
    )
}
export function timeLabel(value?: number | string) {
    if (value == null) return "未报送"
    const date = new Date(typeof value === "number" ? value * 1000 : value)
    return Number.isNaN(date.getTime())
        ? "未报送"
        : date.toLocaleString("zh-CN")
}

/** 输入被明确拒绝时可修正；未知结果继续保留原请求。 */
export const isRejectedPortalCommand = (error: unknown) =>
    commandFailureDisposition(error) === "rejected"
