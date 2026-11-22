"use client"

import { LoadingButton } from "@/components/ui/loading-button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    useDepositBankReceipt,
    useDepositResolutionEvidence,
} from "../hooks/queries"

/** 使用受控读取入口交付核实凭证，错误保留在原对账记录旁。 */
export function DepositResolutionEvidence({
    reconciliationId,
    assetId,
}: {
    reconciliationId: string
    assetId: string
}) {
    const download = useDepositResolutionEvidence(assetId)
    return (
        <span className="inline-flex flex-col items-start gap-1">
            <LoadingButton
                id={`supplier-deposits-resolution-evidence-${toAutomationIdSegment(reconciliationId)}`}
                type="button"
                variant="link"
                size="sm"
                className="h-auto p-0"
                loading={download.isPending}
                disabled={download.isPending}
                onClick={() => download.mutate()}
            >
                下载核实凭证
            </LoadingButton>
            {download.isError ? (
                <span role="alert" className="text-xs text-destructive">
                    凭证读取失败，请确认文件查看权限后重试。
                </span>
            ) : null}
        </span>
    )
}

/** 受控读取充值银行付款回单（预览 / 下载）。 */
export function DepositBankReceiptAction({
    recordId,
    assetId,
    idPrefix = "supplier-deposits-bank-receipt",
}: {
    recordId: string
    assetId: string
    idPrefix?: string
}) {
    const segment = toAutomationIdSegment(recordId)
    const { download, preview } = useDepositBankReceipt(assetId)
    const busy = download.isPending || preview.isPending
    const hasError = download.isError || preview.isError

    return (
        <span className="inline-flex flex-col items-start gap-1">
            <span className="inline-flex flex-wrap items-center gap-2">
                <LoadingButton
                    id={`${idPrefix}-preview-${segment}`}
                    type="button"
                    variant="link"
                    size="sm"
                    className="h-auto p-0 text-xs"
                    loading={preview.isPending}
                    disabled={busy}
                    onClick={() => preview.mutate()}
                >
                    预览银行回单
                </LoadingButton>
                <span
                    aria-hidden="true"
                    className="text-xs text-muted-foreground/50"
                >
                    ·
                </span>
                <LoadingButton
                    id={`${idPrefix}-download-${segment}`}
                    type="button"
                    variant="link"
                    size="sm"
                    className="h-auto p-0 text-xs"
                    loading={download.isPending}
                    disabled={busy}
                    onClick={() => download.mutate()}
                >
                    下载
                </LoadingButton>
            </span>
            {hasError ? (
                <span role="alert" className="text-xs text-destructive">
                    银行回单读取失败，请确认文件查看权限后重试。
                </span>
            ) : null}
        </span>
    )
}
