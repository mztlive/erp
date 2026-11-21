"use client"

import * as React from "react"
import Link from "next/link"
import {
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
    SequentialProcessBar,
    ValidationSummary,
    WorkspaceTaskFooter,
    WorkspaceTaskPageSlot,
    surfacePanelClassName,
    type ValidationIssue,
} from "@/components/business"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
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
import {
    sourceContextFields,
    shipmentSummary,
} from "@/features/fulfillment-operations/pages/lib/presentation"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { FulfillmentPaymentNotice } from "./fulfillment-payment-notice"
import { FulfillmentGateStatus } from "./fulfillment-gate-status"
import { FulfillmentDeliveryDialogSurface } from "./fulfillment-delivery-dialog-surface"

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
    /** W01 内联作业面只处理任务绑定的单个对象。 */
    singleOperation?: boolean
    deliveryDialog?: {
        responsibleLabel: string
        onClose?: () => void
    }
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
export function FulfillmentWorkSurface({
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
    deliveryDialog,
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

    if (
        deliveryDialog &&
        (draft.type === "SUPPLIER_DIRECT" || draft.type === "WAREHOUSE_SHIP")
    ) {
        return (
            <>
                {operation.loadWarnings?.map((message) => (
                    <BusinessFailureState
                        key={message}
                        kind="business"
                        title="部分关联资料读取失败"
                        description={message}
                    />
                ))}
                <FulfillmentDeliveryDialogSurface
                    operation={operation}
                    draft={draft}
                    responsibleLabel={deliveryDialog.responsibleLabel}
                    currentUrl={currentUrl}
                    snapshotUpdatedAt={snapshotUpdatedAt}
                    validationIssues={validationIssues}
                    showValidation={canExecute && dirty}
                    saveMessage={saveMessage}
                    canExecute={canExecute}
                    disabled={formalPending || resultUnknown}
                    readOnlyNote={!canExecute ? readOnlyNote : undefined}
                    headingRef={headingRef}
                    onDraftChange={onDraftChange}
                    actions={(locateDialogIssue) => (
                        <>
                            <Button
                                id="fulfillment-operations-dialog-cancel"
                                type="button"
                                variant="outline"
                                disabled={formalPending}
                                onClick={deliveryDialog.onClose}
                            >
                                取消
                            </Button>
                            {canExecute ? (
                                <FulfillmentExecuteActions
                                    operation={operation}
                                    singleOperation
                                    compact
                                    dirty={dirty}
                                    supportsSave={supportsSave}
                                    formalPending={formalPending}
                                    savePending={savePending}
                                    canPost={canPost}
                                    validationIssues={validationIssues}
                                    onLocate={locateDialogIssue}
                                    autoNext={autoNext}
                                    onSkip={onSkip}
                                    onDiscard={onDiscard}
                                    onSave={onSave}
                                    onConfirm={onConfirm}
                                />
                            ) : null}
                        </>
                    )}
                />
            </>
        )
    }

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
                <SequentialProcessBar
                    id="fulfillment-operations-work-surface-process-bar"
                    current={position}
                    total={total}
                    showPosition={!singleOperation}
                    responsibilityStatus={responsibilityStatus}
                    responsibilityStatusLabel={responsibilityStatusLabel}
                    showProcess={false}
                    processLabel={
                        OPERATION_ACTION_LABEL[operation.operationType]
                    }
                    // 正式确认只留卡片底栏一颗主按钮，避免与连续处理条文案打架
                    showProcessNext={false}
                    processDisabled={formalPending || !canPost}
                    statusExtras={
                        !paymentPending ? (
                            <FulfillmentGateStatus
                                operation={operation}
                                currentUrl={currentUrl}
                                snapshotUpdatedAt={snapshotUpdatedAt}
                                showPaymentAction={showSalesOrderLinks}
                            />
                        ) : undefined
                    }
                    onBack={onBack}
                    backLabel="返回"
                    showBack={showBack}
                    onProcess={onConfirm}
                    onProcessNext={onConfirm}
                />
            ) : null}

            {!singleOperation ? (
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
            ) : null}

            <Card
                size="sm"
                className={
                    singleOperation && addressPacking
                        ? "min-h-0 flex-1 border-0 bg-transparent py-0 shadow-none"
                        : singleOperation
                          ? undefined
                          : surfacePanelClassName
                }
            >
                {singleOperation ? null : (
                    <CardHeader className="border-b border-grid">
                        <div className="flex flex-wrap items-start justify-between gap-2">
                            <div>
                                <CardTitle
                                    ref={headingRef}
                                    tabIndex={-1}
                                    aria-live="polite"
                                    className="outline-none"
                                >
                                    {
                                        OPERATION_TYPE_LABEL[
                                            operation.operationType
                                        ]
                                    }
                                    {salesOrderNo ? ` · ${salesOrderNo}` : ""}
                                </CardTitle>
                                {headerSubtitle ? (
                                    <CardDescription>
                                        {headerSubtitle}
                                    </CardDescription>
                                ) : null}
                            </div>
                            <BusinessStatusBadge
                                context="list"
                                label={operation.statusLabel}
                                tone={operation.statusTone}
                            />
                        </div>
                    </CardHeader>
                )}
                <CardContent
                    className={
                        singleOperation && addressPacking
                            ? "flex min-h-0 flex-1 flex-col gap-3 px-0"
                            : "space-y-5"
                    }
                >
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
                </CardContent>
            </Card>
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
                        <SkipForwardIcon data-icon="inline-start" />
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
                            <EllipsisIcon />
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                            <DropdownMenuItem
                                id="fulfillment-operations-work-surface-discard"
                                onClick={onDiscard}
                            >
                                <Undo2Icon />
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
                            <SaveIcon data-icon="inline-start" />
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
                        <CircleCheckIcon data-icon="inline-start" />
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
    return (
        <div className="flex w-full flex-wrap items-center justify-between gap-2">
            <p className="flex items-center gap-2 text-xs text-muted-foreground">
                <EyeIcon className="size-4 shrink-0" aria-hidden="true" />
                {readOnlyNote}
            </p>
            {showSalesOrderLinks ? (
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
                    <ArrowRightIcon data-icon="inline-end" />
                </Button>
            ) : null}
        </div>
    )
}

