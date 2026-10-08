"use client"

import { useCallback, useState } from "react"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Spinner } from "@/components/ui/spinner"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { ChevronRightIcon } from "lucide-react"
import { getErrorMessage } from "@/lib/api/errors"
import {
    useContractUploadForm,
    type UseContractUploadFormOptions,
} from "@/features/contracts/hooks/use-contract-upload-form"
import { ContractImportHistory } from "./contract-import-history"
import {
    ContractImportReview,
    type ContractImportReviewSubmitState,
} from "./contract-import-review"
import { ContractImportTask } from "./contract-import-task"

export type ContractUploadDialogProps = UseContractUploadFormOptions

export function ContractUploadDialog(props: ContractUploadDialogProps) {
    const state = useContractUploadForm(props)
    const [reviewSubmit, setReviewSubmit] =
        useState<ContractImportReviewSubmitState>({
            taskId: "",
            canSubmit: false,
            isSubmitting: false,
        })
    const updateReviewSubmit = useCallback(
        (value: ContractImportReviewSubmitState) => setReviewSubmit(value),
        [],
    )
    const task = state.detail.data
    const busy =
        state.uploadMutation.isPending ||
        state.runMutation.isPending ||
        state.confirmMutation.isPending
    const error =
        state.confirmMutation.error ||
        state.uploadMutation.error ||
        state.runMutation.error ||
        state.list.error ||
        state.detail.error ||
        state.previewMutation.error
    const processing = task?.status === "processing"
    const recoveryAvailable =
        processing &&
        task.recoverable_at != null &&
        state.detail.dataUpdatedAt >= task.recoverable_at * 1000
    return (
        <Dialog open={props.open} onOpenChange={props.onOpenChange}>
            <DialogContent
                closeButtonId="contract-import-dialog-close"
                className="flex max-h-[calc(100dvh-2rem)] flex-col overflow-hidden sm:max-w-[720px] sm:p-7"
            >
                <DialogHeader className="shrink-0 pr-8">
                    <DialogTitle className="text-xl font-semibold">
                        {props.revisionTarget ? "导入合同新版本" : "导入合同"}
                    </DialogTitle>
                    <DialogDescription>
                        识别合同并匹配签约双方，核对后保存合同档案
                    </DialogDescription>
                </DialogHeader>
                <div className="min-h-0 space-y-5 overflow-y-auto">
                    {error ? (
                        <Alert variant="destructive">
                            <AlertTitle>操作未完成</AlertTitle>
                            <AlertDescription>
                                {getErrorMessage(error, "请查看任务结果后重试")}
                            </AlertDescription>
                            {state.detail.isError && state.selectedId ? (
                                <Button
                                    id="contract-import-load-retry"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    disabled={state.detail.isFetching}
                                    onClick={() => void state.detail.refetch()}
                                >
                                    重新加载
                                </Button>
                            ) : null}
                        </Alert>
                    ) : null}
                    {!state.selectedId ? (
                        <form
                            id="contract-import-upload-form"
                            onSubmit={(event) => {
                                event.preventDefault()
                                if (busy || state.form.state.isSubmitting)
                                    return
                                void state.form
                                    .handleSubmit()
                                    .catch(() => undefined)
                            }}
                            className="space-y-4"
                        >
                            <state.form.AppField
                                name="pdfFile"
                                listeners={{
                                    onChange: ({ value, fieldApi }) => {
                                        if (
                                            !value ||
                                            busy ||
                                            fieldApi.form.state.isSubmitting
                                        )
                                            return
                                        // 请求错误由 mutation 状态统一展示。
                                        void fieldApi.form
                                            .handleSubmit()
                                            .catch(() => undefined)
                                    },
                                }}
                            >
                                {(field) => (
                                    <field.PdfUploadField
                                        id="card-contracts-upload-pdf"
                                        label="已签署合同 PDF"
                                        description="仅支持单个 PDF，文件不超过 20 MB。选择文件后自动上传并识别。"
                                        disabled={busy}
                                    />
                                )}
                            </state.form.AppField>
                            {state.uploadMutation.isPending ? (
                                <p
                                    className="flex items-center gap-2 text-sm text-muted-foreground"
                                    role="status"
                                >
                                    <Spinner />
                                    正在上传，完成后自动识别…
                                </p>
                            ) : state.uploadMutation.isError ? (
                                <state.form.AppForm>
                                    <state.form.SubmitButton
                                        id="card-contracts-upload-submit"
                                        loading={busy}
                                        label="重新上传"
                                    />
                                </state.form.AppForm>
                            ) : null}
                        </form>
                    ) : task ? (
                        <ContractImportTask
                            task={task}
                            showExtraction={false}
                            busy={busy}
                            recoveryAvailable={Boolean(recoveryAvailable)}
                            contextError={state.contextError}
                            previewing={state.previewMutation.isPending}
                            refreshing={state.detail.isFetching}
                            onPreview={() =>
                                state.previewMutation.mutate(task.id)
                            }
                            onRetry={state.retry}
                            onRefresh={() => void state.detail.refetch()}
                            onNew={state.startNew}
                        />
                    ) : state.detail.isPending ? (
                        <p
                            className="flex items-center gap-2 py-8 text-sm text-muted-foreground"
                            role="status"
                        >
                            <Spinner />
                            正在加载识别结果…
                        </p>
                    ) : null}
                    {task?.status === "review" && task.draft ? (
                        <ContractImportReview
                            key={task.id}
                            task={task}
                            busy={busy}
                            disabled={Boolean(state.contextError)}
                            expectedCustomerId={props.initialCustomerId}
                            onSubmitStateChange={updateReviewSubmit}
                            onConfirm={(command) =>
                                state.confirmMutation.mutateAsync({
                                    id: task.id,
                                    command,
                                })
                            }
                        />
                    ) : null}
                    {state.previewUrl ? (
                        <iframe
                            title="导入合同原文"
                            src={state.previewUrl}
                            className="h-96 w-full rounded-lg border border-border"
                        />
                    ) : null}
                    <Collapsible>
                        <CollapsibleTrigger
                            id="contract-import-history-toggle"
                            className="group flex items-center gap-1 text-sm text-muted-foreground"
                        >
                            <ChevronRightIcon className="size-4 group-data-panel-open:rotate-90" />
                            查看导入记录
                        </CollapsibleTrigger>
                        <CollapsibleContent className="mt-3">
                            <ContractImportHistory
                                items={state.list.data?.items ?? []}
                                total={state.list.data?.total ?? 0}
                                loading={state.list.isPending}
                                page={state.page}
                                selectedId={state.selectedId}
                                currentTask={task}
                                disabled={busy}
                                onSelect={state.select}
                                onPageChange={state.setPage}
                            />
                        </CollapsibleContent>
                    </Collapsible>
                </div>
                <DialogFooter className="shrink-0 items-start gap-3 border-t border-border pt-4 sm:flex-wrap sm:items-center sm:justify-between">
                    <p className="text-xs leading-relaxed text-muted-foreground">
                        确认后保存合同档案与原 PDF，供销售单引用
                    </p>
                    <div className="flex min-w-0 max-w-full flex-wrap items-center gap-2">
                        {task?.status === "succeeded" ||
                        state.contextError ||
                        (state.selectedId && state.detail.isError) ? (
                            <Button
                                id="contract-import-new"
                                type="button"
                                variant="ghost"
                                disabled={busy}
                                onClick={state.startNew}
                            >
                                导入另一份
                            </Button>
                        ) : null}
                        <Button
                            id="card-contracts-upload-cancel"
                            type="button"
                            variant="outline"
                            onClick={() => props.onOpenChange(false)}
                        >
                            关闭窗口
                        </Button>
                        {task?.status === "succeeded" ? (
                            <Button
                                id="contract-import-use"
                                type="button"
                                disabled={!state.canAccept}
                                onClick={state.accept}
                            >
                                {props.onSuccess ? "使用此合同" : "完成"}
                            </Button>
                        ) : null}
                        {task?.status === "review" && task.draft ? (
                            <Button
                                id="contract-import-confirm"
                                type="submit"
                                form="contract-import-review-form"
                                disabled={
                                    busy ||
                                    Boolean(state.contextError) ||
                                    reviewSubmit.taskId !== task.id ||
                                    !reviewSubmit.canSubmit ||
                                    reviewSubmit.isSubmitting
                                }
                            >
                                {busy || reviewSubmit.isSubmitting ? (
                                    <Spinner />
                                ) : null}
                                {busy || reviewSubmit.isSubmitting
                                    ? "正在归档…"
                                    : "确认并归档"}
                            </Button>
                        ) : null}
                    </div>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
