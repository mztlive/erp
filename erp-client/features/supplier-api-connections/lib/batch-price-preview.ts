import { formatScaled, parseDecimal } from "@/lib/fixed-decimal"
import type {
    BatchWorkspaceDraft,
    RuleMethod,
} from "../hooks/use-batch-workspace"
import type { BatchRow } from "../api/batch"

/** 仅供规则输入时展示样例；最终价格以服务端核对结果为准。 */
function amount(
    method: RuleMethod,
    input: string,
    source: string | null,
): string | null {
    if (method === "UNSET") return null
    const parameter = parseDecimal(input, {
        maxScale: method === "PERCENTAGE" ? 4 : 2,
    })
    let numerator = parameter.unscaled
    let scale = parameter.scale
    if (method !== "FIXED") {
        if (source == null) return null
        const base = parseDecimal(source, { maxScale: 4 })
        if (method === "ADD") {
            scale = Math.max(base.scale, parameter.scale)
            numerator =
                base.unscaled * BigInt(10) ** BigInt(scale - base.scale) +
                parameter.unscaled *
                    BigInt(10) ** BigInt(scale - parameter.scale)
        } else {
            numerator =
                base.unscaled *
                (BigInt(100) * BigInt(10) ** BigInt(parameter.scale) +
                    parameter.unscaled)
            scale = base.scale + parameter.scale + 2
        }
    }
    if (scale <= 2)
        return formatScaled(numerator * BigInt(10) ** BigInt(2 - scale), 2)
    const divisor = BigInt(10) ** BigInt(scale - 2)
    return formatScaled(
        numerator / divisor +
            ((numerator % divisor) * BigInt(2) >= divisor
                ? BigInt(1)
                : BigInt(0)),
        2,
    )
}
export function batchPriceSample(row: BatchRow, draft: BatchWorkspaceDraft) {
    if (
        row.resolution?.kind === "EXISTING" ||
        Boolean(row.source.sku_id) ||
        row.sales_prices.dropship.origin === "PRESERVED"
    )
        return {
            dropship: row.sales_prices.dropship.value,
            bulk: row.sales_prices.bulk.value,
            preserved: true,
            error: "",
        }
    try {
        const allows = (field: BatchRow["sales_prices"]["dropship"]) =>
            field.origin !== "PRESERVED" &&
            (field.origin === "UNSET" ||
                (draft.overwrite &&
                    (field.origin === "RULE" ||
                        field.origin === "SOURCE" ||
                        !draft.preserveManual)))
        const computed = amount(
            draft.dropshipMethod,
            draft.dropshipValue,
            row.source.purchase_price,
        )
        const dropship =
            computed != null && allows(row.sales_prices.dropship)
                ? computed
                : row.sales_prices.dropship.value
        let bulk = row.sales_prices.bulk.value
        if (
            draft.mode === "SAME" &&
            (allows(row.sales_prices.bulk) || row.sales_prices.bulk_linked)
        )
            bulk = dropship
        else if (draft.mode === "SEPARATE" && allows(row.sales_prices.bulk))
            bulk =
                amount(
                    draft.bulkMethod,
                    draft.bulkValue,
                    row.source.purchase_price,
                ) ?? bulk
        else if (
            draft.mode === "DROPSHIP_ONLY" &&
            draft.dropshipBulkAction === "CLEAR" &&
            row.sales_prices.bulk.origin !== "PRESERVED" &&
            (!draft.preserveManual ||
                !["MANUAL", "CLEARED"].includes(row.sales_prices.bulk.origin))
        )
            bulk = null
        return {
            dropship,
            bulk,
            preserved: false,
            error:
                row.source.purchase_price == null
                    ? "缺少接口采购报价，补齐来源报价后才能入库"
                    : "",
        }
    } catch {
        return {
            dropship: null,
            bulk: null,
            preserved: false,
            error: "填写计算参数后预览",
        }
    }
}
