"use client"

import Image from "next/image"
import { useEffect, useState } from "react"
import { DownloadIcon, EyeIcon } from "lucide-react"
import { BusinessFailureState, MoneyValue } from "@/components/business"
import { PurchaseOrderDetailSection as DetailRecordSection } from "./purchase-order-detail-section"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import type { FinancialFile } from "@/features/file-assets/api/business-finance-files"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import {
    usePurchasePaymentReceipt,
    usePurchasePayments,
} from "../hooks/payment-queries"

/** 采购详情直接展示本单有效付款核销，回单沿采购权限独立读取。 */
export function PurchaseOrderPayments({
    purchaseOrderId,
    costMasked,
}: {
    purchaseOrderId: string
    costMasked: boolean
}) {
    const { query, download } = usePurchasePayments(purchaseOrderId)
    const [previewId, setPreviewId] = useState<string | null>(null)
    return (
        <DetailRecordSection title="财务付款单" count={query.data?.length}>
            <p className="text-xs text-muted-foreground">
                显示已核销到本采购单的付款；合并付款仅列出本单核销金额。
            </p>
            {query.isPending ? (
                <p role="status" className="text-sm text-muted-foreground">
                    正在加载付款单…
                </p>
            ) : query.isError ? (
                <BusinessFailureState
                    title="付款单加载失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : query.data.length ? (
                <ul className="divide-y divide-border/70">
                    {query.data.map((payment) => {
                        const id = `purchase-order-payment-${toAutomationIdSegment(purchaseOrderId)}-${toAutomationIdSegment(payment.payment_id)}`
                        const file = payment.receipt
                        const expanded = previewId === payment.payment_id
                        return (
                            <li
                                key={payment.payment_id}
                                className="space-y-4 py-4 first:pt-0"
                            >
                                <div className="flex flex-wrap items-start justify-between gap-4">
                                    <div className="min-w-0 space-y-1">
                                        <p className="num break-all text-sm font-medium">
                                            {payment.payment_no}
                                        </p>
                                        <p className="text-xs text-muted-foreground">
                                            付款时间：
                                            {formatDateTime(
                                                new Date(
                                                    payment.paid_at * 1000,
                                                ).toISOString(),
                                                "full",
                                            )}
                                        </p>
                                    </div>
                                    <div className="text-right">
                                        <p className="text-xs text-muted-foreground">
                                            本单核销金额
                                        </p>
                                        <p className="mt-1 text-lg font-semibold">
                                            {costMasked ? (
                                                "•••"
                                            ) : (
                                                <MoneyValue
                                                    value={
                                                        payment.allocated_amount
                                                    }
                                                />
                                            )}
                                        </p>
                                    </div>
                                </div>
                                {file ? (
                                    <div className="flex flex-wrap items-center justify-between gap-3">
                                        <p className="min-w-0 break-all text-sm text-muted-foreground">
                                            银行回单：{file.file_name}
                                        </p>
                                        <div className="flex shrink-0 gap-2">
                                            <Button
                                                id={`${id}-preview`}
                                                variant="outline"
                                                size="sm"
                                                aria-expanded={expanded}
                                                aria-controls={`${id}-receipt`}
                                                onClick={() =>
                                                    setPreviewId(
                                                        expanded
                                                            ? null
                                                            : payment.payment_id,
                                                    )
                                                }
                                            >
                                                <EyeIcon />
                                                {expanded
                                                    ? "收起回单"
                                                    : "查看回单"}
                                            </Button>
                                            <LoadingButton
                                                id={`${id}-download`}
                                                variant="outline"
                                                size="sm"
                                                loading={
                                                    download.isPending &&
                                                    download.variables
                                                        ?.document_id ===
                                                        payment.payment_id
                                                }
                                                disabled={download.isPending}
                                                onClick={() =>
                                                    download.mutate(file)
                                                }
                                            >
                                                <DownloadIcon />
                                                下载
                                            </LoadingButton>
                                        </div>
                                    </div>
                                ) : (
                                    <p className="text-sm text-muted-foreground">
                                        暂无可查看的银行回单
                                    </p>
                                )}
                                {file && expanded ? (
                                    <div id={`${id}-receipt`}>
                                        <ReceiptPreview
                                            purchaseOrderId={purchaseOrderId}
                                            file={file}
                                        />
                                    </div>
                                ) : null}
                            </li>
                        )
                    })}
                </ul>
            ) : (
                <p className="text-sm text-muted-foreground">
                    暂无已核销到本采购单的付款记录
                </p>
            )}
        </DetailRecordSection>
    )
}

function ReceiptPreview({
    purchaseOrderId,
    file,
}: {
    purchaseOrderId: string
    file: FinancialFile
}) {
    const query = usePurchasePaymentReceipt(purchaseOrderId, file.document_id)
    const [url, setUrl] = useState<string>()
    useEffect(() => {
        if (!query.data) return
        const next = URL.createObjectURL(query.data)
        setUrl(next)
        return () => URL.revokeObjectURL(next)
    }, [query.data])
    if (query.isError)
        return (
            <BusinessFailureState
                title="回单加载失败"
                error={query.error}
                onRetry={() => void query.refetch()}
            />
        )
    if (!url)
        return (
            <p role="status" className="text-sm text-muted-foreground">
                正在加载回单…
            </p>
        )
    return file.content_type === "application/pdf" ? (
        <iframe
            id={`purchase-payment-pdf-${toAutomationIdSegment(file.document_id)}`}
            title={file.file_name}
            src={url}
            className="h-[32rem] w-full rounded-lg border"
        />
    ) : (
        <Image
            unoptimized
            width={1200}
            height={900}
            src={url}
            alt={`银行回单 ${file.file_name}`}
            className="h-auto max-h-[32rem] w-auto max-w-full rounded-lg border object-contain"
        />
    )
}
