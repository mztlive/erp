import { apiGet, apiPost } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type {
    BatchInput,
    BatchMode,
    BatchResult,
    BatchRow,
} from "../lib/batch-supply"
import { newBatchRow } from "../lib/batch-supply"
import type { ImportRow } from "../lib/batch-supply-import"
import { AVAILABILITY_STATUS_LABELS } from "../types"
import { SupplyBatchValidationError } from "../lib/batch-supply-error"

export function submitSupplyBatch(
    mode: BatchMode,
    rows: { row_id: string; input: BatchInput }[],
    validateOnly: boolean,
) {
    return apiPost<BatchResult>(`/admin/supplier-offerings/batch/${mode}`, {
        rows,
        validate_only: validateOnly,
    })
}
/** 按完整公司 SKU 编号匹配授权目录，绝不按名称猜测或创建商品。 */
export async function resolveSupplyImport(
    rows: ImportRow[],
): Promise<BatchRow[]> {
    const result: BatchRow[] = []
    const seen = new Set<string>()
    for (const source of rows) {
        const code = source.skuCode?.trim() ?? ""
        if (!code) throw new SupplyBatchValidationError("公司 SKU 编号不能为空")
        if (seen.has(code))
            throw new SupplyBatchValidationError(
                `公司 SKU「${code}」在文件中重复，请合并后导入`,
            )
        seen.add(code)
        const page = await apiGet<
            Page<{
                id: string
                sku_no: string
                name?: string
                specification_signature: string
            }>
        >("/admin/skus", {
            sku_no: code,
            status: "active",
            page: 1,
            page_size: 100,
        })
        const exact = page.items.filter((item) => item.sku_no === code)
        if (exact.length !== 1)
            throw new SupplyBatchValidationError(
                `公司 SKU「${code}」不存在、无权选择或匹配不唯一，请先维护商品资料`,
            )
        const item = exact[0]
        const row = newBatchRow({
            skuId: item.id,
            skuCode: item.sku_no,
            skuName: item.name ?? item.specification_signature,
            specification: item.specification_signature,
            baseUnit: "",
        })
        const status = Object.entries(AVAILABILITY_STATUS_LABELS).find(
            ([, label]) => label === source.availabilityStatus,
        )?.[0]
        if (source.availabilityStatus && !status)
            throw new SupplyBatchValidationError(
                `SKU「${code}」的可供状态无法识别，请使用模板中的中文状态`,
            )
        result.push({
            ...row,
            ...source,
            skuCode: code,
            availabilityStatus: (status ??
                "AVAILABLE") as BatchRow["availabilityStatus"],
            validFrom: source.validFrom ?? "",
            validityMode: source.validTo ? "dated" : "ongoing",
            quantityMode: source.availableQuantity ? "provided" : "unknown",
            source: "EXCEL",
        })
    }
    return result
}
