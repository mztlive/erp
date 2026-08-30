"use client"

import * as React from "react"
import { TriangleAlertIcon } from "lucide-react"

import {
    BusinessFailureState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    useCardFundsReviewDefaultUrlSync,
    useCardFundsReviewUrlState,
} from "../hooks/use-card-funds-review-url-state"
import { useCardFundsReviewQueueQuery } from "../hooks/queries"
import { useCardFundsReviewWorkflow } from "../hooks/use-card-funds-review-workflow"
import { DecisionPanel } from "../components/decision-panel"
import { EvidenceNavPanel } from "../components/evidence-nav-panel"
import {
    CompletedQueueEmptyState,
    FilterQueueEmptyState,
} from "../components/queue-empty-states"
import { QueueNavBar } from "../components/queue-nav-bar"
import { QueueFilterToolbar } from "../components/queue-filter-toolbar"
import { ReviewChainPanel } from "../components/review-chain-panel"
import { ReviewConfirmDialogs } from "../components/review-confirm-dialogs"
import { ReviewPageHeader } from "../components/review-page-header"
import { ReviewPageSkeleton } from "../components/review-page-skeleton"
import { ReviewResultBanner } from "../components/review-result-banner"
import { TaskActionUnavailableAlert } from "../components/task-action-unavailable-alert"
import { TaskDetailCard } from "../components/task-detail-card"

