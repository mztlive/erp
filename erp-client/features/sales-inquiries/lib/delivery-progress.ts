import {
    compareDecimal,
    formatFixedDisplay,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import type { InquiryAllocation, InquiryLine } from "../types"

/** Draft inputs can be incomplete; report their state without coercing quantities. */
export function deliveryProgress(line: InquiryLine) {
    if (line.fulfillment_kind !== "physical") {
        return {
            complete: true,
            remaining: "0",
            label:
                line.fulfillment_kind === "service" ? "服务交付" : "虚拟交付",
        }
    }
    try {
        const scale = line.quantity_scale
        const options = { maxScale: scale, outputScale: scale }
        if (
            compareDecimal(line.quantity, "0", scale) <= 0 ||
            line.allocations.some(
                (address) => compareDecimal(address.quantity, "0", scale) <= 0,
            )
        ) {
            return { complete: false, remaining: "0", label: "请填写有效数量" }
        }
        const assigned = sumFixed(
            line.allocations.map((address) => address.quantity),
            options,
        )
        const remaining = subtractFixed(line.quantity, assigned, options)
        const comparison = compareDecimal(remaining, "0", scale)
        const quantity = formatFixedDisplay(remaining, {
            maxScale: scale,
            useGrouping: false,
        })
        if (comparison > 0)
            return {
                complete: false,
                remaining: quantity,
                label: `待分配 ${quantity} ${line.unit_code}`,
            }
        if (comparison < 0)
            return {
                complete: false,
                remaining: "0",
                label: `超出需求 ${quantity.slice(1)} ${line.unit_code}`,
            }
        const missing = line.allocations.some(
            (address) =>
                !address.recipient.trim() ||
                !address.phone.trim() ||
                !address.region_code ||
                !address.address.trim(),
        )
        return {
            complete: !missing,
            remaining: "0",
            label: missing
                ? "数量已分配，待补收件资料"
                : `已分配 · ${line.allocations.length} 个地址`,
        }
    } catch {
        return { complete: false, remaining: "0", label: "请填写有效数量" }
    }
}

/** New addresses take only the remaining quantity; existing allocations stay intact. */
export function newAllocation(line: InquiryLine): InquiryAllocation {
    const remaining = deliveryProgress(line).remaining
    return {
        allocation_id: crypto.randomUUID(),
        recipient: "",
        phone: "",
        region_code: "",
        region_name: "",
        address: "",
        quantity: remaining === "0" ? "" : remaining,
    }
}
