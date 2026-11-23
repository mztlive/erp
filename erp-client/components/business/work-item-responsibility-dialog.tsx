"use client"

import * as React from "react"
import { useMutation, useQueries, useQueryClient } from "@tanstack/react-query"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"

import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { toFieldErrors, useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { FieldGroup } from "@/components/ui/field"
import { toast } from "@/components/ui/toast"
import {
    getWorkItemReassignCandidates,
    submitWorkItemResponsibility,
} from "@/features/work-items/api"
import {
    synchronizeWorkItemConflict,
    workItemKeys,
} from "@/features/work-items/queries"
import type {
    WorkItemReassignCandidate,
    WorkItemResponsibilityCommand,
} from "@/features/work-items/types"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { queryKeyRoots } from "@/lib/query-key-roots"
import {
    WORK_ITEM_CLOSE_COMMENT_MAX_CHARS,
    WORK_ITEM_REASSIGN_REASON_MAX_CHARS,
    workItemCloseCommentSchema,
    workItemReassignReasonSchema,
} from "@/lib/validation/reason-length"

const BATCH_CONCURRENCY = 3
const REPLACEMENT_ID_MAX_CHARS = 128

export type WorkItemCloseReasonOption = Readonly<{
    code: string
    label: string
    /** 选中该原因时必须提供替代任务编号（仅单条模式开放）。 */
    requiresReplacement?: boolean
    /** 选中该原因时必须填写关闭说明。 */
    requiresComment?: boolean
}>

export type WorkItemResponsibilityTarget = Readonly<{
    workItemId: string
    label: string
    expectedTaskVersion: string
    currentOwnerLabel?: string
}>

export type WorkItemResponsibilitySummary = Readonly<{
    succeededIds: string[]
    failed: { id: string; message: string }[]
}>

export type WorkItemResponsibilityDialogProps = Readonly<{
    mode: "reassign" | "close"
    targets: readonly WorkItemResponsibilityTarget[]
    /** `mode="close"` 时传入后端允许的关闭原因码选项；缺省使用 W29 允许的 `MISROUTED` / `DUPLICATE`。 */
    closeReasonOptions?: readonly WorkItemCloseReasonOption[]
    open: boolean
    onOpenChange: (open: boolean) => void
    onCompleted: (summary: WorkItemResponsibilitySummary) => void
    /** 自定义原生自动化 ID 前缀。 */
    idPrefix?: string
    /** 自定义弹窗标题。 */
    title?: string
    /** 自定义弹窗说明。 */
    description?: string
}>

/** 与后端 `W29CloseDecision::new` 严格一致的默认受控关闭原因码。 */
export const DEFAULT_WORK_ITEM_CLOSE_REASON_OPTIONS: readonly WorkItemCloseReasonOption[] =
    [
        {
            code: "MISROUTED",
            label: "分派错误（误派任务）",
            requiresComment: true,
        },
        {
            code: "DUPLICATE",
            label: "任务重复（已有替代任务）",
            requiresReplacement: true,
        },
    ]

async function runWithConcurrency<T, R>(
    items: readonly T[],
    concurrency: number,
    worker: (item: T) => Promise<R>,
): Promise<R[]> {
    if (items.length === 0) return []
    const results: (R | undefined)[] = Array.from({ length: items.length })
    let cursor = 0
    const workerCount = Math.max(1, Math.min(concurrency, items.length))
    await Promise.all(
        Array.from({ length: workerCount }, async () => {
            while (cursor < items.length) {
                const index = cursor
                cursor += 1
                results[index] = await worker(items[index])
            }
        }),
    )
    return results as R[]
}

function intersectReassignCandidates(
    lists: readonly (readonly WorkItemReassignCandidate[])[],
): WorkItemReassignCandidate[] {
    if (lists.length === 0) return []
    let commonIds = new Set(lists[0].map((candidate) => candidate.user_id))
    for (let index = 1; index < lists.length; index += 1) {
        const nextIds = new Set(
            lists[index].map((candidate) => candidate.user_id),
        )
        commonIds = new Set([...commonIds].filter((id) => nextIds.has(id)))
        if (commonIds.size === 0) return []
    }
    return lists[0].filter((candidate) => commonIds.has(candidate.user_id))
}

function firstFieldErrorMessage(
    errors: unknown[] | undefined,
): string | undefined {
    return toFieldErrors(errors).find((item) => item?.message)?.message
}

type ExecutionOutcome =
    | { ok: true; id: string }
    | { ok: false; id: string; label: string; message: string }

/**
 * 统一任务责任管理弹窗：支持单条与批量 `REASSIGN`（转交）及 `CLOSE`（受控关闭）。
 *
 * - `reassign`：按任务并发拉取候选接收人并自动求交集，原因必填且 ≤ 150 字；
 * - `close`：严格对齐后端 `reason_code`、`comment ≤ 100 字` 及 `DUPLICATE` 替代任务编号校验；
 * - 提交统一以并发度 3 调度并回报 `{ succeededIds, failed }`。
 */
export function WorkItemResponsibilityDialog({
    mode,
    targets,
    closeReasonOptions = DEFAULT_WORK_ITEM_CLOSE_REASON_OPTIONS,
    open,
    onOpenChange,
    onCompleted,
    idPrefix,
    title,
    description,
}: WorkItemResponsibilityDialogProps) {
    const isBatch = targets.length > 1
    const singleTarget = targets.length === 1 ? targets[0] : undefined
    const resolvedPrefix =
        idPrefix ??
        (mode === "reassign"
            ? "work-item-responsibility-reassign"
            : "work-item-responsibility-close")
    const idSuffix = singleTarget
        ? `-${toAutomationIdSegment(singleTarget.workItemId)}`
        : "-batch"

    const availableCloseOptions = React.useMemo(
        () =>
            isBatch
                ? closeReasonOptions.filter(
                      (option) => !option.requiresReplacement,
                  )
                : closeReasonOptions,
        [closeReasonOptions, isBatch],
    )

    const defaultCloseReasonCode =
        availableCloseOptions.length === 1 ? availableCloseOptions[0].code : ""

    const queryClient = useQueryClient()
    const [submitFailures, setSubmitFailures] = React.useState<
        readonly { id: string; label: string; message: string }[]
    >([])
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const intentKeysRef = React.useRef(
        new Map<string, { fingerprint: string; key: string }>(),
    )

    const candidateQueries = useQueries({
        queries: targets.map((target) => ({
            queryKey: workItemKeys.reassignCandidates(target.workItemId),
            queryFn: () => getWorkItemReassignCandidates(target.workItemId),
            enabled:
                open &&
                mode === "reassign" &&
                target.workItemId.trim().length > 0,
        })),
    })

    const candidatesPending =
        mode === "reassign" && candidateQueries.some((query) => query.isPending)
    const candidatesFetching =
        mode === "reassign" &&
        candidateQueries.some((query) => query.isFetching)
    const candidateErrorQuery =
        mode === "reassign"
            ? candidateQueries.find((query) => query.isError)
            : undefined
    const candidatesReady =
        mode === "reassign" &&
        targets.length > 0 &&
        candidateQueries.every((query) => query.isSuccess)

    const eligibleCandidates = React.useMemo(() => {
        if (!candidatesReady) return []
        return intersectReassignCandidates(
            candidateQueries.map((query) => query.data ?? []),
        )
    }, [candidateQueries, candidatesReady])

    const candidateOptions = React.useMemo(
        () =>
            eligibleCandidates.map((candidate) => ({
                value: candidate.user_id,
                label: `${candidate.display_name} · ${candidate.account}`,
                keywords: `${candidate.display_name} ${candidate.account}`,
            })),
        [eligibleCandidates],
    )

    const emptyBatchIntersection =
        mode === "reassign" &&
        isBatch &&
        candidatesReady &&
        eligibleCandidates.length === 0

    const refetchCandidates = React.useCallback(async () => {
        const results = await Promise.all(
            candidateQueries.map((query) => query.refetch()),
        )
        const failed = results.find((result) => result.error)
        if (failed?.error) throw failed.error
    }, [candidateQueries])

    const acquireIdempotencyKey = React.useCallback(
        (workItemId: string, fingerprint: string): string => {
            const existing = intentKeysRef.current.get(workItemId)
            if (existing && existing.fingerprint === fingerprint) {
                return existing.key
            }
            const nextKey = `work-item-${mode}:${workItemId}:${crypto.randomUUID()}`
            intentKeysRef.current.set(workItemId, {
                fingerprint,
                key: nextKey,
            })
            return nextKey
        },
        [mode],
    )

    const batchMutation = useMutation({
        meta: { affectsDataScope: true },
        mutationFn: async (
            commands: readonly {
                target: WorkItemResponsibilityTarget
                command: WorkItemResponsibilityCommand
            }[],
        ): Promise<ExecutionOutcome[]> => {
            const outcomes = await runWithConcurrency(
                commands,
                BATCH_CONCURRENCY,
                async ({ target, command }): Promise<ExecutionOutcome> => {
                    try {
                        await submitWorkItemResponsibility(command)
                        intentKeysRef.current.delete(target.workItemId)
                        return { ok: true, id: target.workItemId }
                    } catch (cause) {
                        await synchronizeWorkItemConflict(
                            queryClient,
                            command,
                            cause,
                        )
                        return {
                            ok: false,
                            id: target.workItemId,
                            label: target.label,
                            message: getErrorMessage(
                                cause,
                                mode === "reassign"
                                    ? "任务责任未转交，请刷新后重试"
                                    : "任务未关闭，请刷新后重试",
                            ),
                        }
                    }
                },
            )
            if (outcomes.some((outcome) => outcome.ok)) {
                await Promise.all([
                    queryClient.invalidateQueries({
                        queryKey: workItemKeys.all,
                    }),
                    queryClient.invalidateQueries({
                        queryKey: queryKeyRoots.workspaceHome,
                    }),
                ])
            }
            return outcomes
        },
    })

    const reassignSchema = React.useMemo(
        () =>
            z.object({
                targetUserId: z.string().min(1, "请选择新责任人"),
                reason: workItemReassignReasonSchema,
            }),
        [],
    )

    const closeSchema = React.useMemo(
        () =>
            z
                .object({
                    reasonCode: z.string().min(1, "请选择关闭原因"),
                    comment: workItemCloseCommentSchema,
                    replacementWorkItemId: z.string(),
                })
                .superRefine((value, ctx) => {
                    const option = availableCloseOptions.find(
                        (entry) => entry.code === value.reasonCode,
                    )
                    if (!option) {
                        ctx.addIssue({
                            code: "custom",
                            path: ["reasonCode"],
                            message: "请选择有效的关闭原因",
                        })
                        return
                    }
                    if (option.requiresComment && !value.comment.trim()) {
                        ctx.addIssue({
                            code: "custom",
                            path: ["comment"],
                            message: "请说明任务分派错误的原因",
                        })
                    }
                    if (option.requiresReplacement) {
                        const replacement = value.replacementWorkItemId.trim()
                        if (!replacement) {
                            ctx.addIssue({
                                code: "custom",
                                path: ["replacementWorkItemId"],
                                message: "请填写替代任务编号",
                            })
                        } else if (
                            Array.from(replacement).length >
                            REPLACEMENT_ID_MAX_CHARS
                        ) {
                            ctx.addIssue({
                                code: "custom",
                                path: ["replacementWorkItemId"],
                                message: `替代任务编号不能超过 ${REPLACEMENT_ID_MAX_CHARS} 个字符`,
                            })
                        } else if (replacement.includes(";")) {
                            ctx.addIssue({
                                code: "custom",
                                path: ["replacementWorkItemId"],
                                message: "替代任务编号不能包含分号",
                            })
                        } else if (
                            singleTarget &&
                            replacement === singleTarget.workItemId
                        ) {
                            ctx.addIssue({
                                code: "custom",
                                path: ["replacementWorkItemId"],
                                message: "替代任务不能与当前任务相同",
                            })
                        }
                    }
                }),
        [availableCloseOptions, singleTarget],
    )

    const form = useAppForm({
        defaultValues: {
            targetUserId: "",
            reason: "",
            reasonCode: defaultCloseReasonCode,
            comment: "",
            replacementWorkItemId: "",
        },
        validators: {
            onChange: ({ value }) => {
                if (mode === "reassign") {
                    const parsed = reassignSchema.safeParse({
                        targetUserId: value.targetUserId,
                        reason: value.reason,
                    })
                    if (parsed.success) return undefined
                    return {
                        fields: Object.fromEntries(
                            parsed.error.issues.map((issue) => [
                                String(issue.path[0] ?? "reason"),
                                issue.message,
                            ]),
                        ),
                    }
                }
                const parsed = closeSchema.safeParse({
                    reasonCode: value.reasonCode,
                    comment: value.comment,
                    replacementWorkItemId: value.replacementWorkItemId,
                })
                if (parsed.success) return undefined
                return {
                    fields: Object.fromEntries(
                        parsed.error.issues.map((issue) => [
                            String(issue.path[0] ?? "reasonCode"),
                            issue.message,
                        ]),
                    ),
                }
            },
        },
        onSubmit: async ({ value }) => {
            if (batchMutation.isPending || targets.length === 0) return
            setSubmitError(null)
            setSubmitFailures([])

            let commands: {
                target: WorkItemResponsibilityTarget
                command: WorkItemResponsibilityCommand
            }[]

            if (mode === "reassign") {
                const parsed = reassignSchema.safeParse({
                    targetUserId: value.targetUserId,
                    reason: value.reason,
                })
                if (!parsed.success) {
                    setSubmitError(
                        parsed.error.issues[0]?.message ??
                            "请核对接收人和转交原因",
                    )
                    return
                }
                if (
                    !candidateOptions.some(
                        (option) => option.value === parsed.data.targetUserId,
                    )
                ) {
                    setSubmitError("请选择当前合格的任务接收人")
                    return
                }
                const normalizedReason = parsed.data.reason.trim()
                commands = targets.map((target) => {
                    const fingerprint = JSON.stringify({
                        kind: "REASSIGN",
                        workItemId: target.workItemId,
                        expectedTaskVersion: target.expectedTaskVersion,
                        targetUserId: parsed.data.targetUserId,
                        reason: normalizedReason,
                    })
                    return {
                        target,
                        command: {
                            kind: "REASSIGN",
                            workItemId: target.workItemId,
                            expectedTaskVersion: target.expectedTaskVersion,
                            targetUserId: parsed.data.targetUserId,
                            reason: normalizedReason,
                            idempotencyKey: acquireIdempotencyKey(
                                target.workItemId,
                                fingerprint,
                            ),
                        },
                    }
                })
            } else {
                const parsed = closeSchema.safeParse({
                    reasonCode: value.reasonCode,
                    comment: value.comment,
                    replacementWorkItemId: value.replacementWorkItemId,
                })
                if (!parsed.success) {
                    setSubmitError(
                        parsed.error.issues[0]?.message ??
                            "请核对关闭原因与说明",
                    )
                    return
                }
                const selectedOption = availableCloseOptions.find(
                    (option) => option.code === parsed.data.reasonCode,
                )
                if (!selectedOption) {
                    setSubmitError("请选择有效的关闭原因")
                    return
                }
                const normalizedComment =
                    parsed.data.comment.trim() || undefined
                const normalizedReplacement = selectedOption.requiresReplacement
                    ? parsed.data.replacementWorkItemId.trim()
                    : undefined
                commands = targets.map((target) => {
                    const fingerprint = JSON.stringify({
                        kind: "CLOSE",
                        workItemId: target.workItemId,
                        expectedTaskVersion: target.expectedTaskVersion,
                        reasonCode: selectedOption.code,
                        comment: normalizedComment ?? null,
                        replacementWorkItemId: normalizedReplacement ?? null,
                    })
                    return {
                        target,
                        command: {
                            kind: "CLOSE",
                            workItemId: target.workItemId,
                            expectedTaskVersion: target.expectedTaskVersion,
                            reasonCode: selectedOption.code,
                            comment: normalizedComment,
                            replacementWorkItemId: normalizedReplacement,
                            idempotencyKey: acquireIdempotencyKey(
                                target.workItemId,
                                fingerprint,
                            ),
                        },
                    }
                })
            }

            const outcomes = await batchMutation.mutateAsync(commands)
            const succeededIds = outcomes
                .filter(
                    (outcome): outcome is { ok: true; id: string } =>
                        outcome.ok,
                )
                .map((outcome) => outcome.id)
            const failedDetails = outcomes.filter(
                (
                    outcome,
                ): outcome is {
                    ok: false
                    id: string
                    label: string
                    message: string
                } => !outcome.ok,
            )
            const summary: WorkItemResponsibilitySummary = {
                succeededIds,
                failed: failedDetails.map(({ id, message }) => ({
                    id,
                    message,
                })),
            }

            if (failedDetails.length === 0) {
                toast.add({
                    title:
                        mode === "reassign"
                            ? isBatch
                                ? `已成功转交 ${succeededIds.length} 项任务`
                                : "已转交处理人"
                            : isBatch
                              ? `已关闭 ${succeededIds.length} 项任务`
                              : "任务已关闭",
                    type: "success",
                })
                onOpenChange(false)
                onCompleted(summary)
                return
            }

            setSubmitFailures(failedDetails)
            if (succeededIds.length > 0) {
                toast.add({
                    title:
                        mode === "reassign"
                            ? `已转交 ${succeededIds.length} 项，${failedDetails.length} 项未完成`
                            : `已关闭 ${succeededIds.length} 项，${failedDetails.length} 项未完成`,
                    type: "warning",
                })
                onCompleted(summary)
            } else if (failedDetails.length === 1) {
                setSubmitError(failedDetails[0].message)
            }
        },
    })

    const targetsIdentity = React.useMemo(
        () =>
            targets
                .map(
                    (target) =>
                        `${target.workItemId}:${target.expectedTaskVersion}`,
                )
                .join(","),
        [targets],
    )

    React.useEffect(() => {
        if (!open) return
        form.reset({
            targetUserId: "",
            reason: "",
            reasonCode: defaultCloseReasonCode,
            comment: "",
            replacementWorkItemId: "",
        })
        setSubmitError(null)
        setSubmitFailures([])
        intentKeysRef.current.clear()
    }, [defaultCloseReasonCode, form, mode, open, targetsIdentity])

    const formValues = useStore(form.store, (state) => state.values)
    const selectedCloseOption = availableCloseOptions.find(
        (option) => option.code === formValues.reasonCode,
    )

    const resolvedTitle =
        title ??
        (mode === "reassign"
            ? isBatch
                ? `批量转交 ${targets.length} 项任务`
                : "转交任务"
            : isBatch
              ? `批量关闭 ${targets.length} 项任务`
              : "关闭任务")

    const resolvedDescription =
        description ??
        (mode === "reassign"
            ? singleTarget?.currentOwnerLabel
                ? `当前责任人：${singleTarget.currentOwnerLabel}。请从符合处理资格的人员中选择新责任人并填写转交原因。`
                : isBatch
                  ? `已选择 ${targets.length} 项任务，仅可转交给同时具备全部所选任务处理权限的接收人。`
                  : "请从符合处理资格的人员中选择新责任人并填写转交原因。"
            : isBatch
              ? `已选择 ${targets.length} 项任务。关闭后任务将退出待处理队列，原业务记录保持不变；重复任务合并请单条关闭。`
              : "受控关闭仅终结无效异常任务，不改变已登记的业务记录。")

    const canSubmitReassign =
        mode === "reassign" &&
        !batchMutation.isPending &&
        !candidatesPending &&
        !candidateErrorQuery &&
        !emptyBatchIntersection &&
        targets.length > 0 &&
        candidateOptions.some(
            (option) => option.value === formValues.targetUserId,
        ) &&
        workItemReassignReasonSchema.safeParse(formValues.reason).success

    const canSubmitClose =
        mode === "close" &&
        !batchMutation.isPending &&
        targets.length > 0 &&
        availableCloseOptions.length > 0 &&
        closeSchema.safeParse({
            reasonCode: formValues.reasonCode,
            comment: formValues.comment,
            replacementWorkItemId: formValues.replacementWorkItemId,
        }).success

    const submitDisabled =
        mode === "reassign" ? !canSubmitReassign : !canSubmitClose

    const handleCtrlEnterSubmit = React.useCallback(() => {
        if (!submitDisabled) {
            void form.handleSubmit()
        }
    }, [form, submitDisabled])

    const closeOptionItems = React.useMemo(
        () =>
            availableCloseOptions.map((option) => ({
                value: option.code,
                label: option.label,
            })),
        [availableCloseOptions],
    )

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next && batchMutation.isPending) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                id={`${resolvedPrefix}-dialog${idSuffix}`}
                closeButtonId={`${resolvedPrefix}-close${idSuffix}`}
                showCloseButton={!batchMutation.isPending}
            >
                <DialogHeader>
                    <DialogTitle>{resolvedTitle}</DialogTitle>
                    <DialogDescription>{resolvedDescription}</DialogDescription>
                </DialogHeader>

                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    {isBatch ? (
                        <div className="rounded-md border border-border/70 bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
                            <p className="font-medium text-foreground">
                                待处理目标（{targets.length} 项）
                            </p>
                            <ul className="mt-1 max-h-24 space-y-0.5 overflow-y-auto">
                                {targets.map((target) => (
                                    <li
                                        key={target.workItemId}
                                        className="truncate"
                                        title={target.label}
                                    >
                                        {target.label}
                                    </li>
                                ))}
                            </ul>
                        </div>
                    ) : null}

                    {candidateErrorQuery ? (
                        <Alert variant="destructive">
                            <AlertTitle>候选人员加载失败</AlertTitle>
                            <AlertDescription className="flex flex-wrap items-center justify-between gap-2">
                                <span>
                                    {getErrorMessage(
                                        candidateErrorQuery.error,
                                        "请刷新候选人员后重试",
                                    )}
                                </span>
                                <Button
                                    id={`${resolvedPrefix}-retry-candidates${idSuffix}`}
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    onClick={() => void refetchCandidates()}
                                >
                                    重新加载
                                </Button>
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {emptyBatchIntersection ? (
                        <Alert variant="warning">
                            <AlertTitle>没有共同的合格接收人</AlertTitle>
                            <AlertDescription>
                                所选任务的权限范围不一致，没有同时具备全部任务处理资格的接收人，请按任务类型分批转交。
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {submitError ? (
                        <Alert variant="destructive">
                            <AlertTitle>
                                {mode === "reassign"
                                    ? "转交未完成"
                                    : "关闭未完成"}
                            </AlertTitle>
                            <AlertDescription>{submitError}</AlertDescription>
                        </Alert>
                    ) : null}

                    {submitFailures.length > 1 ||
                    (submitFailures.length === 1 && !submitError) ? (
                        <Alert variant="destructive">
                            <AlertTitle>
                                以下 {submitFailures.length} 项未完成
                            </AlertTitle>
                            <AlertDescription>
                                <ul className="mt-1 max-h-28 list-disc space-y-1 overflow-y-auto pl-4">
                                    {submitFailures.map((failure) => (
                                        <li key={failure.id}>
                                            <span className="font-medium">
                                                {failure.label}
                                            </span>
                                            ：{failure.message}
                                        </li>
                                    ))}
                                </ul>
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {mode === "reassign" ? (
                        <FieldGroup>
                            <form.AppField name="targetUserId">
                                {(field) => (
                                    <field.SelectField
                                        id={`${resolvedPrefix}-target${idSuffix}`}
                                        label="新责任人"
                                        options={candidateOptions}
                                        onRefresh={refetchCandidates}
                                        placeholder="选择接收人"
                                        emptyLabel={
                                            isBatch
                                                ? "暂无共同符合处理权限的员工"
                                                : "暂无符合处理权限的员工"
                                        }
                                        loading={candidatesFetching}
                                        disabled={
                                            batchMutation.isPending ||
                                            candidatesPending ||
                                            emptyBatchIntersection
                                        }
                                        allowClear={false}
                                        required
                                    />
                                )}
                            </form.AppField>

                            <form.Field name="reason">
                                {(field) => (
                                    <ReasonTextareaField
                                        id={`${resolvedPrefix}-reason${idSuffix}`}
                                        name={field.name}
                                        mode="chars"
                                        max={
                                            WORK_ITEM_REASSIGN_REASON_MAX_CHARS
                                        }
                                        label="转交原因"
                                        required
                                        rows={3}
                                        placeholder="说明本次责任调整依据"
                                        value={field.state.value}
                                        onChange={field.handleChange}
                                        onBlur={field.handleBlur}
                                        disabled={batchMutation.isPending}
                                        error={
                                            field.state.meta.isTouched
                                                ? firstFieldErrorMessage(
                                                      field.state.meta.errors,
                                                  )
                                                : undefined
                                        }
                                        onCtrlEnterSubmit={
                                            handleCtrlEnterSubmit
                                        }
                                    />
                                )}
                            </form.Field>
                        </FieldGroup>
                    ) : (
                        <FieldGroup>
                            <form.AppField name="reasonCode">
                                {(field) => (
                                    <field.SelectField
                                        id={`${resolvedPrefix}-reason-code${idSuffix}`}
                                        label="关闭原因"
                                        options={closeOptionItems}
                                        placeholder="选择关闭原因"
                                        emptyLabel="暂无可用关闭原因"
                                        disabled={batchMutation.isPending}
                                        allowClear={false}
                                        required
                                    />
                                )}
                            </form.AppField>

                            {selectedCloseOption?.requiresReplacement &&
                            !isBatch ? (
                                <form.AppField name="replacementWorkItemId">
                                    {(field) => (
                                        <field.TextField
                                            id={`${resolvedPrefix}-replacement${idSuffix}`}
                                            label="替代任务编号"
                                            placeholder="输入同类型有效替代任务的编号"
                                            description="仅可关联同一异常类别的开放正式任务，不能填写当前任务自身。"
                                            disabled={batchMutation.isPending}
                                            required
                                        />
                                    )}
                                </form.AppField>
                            ) : null}

                            <form.Field name="comment">
                                {(field) => (
                                    <ReasonTextareaField
                                        id={`${resolvedPrefix}-comment${idSuffix}`}
                                        name={field.name}
                                        mode="chars"
                                        max={WORK_ITEM_CLOSE_COMMENT_MAX_CHARS}
                                        label="关闭说明"
                                        required={Boolean(
                                            selectedCloseOption?.requiresComment,
                                        )}
                                        rows={3}
                                        placeholder={
                                            selectedCloseOption?.requiresComment
                                                ? "说明分派错误或无效原因（必填）"
                                                : "可选填写核实说明"
                                        }
                                        value={field.state.value}
                                        onChange={field.handleChange}
                                        onBlur={field.handleBlur}
                                        disabled={batchMutation.isPending}
                                        error={
                                            field.state.meta.isTouched
                                                ? firstFieldErrorMessage(
                                                      field.state.meta.errors,
                                                  )
                                                : undefined
                                        }
                                        onCtrlEnterSubmit={
                                            handleCtrlEnterSubmit
                                        }
                                    />
                                )}
                            </form.Field>
                        </FieldGroup>
                    )}

                    <DialogFooter>
                        <DialogClose
                            render={
                                <Button
                                    id={`${resolvedPrefix}-cancel${idSuffix}`}
                                    type="button"
                                    variant="outline"
                                    disabled={batchMutation.isPending}
                                />
                            }
                        >
                            取消
                        </DialogClose>
                        <form.AppForm>
                            <form.SubmitButton
                                id={`${resolvedPrefix}-submit${idSuffix}`}
                                showError={false}
                                label={
                                    mode === "reassign"
                                        ? "确认转交"
                                        : "确认关闭"
                                }
                                pendingLabel={
                                    mode === "reassign" ? "转交中…" : "关闭中…"
                                }
                                variant={
                                    mode === "close" ? "destructive" : "default"
                                }
                                loading={batchMutation.isPending}
                                disabled={submitDisabled}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
