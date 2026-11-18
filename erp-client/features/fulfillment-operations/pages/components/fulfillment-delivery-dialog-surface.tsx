"use client"

import type { ReactNode, Ref } from "react"
import Link from "next/link"
import {
    ArrowUpRightIcon,
    ChevronDownIcon,
    CircleAlertIcon,
} from "lucide-react"

import {
    ValidationSummary,
    WorkspaceTaskFooter,
    type ValidationIssue,
} from "@/components/business"
import { Alert, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { useFulfillmentFieldLocation } from "../hooks/use-fulfillment-field-location"
import { FulfillmentDraftReadOnly } from "../../components/forms/fulfillment-draft-form"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { FulfillmentDeliveryForm } from "../../components/forms/fulfillment-delivery-form"
import { displayText } from "../../lib/readable-label"
import type { FulfillmentDraft, FulfillmentOperation } from "../../types"
import { FulfillmentGateStatus } from "./fulfillment-gate-status"
import { shipmentSummary } from "../lib/presentation"

type DeliveryDraft = Extract<
    FulfillmentDraft,
    { type: "SUPPLIER_DIRECT" | "WAREHOUSE_SHIP" }
>

/** 发货弹窗的单页作业面；表单和固定底栏沿用原确认链路。 */
export function FulfillmentDeliveryDialogSurface({
    operation,
    draft,
    responsibleLabel,
    currentUrl,
    snapshotUpdatedAt,
    validationIssues,
    showValidation,
    saveMessage,
    canExecute,
    disabled,
    readOnlyNote,
    headingRef,
    onDraftChange,
    actions,
}: {
    operation: FulfillmentOperation
    draft: DeliveryDraft
    responsibleLabel: string
    currentUrl: string
    snapshotUpdatedAt: string
    validationIssues: readonly ValidationIssue[]
    showValidation: boolean
    saveMessage: string | null
    canExecute: boolean
    disabled: boolean
    readOnlyNote?: string
    headingRef: Ref<HTMLHeadingElement>
    onDraftChange: (draft: FulfillmentDraft) => void
    actions: (locateIssue: (issue: ValidationIssue) => void) => ReactNode
}) {
    const profile = useAccountProfileQuery()
    const canViewPurchase = hasPermission(
        profile.data?.permissions,
        "purchase_order:detail",
    )
    const { deliveryRef, locateIssue } = useFulfillmentFieldLocation()
    const paymentPending =
        draft.type === "SUPPLIER_DIRECT" && operation.gate.state === "BLOCKED"
    const fields = [
        {
            label: "客户",
            value: displayText(operation.source.customerLabel) || "客户未提供",
        },
        {
            label: draft.type === "SUPPLIER_DIRECT" ? "供应商" : "发货仓",
            value:
                draft.type === "SUPPLIER_DIRECT"
                    ? displayText(operation.source.supplierLabel) ||
                      "供应商未提供"
                    : displayText(operation.source.warehouseLabel) ||
                      "仓库未提供",
        },
        { label: "责任人", value: responsibleLabel || "责任人未提供" },
    ]
    const fieldIssues = validationIssues
    const purchaseNo =
        displayText(
            operation.source.purchaseNo,
            operation.source.purchaseOrderId,
        ) || "采购单号未提供"
    const itemCount = new Set(draft.lines.map((line) => line.salesOrderLineId))
        .size
    const blocker =
        readOnlyNote ||
        operation.actionBlockers.find((item) => item.action === "POST")?.message

    return (
        <div className="space-y-4 px-5 py-4">
            <h3 ref={headingRef} tabIndex={-1} className="sr-only">
                登记发货
            </h3>
            <dl className="grid gap-x-6 gap-y-3 rounded-lg border border-grid px-4 py-3 sm:grid-cols-2">
                {fields.map((field) => (
                    <div
                        key={field.label}
                        className="grid min-w-0 grid-cols-[3rem_minmax(0,1fr)] items-baseline gap-3"
                    >
                        <dt className="text-xs text-muted-foreground">
                            {field.label}
                        </dt>
                        <dd className="min-w-0 break-words text-sm font-medium">
                            {field.value}
                        </dd>
                    </div>
                ))}
            </dl>
            {paymentPending ? (
                <Collapsible>
                    <Alert variant="warning">
                        <CircleAlertIcon aria-hidden="true" />
                        <AlertTitle className="flex flex-wrap items-center justify-between gap-2">
                            <span>预付待完成，不影响确认实际发货</span>
                            <CollapsibleTrigger
                                id="prepayment-gate"
                                render={
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                    />
                                }
                            >
                                查看付款情况
                                <ChevronDownIcon data-icon="inline-end" />
                            </CollapsibleTrigger>
                        </AlertTitle>
                    </Alert>
                    <CollapsibleContent className="pt-3">
                        <FulfillmentGateStatus
                            id="fulfillment-dialog-gate-details"
                            operation={operation}
                            currentUrl={currentUrl}
                            snapshotUpdatedAt={snapshotUpdatedAt}
                            showPaymentAction={false}
                            presentation="panel"
                        />
                    </CollapsibleContent>
                </Collapsible>
            ) : (
                <FulfillmentGateStatus
                    operation={operation}
                    currentUrl={currentUrl}
                    snapshotUpdatedAt={snapshotUpdatedAt}
                    showPaymentAction={false}
                />
            )}
            {canExecute ? (
                <FulfillmentDeliveryForm
                    ref={deliveryRef}
                    operation={operation}
                    draft={draft}
                    onChange={onDraftChange}
                    disabled={disabled}
                    compact
                />
            ) : (
                <FulfillmentDraftReadOnly
                    operation={operation}
                    draft={operation.draft}
                />
            )}
            {showValidation && fieldIssues.length > 0 ? (
                <ValidationSummary
                    id="fulfillment-dialog-field-validation"
                    onLocate={locateIssue}
                    title="还差这些没填好"
                    issues={fieldIssues}
                />
            ) : null}
            {saveMessage ? (
                <p role="status" className="text-xs text-muted-foreground">
                    {saveMessage}
                </p>
            ) : null}
            <div className="divide-y divide-border">
                {operation.source.purchaseOrderId ||
                operation.source.purchaseNo ? (
                    <Collapsible>
                        <CollapsibleTrigger
                            id="fulfillment-dialog-purchase-details"
                            className="flex w-full items-center justify-between gap-3 py-3 text-left text-sm font-medium"
                        >
                            关联采购单
                            <ChevronDownIcon
                                className="size-4 shrink-0"
                                aria-hidden="true"
                            />
                        </CollapsibleTrigger>
                        <CollapsibleContent className="pb-3">
                            <p className="num break-all text-sm text-muted-foreground">
                                {canViewPurchase &&
                                operation.source.purchaseOrderId ? (
                                    <Link
                                        id="fulfillment-dialog-open-purchase-order"
                                        href={`/procurement/orders/${encodeURIComponent(operation.source.purchaseOrderId)}?${new URLSearchParams({ from: "workspace", returnTo: currentUrl })}`}
                                        target="_blank"
                                        rel="noopener noreferrer"
                                        title="在新标签页打开采购单详情"
                                        className="inline-flex max-w-full items-start gap-1 text-primary underline underline-offset-4 hover:text-primary/80"
                                    >
                                        <span className="min-w-0">
                                            {purchaseNo}
                                        </span>
                                        <ArrowUpRightIcon
                                            className="size-4 shrink-0"
                                            aria-hidden="true"
                                        />
                                    </Link>
                                ) : (
                                    purchaseNo
                                )}
                            </p>
                        </CollapsibleContent>
                    </Collapsible>
                ) : null}
            </div>
            <WorkspaceTaskFooter>
                <div className="flex w-full flex-wrap items-center justify-between gap-3">
                    <div className="min-w-0 flex-1 text-xs text-muted-foreground">
                        <p className="num">
                            {shipmentSummary(operation, draft) ??
                                `${itemCount} 项商品 · ${draft.trackingEntries?.length ?? 0} 条物流记录`}
                        </p>
                        {blocker ? <p className="mt-1">{blocker}</p> : null}
                    </div>
                    <div className="flex flex-wrap items-center justify-end gap-2">
                        {actions(locateIssue)}
                    </div>
                </div>
            </WorkspaceTaskFooter>
        </div>
    )
}
