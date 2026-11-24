"use client"

import * as React from "react"
import { keepPreviousData, useQuery } from "@tanstack/react-query"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceSection,
    WorkspaceTaskFooter,
    useWorkspaceTaskPane,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { FormalCommandKeyLedger } from "@/lib/formal-command"
import { cn } from "@/lib/utils"
import { utf8ByteLength } from "@/lib/validation/reason-length"
import { getErrorMessage } from "@/lib/api/errors"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/contracts"
import { AcceptanceTaskRegister } from "./acceptance-task-register"
import { ACCEPTANCE_REASON_MAX_BYTES } from "./acceptance-task-table"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"
import { fetchCustomerAcceptanceWorkspace } from "@/features/sales-orders/api/acceptance"
import {
    type AcceptanceHistoryItem,
    type AcceptanceOverallResult,
    type CustomerAcceptanceWorkspaceView,
    type PostAcceptanceInput,
} from "@/features/sales-orders/lib/acceptance-types"
import {
    acceptanceFactUnit,
    buildDraftLines,
    buildOrderProgress,
    pendingAsPassSelection,
    pendingFactsOf,
} from "@/features/sales-orders/lib/acceptance-model"
import { salesOrderKeys } from "@/features/sales-orders/hooks/queries"
import { useAcceptanceWorkspaceUrlState } from "@/features/sales-orders/hooks/acceptance-url-state"
import { useAcceptanceSelection } from "@/features/sales-orders/hooks/use-acceptance-selection"
import { useAcceptanceForm } from "@/features/sales-orders/hooks/use-acceptance-form"
import { useAcceptanceMutations } from "@/features/sales-orders/hooks/use-acceptance-mutations"
import type { AcceptanceTaskIdentity } from "@/features/sales-orders/lib/acceptance-workspace-fetch"
import { mapWorkItemDto } from "@/features/work-items/types"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import {
    AcceptanceBlockedState,
    AcceptanceNoFactsState,
} from "@/features/sales-orders/components/acceptance-blocked-states"
import { AcceptanceDialogs } from "@/features/sales-orders/components/acceptance-dialogs"
import { AcceptanceFormalResult } from "@/features/sales-orders/components/acceptance-formal-result"
import { AcceptanceHistoryList } from "@/features/sales-orders/components/acceptance-history-list"
import {
    AcceptanceProgressTable,
    AcceptanceSummaryMetrics,
} from "@/features/sales-orders/components/acceptance-progress-table"
import { AcceptanceRegisterDialog } from "@/features/sales-orders/components/acceptance-register-dialog"

type AcceptancePostIntent = Omit<
    PostAcceptanceInput,
    "idempotencyKey" | "acceptanceDraftId" | "evidenceFile"
>

