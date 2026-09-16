"use client"

import { buildListCsv, collectExportPages } from "@/lib/list-export"
import { isScopeChangedError } from "@/lib/funds-scope"
import type {
    ReceivableScopeListView,
    ReceivableScopeQuery,
    ScopedCustomerReceiptWire,
    ScopedInvoiceWire,
    ScopedReceivableAccountWire,
} from "@/features/customer-receivables/api/scoped"
import { fetchReceivableScopeList } from "@/features/customer-receivables/api/scoped-view"

/** CSV 公式注入防护：沿用列表导出统一规则，危险前缀加单引号。 */
function csvRow(values: readonly (string | null | undefined)[]): string[] {
    return values.map((value) => value ?? "")
}

/** 范围导出：逐页收集并绑定首个范围版本，最后一页后重验，失败不输出部分文件。 */
export async function exportReceivableScopeCsv(
    query: ReceivableScopeQuery,
): Promise<{ fileName: string; content: string; rowCount: number }> {
    let scopeVersion: string | undefined
    type ScopeRow =
        | ScopedReceivableAccountWire
        | ScopedCustomerReceiptWire
        | ScopedInvoiceWire
    function pickItems(
        result: ReceivableScopeListView,
        view: ReceivableScopeQuery["view"],
    ): readonly ScopeRow[] {
        if (view === "receivable") return result.receivables
        if (view === "receipt") return result.receipts
        return result.invoices
    }
    const rows = await collectExportPages<ScopeRow>(
        async (
            page,
            pageSize,
        ): Promise<{ items: readonly ScopeRow[]; total: number }> => {
            const result: ReceivableScopeListView =
                await fetchReceivableScopeList({
                    ...query,
                    page,
                    pageSize,
                    scopeVersion,
                })
            scopeVersion = result.scopeVersion
            return { items: pickItems(result, query.view), total: result.total }
        },
        (row) => row.id,
    )
    if (!scopeVersion) throw new Error("范围版本缺失，请重新查询后导出")
    // 最后一页之后、生成文件前再次查询校验；范围变化时拒绝输出旧文件。
    const verify = await fetchReceivableScopeList({
        ...query,
        page: 1,
        pageSize: 1,
        scopeVersion,
    })
    if (verify.scopeVersion !== scopeVersion || isScopeChangedError(verify)) {
        throw new Error("数据范围已变化，请重新查询后导出")
    }
    const header =
        query.view === "receivable"
            ? [
                  "子账",
                  "销售单",
                  "获授权已核销",
                  "整单金额",
                  "整单已核销",
                  "未分配",
                  "状态",
                  "范围版本",
              ]
            : query.view === "receipt"
              ? [
                    "回款单号",
                    "获授权已分配",
                    "整单金额",
                    "整单已分配",
                    "未分配",
                    "状态",
                    "范围版本",
                ]
              : [
                    "发票号码",
                    "获授权已分配",
                    "整单金额",
                    "整单已分配",
                    "未分配",
                    "状态",
                    "范围版本",
                ]
    const body = rows.map((row) => {
        if ("account_seq" in row) {
            return csvRow([
                String(row.account_seq),
                row.sales_order_id,
                row.visible_settled_share,
                row.gross_total,
                row.settled_total,
                row.open_total,
                row.status,
                scopeVersion,
            ])
        }
        if ("receipt_no" in row) {
            return csvRow([
                row.receipt_no,
                row.visible_allocated_share,
                row.amount,
                row.allocated_total,
                row.unallocated_amount,
                row.status,
                scopeVersion,
            ])
        }
        return csvRow([
            row.invoice_no,
            row.visible_allocated_share,
            row.gross_amount,
            row.allocated_total,
            row.unallocated_amount,
            row.status,
            scopeVersion,
        ])
    })
    const content = buildListCsv([header, ...body])
    const date = new Date().toISOString().slice(0, 10)
    const viewLabel =
        query.view === "receivable"
            ? "应收"
            : query.view === "receipt"
              ? "回款"
              : "销项发票"
    return {
        fileName: `客户往来-${viewLabel}-范围-${date}.csv`,
        content,
        rowCount: rows.length,
    }
}
