"use client"

import { LoadingButton } from "@/components/ui/loading-button"
import { getErrorMessage } from "@/lib/api/errors"

/** 原提交人下载已结束任务的待处理行。 */
export function ImportFailuresDownload({
    id,
    pending,
    failed,
    error,
    onDownload,
}: {
    id: string
    pending: boolean
    failed: boolean
    error: unknown
    onDownload: () => void
}) {
    return (
        <div className="space-y-2">
            <LoadingButton
                id={id}
                variant="outline"
                disabled={pending}
                loading={pending}
                onClick={onDownload}
            >
                {pending ? "准备下载…" : "下载待处理行"}
            </LoadingButton>
            {failed ? (
                <p role="alert" className="text-body-compact text-destructive">
                    {getErrorMessage(error, "下载失败，请重试")}
                </p>
            ) : null}
        </div>
    )
}
