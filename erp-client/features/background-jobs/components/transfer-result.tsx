"use client"
import { useMutation } from "@tanstack/react-query"
import { Button } from "@/components/ui/button"
import { readTransferResult, downloadTransfer } from "@/lib/data-transfer"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function TransferResult({
    jobId,
    portal = false,
    label = "下载处理结果",
    disabled = false,
    idPrefix = "background-jobs-download-transfer",
}: {
    jobId: string
    portal?: boolean
    label?: string
    disabled?: boolean
    idPrefix?: string
}) {
    const mutation = useMutation({
        meta: { suppressErrorToast: true },
        mutationFn: async () =>
            downloadTransfer(await readTransferResult(jobId, portal)),
        retry: false,
    })
    return (
        <div className="space-y-2">
            <Button
                id={`${idPrefix}-${toAutomationIdSegment(jobId)}`}
                disabled={disabled || mutation.isPending}
                onClick={() => mutation.mutate()}
            >
                {mutation.isPending ? "准备下载…" : label}
            </Button>
            {mutation.isError && (
                <p className="text-destructive">
                    {getErrorMessage(mutation.error, "下载失败，请重试")}
                </p>
            )}
        </div>
    )
}