export function AcceptanceWorkspace({
    id,
    idPrefix,
    salesOrderId,
    ownerUserId,
    ownerName,
    workItem,
    persistRegisterInUrl = true,
    inlineRegister = false,
    taskBlocker,
    referenceOrder,
    renderTaskPane,
    onPosted,
}: {
    id?: string
    idPrefix?: string
    salesOrderId: string
    ownerUserId?: string
    ownerName?: string
    workItem?: AcceptanceTaskIdentity
    persistRegisterInUrl?: boolean
    /** 在工作台作业面内编制和核对本次验收。 */
    inlineRegister?: boolean
    taskBlocker?: string
    referenceOrder?: SalesOrderDetailView | null
    /** 作业面切换参考栏时，表单与提交身份始终由此组件持有。 */
    renderTaskPane?: (body: React.ReactNode) => React.ReactNode
    onPosted?: (payload: {
        remainingEligibleCount: number
        acceptanceNo: string
    }) => void
}) {
    const {
        workItemId: urlWorkItemId,
        isRegister: urlRegister,
        setRegisterMode: setUrlRegisterMode,
    } = useAcceptanceWorkspaceUrlState()
    const [localRegister, setLocalRegister] = React.useState(false)
    const [dialogStep, setDialogStep] = React.useState<"edit" | "confirm">(
        "edit",
    )
    const workItemId = workItem?.workItemId ?? urlWorkItemId
    const isRegister = persistRegisterInUrl ? urlRegister : localRegister
    const setRegisterMode = React.useCallback(
        (next: boolean, options?: { clearTask?: boolean }) => {
            if (!next) {
                setDialogStep("edit")
            }
            if (!persistRegisterInUrl) {
                setLocalRegister(next)
                return
            }
            setUrlRegisterMode(next, options)
        },
        [persistRegisterInUrl, setUrlRegisterMode],
    )
    const inheritedTask = workItem
    const workItemQuery = useWorkItemDetailQuery(workItemId ?? "")
    const resolvedWorkItem = workItemQuery.data
        ? mapWorkItemDto(workItemQuery.data)
        : inheritedTask
    const waitingForTask =
        Boolean(workItemId) && !resolvedWorkItem && workItemQuery.isPending
    const accountQuery = useAccountProfileQuery()
    const selection = useAcceptanceSelection()
    const [reviewing, setReviewing] = React.useState(false)
    const [reverseError, setReverseError] = React.useState("")
    const [ledger] = React.useState(
        () =>
            new FormalCommandKeyLedger(
                (prefix) => `${prefix}-${crypto.randomUUID()}`,
            ),
    )
    const [reverseTarget, setReverseTarget] =
        React.useState<AcceptanceHistoryItem | null>(null)
    const [reverseReason, setReverseReason] = React.useState("")
    const reverseIdempotencyKeyRef = React.useRef("")
    const resultRef = React.useRef<HTMLDivElement>(null)
    const submittedOverallRef = React.useRef<AcceptanceOverallResult>("PASS")
    const pendingPostLinesRef = React.useRef(selection.selected)
    const postSlot = `register:${workItemId ?? salesOrderId}`
    const postingRef = React.useRef(false)
    const pendingEvidenceRef = React.useRef<File | null>(null)
    const pendingViewRef = React.useRef<CustomerAcceptanceWorkspaceView | null>(
        null,
    )
    const pendingPost = ledger.peek<AcceptancePostIntent>(postSlot)
    const pendingPostWasUnknownRef = React.useRef(false)

    const { form, clientIssues } = useAcceptanceForm({
        selected: selection.selected,
        onValidSubmit: () => {
            if (!ledger.peek(postSlot)) {
                pendingPostLinesRef.current = selection.selected
            }
            if (inlineRegister) {
                setReviewing(true)
            } else {
                setDialogStep("confirm")
            }
        },
    })

    const workspaceQuery = useQuery({
        queryKey: salesOrderKeys.acceptance(salesOrderId, {
            workItemId,
            expectedTaskVersion: resolvedWorkItem?.taskVersion,
        }),
        queryFn: () =>
            fetchCustomerAcceptanceWorkspace({
                salesOrderId,
                workItemId,
                workItem: resolvedWorkItem,
            }),
        enabled:
            (!taskBlocker || Boolean(pendingPost)) &&
            (!workItemId ||
                Boolean(resolvedWorkItem) ||
                workItemQuery.isFetched),
        placeholderData: inlineRegister ? keepPreviousData : undefined,
    })

    const raw = workspaceQuery.data
    const { names } = useProductUnitNames(
        raw?.salesLines.map((line) => line.skuId),
        Boolean(raw?.salesLines.length),
    )
    const queriedView = React.useMemo(
        () =>
            raw && {
                ...raw,
                history: raw.history.map((item) => ({
                    ...item,
                    lines: item.lines.map((line) => ({
                        ...line,
                        unitName: names.get(line.unitCode.trim()),
                    })),
                })),
                salesLines: raw.salesLines.map((line) => {
                    const unitName =
                        line.unitName ?? names.get(line.unitCode.trim())
                    return {
                        ...line,
                        unitName,
                        fulfillmentFacts: line.fulfillmentFacts.map((fact) =>
                            acceptanceFactUnit(
                                fact,
                                { unitCode: line.unitCode, unitName },
                                names,
                            ),
                        ),
                    }
                }),
            },
        [raw, names],
    )

    const view =
        inlineRegister && pendingPost && pendingViewRef.current
            ? pendingViewRef.current
            : queriedView

    const { postMutation, reverseMutation, formalResult, setFormalResult } =
        useAcceptanceMutations({
            salesOrderId,
            unitNames: names,
            idempotencyKey: pendingPost?.idempotencyKey ?? "",
            submittedOverallRef,
            onPostSucceeded: (payload) => {
                ledger.settle(postSlot, "succeeded")
                pendingEvidenceRef.current = null
                pendingViewRef.current = null
                pendingPostWasUnknownRef.current = false
                inlinePrefillKey.current = undefined
                selection.reset()
                form.reset()
                submittedOverallRef.current = "PASS"
                setReviewing(false)
                setDialogStep("edit")
                setRegisterMode(false, {
                    clearTask:
                        persistRegisterInUrl &&
                        payload.remainingEligibleCount === 0,
                })
                onPosted?.(payload)
            },
            onReverseSucceeded: () => {
                setReverseTarget(null)
                setReverseReason("")
                reverseIdempotencyKeyRef.current = ""
            },
        })

    React.useEffect(() => {
        if (formalResult) resultRef.current?.focus()
    }, [formalResult])

    const hasTaskPane = useWorkspaceTaskPane()
    const inWorkspaceTaskPane = hasTaskPane || Boolean(renderTaskPane)
    const wrapTask = (body: React.ReactNode) =>
        renderTaskPane ? renderTaskPane(body) : body
    const baseId =
        idPrefix ??
        id ??
        (inlineRegister
            ? `sales-orders-acceptance-${toAutomationIdSegment(salesOrderId)}`
            : undefined)
    const recovering = Boolean(
        pendingPost && formalResult?.status === "unknown",
    )
    const insetClassName = inWorkspaceTaskPane ? "py-4" : undefined

    const prefilledOpenRef = React.useRef(false)
    const inlinePrefillKey = React.useRef<string>(undefined)
    const replaceSelection = selection.replace
    const registerOpen = Boolean(
        !inlineRegister &&
        isRegister &&
        view &&
        view.salesOrder.businessType !== "CARD_VOUCHER",
    )
    const pendingFactsKey = view
        ? pendingFactsOf(view.salesLines)
              .map(
                  (fact) =>
                      `${fact.fulfillmentLineId}:${fact.eligibleQuantity}`,
              )
              .join("|")
        : ""

    React.useEffect(() => {
        if (inlineRegister) {
            if (
                !view ||
                view.salesOrder.businessType === "CARD_VOUCHER" ||
                workspaceQuery.isPlaceholderData ||
                inlinePrefillKey.current === pendingFactsKey ||
                ledger.peek(postSlot)
            )
                return
            const initial = inlinePrefillKey.current === undefined
            inlinePrefillKey.current = pendingFactsKey
            if (initial) {
                replaceSelection(pendingAsPassSelection(view.salesLines))
            } else {
                const reconciled = new Map()
                for (const fact of pendingFactsOf(view.salesLines)) {
                    const draft = selection.selected.get(fact.fulfillmentLineId)
                    if (draft)
                        reconciled.set(fact.fulfillmentLineId, {
                            ...draft,
                            fact,
                        })
                }
                replaceSelection(reconciled)
                setReviewing(false)
            }
            return
        }
        if (!registerOpen || !view) {
            prefilledOpenRef.current = false
            return
        }
        if (prefilledOpenRef.current) return
        prefilledOpenRef.current = true
        if (ledger.peek(postSlot)) return
        replaceSelection(pendingAsPassSelection(view.salesLines))
    }, [
        inlineRegister,
        ledger,
        pendingFactsKey,
        pendingPost,
        postSlot,
        registerOpen,
        replaceSelection,
        selection.selected,
        view,
        workspaceQuery.isPlaceholderData,
    ])

    const currentUserId = accountQuery.data?.userid
    const ownerId =
        view?.salesOrder.ownerUserId ||
        resolvedWorkItem?.ownerUser?.id ||
        ownerUserId
    const ownerLabel =
        view?.salesOrder.ownerName ||
        resolvedWorkItem?.ownerUser?.displayName ||
        ownerName ||
        ""
    const isOwner = !ownerId || !currentUserId || currentUserId === ownerId

    if (taskBlocker && !pendingPost && !view) {
        return wrapTask(
            <div
                role="alert"
                className="my-3 rounded-r-md border-l-2 border-l-warning bg-warning-soft px-3 py-2 text-xs text-warning-soft-foreground"
            >
                {taskBlocker}
            </div>,
        )
    }

    if (
        (waitingForTask || workspaceQuery.isPending) &&
        !(pendingPost && view)
    ) {
        if (!inWorkspaceTaskPane) {
            return (
                <DetailWorkspaceSection width="form">
                    <div
                        className="min-h-40 animate-pulse rounded-md bg-muted/50"
                        aria-busy="true"
                        aria-label="正在加载客户验收"
                    />
                </DetailWorkspaceSection>
            )
        }
        return wrapTask(
            <div className={insetClassName}>
                <div
                    className="min-h-48 animate-pulse rounded-md bg-muted/40"
                    aria-busy="true"
                    aria-label="正在加载客户验收"
                />
            </div>,
        )
    }

    if (workspaceQuery.isError && !(inlineRegister && pendingPost && view)) {
        const failure = (
            <BusinessFailureState
                title="验收内容加载失败"
                error={workspaceQuery.error}
                action={
                    <Button
                        id={
                            baseId
                                ? `${baseId}-retry`
                                : "sales-orders-acceptance-retry"
                        }
                        type="button"
                        onClick={() => void workspaceQuery.refetch()}
                    >
                        重试
                    </Button>
                }
            />
        )
        if (!inWorkspaceTaskPane) {
            return (
                <DetailWorkspaceSection width="form">
                    {failure}
                </DetailWorkspaceSection>
            )
        }
        return wrapTask(<div className={insetClassName}>{failure}</div>)
    }

    if (!view) {
        const empty = (
            <BusinessEmptyState
                kind="no-data"
                title="暂无可验收内容"
                description="请返回销售单检查当前状态。"
            />
        )
        if (!inWorkspaceTaskPane) {
            return (
                <DetailWorkspaceSection width="form">
                    {empty}
                </DetailWorkspaceSection>
            )
        }
        return wrapTask(<div className={insetClassName}>{empty}</div>)
    }

    const acceptanceEntryBlocked = Boolean(
        taskBlocker || view.workItemConfigBlocker,
    )
    const canPost =
        !acceptanceEntryBlocked &&
        !workspaceQuery.isPlaceholderData &&
        (!inlineRegister || accountQuery.isSuccess) &&
        isOwner &&
        view.permissions.allowedActions.includes("POST_ACCEPTANCE")
    const canCreate =
        !acceptanceEntryBlocked &&
        isOwner &&
        view.permissions.allowedActions.includes("CREATE_ACCEPTANCE")
    const postBlocker = view.permissions.actionBlockers.find(
        (blocker) =>
            blocker.action === "POST_ACCEPTANCE" ||
            blocker.action === "CREATE_ACCEPTANCE",
    )
    const isCard = view.salesOrder.businessType === "CARD_VOUCHER"
    const progress = buildOrderProgress(view.salesLines)
    const pendingCount = pendingFactsOf(view.salesLines).length
    const canRegister = canCreate && pendingCount > 0
    const closeRegister = () => {
        if (!ledger.peek(postSlot)) {
            selection.reset()
            form.reset()
        }
        setRegisterMode(false)
    }

    const requestCloseRegister = () => {
        if (postMutation.isPending) return
        closeRegister()
    }

    const progressHint = canRegister
        ? `还有 ${progress.pendingFactCount} 批待客户验收。`
        : !isOwner && pendingCount > 0
          ? `还有待验批次，由${ownerLabel || "负责销售"}登记。`
          : undefined
    const pageFormalResult =
        registerOpen &&
        pendingCount > 0 &&
        formalResult &&
        formalResult.status !== "succeeded"
            ? null
            : formalResult
    const openRegister = () => {
        prefilledOpenRef.current = true
        if (!ledger.peek(postSlot)) {
            selection.replace(pendingAsPassSelection(view.salesLines))
        }
        setDialogStep("edit")
        setRegisterMode(true)
    }
    const registerButton = (
        <Button
            id="sales-orders-acceptance-register-open"
            type="button"
            size="sm"
            className={!inWorkspaceTaskPane ? "px-3.5" : undefined}
            disabled={!canPost}
            onClick={openRegister}
        >
            登记客户验收
        </Button>
    )
    const confirmSelected = pendingPost
        ? pendingPostLinesRef.current
        : selection.selected
    const confirmOverallPreview = pendingPost
        ? submittedOverallRef.current
        : selection.overallPreview

    const handleConfirmAcceptance = async () => {
        if (postingRef.current) return
        const existing = ledger.peek<AcceptancePostIntent>(postSlot)
        if (!existing && (!canPost || (inlineRegister && !reviewing))) return

        const values = form.state.values
        const identity =
            existing ??
            ledger.acquire<AcceptancePostIntent>(
                postSlot,
                `acc-${salesOrderId}`,
                {
                    workItemId: view.workItem?.id,
                    expectedTaskVersion: view.workItem?.expectedTaskVersion,
                    salesOrderId,
                    expectedDraftVersion: 0,
                    expectedSalesOrderLockVersion: view.salesOrder.lockVersion,
                    acceptedAt: values.acceptedAt
                        ? new Date(values.acceptedAt).toISOString()
                        : new Date().toISOString(),
                    comment: values.comment,
                    lines: buildDraftLines(
                        inlineRegister
                            ? selection.selected
                            : pendingPostLinesRef.current,
                    ),
                },
            )
        if (!existing) {
            pendingEvidenceRef.current = values.evidenceFile
            pendingViewRef.current = view
            submittedOverallRef.current = selection.overallPreview
        }

        postingRef.current = true
        try {
            const result = await postMutation.mutateAsync({
                ...identity.payload,
                evidenceFile: pendingEvidenceRef.current,
                acceptanceDraftId: `draft_${identity.idempotencyKey}`,
                idempotencyKey: identity.idempotencyKey,
            })
            if (result.status === "unknown") {
                pendingPostWasUnknownRef.current = true
            } else if (result.status === "failed") {
                if (pendingPostWasUnknownRef.current) {
                    setFormalResult({
                        kind: "post",
                        status: "unknown",
                        title: "尚未确认上次提交结果",
                        description: result.message,
                        facts: [
                            {
                                label: "原任务号",
                                value: identity.idempotencyKey,
                            },
                        ],
                    })
                } else {
                    ledger.settle(postSlot, "failed")
                    pendingEvidenceRef.current = null
                    pendingViewRef.current = null
                    if (inlineRegister) setReviewing(false)
                }
            }
        } catch {
            pendingPostWasUnknownRef.current = true
            setFormalResult({
                kind: "post",
                status: "unknown",
                title: "验收提交结果待确认",
                description: "请按原提交内容核对结果，避免重复登记。",
                facts: [
                    {
                        label: "原任务号",
                        value: identity.idempotencyKey,
                    },
                ],
            })
        } finally {
            postingRef.current = false
        }
    }

    const openReverse = (item: AcceptanceHistoryItem) => {
        setReverseTarget(item)
        setReverseReason("")
        setReverseError("")
        reverseIdempotencyKeyRef.current = `rev-${item.acceptanceId}-${crypto.randomUUID()}`
    }

    const closeReverse = () => {
        setReverseTarget(null)
        setReverseReason("")
        setReverseError("")
        reverseIdempotencyKeyRef.current = ""
    }

    const confirmReverse = async () => {
        if (!reverseTarget) return
        if (!reverseReason.trim()) throw new Error("请填写冲正理由")
        await reverseMutation.mutateAsync({
            salesOrderId,
            acceptanceId: reverseTarget.acceptanceId,
            originalAcceptanceNo: reverseTarget.acceptanceNo,
            expectedAcceptanceVersion: reverseTarget.version,
            reasonText: reverseReason.trim(),
            idempotencyKey: reverseIdempotencyKeyRef.current,
        })
    }

    if (inlineRegister && baseId && !isCard) {
        const historyId =
            view.history.length > 0 ? `${baseId}-history` : undefined
        const progressId =
            progress.lines.length > 0 ? `${baseId}-progress` : undefined
        const blocker =
            taskBlocker ??
            view.workItemConfigBlocker ??
            postBlocker?.message ??
            (!isOwner
                ? `由${ownerLabel || "负责销售"}登记客户验收。`
                : undefined)
        const matchedOrder =
            (referenceOrder?.lockVersion ?? referenceOrder?.version) ===
            view.salesOrder.lockVersion
                ? referenceOrder
                : undefined
        return wrapTask(
            <div
                id={baseId}
                className="@container/acceptance-task flex min-w-0 flex-col bg-card"
            >
                {formalResult && !recovering ? (
                    <div
                        id={`${baseId}-result`}
                        tabIndex={-1}
                        ref={resultRef}
                        role={
                            formalResult.status === "failed"
                                ? "alert"
                                : "status"
                        }
                        className={cn(
                            "mt-3 flex flex-wrap items-center justify-between gap-2 rounded-r-md border-l-2 px-3 py-2 text-xs outline-none",
                            formalResult.status === "failed"
                                ? "border-l-destructive bg-destructive-soft text-destructive-soft-foreground"
                                : "border-l-success bg-success-soft text-success-soft-foreground",
                        )}
                    >
                        <div className="min-w-0 wrap-anywhere">
                            <p className="font-semibold">
                                {formalResult.title}
                            </p>
                            <p>{formalResult.description}</p>
                            {formalResult.reference ? (
                                <p className="num select-all">
                                    {formalResult.reference}
                                </p>
                            ) : null}
                        </div>
                        {formalResult.status === "failed" ? (
                            <Button
                                id={`${baseId}-refresh-result`}
                                type="button"
                                variant="ghost"
                                size="xs"
                                onClick={() => {
                                    void workItemQuery.refetch()
                                    void workspaceQuery.refetch()
                                }}
                            >
                                刷新最新数据
                            </Button>
                        ) : (
                            <Button
                                id={`${baseId}-dismiss-result`}
                                type="button"
                                variant="ghost"
                                size="xs"
                                onClick={() => setFormalResult(null)}
                            >
                                关闭
                            </Button>
                        )}
                    </div>
                ) : null}

                <AcceptanceTaskRegister
                    idPrefix={baseId}
                    facts={pendingFactsOf(view.salesLines)}
                    progress={progress}
                    referenceOrder={matchedOrder}
                    selection={selection}
                    confirmSelected={confirmSelected}
                    form={form}
                    blocker={blocker}
                    canPost={canPost && canCreate && !reverseTarget}
                    busy={postMutation.isPending || reverseMutation.isPending}
                    reviewing={reviewing}
                    pendingOutcome={recovering}
                    idempotencyKey={pendingPost?.idempotencyKey}
                    result={formalResult}
                    onConfirm={handleConfirmAcceptance}
                    onBack={() => setReviewing(false)}
                    relatedSection={
                        historyId
                            ? { id: historyId, label: "验收记录" }
                            : progressId
                              ? { id: progressId, label: "验收进度" }
                              : undefined
                    }
                />

                {progressId ? (
                    <details
                        id={progressId}
                        className="border-t border-border/60 py-3"
                    >
                        <summary
                            id={`${baseId}-progress-toggle`}
                            className="w-fit cursor-pointer text-xs text-muted-foreground hover:text-foreground"
                        >
                            查看整单验收进度
                        </summary>
                        <AcceptanceProgressTable
                            surface="detail"
                            progress={progress}
                            showSummary={false}
                            compact
                        />
                    </details>
                ) : null}

                {historyId ? (
                    <details
                        id={historyId}
                        className="border-t border-border/60 py-3"
                    >
                        <summary
                            id={`${historyId}-toggle`}
                            className="w-fit cursor-pointer text-xs text-muted-foreground hover:text-foreground"
                        >
                            验收记录{" "}
                            <span className="num">{view.history.length}</span>
                        </summary>
                        {reverseTarget ? (
                            <section className="mt-2 rounded-r-md border-l-2 border-l-warning bg-warning-soft/50 px-3 py-2.5">
                                <h2 className="text-body-compact font-semibold text-foreground">
                                    冲正验收记录
                                </h2>
                                <p className="text-xs text-warning-soft-foreground">
                                    原记录将保留，对应批次会恢复待验。
                                </p>
                                <ReasonTextareaField
                                    id={`${historyId}-reverse-reason`}
                                    className="mt-2"
                                    mode="utf8-bytes"
                                    label="冲正理由"
                                    value={reverseReason}
                                    onChange={setReverseReason}
                                    disabled={reverseMutation.isPending}
                                    required
                                    error={reverseError || undefined}
                                />
                                <div className="mt-2 flex flex-wrap justify-end gap-2">
                                    <Button
                                        id={`${historyId}-reverse-cancel`}
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        disabled={reverseMutation.isPending}
                                        onClick={closeReverse}
                                    >
                                        取消
                                    </Button>
                                    <LoadingButton
                                        id={`${historyId}-reverse-confirm`}
                                        type="button"
                                        size="sm"
                                        variant="destructive"
                                        loading={reverseMutation.isPending}
                                        disabled={
                                            !reverseReason.trim() ||
                                            utf8ByteLength(reverseReason) >
                                                ACCEPTANCE_REASON_MAX_BYTES
                                        }
                                        onClick={() =>
                                            void confirmReverse().catch(
                                                (error: unknown) =>
                                                    setReverseError(
                                                        getErrorMessage(
                                                            error,
                                                            "冲正失败，请稍后重试。",
                                                        ),
                                                    ),
                                            )
                                        }
                                    >
                                        确认冲正
                                    </LoadingButton>
                                </div>
                            </section>
                        ) : null}
                        <AcceptanceHistoryList
                            surface="detail"
                            history={view.history}
                            showGuidance={false}
                            canReverse={
                                !reviewing &&
                                !pendingPost &&
                                !reverseTarget &&
                                view.permissions.allowedActions.includes(
                                    "REVERSE_ACCEPTANCE",
                                )
                            }
                            onReverse={openReverse}
                        />
                    </details>
                ) : null}
            </div>,
        )
    }

    const body = (
        <>
            {view.workItemConfigBlocker ? (
                inWorkspaceTaskPane ? (
                    <div className={insetClassName}>
                        <Alert variant="warning" role="alert">
                            <AlertTitle>暂时不能从这条待办登记</AlertTitle>
                            <AlertDescription>
                                {view.workItemConfigBlocker}
                            </AlertDescription>
                        </Alert>
                    </div>
                ) : (
                    <DetailWorkspaceSection width="form">
                        <Alert variant="warning" role="alert">
                            <AlertTitle>暂时不能从这条待办登记</AlertTitle>
                            <AlertDescription>
                                {view.workItemConfigBlocker}
                            </AlertDescription>
                        </Alert>
                    </DetailWorkspaceSection>
                )
            ) : null}

            {pageFormalResult ? (
                inWorkspaceTaskPane ? (
                    <div className={insetClassName}>
                        <AcceptanceFormalResult
                            formalResult={pageFormalResult}
                            resultRef={resultRef}
                            onDismiss={() => setFormalResult(null)}
                            onRetry={() => {
                                setFormalResult(null)
                                if (inlineRegister) {
                                    setReviewing(true)
                                } else {
                                    if (!registerOpen) setRegisterMode(true)
                                    setDialogStep("confirm")
                                }
                            }}
                        />
                    </div>
                ) : (
                    <DetailWorkspaceSection width="form">
                        <AcceptanceFormalResult
                            formalResult={pageFormalResult}
                            resultRef={resultRef}
                            onDismiss={() => setFormalResult(null)}
                            onRetry={() => {
                                setFormalResult(null)
                                if (inlineRegister) {
                                    setReviewing(true)
                                } else {
                                    if (!registerOpen) setRegisterMode(true)
                                    setDialogStep("confirm")
                                }
                            }}
                        />
                    </DetailWorkspaceSection>
                )
            ) : null}

            {isCard ? (
                inWorkspaceTaskPane ? (
                    <div className={insetClassName}>
                        <AcceptanceBlockedState
                            isCard
                            blockerMessage={postBlocker?.message}
                        />
                    </div>
                ) : (
                    <DetailWorkspaceSection width="form">
                        <AcceptanceBlockedState
                            isCard
                            blockerMessage={postBlocker?.message}
                        />
                    </DetailWorkspaceSection>
                )
            ) : view.salesLines.length === 0 && view.history.length === 0 ? (
                inWorkspaceTaskPane ? (
                    <div className={insetClassName}>
                        <AcceptanceNoFactsState />
                    </div>
                ) : (
                    <DetailWorkspaceSection width="form">
                        <AcceptanceNoFactsState />
                    </DetailWorkspaceSection>
                )
            ) : (
                <>
                    {!inWorkspaceTaskPane ? (
                        <AcceptanceSummaryMetrics progress={progress} />
                    ) : null}

                    <AcceptanceProgressTable
                        surface="detail"
                        progress={progress}
                        showSummary={inWorkspaceTaskPane}
                        pendingHint={progressHint}
                        actions={
                            !inWorkspaceTaskPane && canRegister
                                ? registerButton
                                : undefined
                        }
                    />

                    {inWorkspaceTaskPane && canRegister ? (
                        <WorkspaceTaskFooter fallback={null}>
                            {registerButton}
                        </WorkspaceTaskFooter>
                    ) : null}

                    <AcceptanceHistoryList
                        surface="detail"
                        history={view.history}
                        canReverse={view.permissions.allowedActions.includes(
                            "REVERSE_ACCEPTANCE",
                        )}
                        onReverse={openReverse}
                    />
                </>
            )}

            {!inlineRegister ? (
                <AcceptanceRegisterDialog
                    open={registerOpen}
                    step={dialogStep}
                    onStepChange={setDialogStep}
                    form={form}
                    salesLines={view.salesLines}
                    selection={selection}
                    confirmSelected={confirmSelected}
                    overallPreview={confirmOverallPreview}
                    canPost={canPost && !pendingPost}
                    ownerLabel={ownerLabel}
                    isOwner={isOwner}
                    clientIssues={clientIssues}
                    postBlockerMessage={postBlocker?.message}
                    pendingCount={pendingCount}
                    postPending={postMutation.isPending}
                    onConfirmAcceptance={handleConfirmAcceptance}
                    onOpenChange={(open) => {
                        if (open) setRegisterMode(true)
                        else requestCloseRegister()
                    }}
                >
                    {registerOpen &&
                    formalResult &&
                    formalResult.status !== "succeeded" ? (
                        <div className="mb-4">
                            <AcceptanceFormalResult
                                formalResult={formalResult}
                                resultRef={resultRef}
                                onDismiss={() => setFormalResult(null)}
                                onRetry={() => {
                                    setFormalResult(null)
                                    setDialogStep("confirm")
                                }}
                            />
                        </div>
                    ) : null}
                </AcceptanceRegisterDialog>
            ) : null}

            {!inlineRegister ? (
                <AcceptanceDialogs
                    confirmOpen={false}
                    onConfirmOpenChange={setReviewing}
                    selected={confirmSelected}
                    overallPreview={confirmOverallPreview}
                    onConfirmAcceptance={handleConfirmAcceptance}
                    reverseTarget={reverseTarget}
                    onReverseOpenChange={(open) => {
                        if (!open) closeReverse()
                    }}
                    reverseReason={reverseReason}
                    onReverseReasonChange={setReverseReason}
                    onConfirmReverse={confirmReverse}
                />
            ) : null}
        </>
    )

    if (!inWorkspaceTaskPane && !baseId) {
        return body
    }

    return wrapTask(
        <div id={baseId} className="flex min-w-0 flex-col">
            {body}
        </div>,
    )
}
