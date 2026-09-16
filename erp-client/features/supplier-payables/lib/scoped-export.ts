"use client"

import { buildListCsv, collectExportPages } from "@/lib/list-export"
import type {
    ScopedPayableAccountWire,
    ScopedPurchaseInvoiceAllocationWire,
    ScopedSupplierPaymentWire,
    SupplierScopeListView,
    SupplierScopeQuery,
} from "@/features/supplier-payables/api/scoped"
import { fetchSupplierScopeList } from "@/features/supplier-payables/api/scoped-view"

/** 范围导出：逐页收集并绑定首个范围版本，最后一页后重验，失败不输出部分文件。 */
export async function exportSupplierScopeCsv(
    query: SupplierScopeQuery,
): Promise<{ fileName: string; content: string; rowCount: number }> {
    let scopeVersion: string | undefined
    type ScopeRow =
        | ScopedPayableAccountWire
        | ScopedSupplierPaymentWire
        | ScopedPurchaseInvoiceAllocationWire
    function pickItems(
        result: SupplierScopeListView,
        view: SupplierScopeQuery["view"],
    ): readonly ScopeRow[] {
        if (view === "payable") return result.payables
        if (view === "payment") return result.payments
        return result.allocations
    }
    const rows = await collectExportPages<ScopeRow>(
        async (
            page,
            pageSize,
        ): Promise<{ items: readonly ScopeRow[]; total: number }> => {
            const result = await fetchSupplierScopeList({
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
    const verify = await fetchSupplierScopeList({
        ...query,
        page: 1,
        pageSize: 1,
        scopeVersion,
    })
    if (verify.scopeVersion !== scopeVersion) {
        throw new Error("数据范围已变化，请重新查询后导出")
    }
    const header =
        query.view === "payable"
            ? [
                  "子账",
                  "来源单据",
                  "获授权已核销",
                  "整单金额",
                  "整单已核销",
                  "未分配",
                  "状态",
                  "范围版本",
              ]
            : query.view === "payment"
              ? [
                    "付款单号",
                    "获授权已分配",
                    "整单金额",
                    "整单已分配",
                    "未分配",
                    "状态",
                    "范围版本",
                ]
              : [
                    "分配",
                    "进项发票",
                    "应付子账",
                    "获授权分配",
                    "整单分配",
                    "范围版本",
                ]
    const body = rows.map((row) => {
        if ("source_document_id" in row) {
            return [
                row.id,
                row.source_document_id,
                row.visible_settled_share,
                row.gross_total ?? "",
                row.settled_total ?? "",
                row.open_total ?? "",
                row.status,
                scopeVersion ?? "",
            ]
        }
        if ("payment_no" in row) {
            return [
                row.payment_no,
                row.visible_allocated_share,
                row.amount ?? "",
                row.allocated_total ?? "",
                row.unallocated_amount ?? "",
                row.status,
                scopeVersion ?? "",
            ]
        }
        return [
            row.id,
            row.invoice_no ?? "",
            row.payable_account_id,
            row.visible_allocated_amount,
            row.allocated_gross_amount ?? "",
            scopeVersion ?? "",
        ]
    })
    const content = buildListCsv([header, ...body])
    const date = new Date().toISOString().slice(0, 10)
    const viewLabel =
        query.view === "payable"
            ? "应付"
            : query.view === "payment"
              ? "付款"
              : "进项发票分配"
    return {
        fileName: `供应商往来-${viewLabel}-范围-${date}.csv`,
        content,
        rowCount: rows.length,
    }
}
