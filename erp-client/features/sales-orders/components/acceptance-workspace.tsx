"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useQuery } from "@tanstack/react-query"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { fetchCustomerAcceptanceWorkspace } from "@/features/sales-orders/api/acceptance"
import {
    type AcceptanceHistoryItem,
    type AcceptanceOverallResult,
} from "@/features/sales-orders/lib/acceptance-types"
import { buildDraftLines } from "@/features/sales-orders/lib/acceptance-model"
import { salesOrderKeys } from "@/features/sales-orders/hooks/queries"
import { useAcceptanceWorkspaceUrlState } from "@/features/sales-orders/hooks/acceptance-url-state"
import { useAcceptanceSelection } from "@/features/sales-orders/hooks/use-acceptance-selection"
import { useAcceptanceForm } from "@/features/sales-orders/hooks/use-acceptance-form"
import { useAcceptanceMutations } from "@/features/sales-orders/hooks/use-acceptance-mutations"
import {
    AcceptanceBlockedState,
    AcceptanceNoFactsState,
} from "@/features/sales-orders/components/acceptance-blocked-states"
import { AcceptanceDialogs } from "@/features/sales-orders/components/acceptance-dialogs"
import { AcceptanceEntryForm } from "@/features/sales-orders/components/acceptance-entry-form"
import { AcceptanceFactPool } from "@/features/sales-orders/components/acceptance-fact-pool"
import { AcceptanceFooterBar } from "@/features/sales-orders/components/acceptance-footer-bar"
import { AcceptanceFormalResult } from "@/features/sales-orders/components/acceptance-formal-result"
import { AcceptanceHistoryList } from "@/features/sales-orders/components/acceptance-history-list"
import { AcceptanceSummaryBar } from "@/features/sales-orders/components/acceptance-summary-bar"

