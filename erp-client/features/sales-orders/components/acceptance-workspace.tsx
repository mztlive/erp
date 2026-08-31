"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"

import {
    BusinessEmptyState,
    BusinessFailureState,
    WorkspaceTaskFooter,
    useWorkspaceTaskPane,
    workspaceTaskSurfacePadClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { fetchCustomerAcceptanceWorkspace } from "@/features/sales-orders/api/acceptance"
import {
    type AcceptanceHistoryItem,
    type AcceptanceOverallResult,
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
import { cn } from "@/lib/utils"
import {
    AcceptanceBlockedState,
    AcceptanceNoFactsState,
} from "@/features/sales-orders/components/acceptance-blocked-states"
import { AcceptanceDialogs } from "@/features/sales-orders/components/acceptance-dialogs"
import { AcceptanceFormalResult } from "@/features/sales-orders/components/acceptance-formal-result"
import { AcceptanceHistoryList } from "@/features/sales-orders/components/acceptance-history-list"
import { AcceptanceProgressTable } from "@/features/sales-orders/components/acceptance-progress-table"
import { AcceptanceRegisterDialog } from "@/features/sales-orders/components/acceptance-register-dialog"

export function AcceptanceWorkspace({
    id,
    idPrefix,
    salesOrderId,
    ownerUserId,
    ownerName,
    workItem,
    persistRegisterInUrl = true,
    onPosted,
}: {
    id?: string
    idPrefix?: string
    salesOrderId: string
    ownerUserId?: string
    ownerName?: string
    workItem?: AcceptanceTaskIdentity
    persistRegisterInUrl?: boolean
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
    const workItemId = workItem?.workItemId ?? urlWorkItemId
    const isRegister = persistRegisterInUrl ? urlRegister : localRegister
    const setRegisterMode = React.useCallback(
        (next: boolean, options?: { clearTask?: boolean }) => {
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
    const [idempotencyKey, setIdempotencyKey] = React.useState(
        () => `acc-${salesOrderId}-${crypto.randomUUID()}`,
    )
    const [exitDiscardOpen, setExitDiscardOpen] = React.useState(false)
    const resultRef = React.useRef<HTMLDivElement>(null)
    const submittedOverallRef = React.useRef<AcceptanceOverallResult>("PASS")
    const pendingPostLinesRef = React.useRef(selection.selected)

    const { form, formDirty, clientIssues } = useAcceptanceForm({
        selected: selection.selected,
        onValidSubmit: () => {
            pendingPostLinesRef.current = selection.selected
            setConfirmOpen(true)
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
                selection.reset()
                form.reset()
                setIdempotencyKey(`acc-${salesOrderId}-${crypto.randomUUID()}`)
                submittedOverallRef.current = "PASS"
                setConfirmOpen(false)
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
            },
        })

    React.useEffect(() => {
        if (formalResult) resultRef.current?.focus()
    }, [formalResult])

    const inWorkspaceTaskPane = useWorkspaceTaskPane()
    const insetClassName = inWorkspaceTaskPane
        ? cn(workspaceTaskSurfacePadClassName, "py-5")
        : undefined
    const sectionClassName = inWorkspaceTaskPane
        ? workspaceTaskSurfacePadClassName
        : "py-0"

    const prefilledOpenRef = React.useRef(false)
    const replaceSelection = selection.replace
    const registerOpen = Boolean(
        isRegister && view && view.salesOrder.businessType !== "CARD_VOUCHER",
    )

    React.useEffect(() => {
        if (!registerOpen || !view) {
            prefilledOpenRef.current = false
            return
        }
        if (prefilledOpenRef.current) return
        prefilledOpenRef.current = true
        replaceSelection(pendingAsPassSelection(view.salesLines))
    }, [registerOpen, replaceSelection, view])

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
        return (
            <div className={insetClassName}>
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
            </div>
        )
    }

    if (!view) {
        return (
            <div className={insetClassName}>
                <BusinessEmptyState
                    kind="no-data"
                    title="暂无可验收内容"
                    description="请返回销售单检查当前状态。"
                />
            </div>
        )
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
    const hasUnsavedInput =
        formDirty ||
        selection.hasExceptionResult ||
        selection.selected.size !== pendingCount

    const closeRegister = () => {
        selection.reset()
        form.reset()
        setRegisterMode(false)
    }

    const requestCloseRegister = () => {
        if (postMutation.isPending || confirmOpen) return
        if (hasUnsavedInput) {
            setExitDiscardOpen(true)
            return
        }
        closeRegister()
    }

    const baseId = idPrefix ?? id
    const progressHint = canRegister
        ? `还有 ${progress.pendingFactCount} 批待客户验收。`
        : !isOwner && pendingCount > 0
          ? `还有待验批次，由${ownerLabel || "负责销售"}登记。`
          : undefined
    const pageFormalResult =
        registerOpen && formalResult && formalResult.status !== "succeeded"
            ? null
            : formalResult
    const openRegister = () => {
        prefilledOpenRef.current = true
        selection.replace(pendingAsPassSelection(view.salesLines))
        setRegisterMode(true)
    }
    const registerButton = (
        <Button
            id="sales-orders-acceptance-register-open"
            type="button"
            size="sm"
            disabled={!canPost}
            onClick={openRegister}
        >
            登记客户验收
        </Button>
    )
    return (
        <div
            id={baseId}
            className={cn(
                "flex min-w-0 flex-col",
                !inWorkspaceTaskPane && "gap-4",
            )}
        >
            {view.workItemConfigBlocker ? (
                <div className={insetClassName}>
                    <Alert variant="warning" role="alert">
                        <AlertTitle>暂时不能从这条待办登记</AlertTitle>
                        <AlertDescription>
                            {view.workItemConfigBlocker}
                        </AlertDescription>
                    </Alert>
                </div>
            ) : null}

            {pageFormalResult ? (
                <div className={insetClassName}>
                    <AcceptanceFormalResult
                        formalResult={pageFormalResult}
                        resultRef={resultRef}
                        onDismiss={() => setFormalResult(null)}
                        onRetry={() => {
                            setFormalResult(null)
                            if (!registerOpen) setRegisterMode(true)
                            setConfirmOpen(true)
                        }}
                    />
                </div>
            ) : null}

            {isCard ? (
                <div className={insetClassName}>
                    <AcceptanceBlockedState
                        isCard
                        blockerMessage={postBlocker?.message}
                    />
                </div>
            ) : view.salesLines.length === 0 && view.history.length === 0 ? (
                <div className={insetClassName}>
                    <AcceptanceNoFactsState />
                </div>
            ) : (
                <>
                    <AcceptanceProgressTable
                        progress={progress}
                        pendingHint={
                            inWorkspaceTaskPane ? progressHint : undefined
                        }
                        className={sectionClassName}
                    />

                    {canRegister ? (
                        <WorkspaceTaskFooter
                            fallback={
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <p className="text-sm text-muted-foreground">
                                        {progressHint}
                                    </p>
                                    {registerButton}
                                </div>
                            }
                        >
                            {registerButton}
                        </WorkspaceTaskFooter>
                    ) : !isOwner && pendingCount > 0 && !inWorkspaceTaskPane ? (
                        <p className="text-sm text-muted-foreground">
                            {progressHint}
                        </p>
                    ) : null}

                    <AcceptanceHistoryList
                        history={view.history}
                        canReverse={view.permissions.allowedActions.includes(
                            "REVERSE_ACCEPTANCE",
                        )}
                        onReverse={(item) => {
                            setReverseTarget(item)
                            setReverseReason("")
                        }}
                        className={sectionClassName}
                    />
                </>
            )}

            <AcceptanceRegisterDialog
                open={registerOpen}
                form={form}
                salesLines={view.salesLines}
                selection={selection}
                canPost={canPost}
                ownerLabel={ownerLabel}
                isOwner={isOwner}
                clientIssues={clientIssues}
                postBlockerMessage={postBlocker?.message}
                pendingCount={pendingCount}
                postPending={postMutation.isPending}
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
                                setConfirmOpen(true)
                            }}
                        />
                    </div>
                ) : null}
            </AcceptanceRegisterDialog>

            <AcceptanceDialogs
                confirmOpen={confirmOpen}
                onConfirmOpenChange={setConfirmOpen}
                overallPreview={selection.overallPreview}
                hasExceptionResult={selection.hasExceptionResult}
                onConfirmAcceptance={async () => {
                    submittedOverallRef.current = selection.overallPreview
                    const values = form.state.values
                    const lines = buildDraftLines(pendingPostLinesRef.current)
                    await postMutation.mutateAsync({
                        workItemId: view.workItem?.id,
                        expectedTaskVersion: view.workItem?.expectedTaskVersion,
                        salesOrderId,
                        acceptanceDraftId: `draft_${idempotencyKey}`,
                        expectedDraftVersion: 0,
                        expectedSalesOrderLockVersion:
                            view.salesOrder.lockVersion,
                        idempotencyKey,
                        acceptedAt: values.acceptedAt
                            ? new Date(values.acceptedAt).toISOString()
                            : new Date().toISOString(),
                        comment: values.comment,
                        lines,
                    })
                }}
                reverseTarget={reverseTarget}
                onReverseOpenChange={(open) => {
                    if (!open) {
                        setReverseTarget(null)
                        setReverseReason("")
                    }
                }}
                reverseReason={reverseReason}
                onReverseReasonChange={setReverseReason}
                onConfirmReverse={async () => {
                    if (!reverseTarget) return
                    if (!reverseReason.trim()) {
                        throw new Error("请填写冲正理由")
                    }
                    await reverseMutation.mutateAsync({
                        salesOrderId,
                        acceptanceId: reverseTarget.acceptanceId,
                        expectedAcceptanceVersion: reverseTarget.version,
                        reasonText: reverseReason.trim(),
                        idempotencyKey: `rev-${reverseTarget.acceptanceId}-${crypto.randomUUID()}`,
                    })
                }}
                exitDiscardOpen={exitDiscardOpen}
                onExitDiscardOpenChange={setExitDiscardOpen}
                onConfirmExit={() => {
                    setExitDiscardOpen(false)
                    closeRegister()
                }}
            />
        </div>
    )
}
