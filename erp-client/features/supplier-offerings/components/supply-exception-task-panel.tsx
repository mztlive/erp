"use client"

import { useEffect, useRef, useState, type ReactNode } from "react"
import Link from "next/link"
import {
    ChevronRightIcon,
    CircleHelpIcon,
    TriangleAlertIcon,
} from "lucide-react"
import { z } from "zod"

import {
    BusinessFailureState,
    FormalActionConfirmDialog,
    MoneyValue,
    QuantityValue,
} from "@/components/business"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import {
    WorkspaceTaskFooter,
    useWorkspaceInspectorMode,
} from "@/components/business/workspace-task-pane"
import { useAppForm } from "@/components/form"
import { toFieldErrors } from "@/components/form/utils"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useCompleteSupplierSupplyExceptionTaskMutation } from "@/features/supplier-offerings/hooks/queries"
import {
    supplyExceptionCompletionIntent,
    type SupplyExceptionCompletionPayload,
} from "@/features/supplier-offerings/lib/supply-exception-command"
import {
    AVAILABILITY_STATUS_LABELS,
    OFFERING_STATUS_LABELS,
    SOURCE_TYPE_LABELS,
    type AvailabilityStatus,
    type OfferingStatus,
    type SupplierOfferingView,
    type SupplierSupplyExceptionWorkItem,
} from "@/features/supplier-offerings/types"
import { getErrorMessage, isApiError } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime, unixSecondsToIsoOrEmpty } from "@/lib/datetime"
import {
    FormalCommandKeyLedger,
    isCommandOutcomeUnknown,
} from "@/lib/formal-command"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"

const COMMENT_MAX_CHARS = 500

const evidenceSchema = z
    .string()
    .trim()
    .min(1, "请填写核对凭据编号或链接")
    .max(256, "核对凭据不能超过 256 个字符")

const commentSchema = z
    .string()
    .trim()
    .min(3, "请填写至少 3 个字的核对结论")
    .refine(
        (value) => Array.from(value).length <= COMMENT_MAX_CHARS,
        "核对结论不能超过 500 个字符",
    )

const offeringTone: Record<OfferingStatus, StatusTone> = {
    ACTIVE: "success",
    PAUSED: "warning",
    STOPPED: "destructive",
}

const availabilityTone: Record<AvailabilityStatus, StatusTone> = {
    AVAILABLE: "success",
    UNAVAILABLE: "warning",
    STALE: "warning",
    STOPPED: "destructive",
}

export type SupplyExceptionReceipt = Pick<
    SupplyExceptionCompletionPayload,
    "evidenceReference" | "comment"
>

type SupplyExceptionTaskPanelProps = {
    workItemId: string
    task?: SupplierSupplyExceptionWorkItem
    offering?: SupplierOfferingView
    isPending: boolean
    error?: Error | null
    onRetry: () => void | Promise<void>
    embedded?: boolean
    idPrefix?: string
    onTaskCompleted?: (workItemId: string) => void
    onOpenDiagnostics?: () => void
    renderWorkbench?: (content: {
        body: ReactNode
        receipt?: SupplyExceptionReceipt
    }) => ReactNode
}

type SubmitFailure =
    | { kind: "unknown"; slot: string; message: string }
    | { kind: "conflict" | "failed"; message: string }

const closeCondition =
    "提交凭据和结论后，本条待办结束。恢复供给或商城发布须另行办理。"

export function SupplyExceptionTaskPanel(props: SupplyExceptionTaskPanelProps) {
    return <SupplyExceptionVerification key={props.workItemId} {...props} />
}

