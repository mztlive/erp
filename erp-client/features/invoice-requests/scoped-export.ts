"use client"

import { buildListCsv, collectExportPages } from "@/lib/list-export"
import type { InvoiceRequestScopeQuery } from "@/features/invoice-requests/scoped"
import { fetchInvoiceRequestScopeList } from "@/features/invoice-requests/scoped-view"

/** 范围导出：逐页收集并绑定首个范围版本，最后一页后重验，失败不输出部分文件。 */
export async function exportInvoiceRequestScopeCsv(
    query: InvoiceRequestScopeQuery,
): Promise<{ fileName: string; content: string; rowCount: number }> {
    let scopeVersion: string | undefined
    const rows = await collectExportPages(
        async (page, pageSize) => {
            const result = await fetchInvoiceRequestScopeList({
                ...query,
                page,
                pageSize,
                scopeVersion,
            })
            scopeVersion = result.scopeVersion
            return { items: result.requests, total: result.total }
        },
        (row) => row.id,
    )
    if (!scopeVersion) throw new Error("范围版本缺失，请重新查询后导出")
    const verify = await fetchInvoiceRequestScopeList({
        ...query,
        page: 1,
        pageSize: 1,
        scopeVersion,
    })
    if (verify.scopeVersion !== scopeVersion) {
        throw new Error("数据范围已变化，请重新查询后导出")
    }
    const header = [
        "申请单号",
        "销售单",
        "申请金额",
        "申请人",
        "当前处理人",
        "状态",
        "范围版本",
    ]
    const body = rows.map((row) => [
        row.request_no,
        row.sales_order_no,
        row.amount,
        row.applicant_user_id,
        row.handler_user_id ?? "",
        row.status,
        scopeVersion ?? "",
    ])
    const content = buildListCsv([header, ...body])
    const date = new Date().toISOString().slice(0, 10)
    return {
        fileName: `开票申请-范围-${date}.csv`,
        content,
        rowCount: rows.length,
    }
}
