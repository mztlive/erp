"use client"

import { useEffect, useRef } from "react"
import Link from "next/link"
import {
    ArrowUpRightIcon,
    ChevronDownIcon,
    ChevronRightIcon,
    CircleAlertIcon,
} from "lucide-react"

import {
    WorkspaceFactGrid,
    WorkspaceFactGroup,
    WorkspaceFactItem,
} from "@/components/business"
import { WorkspaceTaskFooter } from "@/components/business/workspace-task-pane"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { LoadingButton } from "@/components/ui/loading-button"
import type { useSettlementCenterActions } from "@/features/supplier-settlements/hooks/use-settlement-center-actions"
import { useSettlementResultFocus } from "@/features/supplier-settlements/hooks/use-settlement-result-focus"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import {
    SECTION_LABEL,
    type SettlementDetailView,
    type SettlementDifferenceView,
} from "@/features/supplier-settlements/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"

import { SettlementCenterAudit } from "./settlement-center-audit"
import { SettlementCenterBlockersAlert } from "./settlement-center-blockers"
import {
    SettlementConfirmSettlementDialog,
    SettlementEvidenceDialog,
    SettlementRejectDialog,
    SettlementResolveDialog,
} from "./settlement-center-dialogs"
import { SettlementCenterPayable } from "./settlement-center-payable"
import { SettlementCenterResultPanel } from "./settlement-center-result"
import { SettlementCenterReview } from "./settlement-center-review"
import { useSettlementReviewCommand } from "./settlement-review-command"
import {
    SettlementReviewFooterSummary,
    SettlementReviewMetrics,
    settlementMoney,
    settlementPayablePreview,
} from "./settlement-review-metrics"
import {
    SettlementReviewTable,
    useSettlementReviewRows,
    type SettlementReviewTableHandle,
} from "./settlement-review-table"

const RECORD_SECTIONS = ["overview", "review", "payable", "audit"] as const

function ReviewDifferenceDetails({
    difference,
    idPrefix,
    canResolve,
    canAppendEvidence,
    onResolve,
    onAppendEvidence,
}: {
    difference: SettlementDifferenceView
    idPrefix: string
    canResolve: boolean
    canAppendEvidence: boolean
    onResolve: () => void
    onAppendEvidence: () => void
}) {
    return (
        <div className="space-y-2 text-xs">
            {difference.leftFields.length > 0 && (
                <dl className="grid gap-1">
                    {difference.leftFields.map((field) => (
                        <div key={field.id} className="flex flex-wrap gap-x-2">
                            <dt className="text-muted-foreground">
                                {field.field}
                            </dt>
                            <dd>
                                ERP {field.before} · 账单 {field.after}
                                {field.note && ` · ${field.note}`}
                            </dd>
                        </div>
                    ))}
                </dl>
            )}
            {difference.resolution && (
                <p>
                    处理结论 {difference.resolution.resolutionLabel} ·{" "}
                    {difference.resolution.reasonLabel} ·{" "}
                    {difference.resolution.by.displayName}
                </p>
            )}
            {difference.evidence.length > 0 ? (
                <ul className="space-y-1" aria-label="已登记证据">
                    {difference.evidence.map((entry) => (
                        <li key={entry.evidenceId}>
                            <span className="font-medium">{entry.label}</span>
                            <span className="text-muted-foreground">
                                {" "}
                                · {entry.by.displayName} ·{" "}
                                <span className="num">
                                    {formatDateTime(entry.at, "default")}
                                </span>
                            </span>
                            {entry.comment && <p>{entry.comment}</p>}
                        </li>
                    ))}
                </ul>
            ) : (
                <p className="text-muted-foreground">暂无差异证据</p>
            )}
            {(canResolve || canAppendEvidence) && (
                <div className="flex flex-wrap items-center gap-2">
                    {canAppendEvidence && (
                        <Button
                            id={`${idPrefix}-append-evidence`}
                            size="xs"
                            variant="outline"
                            onClick={onAppendEvidence}
                        >
                            追加采购证据
                        </Button>
                    )}
                    {canResolve && (
                        <Button
                            id={`${idPrefix}-resolve`}
                            size="xs"
                            variant="outline"
                            onClick={onResolve}
                        >
                            登记结论
                        </Button>
                    )}
                </div>
            )}
        </div>
    )
}