/** 折叠或调整右轨时保留表单、确认层及原提交载荷。 */
function SupplyExceptionVerification({
    workItemId,
    task,
    offering,
    isPending,
    error,
    onRetry,
    embedded = false,
    idPrefix = `supplier-offerings-exception-${toAutomationIdSegment(workItemId)}`,
    onTaskCompleted,
    onOpenDiagnostics,
    renderWorkbench,
}: SupplyExceptionTaskPanelProps) {
    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions
    const mayReadTask = hasPermission(permissions, "work_item:detail")
    const mayComplete =
        mayReadTask &&
        hasPermission(permissions, "supplier_offering:resolve_supply_exception")
    const mayReadOffering = hasPermission(permissions, "supplier_offering:list")
    const mayReadCosts =
        mayReadOffering &&
        hasPermission(permissions, "supplier_offering_cost:detail")
    const visibleOffering =
        mayReadOffering && offering?.id === task?.businessObjectId
            ? offering
            : undefined

    const inspectorMode = useWorkspaceInspectorMode()
    const completeMutation = useCompleteSupplierSupplyExceptionTaskMutation()
    const [commandLedger] = useState(() => new FormalCommandKeyLedger())
    const running = useRef(false)
    const [confirmation, setConfirmation] =
        useState<ReturnType<typeof supplyExceptionCompletionIntent>>()
    const [failure, setFailure] = useState<SubmitFailure>()
    const [receipt, setReceipt] = useState<SupplyExceptionReceipt>()
    const [refreshing, setRefreshing] = useState(false)
    const [formPulse, setFormPulse] = useState(false)
    const formPulseTimer = useRef<number | null>(null)
    const unknown = failure?.kind === "unknown" ? failure : undefined
    const conflict = failure?.kind === "conflict"
    const canProcess = Boolean(
        mayComplete &&
        task?.allowedActions.includes("PROCESS") &&
        !task.processingBlocker &&
        !error &&
        !receipt,
    )
    const canSubmit = canProcess && !unknown && !conflict && !refreshing
    const inlineConfirming = Boolean(inspectorMode && confirmation && canSubmit)
    const formId = `${idPrefix}-form`
    const evidenceId = `${idPrefix}-evidence`
    const inlineConfirmId = `${idPrefix}-inline-confirm`
    const returnHref = buildWorkspaceDetailHref("tasks", workItemId)

    useEffect(
        () => () => {
            if (formPulseTimer.current !== null) {
                window.clearTimeout(formPulseTimer.current)
            }
        },
        [],
    )

    useEffect(() => {
        if (!inlineConfirming) return
        const target = document.getElementById(inlineConfirmId)
        if (!target) return
        const reduceMotion = window.matchMedia(
            "(prefers-reduced-motion: reduce)",
        ).matches
        target.focus({ preventScroll: true })
        target.scrollIntoView({
            block: "nearest",
            behavior: reduceMotion ? "auto" : "smooth",
        })
    }, [inlineConfirmId, inlineConfirming])

    const form = useAppForm({
        defaultValues: { evidenceReference: "", comment: "" },
        validators: {
            onSubmit: z.object({
                evidenceReference: evidenceSchema,
                comment: commentSchema,
            }),
        },
        onSubmit: ({ value }) => {
            if (!task || !canSubmit || running.current) return
            setConfirmation(
                supplyExceptionCompletionIntent({
                    offeringId: task.businessObjectId,
                    workItemId: task.workItemId,
                    expectedTaskVersion: task.taskVersion,
                    expectedSubjectVersion: task.subjectVersion,
                    ...value,
                }),
            )
        },
    })

    async function completeTask() {
        if (!mayComplete || running.current || receipt) return
        if (!unknown && !canSubmit) return

        const intent = unknown ? undefined : confirmation
        const slot = unknown?.slot ?? intent?.slot
        if (!slot) return

        running.current = true
        let completedWorkItemId: string | undefined
        try {
            const command = intent
                ? commandLedger.acquire(
                      intent.slot,
                      intent.prefix,
                      intent.payload,
                  )
                : commandLedger.peek<SupplyExceptionCompletionPayload>(slot)
            if (!command) return

            const result = await completeMutation.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
            commandLedger.settle(slot, "succeeded")
            setFailure(undefined)
            setReceipt({
                evidenceReference: result.evidence_reference,
                comment: command.payload.comment,
            })
            completedWorkItemId = result.work_item_id
        } catch (submitError) {
            const status = isApiError(submitError)
                ? submitError.status
                : undefined
            const cannotVerify =
                unknown &&
                (status === 401 ||
                    status === 403 ||
                    status === 404 ||
                    status === 429)
            const outcomeUnknown =
                isCommandOutcomeUnknown(submitError) ||
                status === 408 ||
                cannotVerify
            const message = getErrorMessage(
                submitError,
                "请检查核对结论后重试。",
            )
            commandLedger.settle(slot, outcomeUnknown ? "unknown" : "failed")
            setFailure(
                outcomeUnknown
                    ? { kind: "unknown", slot, message }
                    : { kind: status === 409 ? "conflict" : "failed", message },
            )
        } finally {
            running.current = false
            setConfirmation(undefined)
        }
        if (completedWorkItemId) onTaskCompleted?.(completedWorkItemId)
    }

    async function refresh() {
        setRefreshing(true)
        try {
            await onRetry()
            if (conflict) setFailure(undefined)
        } catch (refreshError) {
            if (conflict) {
                setFailure({
                    kind: "conflict",
                    message: getErrorMessage(
                        refreshError,
                        "刷新失败，请重试。",
                    ),
                })
            }
        } finally {
            setRefreshing(false)
        }
    }

    function focusEvidence() {
        const target = document.getElementById(evidenceId)
        if (!target) return
        const reduceMotion = window.matchMedia(
            "(prefers-reduced-motion: reduce)",
        ).matches
        target.focus({ preventScroll: true })
        target.scrollIntoView({
            block: "center",
            behavior: reduceMotion ? "auto" : "smooth",
        })
        setFormPulse(true)
        if (formPulseTimer.current !== null) {
            window.clearTimeout(formPulseTimer.current)
        }
        formPulseTimer.current = window.setTimeout(
            () => setFormPulse(false),
            2200,
        )
    }

    const backButton = !embedded ? (
        <Button
            id={`${idPrefix}-back-workspace`}
            size="sm"
            variant="outline"
            render={<Link href={returnHref} />}
        >
            返回待办队列
        </Button>
    ) : null

    const recovery =
        unknown && mayComplete ? (
            <LoadingButton
                id={`${idPrefix}-verify-result`}
                type="button"
                size="sm"
                variant="default"
                loading={completeMutation.isPending}
                disabled={completeMutation.isPending}
                onClick={() => void completeTask()}
            >
                {completeMutation.isPending ? "正在核对结果" : "核对提交结果"}
            </LoadingButton>
        ) : null

    const body = (
        <div className="@container/supply-exception min-w-0 bg-card">
            <div
                className={cn(
                    "grid min-w-0 gap-4 py-4",
                    !embedded && "border-b border-border/60",
                )}
            >
                {!embedded ? (
                    <div className="flex min-w-0 flex-wrap items-center justify-between gap-2">
                        <h2 className="text-section-title font-semibold">
                            供应停止核对
                        </h2>
                        {backButton}
                    </div>
                ) : null}

                {unknown ? (
                    <div
                        role="status"
                        className="space-y-1 border-l-2 border-l-warning bg-warning-soft px-3 py-2 text-body-compact wrap-anywhere text-warning-soft-foreground"
                    >
                        <p className="font-semibold">提交结果待核对</p>
                        <p>
                            {mayComplete
                                ? "点击「核对提交结果」继续查询本次提交。原凭据和结论已锁定。"
                                : "请由有办理权限的人员核对提交结果。"}
                        </p>
                        {mayComplete ? (
                            <p className="text-xs">{unknown.message}</p>
                        ) : null}
                    </div>
                ) : null}

                {receipt && mayReadTask ? (
                    <div
                        role="status"
                        className="space-y-1 border-l-2 border-l-success bg-success-soft px-3 py-2 text-body-compact wrap-anywhere text-success-soft-foreground"
                    >
                        <p className="font-semibold">已完成供应停止核对</p>
                        <p>
                            已登记凭据{" "}
                            <span className="num select-all">
                                {receipt.evidenceReference}
                            </span>
                            。恢复供给或商城发布须另行办理。
                        </p>
                    </div>
                ) : task && mayReadTask && !error ? (
                    <>
                        <div
                            role="alert"
                            className="flex min-w-0 flex-wrap items-start justify-between gap-x-3 gap-y-2 rounded-r-md border-l-2 border-l-destructive bg-destructive-soft/65 px-3 py-2.5 text-destructive-soft-foreground"
                        >
                            <div className="flex min-w-0 flex-[1_1_16rem] items-start gap-2">
                                <TriangleAlertIcon
                                    aria-hidden="true"
                                    className="mt-0.5 size-3.5 shrink-0"
                                />
                                <div className="min-w-0 flex-1 space-y-0.5 text-body-compact wrap-anywhere">
                                    <p className="font-semibold">
                                        {task.reasonLabel}
                                    </p>
                                    <p>{task.impactSummary}</p>
                                    <p className="text-xs opacity-90">
                                        {closeCondition}
                                    </p>
                                </div>
                            </div>
                            {onOpenDiagnostics || canSubmit ? (
                                <Button
                                    id={`${idPrefix}-jump-diagnostics`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    className="font-medium text-destructive-soft-foreground shadow-none hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                    onClick={onOpenDiagnostics ?? focusEvidence}
                                >
                                    {onOpenDiagnostics
                                        ? "查看影响诊断"
                                        : "登记核对凭据"}
                                    <ChevronRightIcon
                                        data-icon="inline-end"
                                        aria-hidden="true"
                                    />
                                </Button>
                            ) : null}
                        </div>
                        <SupplyExceptionKpis
                            idPrefix={idPrefix}
                            task={task}
                            offering={visibleOffering}
                        />
                        <CommercialMeta
                            task={task}
                            offering={visibleOffering}
                            showCosts={mayReadCosts}
                            standalone={!embedded}
                        />

                        {failure && !unknown ? (
                            <div
                                role="alert"
                                className="space-y-1 border-l-2 border-l-warning bg-warning-soft px-3 py-2 text-body-compact wrap-anywhere text-warning-soft-foreground"
                            >
                                <p className="font-semibold">
                                    {conflict
                                        ? "任务状态已变化"
                                        : "核对结论未提交"}
                                </p>
                                <p>{failure.message}</p>
                            </div>
                        ) : null}

                        {inlineConfirming ? (
                            <section
                                id={inlineConfirmId}
                                tabIndex={-1}
                                aria-live="polite"
                                className="space-y-1 rounded-r-md border-l-2 border-l-warning bg-warning-soft/75 px-3 py-2 text-xs wrap-anywhere text-warning-soft-foreground"
                            >
                                <p className="text-body-compact font-semibold">
                                    确认完成供应停止核对
                                </p>
                                <p>{closeCondition}</p>
                                <p>
                                    将登记凭据{" "}
                                    <span className="num font-medium">
                                        {
                                            confirmation?.payload
                                                .evidenceReference
                                        }
                                    </span>{" "}
                                    与核对结论，并结束本条待办。
                                </p>
                            </section>
                        ) : null}

                        {mayComplete ? (
                            <form
                                id={formId}
                                className={cn(
                                    "grid min-w-0 gap-3 border-t border-border/60 bg-card pt-4",
                                    formPulse &&
                                        "rounded-md bg-info-soft/20 px-2 pb-2 ring-1 ring-info-border ring-inset motion-safe:animate-pulse",
                                )}
                                aria-labelledby={`${idPrefix}-verification-title`}
                                onSubmit={(event) => {
                                    event.preventDefault()
                                    if (canSubmit && !inlineConfirming) {
                                        void form.handleSubmit()
                                    }
                                }}
                            >
                                <h2
                                    id={`${idPrefix}-verification-title`}
                                    className="text-section-title font-semibold"
                                >
                                    登记凭据和核对结论
                                </h2>
                                <form.AppField
                                    name="evidenceReference"
                                    validators={{ onChange: evidenceSchema }}
                                    children={(field) => (
                                        <field.TextField
                                            id={evidenceId}
                                            label="核对凭据编号或链接"
                                            placeholder="例如供应商停供函或内部工单编号"
                                            required
                                            disabled={
                                                !canSubmit ||
                                                inlineConfirming ||
                                                completeMutation.isPending
                                            }
                                        />
                                    )}
                                />
                                <form.Field
                                    name="comment"
                                    validators={{ onChange: commentSchema }}
                                    children={(field) => (
                                        <ReasonTextareaField
                                            id={`${idPrefix}-comment`}
                                            name={field.name}
                                            label="核对结论"
                                            description="填写停供来源、受影响发布及后续安排。最多 500 个字符。"
                                            mode="chars"
                                            max={COMMENT_MAX_CHARS}
                                            value={field.state.value}
                                            onChange={field.handleChange}
                                            onBlur={field.handleBlur}
                                            error={
                                                field.state.meta.isTouched
                                                    ? toFieldErrors(
                                                          field.state.meta
                                                              .errors,
                                                      )[0]?.message
                                                    : undefined
                                            }
                                            rows={4}
                                            required
                                            disabled={
                                                !canSubmit ||
                                                inlineConfirming ||
                                                completeMutation.isPending
                                            }
                                            onCtrlEnterSubmit={
                                                canSubmit && !inlineConfirming
                                                    ? () =>
                                                          void form.handleSubmit()
                                                    : undefined
                                            }
                                        />
                                    )}
                                />
                            </form>
                        ) : null}

                        {!canProcess && !unknown ? (
                            <div
                                role="status"
                                className="space-y-1 border-l-2 border-l-warning bg-warning-soft px-3 py-2 text-xs wrap-anywhere text-warning-soft-foreground"
                            >
                                <p className="font-medium">当前仅可查看</p>
                                <p>
                                    {task.processingBlocker?.message ||
                                        task.actionBlockers[0] ||
                                        "请由具备办理权限的任务负责人提交核对结果。"}
                                </p>
                            </div>
                        ) : null}
                    </>
                ) : isPending ||
                  account.isPending ||
                  completeMutation.isPending ? (
                    <div
                        className="h-44 animate-pulse rounded-lg bg-surface-sunken"
                        aria-busy="true"
                        aria-label="正在读取供应停止核对任务"
                    />
                ) : (
                    <BusinessFailureState
                        idPrefix={`${idPrefix}-task-load`}
                        title="暂时无法读取停供核对任务"
                        description="请刷新任务，或从待办队列重新进入。"
                        kind={mayReadTask ? undefined : "permission"}
                        error={error ?? account.error ?? undefined}
                        className="rounded-none border-0 p-0 wrap-anywhere"
                        action={
                            mayReadTask && !unknown ? (
                                <Button
                                    id={`${idPrefix}-task-retry`}
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    disabled={refreshing}
                                    onClick={() => void refresh()}
                                >
                                    刷新任务
                                </Button>
                            ) : undefined
                        }
                    />
                )}
            </div>

            {!receipt && ((task && mayReadTask) || unknown) ? (
                <form.Subscribe
                    selector={(state) => state.values}
                    children={({ evidenceReference, comment }) => {
                        const evidenceReady =
                            evidenceSchema.safeParse(evidenceReference).success
                        const commentReady =
                            commentSchema.safeParse(comment).success
                        const actions = (
                            <>
                                {conflict ? (
                                    <Button
                                        id={`${idPrefix}-refresh-conflict`}
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        disabled={refreshing}
                                        onClick={() => void refresh()}
                                    >
                                        {refreshing
                                            ? "正在刷新"
                                            : "刷新最新数据"}
                                    </Button>
                                ) : null}
                                {recovery ??
                                    (inlineConfirming ? (
                                        <>
                                            <Button
                                                id={`${idPrefix}-cancel-confirm`}
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                disabled={
                                                    completeMutation.isPending
                                                }
                                                onClick={() =>
                                                    setConfirmation(undefined)
                                                }
                                            >
                                                取消
                                            </Button>
                                            <LoadingButton
                                                id={`${idPrefix}-confirm-submit`}
                                                type="button"
                                                variant="default"
                                                size="sm"
                                                loading={
                                                    completeMutation.isPending
                                                }
                                                disabled={
                                                    completeMutation.isPending
                                                }
                                                onClick={() =>
                                                    void completeTask()
                                                }
                                            >
                                                确认完成核对
                                            </LoadingButton>
                                        </>
                                    ) : canSubmit ? (
                                        <Button
                                            id={`${idPrefix}-submit`}
                                            type="submit"
                                            form={formId}
                                            variant="default"
                                            size="sm"
                                            disabled={
                                                completeMutation.isPending
                                            }
                                        >
                                            确认已核对
                                        </Button>
                                    ) : null)}
                            </>
                        )
                        const summary = (
                            <dl
                                aria-live="polite"
                                className="flex min-w-0 flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground"
                            >
                                {canProcess ? (
                                    <>
                                        <div className="flex gap-1">
                                            <dt>凭据</dt>
                                            <dd className="font-medium text-foreground">
                                                {evidenceReady
                                                    ? "已填写"
                                                    : "待完善"}
                                            </dd>
                                        </div>
                                        <div className="flex gap-1">
                                            <dt>结论</dt>
                                            <dd className="font-medium text-foreground">
                                                {commentReady
                                                    ? "已填写"
                                                    : "待完善"}
                                            </dd>
                                        </div>
                                    </>
                                ) : null}
                                <div className="flex gap-1">
                                    <dt>办理</dt>
                                    <dd className="font-medium text-foreground">
                                        {unknown
                                            ? "结果待核对"
                                            : !canProcess
                                              ? "只读"
                                              : conflict
                                                ? "需刷新"
                                                : evidenceReady && commentReady
                                                  ? "可提交"
                                                  : "待填写"}
                                    </dd>
                                </div>
                            </dl>
                        )
                        return (
                            <WorkspaceTaskFooter
                                summary={summary}
                                fallback={
                                    <div
                                        className={cn(
                                            "flex w-full min-w-0 flex-wrap items-center justify-between gap-2",
                                            !embedded && "py-2",
                                        )}
                                    >
                                        {summary}
                                        <div className="ml-auto flex flex-wrap gap-2">
                                            {actions}
                                        </div>
                                    </div>
                                }
                            >
                                {actions}
                            </WorkspaceTaskFooter>
                        )
                    }}
                />
            ) : null}
        </div>
    )

    return (
        <>
            {renderWorkbench ? renderWorkbench({ body, receipt }) : body}
            {mayComplete && !inspectorMode ? (
                <FormalActionConfirmDialog
                    actionVariant="default"
                    idPrefix={`${idPrefix}-complete-confirm`}
                    layout="compact"
                    open={Boolean(confirmation)}
                    onOpenChange={(open) => {
                        if (!open) setConfirmation(undefined)
                    }}
                    actionLabel="完成核对任务"
                    confirmLabel="确认完成核对"
                    title="确认完成供应停止核对"
                    description={closeCondition}
                    fromStatus={{ label: "待核对", tone: "warning" }}
                    toStatus={{ label: "任务已完成", tone: "success" }}
                    effects={["将凭据和核对结论登记到操作记录"]}
                    pending={completeMutation.isPending}
                    confirmDisabled={!canSubmit}
                    onConfirm={completeTask}
                />
            ) : null}
        </>
    )
}

function SupplyExceptionKpis({
    idPrefix,
    task,
    offering,
}: {
    idPrefix: string
    task: SupplierSupplyExceptionWorkItem
    offering?: SupplierOfferingView
}) {
    const availability = offering?.availability_status
    const detectedAt = unixSecondsToIsoOrEmpty(task.createdAt)
    const metricCount = 2 + (availability ? 1 : 0) + (detectedAt ? 1 : 0)

    return (
        <TooltipProvider>
            <dl
                className={cn(
                    "grid grid-cols-1 gap-x-4 gap-y-3 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/supply-exception:grid-cols-2",
                    metricCount === 4 &&
                        "@[840px]/supply-exception:grid-cols-4",
                    metricCount === 3 &&
                        "@[840px]/supply-exception:grid-cols-3",
                )}
            >
                <div className="min-w-0">
                    <dt className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
                        涉及供给
                        <Tooltip>
                            <TooltipTrigger
                                id={`${idPrefix}-scope-help`}
                                render={
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon-xs"
                                        aria-label="涉及供给的统计口径"
                                    />
                                }
                            >
                                <CircleHelpIcon aria-hidden="true" />
                            </TooltipTrigger>
                            <TooltipContent>
                                按本条待办关联的供应商供给计数。受影响的发布范围见影响诊断。
                            </TooltipContent>
                        </Tooltip>
                    </dt>
                    <dd className="num text-value-section font-semibold tracking-tight">
                        1{" "}
                        <span className="text-xs font-normal text-muted-foreground">
                            项
                        </span>
                    </dd>
                </div>
                <div className="min-w-0 space-y-1">
                    <dt className="text-xs text-muted-foreground">
                        {offering ? "当前供给状态" : "停供事件"}
                    </dt>
                    <dd>
                        <StatusBadge
                            appearance="text"
                            tone={
                                offering
                                    ? offeringTone[offering.status]
                                    : "destructive"
                            }
                            label={
                                offering
                                    ? OFFERING_STATUS_LABELS[offering.status]
                                    : "供应已停止"
                            }
                        />
                    </dd>
                </div>
                {availability ? (
                    <div className="min-w-0 space-y-1">
                        <dt className="text-xs text-muted-foreground">
                            当前可供状态
                        </dt>
                        <dd className="flex flex-wrap items-baseline gap-x-2">
                            <StatusBadge
                                appearance="text"
                                tone={availabilityTone[availability]}
                                label={AVAILABILITY_STATUS_LABELS[availability]}
                            />
                            {availability === "AVAILABLE" &&
                            offering.available_quantity != null ? (
                                <QuantityValue
                                    value={offering.available_quantity}
                                    unit={null}
                                />
                            ) : null}
                        </dd>
                    </div>
                ) : null}
                {detectedAt ? (
                    <div className="min-w-0 space-y-1">
                        <dt className="text-xs text-muted-foreground">
                            异常登记时间
                        </dt>
                        <dd className="text-body-compact font-medium">
                            <time className="num" dateTime={detectedAt}>
                                {formatDateTime(detectedAt)}
                            </time>
                        </dd>
                    </div>
                ) : null}
            </dl>
        </TooltipProvider>
    )
}

function CommercialMeta({
    task,
    offering,
    showCosts,
    standalone,
}: {
    task: SupplierSupplyExceptionWorkItem
    offering?: SupplierOfferingView
    showCosts: boolean
    standalone: boolean
}) {
    const fields = [
        {
            label: "受影响供给",
            value: standalone
                ? [
                      offering?.supplier_name,
                      offering?.sku_name,
                      offering?.supplier_sku_code,
                  ]
                      .filter(Boolean)
                      .join(" · ") || task.businessObjectLabel
                : undefined,
        },
        {
            label: "责任人",
            value:
                task.ownerUser?.displayName ||
                task.ownerOrganization.displayName,
        },
        { label: "规格", value: offering?.specification },
        {
            label: "供给来源",
            value: offering && SOURCE_TYPE_LABELS[offering.source_type],
        },
        {
            label: "代发含税价",
            value:
                showCosts && offering?.dropship_supply_price_gross != null ? (
                    <MoneyValue value={offering.dropship_supply_price_gross} />
                ) : undefined,
        },
        {
            label: "集采含税价",
            value:
                showCosts && offering?.bulk_supply_price_gross != null ? (
                    <MoneyValue value={offering.bulk_supply_price_gross} />
                ) : undefined,
        },
    ]

    return (
        <dl className="flex min-w-0 flex-wrap gap-x-5 gap-y-1.5 text-xs">
            {fields.map(({ label, value }) =>
                value ? (
                    <div
                        key={label}
                        className="flex min-w-0 flex-wrap gap-x-1.5"
                    >
                        <dt className="text-muted-foreground">{label}</dt>
                        <dd className="min-w-0 wrap-anywhere">{value}</dd>
                    </div>
                ) : null,
            )}
        </dl>
    )
}