function FulfillmentSourceContext({
    operation,
    currentUrl,
    customerInTitle = false,
}: {
    operation: FulfillmentOperation
    currentUrl: string
    customerInTitle?: boolean
}) {
    const profile = useAccountProfileQuery()
    const canViewPurchase = hasPermission(
        profile.data?.permissions,
        "purchase_order:detail",
    )
    const canViewSales = hasPermission(
        profile.data?.permissions,
        "sales_order:detail",
    )
    const fields = [
        ...sourceContextFields(
            operation,
            salesOrderHref(operation.source.salesOrderId, currentUrl),
        ),
    ]
        .filter((field) => !customerInTitle || field.label !== "客户")
        .sort((left, right) => {
            const order = ["客户", "供应商", "仓库", "销售单", "采购单"]
            return order.indexOf(left.label) - order.indexOf(right.label)
        })
    if (fields.length === 0) return null
    return (
        <section aria-label="来源单据">
            <dl className="my-3 flex flex-wrap gap-x-6 gap-y-2 border-t border-border/50 pt-3">
                {fields.map((field) => {
                    const purchaseLink =
                        field.label === "采购单" &&
                        operation.source.purchaseOrderId &&
                        canViewPurchase
                            ? `/procurement/orders/${encodeURIComponent(operation.source.purchaseOrderId)}`
                            : undefined
                    const href =
                        purchaseLink || (canViewSales ? field.href : undefined)
                    const isDocument =
                        field.label === "销售单" || field.label === "采购单"
                    return (
                        <div
                            key={field.label}
                            className="flex min-w-0 items-baseline gap-3"
                        >
                            <dt className="shrink-0 text-xs text-muted-foreground">
                                {field.label}
                            </dt>
                            <dd
                                title={isDocument ? field.value : undefined}
                                className={
                                    isDocument
                                        ? "min-w-0 truncate text-body-compact font-medium"
                                        : "min-w-0 break-words text-body-compact font-medium"
                                }
                            >
                                {href ? (
                                    <Link
                                        id={`fulfillment-operations-work-surface-source-${toAutomationIdSegment(field.label)}`}
                                        href={href}
                                        target="_blank"
                                        rel="noopener noreferrer"
                                        className="text-primary underline underline-offset-4"
                                    >
                                        {field.value}
                                        <span className="sr-only">
                                            （在新标签页打开{field.label}）
                                        </span>
                                    </Link>
                                ) : (
                                    field.value
                                )}
                            </dd>
                        </div>
                    )
                })}
            </dl>
        </section>
    )
}
