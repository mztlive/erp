"use client"

import * as React from "react"
import Link from "next/link"
import {
    ArrowRightIcon,
    CircleCheckIcon,
    EyeIcon,
    LoaderCircleIcon,
    SaveIcon,
    SkipForwardIcon,
    Undo2Icon,
} from "lucide-react"

import {
    BusinessStatusBadge,
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
import { LoadingButton } from "@/components/ui/loading-button"
import { FulfillmentDraftForm } from "@/features/fulfillment-operations/components/forms/fulfillment-draft-form"
import {
    OPERATION_ACTION_LABEL,
    OPERATION_TYPE_LABEL,
    type FulfillmentDraft,
    type FulfillmentOperation,
} from "@/features/fulfillment-operations/types"
import { salesOrderHref } from "@/features/fulfillment-operations/pages/lib/gate-copy"
import { displayText } from "@/features/fulfillment-operations/lib/readable-label"
import { displayName } from "@/lib/display-name"
import { sourceContextFields } from "@/features/fulfillment-operations/pages/lib/presentation"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { gateBlockedMessage } from "../../lib/validation"
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
    const paymentBlocked =
        operation.gate.state === "BLOCKED" && draft.type !== "WAREHOUSE_SHIP"
    const fieldIssues = validationIssues.filter((issue) => issue.id !== "gate")
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
            <FulfillmentDeliveryDialogSurface
                operation={operation}
                draft={draft}
                responsibleLabel={deliveryDialog.responsibleLabel}
                currentUrl={currentUrl}
                snapshotUpdatedAt={snapshotUpdatedAt}
                validationIssues={validationIssues}
                showValidation={dirty}
                saveMessage={saveMessage}
                disabled={formalPending || !canExecute || resultUnknown}
                readOnlyNote={!canExecute ? readOnlyNote : undefined}
                headingRef={headingRef}
                onDraftChange={onDraftChange}
                actions={
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
                                autoNext={autoNext}
                                onSkip={onSkip}
                                onDiscard={onDiscard}
                                onSave={onSave}
                                onConfirm={onConfirm}
                            />
                        ) : null}
                    </>
                }
            />
        )
    }

    return (
        <div className={singleOperation ? "min-w-0" : "min-w-0 space-y-3"}>
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
                        !paymentBlocked ? (
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
                className={singleOperation ? undefined : surfacePanelClassName}
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
                <CardContent className="space-y-5">
                    {singleOperation ? (
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <h3
                                ref={headingRef}
                                tabIndex={-1}
                                className="text-base font-semibold outline-none"
                            >
                                {operation.operationType === "SUPPLIER_DIRECT"
                                    ? "确认供应商直发"
                                    : OPERATION_ACTION_LABEL[
                                          operation.operationType
                                      ]}
                            </h3>
                            {!paymentBlocked ? (
                                <FulfillmentGateStatus
                                    operation={operation}
                                    currentUrl={currentUrl}
                                    snapshotUpdatedAt={snapshotUpdatedAt}
                                    showPaymentAction={false}
                                />
                            ) : null}
                        </div>
                    ) : null}
                    {paymentBlocked ? (
                        <WorkspaceTaskPageSlot slot="aside">
                            <FulfillmentPaymentNotice
                                operation={operation}
                                canSaveDraft={canExecute && supportsSave}
                            />
                        </WorkspaceTaskPageSlot>
                    ) : null}
                    <FulfillmentSourceContext
                        operation={operation}
                        currentUrl={currentUrl}
                    />

                    <FulfillmentDraftForm
                        operation={operation}
                        draft={draft}
                        onChange={onDraftChange}
                        disabled={formalPending || !canExecute || resultUnknown}
                    />

                    {fieldIssues.length > 0 ? (
                        <ValidationSummary
                            id="fulfillment-operations-field-validation"
                            compact
                            title="请补充或检查履约信息"
                            issues={fieldIssues}
                        />
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
                            fallback={
                                <div className="sticky bottom-0 flex flex-wrap justify-end gap-2 border-t border-grid bg-card/95 py-3 backdrop-blur">
                                    <FulfillmentExecuteActions
                                        operation={operation}
                                        singleOperation={singleOperation}
                                        dirty={dirty}
                                        supportsSave={supportsSave}
                                        formalPending={formalPending}
                                        savePending={savePending}
                                        canPost={canPost}
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
    autoNext: boolean
    onSkip: () => void
    onDiscard: () => void
    onSave: () => void
    onConfirm: () => void
}) {
    const paymentBlocked =
        operation.gate.state === "BLOCKED" &&
        operation.operationType !== "WAREHOUSE_SHIP"
    const blockedMessage = paymentBlocked
        ? gateBlockedMessage(operation)
        : operation.actionBlockers.find((item) => item.action === "POST")
              ?.message || "请先处理页面中的提示后再确认"
    return (
        <div className="flex flex-wrap items-center justify-end gap-2">
            {!canPost && blockedMessage ? (
                <p
                    id="fulfillment-operations-confirm-blocker"
                    className="max-w-md text-xs text-muted-foreground"
                >
                    {blockedMessage}
                </p>
            ) : null}
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
            {dirty ? (
                <Button
                    id="fulfillment-operations-work-surface-discard"
                    type="button"
                    variant="ghost"
                    disabled={formalPending}
                    onClick={onDiscard}
                >
                    <Undo2Icon data-icon="inline-start" />
                    放弃修改
                </Button>
            ) : null}
            {supportsSave && (!compact || dirty) ? (
                <LoadingButton
                    id="fulfillment-operations-work-surface-save"
                    type="button"
                    variant={paymentBlocked ? "default" : "secondary"}
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
            <p className="flex items-center gap-2 text-sm text-muted-foreground">
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
}: {
    operation: FulfillmentOperation
    currentUrl: string
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
    const fields = sourceContextFields(
        operation,
        salesOrderHref(operation.source.salesOrderId, currentUrl),
    )
    if (fields.length === 0) return null
    return (
        <section aria-label="来源单据">
            <dl className="flex flex-wrap gap-x-8 gap-y-3 rounded-lg bg-muted/20 px-4 py-3">
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
                            className="min-w-0 max-w-full flex-[1_1_auto]"
                        >
                            <dt className="text-xs text-muted-foreground">
                                {field.label}
                            </dt>
                            <dd
                                title={isDocument ? field.value : undefined}
                                className={
                                    isDocument
                                        ? "mt-1 truncate text-sm font-medium"
                                        : "mt-1 break-words text-sm font-medium"
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