export function CardFundsReviewPage({
    forcedWorkItemId,
    embedded = false,
    onTaskCompleted,
}: {
    forcedWorkItemId?: string
    embedded?: boolean
    onTaskCompleted?: (workItemId: string, preferredWorkItemId?: string) => void
} = {}) {
    const {
        scope,
        type,
        status,
        due,
        q,
        currentWorkItemId: urlWorkItemId,
        queueContextId: urlQueueContextId,
        autoNext: urlAutoNext,
        searchInput,
        setSearchInput,
        setAutoNext,
        replaceUrl,
        pathname,
        searchParams,
        router,
    } = useCardFundsReviewUrlState()
    const currentWorkItemId = forcedWorkItemId ?? urlWorkItemId
    const queryQueueContextId = embedded ? undefined : urlQueueContextId
    const autoNext = embedded ? false : urlAutoNext

    const filters = React.useMemo(
        () => ({
            scope,
            type,
            status,
            due,
            q,
            currentWorkItemId,
            queueContextId: queryQueueContextId,
        }),
        [scope, type, status, due, q, currentWorkItemId, queryQueueContextId],
    )

    const queueQuery = useCardFundsReviewQueueQuery(
        filters,
        embedded ? currentWorkItemId : undefined,
    )

    const view = queueQuery.data
    const tasks = React.useMemo(() => view?.tasks ?? [], [view?.tasks])
    const context = view?.context
    const queueContextId =
        context?.queueContextId ??
        queryQueueContextId ??
        `focused-card-funds:${currentWorkItemId ?? "pending"}`
    const task =
        tasks.find((t) => t.workItem.workItemId === currentWorkItemId) ??
        view?.current ??
        tasks[0]
    const currentIndex = task
        ? Math.max(
              0,
              tasks.findIndex(
                  (t) => t.workItem.workItemId === task.workItem.workItemId,
              ),
          )
        : 0

    useCardFundsReviewDefaultUrlSync({
        queuePending: embedded || queueQuery.isPending,
        view,
        task,
        taskCount: tasks.length,
        scope,
        type,
        queueContextId,
    })

    const completed = Boolean(view) && tasks.length === 0

    const workflow = useCardFundsReviewWorkflow({
        task,
        tasks,
        context,
        currentIndex,
        queueContextId,
        autoNext,
        replaceUrl,
        setSearchInput,
        onTaskCompleted: embedded ? onTaskCompleted : undefined,
    })
    const {
        confirmMode,
        setConfirmMode,
        lastResult,
        actionError,
        setActionError,
        pendingNav,
        setPendingNav,
        evidenceOk,
        evidenceRef,
        setEvidenceRef,
        evidenceDocId,
        setEvidenceDocId,
        comment,
        setComment,
        setEvidenceDirty,
        keyHint,
        receiptForm,
        setReceiptForm,
        invoiceForm,
        setInvoiceForm,
        allocLines,
        setAllocLines,
        allocationMode,
        setAllocationMode,
        allocTarget,
        allocatedSum,
        receiptPending,
        invoicePending,
        completePending,
        formalPending,
        responsibilityStatus,
        canConfirmZero,
        goToWorkItem,
        clearFilters,
        neighborId,
        openAllocation,
        submitReceipt,
        submitInvoice,
        runApprove,
        submitReject,
    } = workflow
    const headingRef = React.useRef<HTMLHeadingElement>(null)
    const resultRef = React.useRef<HTMLDivElement>(null)

    // 焦点：结果区 / 对象标题；位置播报由 SequentialProcessBar aria-live
    React.useEffect(() => {
        if (lastResult) {
            resultRef.current?.focus()
        } else if (task) {
            headingRef.current?.focus()
        }
    }, [task, lastResult])

    // 清除筛选：清 type/status/due/q + 焦点，保留 scope/queueContextId（P4）。
    // type 不写默认值「all」，遵循 URL 最小化（默认值省略语义，D18）
    const hasActiveQueueFilters = Boolean(
        q || status !== "OPEN" || due !== "all" || type !== "all",
    )

    const w05Href = task
        ? `/sales/orders/${task.salesOrder.id}?from=W13&returnTo=${encodeURIComponent(`${pathname}?${searchParams.toString()}`)}&sourceWorkItemId=${task.workItem.workItemId}`
        : "#"
    const w11Href = task
        ? `/finance/customer-accounts?customer=${task.account.customerId}&from=W13&returnTo=${encodeURIComponent(`${pathname}?queueContextId=${queueContextId}&currentWorkItemId=${task.workItem.workItemId}&type=${type}&scope=${scope}`)}`
        : "/finance/customer-accounts"

    if (queueQuery.isPending) {
        return <ReviewPageSkeleton />
    }

    if (queueQuery.isError) {
        return (
            <PageScaffold
                density={embedded ? "compact" : "default"}
                className={embedded ? "max-w-none p-0" : undefined}
            >
                <PageHeader title="卡券票款复核" />
                <BusinessFailureState
                    error={queueQuery.error}
                    onRetry={() => void queueQuery.refetch()}
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold
            density={embedded ? "compact" : "default"}
            className={embedded ? "max-w-none p-0" : undefined}
        >
            {!embedded ? <ReviewPageHeader context={context} /> : null}

            {!embedded ? (
                <QueueFilterToolbar
                    scope={scope}
                    type={type}
                    due={due}
                    status={status}
                    searchInput={searchInput}
                    onSearchInputChange={setSearchInput}
                    autoNext={autoNext}
                    setAutoNext={setAutoNext}
                    replaceUrl={replaceUrl}
                />
            ) : null}

            {lastResult ? (
                <div ref={resultRef} tabIndex={-1} className="outline-none">
                    <ReviewResultBanner
                        lastResult={lastResult}
                        onNext={() => {
                            const next =
                                context?.nextWorkItemId ??
                                neighborId(1) ??
                                tasks[0]?.workItem.workItemId
                            goToWorkItem(next)
                        }}
                        w05Href={w05Href}
                        hasTask={Boolean(task)}
                    />
                </div>
            ) : null}

            {actionError ? (
                <Alert variant="destructive" role="alert">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>操作未生效</AlertTitle>
                    <AlertDescription>{actionError}</AlertDescription>
                </Alert>
            ) : null}

            {completed ? (
                <CompletedQueueEmptyState
                    hasActiveQueueFilters={hasActiveQueueFilters}
                    onClearFilters={clearFilters}
                />
            ) : task ? (
                <>
                    {!embedded ? (
                        <QueueNavBar
                            current={context?.position ?? currentIndex + 1}
                            total={context?.total ?? tasks.length}
                            responsibilityStatus={responsibilityStatus}
                            responsibilityStatusLabel={
                                task.workItem.ownerUser
                                    ? `处理人：${task.workItem.ownerUser.displayName}`
                                    : undefined
                            }
                            formalPending={formalPending}
                            evidenceOk={evidenceOk}
                            canApprove={task.workItem.allowedActions.includes(
                                "APPROVE",
                            )}
                            onBack={() => router.push("/workspace")}
                            onApprove={(advance) =>
                                setConfirmMode({
                                    kind: "approve",
                                    conclusion: "RECORDED_FACTS_RECONCILED",
                                    advance,
                                })
                            }
                            onMissingEvidence={() =>
                                setActionError(
                                    "请先填写凭证编号或证据说明；证据将随正式决定一并提交。",
                                )
                            }
                        />
                    ) : null}

                    {task.workItem.workItemStatus === "OPEN" ? (
                        <TaskActionUnavailableAlert task={task} />
                    ) : null}

                    <div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,64fr)_minmax(17rem,36fr)]">
                        <div className="min-w-0 space-y-4">
                            <TaskDetailCard
                                task={task}
                                headingRef={headingRef}
                                w11Href={w11Href}
                                openAllocation={openAllocation}
                                allocationMode={allocationMode}
                                receiptForm={receiptForm}
                                setReceiptForm={setReceiptForm}
                                invoiceForm={invoiceForm}
                                setInvoiceForm={setInvoiceForm}
                                allocLines={allocLines}
                                setAllocLines={setAllocLines}
                                allocTarget={allocTarget}
                                allocatedSum={allocatedSum}
                                receiptPending={receiptPending}
                                invoicePending={invoicePending}
                                setAllocationMode={setAllocationMode}
                                submitReceipt={submitReceipt}
                                submitInvoice={submitInvoice}
                            />

                            <DecisionPanel
                                task={task}
                                evidenceDocId={evidenceDocId}
                                evidenceRef={evidenceRef}
                                comment={comment}
                                evidenceOk={evidenceOk}
                                keyHint={keyHint}
                                canConfirmZero={canConfirmZero}
                                formalPending={formalPending}
                                autoNext={autoNext}
                                onEvidenceDocIdChange={(value) => {
                                    setEvidenceDocId(value)
                                    setEvidenceDirty(true)
                                }}
                                onEvidenceRefChange={(value) => {
                                    setEvidenceRef(value)
                                    setEvidenceDirty(true)
                                }}
                                onCommentChange={(value) => {
                                    setComment(value)
                                    setEvidenceDirty(true)
                                }}
                                onZero={() =>
                                    setConfirmMode({
                                        kind: "zero",
                                        advance: autoNext,
                                    })
                                }
                                onApprove={(advance) =>
                                    setConfirmMode({
                                        kind: "approve",
                                        conclusion: "RECORDED_FACTS_RECONCILED",
                                        advance,
                                    })
                                }
                                onReject={() =>
                                    setConfirmMode({ kind: "reject" })
                                }
                            />
                        </div>

                        <aside className="min-w-0 space-y-4 xl:sticky xl:top-4 xl:self-start">
                            <ReviewChainPanel task={task} />

                            <EvidenceNavPanel
                                task={task}
                                w05Href={w05Href}
                                w11Href={w11Href}
                            />
                        </aside>
                    </div>
                </>
            ) : (
                <FilterQueueEmptyState onClearFilters={clearFilters} />
            )}

            <ReviewConfirmDialogs
                confirmMode={confirmMode}
                setConfirmMode={setConfirmMode}
                task={task}
                completePending={completePending}
                pendingNav={pendingNav}
                setPendingNav={setPendingNav}
                neighborId={neighborId}
                goToWorkItem={goToWorkItem}
                runApprove={runApprove}
                submitReject={submitReject}
            />
        </PageScaffold>
    )
}
