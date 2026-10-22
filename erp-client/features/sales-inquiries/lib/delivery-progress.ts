import {
    compareDecimal,
    formatFixedDisplay,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import type { InquiryAllocation, InquiryLine, SalesInquiry } from "../types"

/** 已提交的收件资料不可原地改写，销售占用中的地址不可移除或替换。 */
export function allocationRestrictions(
    inquiry: SalesInquiry | undefined,
    allocationId: string,
) {
    const frozen =
        inquiry?.requirements.some((snapshot) =>
            snapshot.data.lines.some((line) =>
                line.allocations.some(
                    (address) => address.allocation_id === allocationId,
                ),
            ),
        ) ?? false
    const occupied =
        inquiry?.quantity_ledger.some(
            (entry) =>
                entry.state !== "released" &&
                entry.selections.some(
                    (selection) => selection.allocation_id === allocationId,
                ),
        ) ?? false
    return { frozen: frozen || occupied, occupied }
}

/** Draft inputs can be incomplete; report their state without coercing quantities. */
export function deliveryProgress(line: InquiryLine): {
    complete: boolean
    remaining: string
    label: string
    overAllocated?: boolean
    invalidQuantity?: boolean
} {
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
            return {
                complete: false,
                remaining: "0",
                label: "请填写有效数量",
                invalidQuantity: true,
            }
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
            overAllocated: comparison < 0,
            label: missing
                ? "数量已分配，待补收件资料"
                : comparison < 0
                  ? `超出需求 ${quantity.slice(1)} ${line.unit_code}`.trim()
                  : `已分配 · ${line.allocations.length} 个地址`,
        }
    } catch {
        return {
            complete: false,
            remaining: "0",
            label: "请填写有效数量",
            invalidQuantity: true,
        }
    }
}

/** 表格、汇总及详情共用配送提醒；超量仍按原规则视为数量已分配。 */
export function deliveryAttention(line: InquiryLine): string | null {
    const progress = deliveryProgress(line)
    if (progress.complete && !progress.overAllocated) return null
    if (progress.invalidQuantity) return "配送数量无效，请核对需求及分配数量"
    try {
        const options = {
            maxScale: line.quantity_scale,
            outputScale: line.quantity_scale,
        }
        const allocated = sumFixed(
            line.allocations.map((address) => address.quantity),
            options,
        )
        const quantity = (value: string) =>
            `${formatFixedDisplay(value, { maxScale: line.quantity_scale })} ${line.unit_code}`.trim()
        const issues: string[] = []
        if (progress.overAllocated) {
            issues.push(
                `超出 ${quantity(subtractFixed(allocated, line.quantity, options))}`,
            )
        }
        if (!progress.complete) issues.push(progress.label)
        return `配送已分配 ${quantity(allocated)} · ${issues.join(" · ")}`
    } catch {
        return "配送数量无效，请核对需求及分配数量"
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