export function AcceptanceWorkspace({
    salesOrderId,
}: {
    salesOrderId: string
}) {
    const router = useRouter()
    const { workItemId, remainingOnly, setRemainingOnly } =
        useAcceptanceWorkspaceUrlState()
    const selection = useAcceptanceSelection()
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [reverseTarget, setReverseTarget] =
        React.useState<AcceptanceHistoryItem | null>(null)
    const [reverseReason, setReverseReason] = React.useState("")
    const [idempotencyKey, setIdempotencyKey] = React.useState(
        () => `acc-${salesOrderId}-${crypto.randomUUID()}`,
    )
    const [draftSavedAt, setDraftSavedAt] = React.useState<string | null>(null)
    const [exitDiscardOpen, setExitDiscardOpen] = React.useState(false)
    const resultRef = React.useRef<HTMLDivElement>(null)
    const restoredDraftRef = React.useRef(false)
    /** 提交瞬间的总体结果快照（含服务不通过），用于结果反馈不被服务端降级。 */
    const submittedOverallRef = React.useRef<AcceptanceOverallResult>("PASS")

    const { form, formDirty, clientIssues } = useAcceptanceForm({
        selected: selection.selected,
        lineResults: selection.lineResults,
        onValidSubmit: () => setConfirmOpen(true),
    })

    const workspaceQuery = useQuery({
        queryKey: salesOrderKeys.acceptance(salesOrderId, {
            remainingOnly,
            workItemId,
        }),
        queryFn: () =>
            fetchCustomerAcceptanceWorkspace({
                salesOrderId,
                remainingOnly,
                workItemId,
            }),
    })

    const view = workspaceQuery.data

    const {
        saveDraftMutation,
        postMutation,
        reverseMutation,
        formalResult,
        setFormalResult,
    } = useAcceptanceMutations({
        salesOrderId,
        idempotencyKey,
        submittedOverallRef,
        setDraftSavedAt,
        onPostSucceeded: () => {
            selection.reset()
            form.reset()
            setIdempotencyKey(`acc-${salesOrderId}-${crypto.randomUUID()}`)
            restoredDraftRef.current = false
            submittedOverallRef.current = "PASS"
        },
        onReverseSucceeded: () => {
            setReverseTarget(null)
            setReverseReason("")
        },
    })

    const { restoreDraft } = selection

    // 恢复草稿（刷新后 session-state 仍在）
    React.useEffect(() => {
        if (!view?.draft || restoredDraftRef.current) return
        restoredDraftRef.current = true
        form.setFieldValue("acceptedAt", view.draft.acceptedAt.slice(0, 16))
        form.setFieldValue("comment", view.draft.comment)
        setDraftSavedAt(view.draft.updatedAt)
        restoreDraft(view.draft.lines, view.salesLines)
    }, [view, form, restoreDraft])

    React.useEffect(() => {
        if (formalResult) {
            resultRef.current?.focus()
        }
    }, [formalResult])

    const handleSaveDraft = React.useCallback(async () => {
        if (!view) return
        if (!view.permissions.allowedActions.includes("SAVE_DRAFT")) return
        const values = form.state.values
        const lines = buildDraftLines(selection.selected, selection.lineResults)
        await saveDraftMutation.mutateAsync({
            salesOrderId,
            acceptanceDraftId: view.draft?.acceptanceDraftId,
            expectedDraftVersion: view.draft?.draftVersion,
            acceptedAt: values.acceptedAt
                ? new Date(values.acceptedAt).toISOString()
                : new Date().toISOString(),
            comment: values.comment,
            lines,
        })
    }, [
        form,
        selection.lineResults,
        salesOrderId,
        saveDraftMutation,
        selection.selected,
        view,
    ])

    // 快捷键 ⌘S 保存草稿、⌘↵ 打开确认（界面在底栏给出提示）
    React.useEffect(() => {
        function onKeyDown(event: KeyboardEvent) {
            const meta = event.metaKey || event.ctrlKey
            if (!meta) return
            if (event.key === "s") {
                event.preventDefault()
                void handleSaveDraft()
            }
            if (event.key === "Enter") {
                event.preventDefault()
                void form.handleSubmit()
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [form, handleSaveDraft])

    if (workspaceQuery.isPending) {
        return (
            <div
                className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,62fr)_minmax(18rem,38fr)]"
                aria-busy="true"
                aria-label="正在加载客户验收工作区"
            >
                <div className="min-h-64 animate-pulse rounded-md bg-muted/40" />
                <div className="min-h-64 animate-pulse rounded-md bg-muted/40" />
            </div>
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
        view.permissions.allowedActions.includes("POST_ACCEPTANCE")
    const canSave =
        !acceptanceEntryBlocked &&
        view.permissions.allowedActions.includes("SAVE_DRAFT")
    const canCreate =
        !acceptanceEntryBlocked &&
        view.permissions.allowedActions.includes("CREATE_ACCEPTANCE")
    const postBlocker = view.permissions.actionBlockers.find(
        (b) =>
            b.action === "POST_ACCEPTANCE" || b.action === "CREATE_ACCEPTANCE",
    )
    const isCard = view.salesOrder.businessType === "CARD_VOUCHER"

    const hasUnsavedInput =
        formDirty ||
        selection.selected.size > 0 ||
        selection.lineResults.size > 0

    return (
        <div className="flex min-w-0 flex-col gap-4">
            {view.workItemConfigBlocker ? (
                <Alert variant="warning" role="alert">
                    <AlertTitle>验收入口暂不可用</AlertTitle>
                    <AlertDescription>
                        {view.workItemConfigBlocker}
                    </AlertDescription>
                </Alert>
            ) : null}

            <AcceptanceFormalResult
                formalResult={formalResult}
                resultRef={resultRef}
                onDismiss={() => {
                    setFormalResult(null)
                    const params = new URLSearchParams(window.location.search)
                    params.set("section", "fulfillment")
                    params.delete("mode")
                    const qs = params.toString()
                    router.replace(
                        qs
                            ? `/sales/orders/${salesOrderId}?${qs}`
                            : `/sales/orders/${salesOrderId}?section=fulfillment`,
                    )
                }}
                onRetry={() => {
                    setFormalResult(null)
                    setConfirmOpen(true)
                }}
            />

            <AcceptanceSummaryBar
                metrics={view.metrics}
                fulfillmentProgress={view.salesOrder.fulfillmentProgress}
                freshness={view.freshness}
                remainingOnly={remainingOnly}
                onRemainingOnlyChange={setRemainingOnly}
            />

            {isCard || !canCreate ? (
                <AcceptanceBlockedState
                    isCard={isCard}
                    blockerMessage={postBlocker?.message}
                    salesOrderId={salesOrderId}
                />
            ) : view.metrics.eligibleFulfillmentCount === 0 &&
              view.history.length === 0 ? (
                <AcceptanceNoFactsState />
            ) : (
                <>
                    {/* 1440：约 62/38；1024 以下单列 */}
                    <div className="grid min-w-0 gap-4 lg:grid-cols-1 xl:grid-cols-[minmax(0,62fr)_minmax(20rem,38fr)]">
                        <AcceptanceFactPool
                            salesLines={view.salesLines}
                            selected={selection.selected}
                            canPost={canPost}
                            onToggleFact={selection.toggleFact}
                            onAllocQtyChange={selection.setAllocQty}
                        />

                        <div className="space-y-4 xl:sticky xl:top-4 xl:self-start">
                            <AcceptanceEntryForm
                                form={form}
                                salesOrderNo={view.salesOrder.salesOrderNo}
                                customerLabel={view.salesOrder.customerLabel}
                                selection={selection}
                                canPost={canPost}
                                clientIssues={clientIssues}
                                postBlockerMessage={postBlocker?.message}
                                draftSavedAt={draftSavedAt}
                                draftVersion={view.draft?.draftVersion}
                            />

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
                        </div>
                    </div>

                    {/* 底栏：校验与主动作（桌面同屏） */}
                    <AcceptanceFooterBar
                        salesOrderNo={view.salesOrder.salesOrderNo}
                        selectedCount={selection.selected.size}
                        overallPreview={selection.overallPreview}
                        hasExceptionResult={selection.hasExceptionResult}
                        canSave={canSave}
                        canPost={canPost}
                        savePending={saveDraftMutation.isPending}
                        postPending={postMutation.isPending}
                        onExit={() => {
                            if (hasUnsavedInput) {
                                setExitDiscardOpen(true)
                                return
                            }
                            router.push(`/sales/orders/${salesOrderId}`)
                        }}
                        onSaveDraft={() => void handleSaveDraft()}
                    />
                </>
            )}

            <AcceptanceDialogs
                confirmOpen={confirmOpen}
                onConfirmOpenChange={setConfirmOpen}
                overallPreview={selection.overallPreview}
                hasExceptionResult={selection.hasExceptionResult}
                onConfirmAcceptance={async () => {
                    if (!view) return
                    // 先确保草稿版本
                    submittedOverallRef.current = selection.overallPreview
                    const values = form.state.values
                    const lines = buildDraftLines(
                        selection.selected,
                        selection.lineResults,
                    )
                    const draft = await saveDraftMutation.mutateAsync({
                        salesOrderId,
                        acceptanceDraftId: view.draft?.acceptanceDraftId,
                        expectedDraftVersion: view.draft?.draftVersion,
                        acceptedAt: values.acceptedAt
                            ? new Date(values.acceptedAt).toISOString()
                            : new Date().toISOString(),
                        comment: values.comment,
                        lines,
                    })
                    await postMutation.mutateAsync({
                        salesOrderId,
                        acceptanceDraftId: draft.acceptanceDraftId,
                        expectedDraftVersion: draft.draftVersion,
                        expectedSalesOrderLockVersion:
                            view.salesOrder.lockVersion,
                        idempotencyKey,
                        acceptedAt: draft.acceptedAt,
                        comment: draft.comment,
                        lines: draft.lines,
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
                    router.push(`/sales/orders/${salesOrderId}`)
                }}
            />
        </div>
    )
}
