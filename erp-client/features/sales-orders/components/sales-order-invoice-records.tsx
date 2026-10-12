"use client"

import type { ReactNode } from "react"
import { BusinessFailureState } from "@/components/business"
import { DetailRecordSection } from "@/components/business/detail-presentation"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { FinancialFileList } from "@/features/file-assets/components/financial-file-list"
import { useSalesInvoiceFiles } from "@/features/file-assets/hooks/business-finance-files"
import type { FinancialFile } from "@/features/file-assets/api/business-finance-files"
import type { useSalesOrderReceivable } from "../hooks/use-sales-order-receivable"
import type { amountAllocatedToTargets } from "../lib/sales-order-receivable"

/** 文件沿用销售单授权；财务台账失败、分页或无权限均不阻断本单文件下载。 */
export function SalesOrderInvoiceRecords({
    salesOrderId,
    state,
    rowAmount,
    previewButton,
}: {
    salesOrderId: string
    state: ReturnType<typeof useSalesOrderReceivable>
    rowAmount: (
        allocations: Parameters<typeof amountAllocatedToTargets>[0],
    ) => ReactNode
    previewButton: (id: string, label: string) => ReactNode
}) {
    const { query, download } = useSalesInvoiceFiles(salesOrderId)
    const allowed = state.canRead("invoice:list")
    const records =
        allowed && state.invoices.isSuccess ? state.invoices.data.items : []
    const filesByInvoice = new Map<string, FinancialFile[]>()
    for (const file of query.isSuccess ? query.data : []) {
        const group = filesByInvoice.get(file.document_id) ?? []
        group.push(file)
        filesByInvoice.set(file.document_id, group)
    }
    const recordIds = new Set(records.map((row) => row.invoiceId))
    const remainingFiles = [...filesByInvoice].filter(
        ([id]) => !recordIds.has(id),
    )
    const fileList = (files: FinancialFile[]) => (
        <FinancialFileList
            files={files}
            idPrefix="sales-order-invoice-file"
            onDownload={(file) => download.mutate(file)}
            pendingAssetId={
                download.isPending
                    ? download.variables?.file_asset_id
                    : undefined
            }
            showDocumentNumber={false}
        />
    )
    const pending =
        state.profile.isPending ||
        (allowed && state.invoices.isPending) ||
        query.isPending
    return (
        <DetailRecordSection
            title="已开发票"
            count={
                allowed && state.invoices.isSuccess
                    ? state.invoices.data.total
                    : undefined
            }
        >
            {pending ? (
                <p role="status" className="text-sm text-muted-foreground">
                    正在读取发票…
                </p>
            ) : null}
            {allowed && state.invoices.isError ? (
                <BusinessFailureState
                    title="发票明细读取失败"
                    error={state.invoices.error}
                    onRetry={() => void state.invoices.refetch()}
                />
            ) : null}
            {query.isError ? (
                <BusinessFailureState
                    title="发票文件读取失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : null}
            <div className="divide-y divide-border/70">
                {records.map((row) => (
                    <article
                        key={row.invoiceId}
                        className="py-4 first:pt-0 last:pb-0"
                    >
                        <div className="flex flex-wrap items-start justify-between gap-3">
                            <div className="min-w-0">
                                <div className="flex flex-wrap items-center gap-2">
                                    <h3 className="text-sm font-medium wrap-anywhere">
                                        {row.invoiceNo}
                                    </h3>
                                    <span className="text-xs text-muted-foreground">
                                        {row.invoiceKindLabel}
                                    </span>
                                    <StatusBadge
                                        label={row.statusLabel}
                                        tone={row.statusTone}
                                    />
                                </div>
                                {row.invoiceDate && row.invoiceDate !== "—" ? (
                                    <p className="mt-1 text-xs text-muted-foreground">
                                        开票日期 {row.invoiceDate}
                                    </p>
                                ) : null}
                            </div>
                            <div className="flex items-center gap-4">
                                <dl className="text-right">
                                    <dt className="text-xs text-muted-foreground">
                                        本单开票金额
                                    </dt>
                                    <dd className="mt-1 text-lg">
                                        {rowAmount(row.allocations)}
                                    </dd>
                                </dl>
                                {previewButton(row.invoiceId, row.invoiceNo)}
                            </div>
                        </div>
                        {filesByInvoice.has(row.invoiceId) ? (
                            fileList(filesByInvoice.get(row.invoiceId)!)
                        ) : query.isSuccess ? (
                            <p className="mt-3 text-xs text-muted-foreground">
                                未上传发票文件
                            </p>
                        ) : null}
                    </article>
                ))}
                {remainingFiles.map(([id, files]) => (
                    <article key={id} className="py-4 first:pt-0 last:pb-0">
                        <h3 className="text-sm font-medium wrap-anywhere">
                            {files[0]!.document_no || "发票附件"}
                        </h3>
                        {fileList(files)}
                    </article>
                ))}
            </div>
            {!pending &&
            query.isSuccess &&
            !records.length &&
            !remainingFiles.length ? (
                <p className="text-sm text-muted-foreground">
                    {allowed && state.invoices.isSuccess
                        ? "暂无已开发票"
                        : "暂无可下载的发票文件"}
                </p>
            ) : null}
            {allowed &&
            state.invoices.data &&
            (state.invoices.data.total > 20 || state.invoicePage > 1) ? (
                <div className="flex flex-wrap items-center justify-end gap-3 text-xs text-muted-foreground">
                    <span>
                        发票明细第 {state.invoicePage} 页 · 共{" "}
                        {state.invoices.data.total} 笔
                    </span>
                    <Button
                        id="sales-order-invoices-previous"
                        variant="outline"
                        size="sm"
                        disabled={state.invoicePage === 1}
                        onClick={() =>
                            state.setInvoicePage(state.invoicePage - 1)
                        }
                    >
                        上一页
                    </Button>
                    <Button
                        id="sales-order-invoices-next"
                        variant="outline"
                        size="sm"
                        disabled={
                            state.invoicePage * 20 >= state.invoices.data.total
                        }
                        onClick={() =>
                            state.setInvoicePage(state.invoicePage + 1)
                        }
                    >
                        下一页
                    </Button>
                </div>
            ) : null}
        </DetailRecordSection>
    )
}
