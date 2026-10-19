import {
    compactFixed,
    compareDecimal,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"

/** 开单与改单共用的分地址进度：数量合计与收货资料是否齐全。 */
export function allocationSummary(line: {
    deliveryAllocations?: {
        recipient: string
        phone: string
        region_code: string
        address: string
        quantity: string
    }[]
    quantity: string
    unit: string
}) {
    const rows = line.deliveryAllocations ?? []
    if (!rows.length)
        return {
            summary: "未指定，可在履约时填写",
            status: "",
            complete: false,
        }
    const detailsComplete = rows.every(
        (row) =>
            row.recipient.trim() &&
            row.recipient.trim().length <= 64 &&
            row.phone.trim() &&
            row.phone.trim().length <= 32 &&
            row.region_code &&
            row.address.trim() &&
            row.address.trim().length <= 512,
    )
    try {
        const options = { maxScale: 6, outputScale: 6 }
        const total = sumFixed(
            rows.map((row) => row.quantity || "0"),
            options,
        )
        const comparison = compareDecimal(total, line.quantity, 6)
        const summary = `已分配 ${compactFixed(total)} / ${compactFixed(line.quantity)} ${line.unit}`
        if (
            rows.some((row) => compareDecimal(row.quantity || "0", "0", 6) <= 0)
        ) {
            return {
                summary,
                status: "请填写大于 0 的分配数量",
                complete: false,
            }
        }
        if (comparison !== 0) {
            const difference =
                comparison < 0
                    ? subtractFixed(line.quantity, total, options)
                    : subtractFixed(total, line.quantity, options)
            return {
                summary,
                status: `${comparison < 0 ? "还差" : "超出"} ${compactFixed(difference)} ${line.unit}`,
                complete: false,
            }
        }
        return {
            summary,
            status: detailsComplete ? "已填写完整" : "待填写收货资料",
            complete: detailsComplete,
        }
    } catch {
        return {
            summary: "数量待核对",
            status: "请填写有效数量（最多 6 位小数）",
            complete: false,
        }
    }
}
