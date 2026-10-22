"use client"
import { useMutation } from "@tanstack/react-query"
import { Button } from "@/components/ui/button"
import { readTransferResult, downloadTransfer } from "@/lib/data-transfer"
import { getErrorMessage } from "@/lib/api/errors"
export function TransferResult({ jobId }: { jobId: string }) {
    const mutation = useMutation({
        mutationFn: async () =>
            downloadTransfer(await readTransferResult(jobId)),
    })
    return (
        <div className="space-y-2">
            <Button
                id="background-jobs-download-transfer"
                disabled={mutation.isPending}
                onClick={() => mutation.mutate()}
            >
                {mutation.isPending ? "准备下载…" : "下载处理结果"}
            </Button>
            {mutation.isError && (
                <p className="text-destructive">
                    {getErrorMessage(mutation.error, "下载失败，请重试")}
                </p>
            )}
        </div>
    )
}
