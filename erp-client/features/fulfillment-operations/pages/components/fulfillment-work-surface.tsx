"use client"

import * as React from "react"
import Link from "next/link"
import {
    ArrowLeftIcon,
    ArrowRightIcon,
    CircleCheckIcon,
    EyeIcon,
    EllipsisIcon,
    LoaderCircleIcon,
    SaveIcon,
    SkipForwardIcon,
    Undo2Icon,
} from "lucide-react"

import {
    BusinessStatusBadge,
    BusinessFailureState,
    ValidationSummary,
    WorkspaceTaskFooter,
    WorkspaceTaskPageSlot,
    type ValidationIssue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import {
    DropdownMenu,
    DropdownMenuTrigger,
    DropdownMenuContent,
    DropdownMenuItem,
} from "@/components/ui/dropdown-menu"
import { LoadingButton } from "@/components/ui/loading-button"
import { useFulfillmentFieldLocation } from "../hooks/use-fulfillment-field-location"
import {
    FulfillmentDraftForm,
    FulfillmentDraftReadOnly,
} from "@/features/fulfillment-operations/components/forms/fulfillment-draft-form"
import {
    OPERATION_ACTION_LABEL,
    OPERATION_TYPE_LABEL,
    type FulfillmentDraft,
    type FulfillmentOperation,
} from "@/features/fulfillment-operations/types"
import { salesOrderHref } from "@/features/fulfillment-operations/pages/lib/gate-copy"
import { displayText } from "@/features/fulfillment-operations/lib/readable-label"
import { displayName } from "@/lib/display-name"
import { shipmentSummary } from "../lib/presentation"
import { FulfillmentPaymentNotice } from "./fulfillment-payment-notice"
import { FulfillmentGateStatus } from "./fulfillment-gate-status"
import {
    FulfillmentSourceContext,
    type FulfillmentTaskContext,
} from "./fulfillment-task-overview"
import { FulfillmentTaskWorkSurface } from "./fulfillment-task-work-surface"

export type FulfillmentWorkSurfaceProps = {
    operation: FulfillmentOperation
    draft: FulfillmentDraft
    validationIssues: readonly ValidationIssue[]
    saveMessage: string | null
    canExecute: boolean
    canPost: boolean
    formalPending: boolean
    savePending?: boolean
    supportsSave: boolean
    dirty: boolean
    autoNext: boolean
    readOnlyNote: string
    responsibilityStatus: "blocked" | "assigned_to_me" | "assigned_to_other"
    responsibilityStatusLabel: string
    currentUrl: string
    snapshotUpdatedAt: string
    position: number
    total: number
    shortcutsOpen: boolean
    headingRef: React.Ref<HTMLHeadingElement>
    resultUnknown: boolean
    taskContext?: FulfillmentTaskContext
    actionError?: string | null
    recovery?: {
        resolving: boolean
        retrying: boolean
        onResolve: () => void
        onRetry?: () => void
    }
    /** W01 内联作业面只处理任务绑定的单个对象。 */
    singleOperation?: boolean
    showBack?: boolean
    showSalesOrderLinks?: boolean
    onDraftChange: (next: FulfillmentDraft) => void
    onSkip: () => void
    onDiscard: () => void
    onSave: () => void
    onConfirm: () => void
    onBack: () => void
    onToggleShortcuts: () => void
}

/** 当前单据的处理面：处理条、来源上下文、草稿表单与动作区。 */
export function FulfillmentWorkSurface(props: FulfillmentWorkSurfaceProps) {
    if (props.taskContext) {
        return (
            <FulfillmentTaskWorkSurface
                key={props.operation.operationId}
                {...props}
                taskContext={props.taskContext}
            />
        )
    }
    return <FulfillmentQueueWorkSurface {...props} />
}

function FulfillmentQueueWorkSurface({
    operation,
    draft,
    validationIssues,
    saveMessage,
    canExecute,
    canPost,
    formalPending,
    savePending = false,
    supportsSave,
    dirty,
    autoNext,
    readOnlyNote,
    responsibilityStatus,
    responsibilityStatusLabel,
    currentUrl,
    snapshotUpdatedAt,
    position,
    total,
    shortcutsOpen,
    headingRef,
    resultUnknown,
    singleOperation = false,
    showBack = true,
    showSalesOrderLinks = true,
    onDraftChange,
    onSkip,
    onDiscard,
    onSave,
    onConfirm,
    onBack,
    onToggleShortcuts,
}: FulfillmentWorkSurfaceProps) {
    const { deliveryRef, locateIssue } = useFulfillmentFieldLocation()
    const paymentPending =
        operation.gate.state === "BLOCKED" && draft.type !== "WAREHOUSE_SHIP"
    const addressPacking =
        (draft.type === "WAREHOUSE_SHIP" || draft.type === "SUPPLIER_DIRECT") &&
        !!draft.packing
    const fieldIssues = validationIssues.filter((issue) => issue.id !== "gate")
    const summary = shipmentSummary(operation, draft)
    const summaryContent = summary ? (
        <p className="text-xs text-muted-foreground" role="status">
            {summary}
        </p>
    ) : undefined
    const validationContent =
        fieldIssues.length > 0 ? (
            <ValidationSummary
                id="fulfillment-operations-field-validation"
                onLocate={locateIssue}
                compact
                title="请补充或检查履约信息"
                issues={fieldIssues}
            />
        ) : null
    const purchaseNo = displayText(
        operation.source.purchaseNo,
        operation.source.purchaseOrderId,
    )
    const salesOrderNo = displayText(
        operation.source.salesOrderNo,
        operation.source.salesOrderId,
    )
    const headerSubtitle = [
        displayName(operation.source.customerLabel),
        purchaseNo ? `采购 ${purchaseNo}` : "",
        displayName(operation.source.supplierLabel),
    ]
        .filter(Boolean)
        .join(" · ")
    const responsibilityTone =
        responsibilityStatus === "blocked"
            ? "destructive"
            : responsibilityStatus === "assigned_to_me"
              ? "info"
              : "neutral"

    return (
        <div
            className={
                singleOperation && addressPacking
                    ? "flex min-h-0 min-w-0 flex-1 flex-col"
                    : singleOperation
                      ? "min-w-0"
                      : "min-w-0 space-y-3"
            }
        >
            {operation.loadWarnings?.map((message) => (
                <BusinessFailureState
                    key={message}
                    kind="business"
                    title="部分关联资料读取失败"
                    description={message}
                />
            ))}
            {!singleOperation ? (
                <div
                    id="fulfillment-operations-work-surface-process-bar"
                    className="flex flex-wrap items-center justify-between gap-2 border-b border-border/60 pb-2.5"
                >
                    <div className="flex flex-wrap items-center gap-2">
                        {showBack ? (
                            <Button
                                id="fulfillment-operations-work-surface-back"
                                type="button"
                                size="sm"
                                variant="ghost"
                                onClick={onBack}
                            >
                                <ArrowLeftIcon
                                    aria-hidden="true"
                                    data-icon="inline-start"
                                />
                                返回
                            </Button>
                        ) : null}
                        <span className="num text-xs text-muted-foreground">
                            第 {position} / {total} 项
                        </span>
                        <BusinessStatusBadge
                            context="list"
                            label={responsibilityStatusLabel}
                            tone={responsibilityTone}
                        />
                        {!paymentPending ? (
                            <FulfillmentGateStatus
                                operation={operation}
                                currentUrl={currentUrl}
                                snapshotUpdatedAt={snapshotUpdatedAt}
                                showPaymentAction={showSalesOrderLinks}
                            />
                        ) : null}
                    </div>
                </div>
            ) : null}

            <div className="flex items-center justify-between gap-2 pt-1">
                <button
                    id="fulfillment-operations-work-surface-toggle-shortcuts"
                    type="button"
                    onClick={onToggleShortcuts}
                    aria-expanded={shortcutsOpen}
                    className="self-start text-xs text-muted-foreground hover:text-foreground"
                >
                    {shortcutsOpen
                        ? `快捷键：J / K 上下条${
                              canExecute
                                  ? " · Ctrl+S 保存 · Ctrl+Enter 确认"
                                  : ""
                          } · 再按 ? 收起`
                        : "按 ? 看快捷键"}
                </button>
            </div>

            <section className="min-w-0 bg-card">
                {singleOperation ? null : (
                    <header className="border-b border-grid pb-2">
                        <div className="flex flex-wrap items-start justify-between gap-2">
                            <div>
                                <h3
                                    ref={headingRef}
                                    tabIndex={-1}
                                    aria-live="polite"
                                    className="text-body-compact font-semibold outline-none"
                                >
                                    {
                                        OPERATION_TYPE_LABEL[
                                            operation.operationType
                                        ]
                                    }
                                    {salesOrderNo ? ` · ${salesOrderNo}` : ""}
                                </h3>
                                {headerSubtitle ? (
                                    <p className="text-xs text-muted-foreground">
                                        {headerSubtitle}
                                    </p>
                                ) : null}
                            </div>
                            <BusinessStatusBadge
                                context="list"
                                label={operation.statusLabel}
                                tone={operation.statusTone}
                            />
                        </div>
                    </header>
                )}
                <div className="space-y-3 py-3">
                    {singleOperation && addressPacking && (
                        <h3
                            ref={headingRef}
                            tabIndex={-1}
                            className="sr-only outline-none"
                        >
                            发货明细与物流
                        </h3>
                    )}
                    {singleOperation && !addressPacking ? (
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <h3
                                ref={headingRef}
                                tabIndex={-1}
                                className={
                                    addressPacking
                                        ? "sr-only outline-none"
                                        : "text-body-compact font-semibold outline-none"
                                }
                            >
                                {operation.operationType === "SUPPLIER_DIRECT"
                                    ? "确认供应商直发"
                                    : OPERATION_ACTION_LABEL[
                                          operation.operationType
                                      ]}
                            </h3>
                            {!paymentPending ? (
                                <FulfillmentGateStatus
                                    operation={operation}
                                    currentUrl={currentUrl}
                                    snapshotUpdatedAt={snapshotUpdatedAt}
                                    showPaymentAction={false}
                                />
                            ) : null}
                        </div>
                    ) : null}
                    <WorkspaceTaskPageSlot slot="summary">
                        <FulfillmentSourceContext
                            operation={operation}
                            currentUrl={currentUrl}
                            customerInTitle={singleOperation}
                        />
                    </WorkspaceTaskPageSlot>
                    {singleOperation && addressPacking && !paymentPending && (
                        <WorkspaceTaskPageSlot slot="summary">
                            <FulfillmentGateStatus
                                operation={operation}
                                currentUrl={currentUrl}
                                snapshotUpdatedAt={snapshotUpdatedAt}
                                showPaymentAction={false}
                            />
                        </WorkspaceTaskPageSlot>
                    )}
                    {paymentPending && !singleOperation ? (
                        <WorkspaceTaskPageSlot slot="summary">
                            <FulfillmentPaymentNotice
                                operation={operation}
                                showAmounts={!singleOperation}
                            />
                        </WorkspaceTaskPageSlot>
                    ) : null}

                    {canExecute ? (
                        <FulfillmentDraftForm
                            deliveryRef={deliveryRef}
                            operation={operation}
                            draft={draft}
                            onChange={onDraftChange}
                            disabled={formalPending || resultUnknown}
                        />
                    ) : (
                        <FulfillmentDraftReadOnly
                            operation={operation}
                            draft={operation.draft}
                        />
                    )}

                    {canExecute &&
                    fieldIssues.length > 0 &&
                    (!addressPacking || dirty) ? (
                        addressPacking ? (
                            <details className="text-xs text-muted-foreground">
                                <summary
                                    id="fulfillment-operations-validation-toggle"
                                    className="w-fit cursor-pointer py-1"
                                >
                                    还有 {fieldIssues.length} 项待完善
                                </summary>
                                <div className="pt-2">{validationContent}</div>
                            </details>
                        ) : (
                            validationContent
                        )
                    ) : null}

                    {saveMessage ? (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            {saveMessage}
                        </p>
                    ) : null}

                    {canExecute ? (
                        <WorkspaceTaskFooter
                            summary={summaryContent}
                            fallback={
                                <div className="sticky bottom-0 flex flex-wrap items-center justify-end gap-2 border-t border-grid bg-card/95 py-3 backdrop-blur">
                                    {summaryContent && (
                                        <div className="mr-auto">
                                            {summaryContent}
                                        </div>
                                    )}
                                    <FulfillmentExecuteActions
                                        operation={operation}
                                        singleOperation={singleOperation}
                                        dirty={dirty}
                                        supportsSave={supportsSave}
                                        formalPending={formalPending}
                                        savePending={savePending}
                                        canPost={canPost}
                                        validationIssues={validationIssues}
                                        onLocate={locateIssue}
                                        autoNext={autoNext}
                                        onSkip={onSkip}
                                        onDiscard={onDiscard}
                                        onSave={onSave}
                                        onConfirm={onConfirm}
                                    />
                                </div>
                            }
                        >
                            <FulfillmentExecuteActions
                                operation={operation}
                                singleOperation={singleOperation}
                                dirty={dirty}
                                supportsSave={supportsSave}
                                formalPending={formalPending}
                                savePending={savePending}
                                canPost={canPost}
                                validationIssues={validationIssues}
                                onLocate={locateIssue}
                                autoNext={autoNext}
                                onSkip={onSkip}
                                onDiscard={onDiscard}
                                onSave={onSave}
                                onConfirm={onConfirm}
                            />
                        </WorkspaceTaskFooter>
                    ) : (
                        <WorkspaceTaskFooter
                            fallback={
                                <div className="sticky bottom-0 flex flex-wrap items-center justify-between gap-2 border-t border-grid bg-card/95 py-3 backdrop-blur">
                                    <FulfillmentReadOnlyActions
                                        readOnlyNote={readOnlyNote}
                                        showSalesOrderLinks={
                                            showSalesOrderLinks
                                        }
                                        salesOrderId={
                                            operation.source.salesOrderId
                                        }
                                        currentUrl={currentUrl}
                                    />
                                </div>
                            }
                        >
                            <FulfillmentReadOnlyActions
                                readOnlyNote={readOnlyNote}
                                showSalesOrderLinks={showSalesOrderLinks}
                                salesOrderId={operation.source.salesOrderId}
                                currentUrl={currentUrl}
                            />
                        </WorkspaceTaskFooter>
                    )}
                </div>
            </section>
        </div>
    )
}

function FulfillmentExecuteActions({
    operation,
    singleOperation,
    compact = false,
    dirty,
    supportsSave,
    formalPending,
    savePending,
    canPost,
    validationIssues,
    onLocate,
    autoNext,
    onSkip,
    onDiscard,
    onSave,
    onConfirm,
}: {
    operation: FulfillmentOperation
    singleOperation: boolean
    compact?: boolean
    dirty: boolean
    supportsSave: boolean
    formalPending: boolean
    savePending: boolean
    canPost: boolean
    validationIssues: readonly ValidationIssue[]
    onLocate: (issue: ValidationIssue) => void
    autoNext: boolean
    onSkip: () => void
    onDiscard: () => void
    onSave: () => void
    onConfirm: () => void
}) {
    const actionBlocker = operation.actionBlockers.find(
        (item) => item.action === "POST",
    )
    const firstIssue = validationIssues[0]
    const blockedMessage =
        actionBlocker?.message ||
        firstIssue?.message ||
        "请检查本次填写的信息后再确认"
    return (
        <div className="flex min-w-0 flex-col items-end gap-2">
            {!canPost && blockedMessage ? (
                <div className="flex max-w-xl items-center gap-2 text-xs text-muted-foreground">
                    <p
                        id="fulfillment-operations-confirm-blocker"
                        role="status"
                    >
                        {blockedMessage}
                    </p>
                    {!actionBlocker && firstIssue?.targetId && (
                        <Button
                            id="fulfillment-operations-locate-blocker"
                            size="sm"
                            variant="link"
                            onClick={() => onLocate(firstIssue)}
                        >
                            去填写
                        </Button>
                    )}
                </div>
            ) : null}
            <div className="flex flex-wrap items-center justify-end gap-2">
                {!singleOperation ? (
                    <Button
                        id="fulfillment-operations-work-surface-skip"
                        type="button"
                        variant="ghost"
                        disabled={formalPending}
                        onClick={onSkip}
                    >
                        <SkipForwardIcon
                            aria-hidden="true"
                            data-icon="inline-start"
                        />
                        先跳过
                    </Button>
                ) : null}
                {dirty && (
                    <DropdownMenu>
                        <DropdownMenuTrigger
                            id="fulfillment-operations-more-actions"
                            render={
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="icon"
                                    aria-label="更多操作"
                                    title="更多操作"
                                    disabled={formalPending || savePending}
                                />
                            }
                        >
                            <EllipsisIcon aria-hidden="true" />
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                            <DropdownMenuItem
                                id="fulfillment-operations-work-surface-discard"
                                onClick={onDiscard}
                            >
                                <Undo2Icon aria-hidden="true" />
                                放弃修改
                            </DropdownMenuItem>
                        </DropdownMenuContent>
                    </DropdownMenu>
                )}
                {supportsSave && (!compact || dirty) ? (
                    <LoadingButton
                        id="fulfillment-operations-work-surface-save"
                        type="button"
                        variant="outline"
                        className="rounded-lg shadow-none"
                        disabled={formalPending || !dirty}
                        loading={savePending}
                        onClick={() => void onSave()}
                    >
                        {!savePending ? (
                            <SaveIcon
                                aria-hidden="true"
                                data-icon="inline-start"
                            />
                        ) : null}
                        {savePending ? "保存中…" : "保存草稿"}
                    </LoadingButton>
                ) : null}
                <Button
                    id="fulfillment-operations-work-surface-confirm"
                    type="button"
                    disabled={formalPending || savePending || !canPost}
                    aria-describedby={
                        !canPost && blockedMessage
                            ? "fulfillment-operations-confirm-blocker"
                            : undefined
                    }
                    onClick={onConfirm}
                >
                    {formalPending ? (
                        <LoaderCircleIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                            className="animate-spin"
                        />
                    ) : (
                        <CircleCheckIcon
                            aria-hidden="true"
                            data-icon="inline-start"
                        />
                    )}
                    {formalPending
                        ? "正在确认…"
                        : autoNext
                          ? `${OPERATION_ACTION_LABEL[operation.operationType]}并下一条`
                          : OPERATION_ACTION_LABEL[operation.operationType]}
                </Button>
            </div>
        </div>
    )
}

function FulfillmentReadOnlyActions({
    readOnlyNote,
    showSalesOrderLinks,
    salesOrderId,
    currentUrl,
}: {
    readOnlyNote: string
    showSalesOrderLinks: boolean
    salesOrderId: string
    currentUrl: string
}) {
    const profile = useAccountProfileQuery()
    const canOpenSales =
        showSalesOrderLinks &&
        salesOrderId &&
        hasPermission(profile.data?.permissions, "sales_order:detail")
    return (
        <div className="flex w-full flex-wrap items-center justify-between gap-2">
            <p className="flex items-center gap-2 text-xs text-muted-foreground">
                <EyeIcon className="size-4 shrink-0" aria-hidden="true" />
                {readOnlyNote}
            </p>
            {canOpenSales ? (
                <Button
                    id="fulfillment-operations-work-surface-open-sales-order"
                    type="button"
                    size="sm"
                    variant="secondary"
                    className="rounded-lg shadow-none"
                    render={
                        <Link href={salesOrderHref(salesOrderId, currentUrl)} />
                    }
                >
                    打开销售单
                    <ArrowRightIcon aria-hidden="true" data-icon="inline-end" />
                </Button>
            ) : null}
        </div>
    )
}
