"use client"

import { useCallback, useState } from "react"
import { Button } from "@/components/ui/button"
import {
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Spinner } from "@/components/ui/spinner"
import { cn } from "@/lib/utils"
import { ContractImportPreview } from "./contract-import-preview"
import { ContractImportSteps } from "./contract-import-steps"
import {
    IMPORT_REVIEW_STEPS,
    type ImportReviewStep,
} from "../lib/import-review"
import { getErrorCode, getErrorMessage } from "@/lib/api/errors"
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

type Props = UseContractUploadFormOptions & {
    presentation: "page" | "dialog"
    backLabel?: string
}

/** 上传、识别与核对共用同一流程，由入口决定页面或弹窗容器。 */
export function ContractImportWorkspace(props: Props) {
    const inline = props.presentation === "page"
    const Container = inline ? "section" : DialogContent
    const Header = inline ? "div" : DialogHeader
    const Title = inline ? "h2" : DialogTitle
    const Description = inline ? "p" : DialogDescription
    const Footer = inline ? "div" : DialogFooter
    const state = useContractUploadForm(props)
    const [reviewSubmit, setReviewSubmit] =
        useState<ContractImportReviewSubmitState>({
            taskId: "",
            canSubmit: false,
            validSteps: [false, false, false],
            isSubmitting: false,
        })
    const updateReviewSubmit = useCallback(
        (value: ContractImportReviewSubmitState) => setReviewSubmit(value),
        [],
    )
    const [reviewPosition, setReviewPosition] = useState<{
        taskId: string
        step: ImportReviewStep
    }>({ taskId: "", step: 0 })
    const task = state.detail.data
    const reviewing = task?.status === "review" && Boolean(task.draft)
    const step = reviewPosition.taskId === task?.id ? reviewPosition.step : 0
    const changeStep = (step: ImportReviewStep) => {
        if (!task) return
        setReviewPosition({ taskId: task.id, step })
        const scroll = document.getElementById("contract-import-review-scroll")
        if (inline) scroll?.scrollIntoView({ block: "start" })
        else scroll?.scrollTo({ top: 0 })
    }
    const changeOpen = (open: boolean) => {
        if (!open && !inline) setReviewPosition({ taskId: "", step: 0 })
        props.onOpenChange(open)
    }
    const validSteps =
        reviewSubmit.taskId === task?.id
            ? reviewSubmit.validSteps
            : [false, false, false]
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
    const reviewHint =
        reviewSubmit.taskId === task?.id
            ? reviewSubmit.stepHints?.slice(0, step + 1).find(Boolean)
            : undefined
    const recoveryAvailable =
        processing &&
        task.recoverable_at != null &&
        state.detail.dataUpdatedAt >= task.recoverable_at * 1000
    return (
        <Container
            {...(inline
                ? { "aria-labelledby": "contract-import-workspace-title" }
                : { closeButtonId: "contract-import-dialog-close" })}
            className={cn(
                inline
                    ? "flex min-w-0 flex-col gap-5 rounded-xl border border-grid bg-card p-4 sm:p-6"
                    : "flex max-h-[calc(100dvh-2rem)] flex-col overflow-hidden sm:p-7",
                !inline &&
                    (reviewing
                        ? "h-[min(820px,calc(100dvh-3rem))] sm:max-w-[min(1280px,calc(100vw-3rem))]"
                        : "sm:max-w-[720px]"),
            )}
        >
            <Header className="shrink-0 space-y-2 pr-8">
                <Title
                    id="contract-import-workspace-title"
                    tabIndex={-1}
                    className="text-xl font-semibold outline-none"
                >
                    {props.revisionTarget ? "导入合同新版本" : "导入合同"}
                </Title>
                <Description className="text-sm text-muted-foreground">
                    {reviewing
                        ? "识别已完成，请核对并补充"
                        : "识别合同并匹配签约双方，核对后保存合同档案"}
                </Description>
            </Header>
            {reviewing ? (
                <ContractImportSteps
                    step={step}
                    validSteps={validSteps}
                    disabled={busy || reviewSubmit.isSubmitting}
                    onChange={changeStep}
                />
            ) : null}
            <div
                className={cn(
                    "min-h-0",
                    reviewing
                        ? inline
                            ? "space-y-5 lg:grid lg:grid-cols-[minmax(0,1.6fr)_minmax(0,1fr)] lg:items-start lg:gap-6 lg:space-y-0"
                            : "flex-1 space-y-5 overflow-y-auto lg:grid lg:grid-cols-[minmax(0,1.6fr)_minmax(0,1fr)] lg:gap-5 lg:space-y-0 lg:overflow-hidden"
                        : "space-y-5 overflow-y-auto",
                )}
            >
                <div
                    id="contract-import-review-scroll"
                    className={cn(
                        "min-w-0 scroll-mt-5 space-y-5",
                        reviewing &&
                            !inline &&
                            "min-w-0 lg:min-h-0 lg:overflow-y-auto lg:pr-2",
                    )}
                >
                    {error ? (
                        <Alert variant="destructive">
                            <AlertTitle>
                                {getErrorCode(state.confirmMutation.error) ===
                                "PERMISSION_DENIED"
                                    ? "合同未归档：当前账号无归档权限"
                                    : "操作未完成"}
                            </AlertTitle>
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
                    {!state.selectedId && state.uploadMutation.isPending ? (
                        <p
                            className="flex items-center gap-2 py-8 text-sm text-muted-foreground"
                            role="status"
                        >
                            <Spinner />
                            正在上传，完成后自动识别…
                        </p>
                    ) : !state.selectedId ? (
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
                                        required
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
                    ) : task && !reviewing ? (
                        <ContractImportTask
                            task={task}
                            showExtraction={false}
                            showCloseHint={!inline}
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
                    ) : !reviewing && state.detail.isPending ? (
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
                            step={step}
                            onStepChange={changeStep}
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
                    {reviewing && state.contextError ? (
                        <Alert variant="destructive">
                            <AlertTitle>此合同不适用于当前操作</AlertTitle>
                            <AlertDescription>
                                {state.contextError}
                            </AlertDescription>
                        </Alert>
                    ) : null}
                    {!reviewing && state.previewUrl ? (
                        <iframe
                            title="导入合同原文"
                            src={state.previewUrl}
                            className="h-96 w-full rounded-lg border border-border"
                        />
                    ) : null}
                    {!reviewing ? (
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
                    ) : null}
                </div>
                {reviewing && task ? (
                    <ContractImportPreview
                        key={task.id}
                        task={task}
                        className={
                            inline
                                ? "lg:sticky lg:top-5 lg:h-[min(560px,calc(100dvh-24rem))] lg:min-h-80"
                                : undefined
                        }
                    />
                ) : null}
            </div>
            <Footer className="flex flex-col sm:flex-row shrink-0 items-start gap-3 border-t border-border pt-4 sm:flex-wrap sm:items-center sm:justify-between">
                <div className="space-y-1 text-xs leading-relaxed text-muted-foreground">
                    {reviewing ? (
                        <div className="flex items-center gap-3">
                            <p>第 {step + 1} 步，共 3 步</p>
                            <Button
                                id="contract-import-back-to-history"
                                type="button"
                                variant="link"
                                size="xs"
                                disabled={busy || reviewSubmit.isSubmitting}
                                onClick={state.startNew}
                            >
                                返回导入记录
                            </Button>
                        </div>
                    ) : null}
                    <p>
                        {reviewing && step < 2
                            ? "填写内容会在切换步骤时保留"
                            : "确认后保存合同档案与原 PDF，供销售单引用"}
                    </p>
                </div>
                <div className="flex min-w-0 max-w-full flex-wrap items-center gap-2">
                    {reviewing && reviewHint && !state.contextError ? (
                        <p
                            id="contract-import-review-action-hint"
                            className="max-w-sm text-xs text-muted-foreground"
                            aria-live="polite"
                        >
                            {reviewHint}
                        </p>
                    ) : null}
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
                        onClick={() => changeOpen(false)}
                    >
                        {inline
                            ? (props.backLabel ?? "返回开单方式")
                            : "关闭窗口"}
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
                    {reviewing && step > 0 ? (
                        <Button
                            id="contract-import-previous"
                            type="button"
                            variant="outline"
                            disabled={busy || reviewSubmit.isSubmitting}
                            onClick={() =>
                                changeStep((step - 1) as ImportReviewStep)
                            }
                        >
                            上一步
                        </Button>
                    ) : null}
                    {reviewing && step < 2 ? (
                        <Button
                            id="contract-import-next"
                            aria-describedby={
                                reviewHint
                                    ? "contract-import-review-action-hint"
                                    : undefined
                            }
                            type="button"
                            disabled={
                                busy ||
                                Boolean(state.contextError) ||
                                reviewSubmit.isSubmitting ||
                                !validSteps.slice(0, step + 1).every(Boolean)
                            }
                            onClick={() =>
                                changeStep((step + 1) as ImportReviewStep)
                            }
                        >
                            下一步：{IMPORT_REVIEW_STEPS[step + 1]}
                        </Button>
                    ) : null}
                    {reviewing && step === 2 && task ? (
                        <Button
                            id="contract-import-confirm"
                            aria-describedby={
                                reviewHint
                                    ? "contract-import-review-action-hint"
                                    : undefined
                            }
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
                                : inline && props.autoAccept
                                  ? "确认归档并继续开单"
                                  : "确认并归档"}
                        </Button>
                    ) : null}
                </div>
            </Footer>
        </Container>
    )
}
