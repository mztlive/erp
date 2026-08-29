"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
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
    defaultBatchDraft,
    pendingFactsOf,
    type AcceptanceBatchSelection,
} from "@/features/sales-orders/lib/acceptance-model"
import { salesOrderKeys } from "@/features/sales-orders/hooks/queries"
import { useAcceptanceWorkspaceUrlState } from "@/features/sales-orders/hooks/acceptance-url-state"
import { useAcceptanceSelection } from "@/features/sales-orders/hooks/use-acceptance-selection"
import { useAcceptanceForm } from "@/features/sales-orders/hooks/use-acceptance-form"
import { useAcceptanceMutations } from "@/features/sales-orders/hooks/use-acceptance-mutations"
import type { WorkItemProjection } from "@/features/work-items/types"
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
    salesOrderId,
    ownerUserId,
    ownerName,
    workItem,
}: {
    salesOrderId: string
    ownerUserId?: string
    ownerName?: string
    workItem?: WorkItemProjection
}) {
    const { workItemId, isRegister, setRegisterMode } =
        useAcceptanceWorkspaceUrlState()
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
            expectedTaskVersion: workItem?.taskVersion,
        }),
        queryFn: () =>
            fetchCustomerAcceptanceWorkspace({
                salesOrderId,
                workItemId,
                workItem,
            }),
    })

    const view = workspaceQuery.data

    const { postMutation, reverseMutation, formalResult, setFormalResult } =
        useAcceptanceMutations({
            salesOrderId,
            idempotencyKey,
            submittedOverallRef,
            onPostSucceeded: () => {
                selection.reset()
                form.reset()
                setIdempotencyKey(`acc-${salesOrderId}-${crypto.randomUUID()}`)
                submittedOverallRef.current = "PASS"
                setConfirmOpen(false)
                setRegisterMode(false, { clearTask: true })
            },
            onReverseSucceeded: () => {
                setReverseTarget(null)
                setReverseReason("")
            },
        })

    React.useEffect(() => {
        if (formalResult) resultRef.current?.focus()
    }, [formalResult])

    const currentUserId = accountQuery.data?.userid
    const ownerId = workItem?.ownerUser?.id || ownerUserId
    const ownerLabel = workItem?.ownerUser?.displayName || ownerName || ""
    const isOwner = !ownerId || !currentUserId || currentUserId === ownerId

    if (workspaceQuery.isPending) {
        return (
            <div
                className="min-h-48 animate-pulse rounded-md bg-muted/40"
                aria-busy="true"
                aria-label="正在加载客户验收"
            />
        )
    }

    if (workspaceQuery.isError) {
        return (
            <BusinessFailureState
                title="验收内容加载失败"
                error={workspaceQuery.error}
                action={
                    <Button
                        type="button"
                        onClick={() => void workspaceQuery.refetch()}
                    >
                        重试
                    </Button>
                }
            />
        )
    }

    if (!view) {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="暂无可验收内容"
                description="请返回销售单检查当前状态。"
            />
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
    const hasUnsavedInput = formDirty || selection.selected.size > 0
    const registerOpen = isRegister && !isCard

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

    return (
        <div className="flex min-w-0 flex-col gap-4">
            {view.workItemConfigBlocker ? (
                <Alert variant="warning" role="alert">
                    <AlertTitle>暂时不能从这条待办登记</AlertTitle>
                    <AlertDescription>
                        {view.workItemConfigBlocker}
                    </AlertDescription>
                </Alert>
            ) : null}

            <AcceptanceFormalResult
                formalResult={
                    registerOpen &&
                    formalResult &&
                    formalResult.status !== "succeeded"
                        ? null
                        : formalResult
                }
                resultRef={resultRef}
                onDismiss={() => setFormalResult(null)}
                onRetry={() => {
                    setFormalResult(null)
                    if (!registerOpen) setRegisterMode(true)
                    setConfirmOpen(true)
                }}
            />

            {isCard ? (
                <AcceptanceBlockedState
                    isCard
                    blockerMessage={postBlocker?.message}
                />
            ) : view.salesLines.length === 0 && view.history.length === 0 ? (
                <AcceptanceNoFactsState />
            ) : (
                <>
                    <AcceptanceProgressTable progress={progress} />

                    {canRegister ? (
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <p className="text-sm text-muted-foreground">
                                还有 {progress.pendingFactCount} 批待客户验收。
                            </p>
                            <Button
                                type="button"
                                size="sm"
                                disabled={!canPost}
                                onClick={() => setRegisterMode(true)}
                            >
                                登记客户验收
                            </Button>
                        </div>
                    ) : !isOwner && pendingCount > 0 ? (
                        <p className="text-sm text-muted-foreground">
                            还有待验批次，由{ownerLabel || "负责销售"}
                            登记。
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
                onPassAll={() => {
                    const next: AcceptanceBatchSelection = new Map()
                    for (const fact of pendingFactsOf(view.salesLines)) {
                        next.set(
                            fact.fulfillmentLineId,
                            defaultBatchDraft(fact),
                        )
                    }
                    selection.replace(next)
                    pendingPostLinesRef.current = next
                    submittedOverallRef.current = "PASS"
                    setConfirmOpen(true)
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
