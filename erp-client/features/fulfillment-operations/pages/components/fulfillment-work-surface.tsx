"use client"

import * as React from "react"
import Link from "next/link"
import {
    ArrowRightIcon,
    CircleCheckIcon,
    EyeIcon,
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
import { Separator } from "@/components/ui/separator"
import { Button } from "@/components/ui/button"
import { FulfillmentDraftForm } from "@/features/fulfillment-operations/components/forms/fulfillment-draft-form"
import {
    OPERATION_ACTION_LABEL,
    OPERATION_TYPE_LABEL,
    type FulfillmentDraft,
    type FulfillmentOperation,
} from "@/features/fulfillment-operations/types"
import { salesOrderHref } from "@/features/fulfillment-operations/pages/lib/gate-copy"
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
    onDraftChange,
    onSkip,
    onDiscard,
    onSave,
    onConfirm,
    onBack,
    onToggleShortcuts,
}: FulfillmentWorkSurfaceProps) {
    return (
        <div className="min-w-0 space-y-3">
            <SequentialProcessBar
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
                    />
                }
                onBack={onBack}
                backLabel="返回"
                onProcess={onConfirm}
                onProcessNext={onConfirm}
            />

            <button
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

            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-grid">
                    <div className="flex flex-wrap items-start justify-between gap-2">
                        <div>
                            <CardTitle
                                ref={headingRef}
                                tabIndex={-1}
                                aria-live="polite"
                                className="outline-none"
                            >
                                {OPERATION_TYPE_LABEL[operation.operationType]}{" "}
                                · {operation.source.salesOrderNo}
                            </CardTitle>
                            <CardDescription>
                                {operation.source.customerLabel}
                                {operation.source.purchaseNo
                                    ? ` · 采购 ${operation.source.purchaseNo}`
                                    : ""}
                                {operation.source.supplierLabel
                                    ? ` · ${operation.source.supplierLabel}`
                                    : ""}
                            </CardDescription>
                        </div>
                        <BusinessStatusBadge
                            context="list"
                            label={operation.statusLabel}
                            tone={operation.statusTone}
                        />
                    </div>
                </CardHeader>
                <CardContent className="space-y-4">
                    <section aria-label="来源上下文">
                        <dl className="grid gap-px overflow-hidden rounded-lg border border-grid bg-grid sm:grid-cols-2 lg:grid-cols-3">
                            {[
                                {
                                    label: "销售单",
                                    value: operation.source.salesOrderNo,
                                    href: salesOrderHref(
                                        operation.source.salesOrderId,
                                        currentUrl,
                                    ),
                                },
                                {
                                    label: "采购单",
                                    value:
                                        operation.source.purchaseNo ?? "—",
                                },
                                {
                                    label: "仓库",
                                    value:
                                        operation.source.warehouseLabel ??
                                        "不涉及仓库",
                                },
                                {
                                    label: "还剩多少",
                                    value: operation.lines
                                        .map(
                                            (l) =>
                                                `${l.itemName} ${l.remainingQuantity}${l.unitCode}`,
                                        )
                                        .join("；"),
                                    numeric: true,
                                },
                                {
                                    label: "供应商",
                                    value:
                                        operation.source.supplierLabel ?? "—",
                                },
                                {
                                    label: "客户",
                                    value: operation.source.customerLabel,
                                },
                            ].map((field) => (
                                <div
                                    key={field.label}
                                    className="bg-card p-3"
                                >
                                    <dt className="text-xs text-muted-foreground">
                                        {field.label}
                                    </dt>
                                    <dd
                                        className={cn(
                                            "mt-1 font-medium",
                                            field.numeric && "num",
                                        )}
                                    >
                                        {field.href &&
                                        field.value !== "—" ? (
                                            <Link
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

                    <Separator />

                    <FulfillmentDraftForm
                        operation={operation}
                        draft={draft}
                        onChange={onDraftChange}
                        disabled={
                            formalPending || !canExecute || resultUnknown
                        }
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
                            <Button
                                type="button"
                                variant="ghost"
                                disabled={formalPending}
                                onClick={onSkip}
                            >
                                <SkipForwardIcon data-icon="inline-start" />
                                先跳过
                            </Button>
                            <Button
                                type="button"
                                variant="ghost"
                                disabled={formalPending || !dirty}
                                onClick={onDiscard}
                            >
                                <Undo2Icon data-icon="inline-start" />
                                放弃修改
                            </Button>
                            <Button
                                type="button"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                disabled={
                                    formalPending || !dirty || !supportsSave
                                }
                                onClick={() => void onSave()}
                            >
                                <SaveIcon data-icon="inline-start" />
                                保存草稿
                            </Button>
                            <Button
                                type="button"
                                disabled={formalPending || !canPost}
                                onClick={onConfirm}
                            >
                                <CircleCheckIcon data-icon="inline-start" />
                                {autoNext
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
                            <Button
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
                        </div>
                    )}
                </CardContent>
            </Card>
        </div>
    )
}