export function SettlementReviewWorkspace({
    detail,
    actions,
    idPrefix,
    fullTaskHref,
    canReview,
    canManageDifferences,
    section,
    patchUrl,
    onTaskCompleted,
}: {
    detail: SettlementDetailView
    actions: ReturnType<typeof useSettlementCenterActions>
    idPrefix: string
    fullTaskHref?: string
    canReview: boolean
    canManageDifferences: boolean
    section: SettlementsUrlState["section"]
    patchUrl: (patch: Partial<SettlementsUrlState>) => void
    onTaskCompleted?: (workItemId: string) => void
}) {
    const table = useRef<SettlementReviewTableHandle>(null)
    const records = useRef<HTMLDivElement>(null)
    const resultRef = useRef<HTMLDivElement>(null)
    const rows = useSettlementReviewRows(detail)
    const unresolved = rows.open.length
    const review = useSettlementReviewCommand({
        detail,
        onCompleted(action, workItemId) {
            patchUrl({ section: action === "CONFIRM" ? "payable" : "review" })
            onTaskCompleted?.(workItemId)
        },
        onCloseForm() {
            actions.setConfirmOpen(false)
            actions.setRejectOpen(false)
        },
        onConflict() {
            void actions.detailQuery.refetch()
        },
    })
    useSettlementResultFocus(review.result, resultRef)

    const payablePreview = settlementPayablePreview(detail)
    const { netPayable } = payablePreview
    const currentDetails = !actions.detailQuery.isError
    const ownsTask =
        currentDetails && actions.responsibilityStatus === "assigned_to_me"
    const canConfirm = Boolean(
        detail.workItem &&
        canReview &&
        ownsTask &&
        actions.allowed.has("CONFIRM"),
    )
    const canReject = Boolean(
        detail.workItem &&
        canReview &&
        ownsTask &&
        actions.allowed.has("REJECT"),
    )
    const canChangeDifferences =
        currentDetails &&
        canManageDifferences &&
        !fullTaskHref &&
        !review.awaitingResult
    const canAppendEvidence =
        canChangeDifferences && actions.allowed.has("APPEND_EVIDENCE")
    const canResolve =
        canChangeDifferences && actions.allowed.has("RESOLVE_DIFFERENCE")
    const canResolveActive =
        canResolve && actions.activeDiff?.status === "PENDING"
    const visibleRecord = RECORD_SECTIONS.find((item) => item === section)

    function locateDifferences() {
        patchUrl({ section: "items" })
        table.current?.locateOpenDifferences()
    }

    function showRecord(next: (typeof RECORD_SECTIONS)[number]) {
        patchUrl({ section: next })
        requestAnimationFrame(() =>
            records.current?.scrollIntoView({ block: "nearest" }),
        )
    }

    useEffect(() => {
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action === "history") {
                if (section === "audit") {
                    patchUrl({ section: "items" })
                } else {
                    patchUrl({ section: "audit" })
                    requestAnimationFrame(() =>
                        records.current?.scrollIntoView({ block: "nearest" }),
                    )
                }
                return
            }
            if (
                fullTaskHref ||
                review.awaitingResult ||
                review.pending ||
                actions.confirmOpen ||
                actions.rejectOpen
            ) {
                return
            }
            if (action === "approve" && canConfirm) {
                actions.setConfirmOpen(true)
            } else if (action === "reject" && canReject) {
                actions.setRejectOpen(true)
            }
        })
    }, [
        actions,
        canConfirm,
        canReject,
        fullTaskHref,
        patchUrl,
        review.awaitingResult,
        review.pending,
        section,
    ])

    const summary = (
        <SettlementReviewFooterSummary
            itemCount={detail.items.length}
            matched={rows.matched.length}
            unresolved={unresolved}
            netPayable={netPayable}
        />
    )
    const buttons = (
        <>
            {unresolved > 0 && (
                <Button
                    id={`${idPrefix}-footer-locate`}
                    variant="ghost"
                    size="sm"
                    className="text-warning-soft-foreground"
                    onClick={locateDifferences}
                >
                    <span className="num">{unresolved}</span> 项差异待处理 ·
                    定位
                </Button>
            )}
            {fullTaskHref ? (
                <Button
                    id={`${idPrefix}-open-full`}
                    nativeButton={false}
                    render={<Link href={fullTaskHref} />}
                    variant="default"
                    size="sm"
                >
                    进入完整复核{" "}
                    <ArrowUpRightIcon
                        data-icon="inline-end"
                        aria-hidden="true"
                    />
                </Button>
            ) : review.awaitingResult && canReview ? (
                <LoadingButton
                    id={`${idPrefix}-review-check-result`}
                    variant="default"
                    size="sm"
                    loading={review.pending}
                    disabled={actions.detailQuery.isFetching}
                    onClick={() => void review.retry()}
                >
                    核对提交结果
                </LoadingButton>
            ) : (
                !review.awaitingResult && (
                    <>
                        {canReject && (
                            <Button
                                id={`${idPrefix}-reject-trigger`}
                                variant="outline"
                                size="sm"
                                className="border-destructive-border text-destructive-soft-foreground hover:bg-destructive-soft"
                                disabled={review.pending}
                                onClick={() => actions.setRejectOpen(true)}
                            >
                                驳回复核
                            </Button>
                        )}
                        {detail.workItem && canReview && ownsTask && (
                            <LoadingButton
                                id={`${idPrefix}-confirm-trigger`}
                                variant="default"
                                size="sm"
                                loading={review.pending}
                                disabled={!canConfirm}
                                onClick={() => actions.setConfirmOpen(true)}
                            >
                                确认结算
                            </LoadingButton>
                        )}
                    </>
                )
            )}
        </>
    )

    return (
        <div className="min-w-0 space-y-3 bg-card py-3">
            {unresolved > 0 && (
                <section
                    aria-label="未处理差异"
                    className={cn(
                        "flex min-w-0 flex-wrap items-center justify-between gap-2 rounded-r-md border-l-2 px-3 py-1.5 text-body-compact",
                        rows.disputed > 0
                            ? "border-l-destructive bg-destructive-soft text-destructive-soft-foreground"
                            : "border-l-warning bg-warning-soft text-warning-soft-foreground",
                    )}
                >
                    <p
                        className="flex min-w-0 items-center gap-1.5"
                        aria-live="polite"
                    >
                        <CircleAlertIcon
                            className="size-4 shrink-0"
                            aria-hidden="true"
                        />
                        <span>
                            <span className="num font-semibold">
                                {unresolved}
                            </span>{" "}
                            项差异待处理
                            {rows.disputed > 0 && (
                                <span>
                                    ，其中{" "}
                                    <span className="num">{rows.disputed}</span>{" "}
                                    项阻塞复核
                                </span>
                            )}
                        </span>
                    </p>
                    <Button
                        id={`${idPrefix}-only-differences`}
                        variant="ghost"
                        size="xs"
                        className="shrink-0 text-current"
                        onClick={locateDifferences}
                    >
                        仅看差异项<span className="num">（{unresolved}）</span>
                        <ChevronRightIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                        />
                    </Button>
                </section>
            )}
            <SettlementReviewMetrics
                detail={detail}
                idPrefix={idPrefix}
                matched={rows.matched.length}
                pending={rows.amount.length + unresolved - rows.disputed}
                disputed={rows.disputed}
                canConfirm={canConfirm}
            />

            {detail.statement.status === "PENDING_REVIEW" &&
                detail.workItemBlocker &&
                !detail.actionBlockers.some(
                    ({ message }) =>
                        message === detail.workItemBlocker?.message,
                ) && (
                    <p className="rounded-r-md border-l-2 border-l-warning bg-warning-soft px-3 py-2 text-xs text-warning-soft-foreground">
                        {detail.workItemBlocker.message}
                    </p>
                )}
            <SettlementCenterBlockersAlert blockers={detail.actionBlockers} />
            {!currentDetails && (
                <div
                    className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-warning-soft-foreground"
                    role="status"
                >
                    结算单刷新失败，当前显示上次加载的数据。
                    <LoadingButton
                        id={`${idPrefix}-reload-detail`}
                        size="xs"
                        variant="outline"
                        loading={actions.detailQuery.isFetching}
                        onClick={() => void actions.detailQuery.refetch()}
                    >
                        重新加载
                    </LoadingButton>
                </div>
            )}
            <SettlementCenterResultPanel
                idPrefix={`${idPrefix}-difference-result`}
                result={actions.result}
                resultRef={actions.resultRef}
                onClose={actions.clearResult}
                onRefresh={
                    actions.result?.status === "unknown"
                        ? () => void actions.detailQuery.refetch()
                        : undefined
                }
                refreshing={actions.detailQuery.isFetching}
            />
            <SettlementCenterResultPanel
                idPrefix={`${idPrefix}-review-result`}
                resultRef={resultRef}
                result={review.result}
                onClose={review.awaitingResult ? undefined : review.clearResult}
                onRefresh={
                    review.result
                        ? () => void actions.detailQuery.refetch()
                        : undefined
                }
                refreshing={actions.detailQuery.isFetching}
                onRetry={
                    review.awaitingResult && canReview
                        ? () => void review.retry()
                        : undefined
                }
                retrying={review.pending}
            />

            <SettlementReviewTable
                idPrefix={idPrefix}
                detail={detail}
                rows={rows}
                ref={table}
                utilityActions={
                    <DropdownMenu>
                        <DropdownMenuTrigger
                            id={`${idPrefix}-records`}
                            render={<Button variant="outline" size="sm" />}
                        >
                            结算资料{" "}
                            <ChevronDownIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                            {RECORD_SECTIONS.map((item) => (
                                <DropdownMenuItem
                                    key={item}
                                    id={`${idPrefix}-records-${item}`}
                                    onClick={() => showRecord(item)}
                                >
                                    {SECTION_LABEL[item]}
                                </DropdownMenuItem>
                            ))}
                        </DropdownMenuContent>
                    </DropdownMenu>
                }
                onSelectDifference={(diff) => patchUrl({ diff })}
                renderDifference={(difference) => (
                    <ReviewDifferenceDetails
                        difference={difference}
                        idPrefix={idPrefix}
                        canResolve={
                            canResolve && difference.status === "PENDING"
                        }
                        canAppendEvidence={canAppendEvidence}
                        onResolve={() => actions.setResolveOpen(true)}
                        onAppendEvidence={() => actions.setEvidenceOpen(true)}
                    />
                )}
            />

            <div
                ref={records}
                className="space-y-2 border-t border-border/60 pt-2 empty:hidden"
            >
                {visibleRecord && (
                    <div className="flex justify-end">
                        <Button
                            id={`${idPrefix}-records-close`}
                            variant="ghost"
                            size="xs"
                            onClick={() => patchUrl({ section: "items" })}
                        >
                            收起结算资料
                        </Button>
                    </div>
                )}
                {visibleRecord === "overview" && (
                    <WorkspaceFactGroup title="结算单概览">
                        <WorkspaceFactGrid columns={3} ariaLabel="结算单概览">
                            <WorkspaceFactItem
                                label="供应商账单"
                                value={
                                    detail.statement.externalBillNo ??
                                    "尚未同步"
                                }
                                numeric={Boolean(
                                    detail.statement.externalBillNo,
                                )}
                                selectable={Boolean(
                                    detail.statement.externalBillNo,
                                )}
                                tone={
                                    detail.statement.externalBillNo
                                        ? "default"
                                        : "muted"
                                }
                            />
                            <WorkspaceFactItem
                                label="账单版本"
                                value={detail.statement.externalBillVersion}
                                numeric
                                selectable={Boolean(
                                    detail.statement.externalBillVersion,
                                )}
                            />
                            <WorkspaceFactItem
                                label="业务记录截至"
                                value={formatDateTime(
                                    detail.freshness.immutableFactsAsOf,
                                    "default",
                                )}
                                numeric
                            />
                            <WorkspaceFactItem
                                label="经办"
                                value={detail.statement.preparedBy?.displayName}
                            />
                            <WorkspaceFactItem
                                label="订单结算价"
                                value={settlementMoney(
                                    detail.totals.orderAmountGross,
                                )}
                                numeric
                            />
                            <WorkspaceFactItem
                                label="运费"
                                value={settlementMoney(
                                    detail.totals.freightGross,
                                )}
                                numeric
                            />
                            <WorkspaceFactItem
                                label="服务费"
                                value={settlementMoney(
                                    detail.totals.serviceFeeGross,
                                )}
                                numeric
                            />
                            <WorkspaceFactItem
                                label="退款"
                                value={settlementMoney(
                                    detail.totals.refundGross,
                                )}
                                numeric
                            />
                        </WorkspaceFactGrid>
                    </WorkspaceFactGroup>
                )}
                {visibleRecord === "review" && (
                    <SettlementCenterReview detail={detail} embedded />
                )}
                {visibleRecord === "payable" && (
                    <SettlementCenterPayable
                        payable={detail.payable}
                        idPrefix={idPrefix}
                        grossAmount={netPayable}
                    />
                )}
                {visibleRecord === "audit" && (
                    <SettlementCenterAudit events={detail.auditEvents} />
                )}
            </div>

            <WorkspaceTaskFooter
                summary={summary}
                fallback={
                    <div className="flex w-full min-w-0 flex-wrap items-center justify-between gap-2">
                        {summary}
                        <div className="ml-auto flex min-w-0 flex-wrap items-center justify-end gap-2">
                            {buttons}
                        </div>
                    </div>
                }
            >
                {buttons}
            </WorkspaceTaskFooter>

            {canResolveActive && (
                <SettlementResolveDialog
                    idPrefix={`${idPrefix}-resolve-dialog`}
                    open={actions.resolveOpen}
                    onOpenChange={actions.setResolveOpen}
                    activeDiff={actions.activeDiff}
                    resolution={actions.resolution}
                    onResolutionChange={actions.setResolution}
                    reasonCode={actions.reasonCode}
                    onReasonCodeChange={actions.setReasonCode}
                    pending={actions.resolveMutation.isPending}
                    onSubmit={actions.onResolve}
                />
            )}
            {canAppendEvidence && (
                <SettlementEvidenceDialog
                    idPrefix={`${idPrefix}-evidence-dialog`}
                    open={actions.evidenceOpen}
                    onOpenChange={actions.setEvidenceOpen}
                    referenceId={actions.evidenceReferenceId}
                    onReferenceIdChange={actions.setEvidenceReferenceId}
                    comment={actions.evidenceComment}
                    onCommentChange={actions.setEvidenceComment}
                    pending={actions.evidenceMutation.isPending}
                    onSubmit={actions.onEvidence}
                />
            )}
            {!fullTaskHref && !review.awaitingResult && canConfirm && (
                <SettlementConfirmSettlementDialog
                    idPrefix={`${idPrefix}-confirm-dialog`}
                    open={actions.confirmOpen}
                    onOpenChange={actions.setConfirmOpen}
                    statement={detail.statement}
                    totals={detail.totals}
                    payablePreview={payablePreview}
                    pending={review.pending}
                    onConfirm={() => review.submit("CONFIRM")}
                />
            )}
            {!fullTaskHref && !review.awaitingResult && canReject && (
                <SettlementRejectDialog
                    idPrefix={`${idPrefix}-reject-dialog`}
                    open={actions.rejectOpen}
                    onOpenChange={actions.setRejectOpen}
                    statement={detail.statement}
                    reasonCode={actions.rejectReason}
                    onReasonCodeChange={actions.setRejectReason}
                    comment={actions.rejectComment}
                    onCommentChange={actions.setRejectComment}
                    pending={review.pending}
                    onSubmit={() =>
                        review.submit(
                            "REJECT",
                            actions.rejectReason,
                            actions.rejectComment,
                        )
                    }
                />
            )}
        </div>
    )
}
