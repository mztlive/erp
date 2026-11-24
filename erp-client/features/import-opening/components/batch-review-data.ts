import { useQuery } from "@tanstack/react-query"

import type { BackendRow } from "@/features/import-opening/api/mapping/dto"
import { importOpeningKeys } from "@/features/import-opening/hooks/queries"
import {
    ISSUE_CODE_LABEL,
    OBJECT_CODE_LABEL,
    type ConfirmationScope,
    type ImportBatchView,
    type ImportIssueCode,
    type ImportObjectCode,
} from "@/features/import-opening/types"
import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

const rowPageSize = 100

// 与后端 W18 OBJECT_TYPE_SCOPE_TABLE 的来源对象口径一致。
const objectScopes: Record<ImportObjectCode, ConfirmationScope> = {
    CUSTOMER: "SALES",
    CONTRACT: "SALES",
    CARD_SALES_ORDER: "SALES",
    SUPPLIER: "PROCUREMENT",
    SKU: "PROCUREMENT",
    CARD_CATEGORY: "OPERATIONS",
    WAREHOUSE: "WAREHOUSE",
    OPENING_STOCK: "WAREHOUSE",
    CARD_OPENING_AR: "FINANCE",
}

const objectAliases = new Map<string, ImportObjectCode>([
    ["客户", "CUSTOMER"],
    ["合同", "CONTRACT"],
    ["卡券销售", "CARD_SALES_ORDER"],
    ["卡券销售单", "CARD_SALES_ORDER"],
    ["供应商", "SUPPLIER"],
    ["商品SKU", "SKU"],
    ["商品 SKU", "SKU"],
    ["卡券类目", "CARD_CATEGORY"],
    ["仓库", "WAREHOUSE"],
    ["期初库存", "OPENING_STOCK"],
    ["卡券期初应收", "CARD_OPENING_AR"],
    ["期初应收", "CARD_OPENING_AR"],
])

const issueCodes = Object.keys(ISSUE_CODE_LABEL) as ImportIssueCode[]

export type ReviewRow = Readonly<{
    id: string
    sourceKey: string
    objectLabel: string
    scope?: ConfirmationScope
    result: "valid" | "warning" | "invalid"
    problems: string[]
}>

function reviewRow(row: BackendRow): ReviewRow {
    const object = row.source_object_type.trim().toUpperCase()
    const code = Object.hasOwn(objectScopes, object)
        ? (object as ImportObjectCode)
        : objectAliases.get(object)
    const invalid =
        row.parse_status === "invalid" ||
        row.mapping_status === "conflict" ||
        row.import_status === "failed"
    const problems = [
        row.parse_status === "invalid" ? "来源数据解析无效" : null,
        row.mapping_status === "conflict" ? "业务对象映射冲突" : null,
        row.import_status === "failed" ? "导入失败" : null,
        row.parse_status === "pending_parse" ? "待解析" : null,
        row.mapping_status === "pending_mapping" ? "待映射" : null,
        row.import_status === "skipped" ? "已跳过" : null,
    ].filter((value): value is string => value !== null)

    const errorCode = row.error_code?.toUpperCase()
    const issue =
        errorCode && issueCodes.find((value) => errorCode.includes(value))
    if (issue) problems.push(ISSUE_CODE_LABEL[issue])

    return {
        id: row.id,
        sourceKey: row.source_row_key,
        objectLabel: code ? OBJECT_CODE_LABEL[code] : "其他数据",
        scope: code ? objectScopes[code] : undefined,
        result: invalid ? "invalid" : problems.length ? "warning" : "valid",
        problems,
    }
}

export function useBatchReviewQuery(batch: ImportBatchView, enabled: boolean) {
    return useQuery({
        queryKey: [
            ...importOpeningKeys.all,
            "issues",
            "confirmation-review",
            batch.batchId,
            batch.version,
            batch.trialVersion,
        ],
        enabled,
        queryFn: async ({ signal }) => {
            // 行接口按单一状态分页。读取完整投影后统一统计和筛选，
            // 避免状态重叠重复计数，以及只在已加载页面内搜索。
            const fetchPage = (page: number) =>
                apiGet<Page<BackendRow>>(
                    `/admin/legacy-import-batches/${encodeURIComponent(batch.batchId)}/rows`,
                    {
                        page,
                        page_size: rowPageSize,
                        sort_by: "source_row_key",
                        sort_dir: "asc",
                    },
                    { signal },
                )
            const firstPage = await fetchPage(1)
            const rows = firstPage.items.map(reviewRow)
            const pageCount = Math.ceil(firstPage.total / firstPage.page_size)
            for (let page = 2; page <= pageCount; page++) {
                const nextPage = await fetchPage(page)
                rows.push(...nextPage.items.map(reviewRow))
            }

            const uniqueRows = new Set(rows.map((row) => row.id))
            if (
                rows.length !== firstPage.total ||
                uniqueRows.size !== rows.length
            ) {
                throw new Error("批次明细已变化，请重新加载。")
            }
            const totals = { valid: 0, warning: 0, invalid: 0 }
            for (const row of rows) totals[row.result]++
            return { rows, totals }
        },
    })
}
