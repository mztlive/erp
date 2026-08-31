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
import { FulfillmentDraftForm } from "@/features/fulfillment-operations/components/forms/fulfillment-draft-form"
import {
    OPERATION_ACTION_LABEL,
    OPERATION_TYPE_LABEL,
    type FulfillmentDraft,
    type FulfillmentOperation,
} from "@/features/fulfillment-operations/types"
import { salesOrderHref } from "@/features/fulfillment-operations/pages/lib/gate-copy"
import { displayText } from "@/features/fulfillment-operations/lib/readable-label"
import { sourceContextFields } from "@/features/fulfillment-operations/pages/lib/presentation"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { FulfillmentGateStatus } from "./fulfillment-gate-status"

export type FulfillmentWorkSurfaceProps = {
    operation: FulfillmentOperation
    draft: FulfillmentDraft
    validationIssues: readonly ValidationIssue[]
    saveMessage: string | null
    canExecute: boolean
    canPost: boolean
    formalPending: boolean
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
    const headerSubtitle = [
        displayText(operation.source.customerLabel),
        displayText(operation.source.purchaseNo)
            ? `采购 ${displayText(operation.source.purchaseNo)}`
            : "",
        displayText(operation.source.supplierLabel),
    ]
        .filter(Boolean)
        .join(" · ")

    return (
        <div className={singleOperation ? "min-w-0" : "min-w-0 space-y-3"}>
            <SequentialProcessBar
                id="fulfillment-operations-work-surface-process-bar"
                current={position}
                total={total}
                responsibilityStatus={responsibilityStatus}
                responsibilityStatusLabel={responsibilityStatusLabel}
                showProcess={false}
                processLabel={OPERATION_ACTION_LABEL[operation.operationType]}
                // 正式确认只留卡片底栏一颗主按钮，避免与连续处理条文案打架
                showProcessNext={false}
                processDisabled={formalPending || !canPost}
                statusExtras={
                    <FulfillmentGateStatus
                        operation={operation}
                        currentUrl={currentUrl}
                        snapshotUpdatedAt={snapshotUpdatedAt}
                        showPaymentAction={showSalesOrderLinks}
                    />
                }
                onBack={onBack}
                backLabel="返回"
                showBack={showBack}
                onProcess={onConfirm}
                onProcessNext={onConfirm}
            />

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
                                    {displayText(operation.source.salesOrderNo)
                                        ? ` · ${displayText(operation.source.salesOrderNo)}`
                                        : ""}
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
                        <h3 ref={headingRef} tabIndex={-1} className="sr-only">
                            {OPERATION_TYPE_LABEL[operation.operationType]}
                        </h3>
                    ) : null}
                    <FulfillmentSourceContext
                        operation={operation}
                        currentUrl={currentUrl}
                        showSalesOrderLinks={showSalesOrderLinks}
                    />

                    <FulfillmentDraftForm
                        operation={operation}
                        draft={draft}
                        onChange={onDraftChange}
                        disabled={formalPending || !canExecute || resultUnknown}
                    />

                    {validationIssues.length > 0 ? (
                        <ValidationSummary
                            title="还差这些没填好"
                            issues={validationIssues}
                        />
                    ) : null}

                    {saveMessage ? (
                        <p className="text-xs text-muted-foreground">
                            {saveMessage}
                        </p>
                    ) : null}

                    {canExecute ? (
                        <div className="sticky bottom-0 flex flex-wrap justify-end gap-2 border-t border-grid bg-card/95 py-3 backdrop-blur">
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
                            {supportsSave ? (
                                <Button
                                    id="fulfillment-operations-work-surface-save"
                                    type="button"
                                    variant="secondary"
                                    className="rounded-lg shadow-none"
                                    disabled={formalPending || !dirty}
                                    onClick={() => void onSave()}
                                >
                                    {formalPending ? (
                                        <LoaderCircleIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                            className="animate-spin"
                                        />
                                    ) : (
                                        <SaveIcon data-icon="inline-start" />
                                    )}
                                    {formalPending ? "保存中…" : "保存"}
                                </Button>
                            ) : null}
                            <Button
                                id="fulfillment-operations-work-surface-confirm"
                                type="button"
                                disabled={formalPending || !canPost}
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
                                    ? "处理中…"
                                    : autoNext
                                      ? `${OPERATION_ACTION_LABEL[operation.operationType]}并下一条`
                                      : OPERATION_ACTION_LABEL[
                                            operation.operationType
                                        ]}
                            </Button>
                        </div>
                    ) : (
                        /* 只读角色：与其摆一排点不动的按钮，不如说清楚谁在处理 */
                        <div className="sticky bottom-0 flex flex-wrap items-center justify-between gap-2 border-t border-grid bg-card/95 py-3 backdrop-blur">
                            <p className="flex items-center gap-2 text-sm text-muted-foreground">
                                <EyeIcon
                                    className="size-4 shrink-0"
                                    aria-hidden="true"
                                />
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
                                        <Link
                                            href={salesOrderHref(
                                                operation.source.salesOrderId,
                                                currentUrl,
                                            )}
                                        />
                                    }
                                >
                                    打开销售单
                                    <ArrowRightIcon data-icon="inline-end" />
                                </Button>
                            ) : null}
                        </div>
                    )}
                </CardContent>
            </Card>
        </div>
    )
}

function FulfillmentSourceContext({
    operation,
    currentUrl,
    showSalesOrderLinks,
}: {
    operation: FulfillmentOperation
    currentUrl: string
    showSalesOrderLinks: boolean
}) {
    const fields = sourceContextFields(
        operation,
        salesOrderHref(operation.source.salesOrderId, currentUrl),
    )
    if (fields.length === 0) return null
    return (
        <section aria-label="来源单据">
            <dl
                className={cn(
                    "grid gap-px overflow-hidden rounded-lg border border-grid bg-grid",
                    fields.length === 1
                        ? "grid-cols-1"
                        : "sm:grid-cols-2 lg:grid-cols-3",
                )}
            >
                {fields.map((field) => (
                    <div
                        key={field.label}
                        className="bg-card px-3 py-2.5 last:col-end-[-1]"
                    >
                        <dt className="text-xs text-muted-foreground">
                            {field.label}
                        </dt>
                        <dd className="mt-1 text-sm font-medium">
                            {showSalesOrderLinks &&
                            field.href &&
                            operation.source.salesOrderId ? (
                                <Link
                                    id={`fulfillment-operations-work-surface-source-${toAutomationIdSegment(field.label)}`}
                                    href={field.href}
                                    className="text-primary underline-offset-4 hover:underline"
                                >
                                    {field.value}
                                </Link>
                            ) : (
                                field.value
                            )}
                        </dd>
                    </div>
                ))}
            </dl>
        </section>
    )
}
