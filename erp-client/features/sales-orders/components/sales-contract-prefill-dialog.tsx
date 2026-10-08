"use client"

import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Spinner } from "@/components/ui/spinner"
import { useContractUploadForm } from "@/features/contracts/hooks/use-contract-upload-form"
import { ContractImportTask } from "@/features/contracts/components/contract-import-task"
import { getErrorMessage } from "@/lib/api/errors"
import type { SalesContractPrefill } from "../api/contract-prefill"
import { useSalesContractMatches } from "../hooks/use-sales-contract-matches"
import { SalesContractPrefillReview } from "./sales-contract-prefill-review"

export function SalesContractPrefillDialog({
    open,
    onOpenChange,
    currentCustomerId,
    onApply,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    currentCustomerId: string
    onApply: (value: SalesContractPrefill) => void
}) {
    const state = useContractUploadForm({ open, onOpenChange })
    const task = state.detail.data
    const busy = state.uploadMutation.isPending || state.runMutation.isPending
    const error =
        state.uploadMutation.error ||
        state.runMutation.error ||
        state.detail.error ||
        state.previewMutation.error
    const review =
        task?.status === "review" && task.draft && !task.revision_target
    const matches = useSalesContractMatches(review ? task : undefined)
    const recoveryAvailable =
        task?.status === "processing" &&
        task.recoverable_at != null &&
        state.detail.dataUpdatedAt >= task.recoverable_at * 1000
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="sales-contract-prefill-close"
                className="flex max-h-[calc(100dvh-2rem)] flex-col overflow-hidden sm:max-w-[720px]"
            >
                <DialogHeader className="shrink-0 pr-8">
                    <DialogTitle>上传合同，预填销售单</DialogTitle>
                    <DialogDescription>
                        自动匹配客户与结算主体，预填付款、开票要求和税率。
                    </DialogDescription>
                </DialogHeader>
                <div className="min-h-0 space-y-5 overflow-y-auto">
                    {error ? (
                        <Alert variant="destructive">
                            <AlertTitle>操作未完成</AlertTitle>
                            <AlertDescription>
                                {getErrorMessage(error, "请重试")}
                            </AlertDescription>
                            {state.detail.isError ? (
                                <Button
                                    id="sales-contract-prefill-load-retry"
                                    onClick={() => void state.detail.refetch()}
                                >
                                    重新加载
                                </Button>
                            ) : null}
                        </Alert>
                    ) : null}
                    {!state.selectedId ? (
                        <form
                            id="sales-contract-prefill-upload"
                            onSubmit={(event) => {
                                event.preventDefault()
                                void state.form
                                    .handleSubmit()
                                    .catch(() => undefined)
                            }}
                        >
                            <state.form.AppField name="pdfFile">
                                {(field) => (
                                    <field.PdfUploadField
                                        id="sales-contract-prefill-file"
                                        label="合同 PDF"
                                        disabled={busy}
                                    />
                                )}
                            </state.form.AppField>
                        </form>
                    ) : review && task ? (
                        <>
                            <div className="flex items-center justify-between gap-3 rounded-lg border p-3">
                                <span className="min-w-0 break-all text-sm">
                                    {task.file_name}
                                </span>
                                <Button
                                    id="sales-contract-prefill-preview"
                                    type="button"
                                    variant="outline"
                                    disabled={state.previewMutation.isPending}
                                    onClick={() =>
                                        state.previewMutation.mutate(task.id)
                                    }
                                >
                                    查看原文
                                </Button>
                            </div>
                            {matches.isPending ? (
                                <p
                                    role="status"
                                    className="flex items-center gap-2 py-6 text-sm"
                                >
                                    <Spinner />
                                    正在匹配系统客户与结算主体…
                                </p>
                            ) : matches.data ? (
                                <SalesContractPrefillReview
                                    key={`${task.id}-${task.version}`}
                                    task={task}
                                    matches={matches.data}
                                    currentCustomerId={currentCustomerId}
                                    onApply={(value) => {
                                        onApply(value)
                                        onOpenChange(false)
                                    }}
                                />
                            ) : (
                                <Alert variant="destructive">
                                    <AlertTitle>匹配暂未完成</AlertTitle>
                                    <AlertDescription>
                                        {getErrorMessage(
                                            matches.error,
                                            "请重试匹配",
                                        )}
                                    </AlertDescription>
                                    <Button
                                        id="sales-contract-prefill-match-retry"
                                        type="button"
                                        onClick={() => void matches.refetch()}
                                    >
                                        重新匹配
                                    </Button>
                                </Alert>
                            )}
                        </>
                    ) : task ? (
                        <>
                            <ContractImportTask
                                task={task}
                                showExtraction={false}
                                busy={busy}
                                recoveryAvailable={Boolean(recoveryAvailable)}
                                previewing={state.previewMutation.isPending}
                                refreshing={state.detail.isFetching}
                                onPreview={() =>
                                    state.previewMutation.mutate(task.id)
                                }
                                onRetry={state.retry}
                                onRefresh={() => void state.detail.refetch()}
                                onNew={state.startNew}
                            />
                            {task.status === "succeeded" ? (
                                <p className="text-sm">
                                    此合同已归档，请回到销售单选择“关联合同”使用。
                                </p>
                            ) : null}
                        </>
                    ) : (
                        <p role="status">正在加载识别结果…</p>
                    )}
                    {state.previewUrl ? (
                        <iframe
                            title="销售开单合同原文"
                            src={state.previewUrl}
                            className="h-96 w-full rounded-lg border"
                        />
                    ) : null}
                </div>
                <DialogFooter className="shrink-0 border-t pt-4">
                    {state.selectedId ? (
                        <Button
                            id="sales-contract-prefill-new"
                            type="button"
                            variant="ghost"
                            disabled={busy}
                            onClick={state.startNew}
                        >
                            更换合同
                        </Button>
                    ) : null}
                    <Button
                        id="sales-contract-prefill-cancel"
                        type="button"
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        返回销售单
                    </Button>
                    {!state.selectedId ? (
                        <Button
                            id="sales-contract-prefill-start"
                            type="submit"
                            form="sales-contract-prefill-upload"
                            disabled={busy}
                        >
                            {busy ? <Spinner /> : null}开始识别
                        </Button>
                    ) : null}
                    {review ? (
                        <Button
                            id="sales-contract-prefill-apply"
                            type="submit"
                            form="sales-contract-prefill-review"
                            disabled={
                                !task?.source_file_asset_id ||
                                !matches.isSuccess
                            }
                        >
                            填入销售单
                        </Button>
                    ) : null}
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
