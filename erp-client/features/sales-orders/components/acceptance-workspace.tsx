"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceSection,
    WorkspaceTaskFooter,
    useWorkspaceTaskPane,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { fetchCustomerAcceptanceWorkspace } from "@/features/sales-orders/api/acceptance"
import {
    type AcceptanceHistoryItem,
    type AcceptanceOverallResult,
    type PostAcceptanceInput,
} from "@/features/sales-orders/lib/acceptance-types"
import {
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
import {
    AcceptanceRegisterDialog,
    AcceptanceRegisterFormBody,
} from "@/features/sales-orders/components/acceptance-register-dialog"

export function AcceptanceWorkspace({
    id,
    idPrefix,
    salesOrderId,
    ownerUserId,
    ownerName,
    workItem,
    persistRegisterInUrl = true,
    inlineRegister = false,
    onPosted,
}: {
    id?: string
    idPrefix?: string
    salesOrderId: string
    ownerUserId?: string
    ownerName?: string
    workItem?: AcceptanceTaskIdentity
    persistRegisterInUrl?: boolean
    /** true 时直接在页面主画布内联渲染可编辑验收表单，仅底栏提交弹单层确认，零弹窗套娃。 */
    inlineRegister?: boolean
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
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [reverseTarget, setReverseTarget] =
        React.useState<AcceptanceHistoryItem | null>(null)
    const [reverseReason, setReverseReason] = React.useState("")
    const reverseIdempotencyKeyRef = React.useRef("")
    const [idempotencyKey, setIdempotencyKey] = React.useState(
        () => `acc-${salesOrderId}-${crypto.randomUUID()}`,
    )
    const resultRef = React.useRef<HTMLDivElement>(null)
    const submittedOverallRef = React.useRef<AcceptanceOverallResult>("PASS")
    const pendingPostLinesRef = React.useRef(selection.selected)
    const pendingPostInputRef = React.useRef<PostAcceptanceInput | null>(null)
    const pendingPostWasUnknownRef = React.useRef(false)

    const { form, clientIssues } = useAcceptanceForm({
        selected: selection.selected,
        onValidSubmit: () => {
            if (!pendingPostInputRef.current) {
                pendingPostLinesRef.current = selection.selected
            }
            if (inlineRegister) {
                setConfirmOpen(true)
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
            !workItemId || Boolean(resolvedWorkItem) || workItemQuery.isFetched,
    })

    const view = workspaceQuery.data

    const { postMutation, reverseMutation, formalResult, setFormalResult } =
        useAcceptanceMutations({
            salesOrderId,
            idempotencyKey,
            submittedOverallRef,
            onPostSucceeded: (payload) => {
                pendingPostInputRef.current = null
                pendingPostWasUnknownRef.current = false
                selection.reset()
                form.reset()
                setIdempotencyKey(`acc-${salesOrderId}-${crypto.randomUUID()}`)
                submittedOverallRef.current = "PASS"
                setConfirmOpen(false)
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

    const inWorkspaceTaskPane = useWorkspaceTaskPane()
    const insetClassName = inWorkspaceTaskPane ? "py-4" : undefined

    const prefilledOpenRef = React.useRef(false)
    const prefilledInlineKeyRef = React.useRef<string>("")
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
                !pendingFactsKey
            ) {
                prefilledInlineKeyRef.current = ""
                return
            }
            if (prefilledInlineKeyRef.current === pendingFactsKey) return
            prefilledInlineKeyRef.current = pendingFactsKey
            if (pendingPostInputRef.current) return
            replaceSelection(pendingAsPassSelection(view.salesLines))
            return
        }
        if (!registerOpen || !view) {
            prefilledOpenRef.current = false
            return
        }
        if (prefilledOpenRef.current) return
        prefilledOpenRef.current = true
        if (pendingPostInputRef.current) return
        replaceSelection(pendingAsPassSelection(view.salesLines))
    }, [inlineRegister, pendingFactsKey, registerOpen, replaceSelection, view])

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

    if (waitingForTask || workspaceQuery.isPending) {
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
        return (
            <div className={insetClassName}>
                <div
                    className="min-h-48 animate-pulse rounded-md bg-muted/40"
                    aria-busy="true"
                    aria-label="正在加载客户验收"
                />
            </div>
        )
    }

    if (workspaceQuery.isError) {
        const failure = (
            <BusinessFailureState
                title="验收内容加载失败"
                error={workspaceQuery.error}
                action={
                    <Button
                        id="sales-orders-acceptance-retry"
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
        return <div className={insetClassName}>{failure}</div>
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
        return <div className={insetClassName}>{empty}</div>
    }

    const acceptanceEntryBlocked = Boolean(view.workItemConfigBlocker)
    const canPost =
        !acceptanceEntryBlocked &&
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
        if (!pendingPostInputRef.current) {
            selection.reset()
            form.reset()
        }
        setRegisterMode(false)
    }

    const requestCloseRegister = () => {
        if (postMutation.isPending || confirmOpen) return
        closeRegister()
    }

    const baseId = idPrefix ?? id
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
        if (!pendingPostInputRef.current) {
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
    const inlinePassCount = [...selection.selected.values()].filter(
        (draft) => draft.result === "PASS",
    ).length
    const inlineExceptionCount = selection.selected.size - inlinePassCount
    const inlineSkippedCount = Math.max(
        0,
        pendingCount - selection.selected.size,
    )
    const confirmSelected = pendingPostInputRef.current
        ? pendingPostLinesRef.current
        : selection.selected
    const confirmOverallPreview = pendingPostInputRef.current
        ? submittedOverallRef.current
        : selection.overallPreview

    const handleConfirmAcceptance = async () => {
        if (!pendingPostInputRef.current) {
            submittedOverallRef.current = selection.overallPreview
            const values = form.state.values
            pendingPostInputRef.current = {
                workItemId: view.workItem?.id,
                expectedTaskVersion: view.workItem?.expectedTaskVersion,
                salesOrderId,
                acceptanceDraftId: `draft_${idempotencyKey}`,
                expectedDraftVersion: 0,
                expectedSalesOrderLockVersion: view.salesOrder.lockVersion,
                idempotencyKey,
                acceptedAt: values.acceptedAt
                    ? new Date(values.acceptedAt).toISOString()
                    : new Date().toISOString(),
                comment: values.comment,
                evidenceFile: values.evidenceFile,
                lines: buildDraftLines(pendingPostLinesRef.current),
            }
        }
        try {
            const result = await postMutation.mutateAsync(
                pendingPostInputRef.current,
            )
            if (result.status === "unknown") {
                pendingPostWasUnknownRef.current = true
            } else if (result.status === "failed") {
                if (pendingPostWasUnknownRef.current) {
                    setFormalResult({
                        kind: "post",
                        status: "unknown",
                        title: "尚未确认上次提交结果",
                        description: `${result.message} 请保留原操作并重试。`,
                        facts: [],
                    })
                } else {
                    pendingPostInputRef.current = null
                    setIdempotencyKey(
                        `acc-${salesOrderId}-${crypto.randomUUID()}`,
                    )
                }
            }
        } catch {
            pendingPostWasUnknownRef.current = true
        }
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
                                    setConfirmOpen(true)
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
                                    setConfirmOpen(true)
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

                    {inlineRegister && canRegister ? (
                        <DetailWorkspaceSection
                            width="full"
                            title="登记本次客户验收"
                            subtitle={`默认纳入全部 ${pendingCount} 批并预填为全部通过，核对并上传签收单凭证后即可直接提交。`}
                        >
                            <AcceptanceRegisterFormBody
                                inline
                                form={form}
                                salesLines={view.salesLines}
                                selection={selection}
                                canPost={
                                    canPost && !pendingPostInputRef.current
                                }
                                ownerLabel={ownerLabel}
                                isOwner={isOwner}
                                clientIssues={clientIssues}
                                postBlockerMessage={postBlocker?.message}
                                postPending={postMutation.isPending}
                                activeKey={pendingFactsKey}
                            />
                        </DetailWorkspaceSection>
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
                        inlineRegister ? (
                            <WorkspaceTaskFooter
                                summary={
                                    <p
                                        className="text-xs text-muted-foreground"
                                        role="status"
                                        aria-live="polite"
                                    >
                                        <span className="font-medium text-foreground">
                                            本次登记 {selection.selected.size}{" "}
                                            批
                                        </span>
                                        {" · "}全部通过 {inlinePassCount} 批 ·
                                        有异常 {inlineExceptionCount} 批 ·
                                        暂不登记 {inlineSkippedCount} 批
                                    </p>
                                }
                                fallback={null}
                            >
                                <form.AppForm>
                                    <form.SubmitButton
                                        id="sales-orders-acceptance-register-submit"
                                        form="acceptance-form"
                                        size="sm"
                                        label="提交本次验收"
                                        pendingLabel="提交中…"
                                        loading={postMutation.isPending}
                                        disabled={
                                            !canPost ||
                                            postMutation.isPending ||
                                            selection.selected.size === 0 ||
                                            Boolean(pendingPostInputRef.current)
                                        }
                                    />
                                </form.AppForm>
                            </WorkspaceTaskFooter>
                        ) : (
                            <WorkspaceTaskFooter fallback={null}>
                                {registerButton}
                            </WorkspaceTaskFooter>
                        )
                    ) : null}

                    <AcceptanceHistoryList
                        surface="detail"
                        history={view.history}
                        canReverse={view.permissions.allowedActions.includes(
                            "REVERSE_ACCEPTANCE",
                        )}
                        onReverse={(item) => {
                            setReverseTarget(item)
                            setReverseReason("")
                            reverseIdempotencyKeyRef.current = `rev-${item.acceptanceId}-${crypto.randomUUID()}`
                        }}
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
                    canPost={canPost && !pendingPostInputRef.current}
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

            <AcceptanceDialogs
                confirmOpen={inlineRegister && confirmOpen}
                onConfirmOpenChange={setConfirmOpen}
                selected={confirmSelected}
                overallPreview={confirmOverallPreview}
                onConfirmAcceptance={handleConfirmAcceptance}
                reverseTarget={reverseTarget}
                onReverseOpenChange={(open) => {
                    if (!open) {
                        setReverseTarget(null)
                        setReverseReason("")
                        reverseIdempotencyKeyRef.current = ""
                    }
                }}
                reverseReason={reverseReason}
                onReverseReasonChange={setReverseReason}
                onConfirmReverse={async () => {
                    if (!reverseTarget) return
                    if (!reverseReason.trim()) {
                        throw new Error("请填写冲正理由")
                    }
                    if (!reverseIdempotencyKeyRef.current) {
                        reverseIdempotencyKeyRef.current = `rev-${reverseTarget.acceptanceId}-${crypto.randomUUID()}`
                    }
                    await reverseMutation.mutateAsync({
                        salesOrderId,
                        acceptanceId: reverseTarget.acceptanceId,
                        originalAcceptanceNo: reverseTarget.acceptanceNo,
                        expectedAcceptanceVersion: reverseTarget.version,
                        reasonText: reverseReason.trim(),
                        idempotencyKey: reverseIdempotencyKeyRef.current,
                    })
                }}
            />
        </>
    )

    if (!inWorkspaceTaskPane && !baseId) {
        return body
    }

    return (
        <div id={baseId} className="flex min-w-0 flex-col">
            {body}
        </div>
    )
}
