"use client"

import { useMemo, useState } from "react"
import { useSelector } from "@tanstack/react-form"
import { ArrowUpRightIcon, PencilLineIcon } from "lucide-react"
import { z } from "zod"
import { useAppForm, toFieldErrors } from "@/components/form"
import { FormalActionConfirmDialog } from "@/components/business"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { FinancialDraftEditor } from "@/features/financial-draft-edit/components/financial-draft-edit-dialog"
import type { FinancialDraftKind } from "@/features/financial-draft-edit/types"
import { AdjustmentEditDialog } from "@/features/inventory/pages/components/adjustment-edit-dialog"
import { PurchaseChangeOrderEditDialog } from "@/features/purchase-orders/components/purchase-change-order-edit-dialog"
import { SalesChangeOrderEditDialog } from "@/features/sales-orders/components/sales-change-order-edit-dialog"
import { useSalesOrderDetailQuery } from "@/features/sales-orders/hooks/queries"
import { useAbandonApprovalReworkMutation } from "@/features/work-items/queries"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    FormalCommandKeyLedger,
    classifyFormalCommandError,
} from "@/lib/formal-command"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import {
    WORK_ITEM_ABANDON_REASON_MAX_CHARS,
    workItemAbandonReasonSchema,
} from "@/lib/validation/reason-length"
import { navigateWorkspace } from "@/lib/workspace-tabs"
import { buildHandlerHref } from "../lib/handler-destination"
import type { WorkspaceWorkItem } from "../types"

const FINANCIAL_INLINE_KINDS = new Set<FinancialDraftKind>([
    "customer_receipt",
    "customer_refund",
    "receipt_reversal",
    "supplier_refund",
    "payment_reversal",
])

function resolveFinancialDraftKind(
    documentType?: string,
): FinancialDraftKind | null {
    if (
        documentType &&
        FINANCIAL_INLINE_KINDS.has(documentType as FinancialDraftKind)
    ) {
        return documentType as FinancialDraftKind
    }
    return null
}

/**
 * 驳回归提交人处理：
 * - 财务单据（回款/退款/冲正）、库存调整、销售/采购变更单支持原位直接唤起编辑弹窗就地修改重提；
 * - 销售单/采购单/开票申请/付款申请等重型单据通过 `navigateWorkspace` 在工作台新页签打开编辑，不冲毁当前工作台上下文；
 * - 放弃本次办理接入统一 `ReasonTextareaField`（512 字符实时计数 + Ctrl/⌘+Enter 提交）。
 */
export function WorkspaceApprovalReworkActions({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const router = useRouter()
    const profile = useAccountProfileQuery()
    const mutation = useAbandonApprovalReworkMutation()
    const ledger = useMemo(() => new FormalCommandKeyLedger(), [])
    const [abandonOpen, setAbandonOpen] = useState(false)
    const [inlineEditOpen, setInlineEditOpen] = useState(false)
    const [error, setError] = useState<string | null>(null)
    const [unknown, setUnknown] = useState(false)
    const id = `workspace-rework-${toAutomationIdSegment(item.workItemId)}`
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const destination = buildHandlerHref({
        ...item,
        approvalInstanceId: item.approvalProcessInstanceId,
    })
    const [path, query] = destination?.split("?", 2) ?? []
    const params = new URLSearchParams(query)
    const qs = searchParams.toString()
    params.set("returnTo", qs ? `${pathname}?${qs}` : pathname)
    const href = path ? `${path}?${params.toString()}` : null

    const docType = item.routeContext?.documentType ?? item.businessObjectType
    const financialKind = resolveFinancialDraftKind(docType)
    const isStockAdjustment = docType === "stock_adjustment"
    const isSalesChangeOrder =
        docType === "sales_change_order" &&
        Boolean(item.rootBusinessObjectId?.trim())
    const isPurchaseChangeOrder =
        docType === "purchase_change_order" &&
        Boolean(item.rootBusinessObjectId?.trim())
    const supportsInlineEdit = Boolean(
        financialKind ||
        isStockAdjustment ||
        isSalesChangeOrder ||
        isPurchaseChangeOrder,
    )

    const salesOrderForChangeQuery = useSalesOrderDetailQuery(
        isSalesChangeOrder && inlineEditOpen
            ? (item.rootBusinessObjectId ?? "")
            : "",
        isSalesChangeOrder && inlineEditOpen,
    )

    const canProcess =
        item.status === "OPEN" && item.allowedActions.includes("PROCESS")
    const canSubmitStockAdjustment = hasPermission(
        profile.data?.permissions,
        "stock_adjustment:submit",
    )

    const openInWorkspaceTab = () => {
        if (!href) return
        if (!navigateWorkspace(href)) {
            router.push(href)
        }
    }

    const form = useAppForm({
        defaultValues: { reason: "" },
        validators: {
            onChange: z.object({
                reason: workItemAbandonReasonSchema,
            }),
        },
        onSubmit: async ({ value }) => {
            if (mutation.isPending) return
            const reason = value.reason.trim()
            if (!reason) {
                setError("请填写放弃办理的原因")
                return
            }
            const command = ledger.acquire(
                "abandon",
                `rework:${item.workItemId}`,
                {
                    workItemId: item.workItemId,
                    expectedTaskVersion: item.taskVersion,
                    reason,
                },
            )
            if (!command) return
            setError(null)
            try {
                await mutation.mutateAsync({
                    ...command.payload,
                    idempotencyKey: command.idempotencyKey,
                })
                ledger.settle("abandon", "succeeded")
                setUnknown(false)
                setAbandonOpen(false)
                onTaskCompleted?.(item.workItemId)
            } catch (cause) {
                const result = classifyFormalCommandError(cause)
                ledger.settle("abandon", result)
                setUnknown(result === "unknown")
                setError(getErrorMessage(cause, "放弃办理未完成，请重试"))
                throw cause
            }
        },
    })
    const rawReason = useSelector(form.store, (state) => state.values.reason)
    const reasonValid = workItemAbandonReasonSchema.safeParse(rawReason).success

    return (
        <>
            <div className="flex flex-wrap items-center gap-2">
                {canProcess && supportsInlineEdit ? (
                    <>
                        <Button
                            id={`${id}-inline-edit`}
                            type="button"
                            size="sm"
                            disabled={mutation.isPending || unknown}
                            onClick={() => setInlineEditOpen(true)}
                        >
                            <PencilLineIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            就地修改并重提
                        </Button>
                        {href ? (
                            <Button
                                id={`${id}-edit`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={mutation.isPending || unknown}
                                onClick={openInWorkspaceTab}
                            >
                                打开原单页面
                                <ArrowUpRightIcon
                                    data-icon="inline-end"
                                    aria-hidden="true"
                                />
                            </Button>
                        ) : null}
                    </>
                ) : canProcess && href ? (
                    <Button
                        id={`${id}-edit`}
                        type="button"
                        size="sm"
                        disabled={mutation.isPending || unknown}
                        onClick={openInWorkspaceTab}
                    >
                        修改或重新提交
                        <ArrowUpRightIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                        />
                    </Button>
                ) : null}
                {item.status === "OPEN" &&
                item.allowedActions.includes("ABANDON_REWORK") ? (
                    <Button
                        id={`${id}-abandon`}
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={mutation.isPending || unknown}
                        onClick={() => setAbandonOpen(!abandonOpen)}
                    >
                        放弃本次办理
                    </Button>
                ) : null}
            </div>

            {inlineEditOpen && financialKind ? (
                <FinancialDraftEditor
                    key={`${financialKind}:${item.businessObjectId}`}
                    kind={financialKind}
                    documentId={item.businessObjectId}
                    onClose={() => setInlineEditOpen(false)}
                    onSubmitted={() => {
                        setInlineEditOpen(false)
                        onTaskCompleted?.(item.workItemId)
                    }}
                />
            ) : null}

            {inlineEditOpen && isStockAdjustment ? (
                <AdjustmentEditDialog
                    key={item.businessObjectId}
                    id={item.businessObjectId}
                    canSubmit={canSubmitStockAdjustment}
                    onClose={() => setInlineEditOpen(false)}
                    onSubmitted={() => {
                        setInlineEditOpen(false)
                        onTaskCompleted?.(item.workItemId)
                    }}
                />
            ) : null}

            {inlineEditOpen &&
            isSalesChangeOrder &&
            item.rootBusinessObjectId ? (
                <SalesChangeOrderEditDialog
                    open={inlineEditOpen}
                    onOpenChange={setInlineEditOpen}
                    salesOrderId={item.rootBusinessObjectId}
                    changeOrderId={item.businessObjectId}
                    nature={
                        salesOrderForChangeQuery.data?.nature ??
                        "physical_service"
                    }
                    onApplied={() => {
                        setInlineEditOpen(false)
                        onTaskCompleted?.(item.workItemId)
                    }}
                />
            ) : null}

            {inlineEditOpen &&
            isPurchaseChangeOrder &&
            item.rootBusinessObjectId ? (
                <PurchaseChangeOrderEditDialog
                    open={inlineEditOpen}
                    onOpenChange={setInlineEditOpen}
                    purchaseOrderId={item.rootBusinessObjectId}
                    changeOrderId={item.businessObjectId}
                    onResult={() => {
                        setInlineEditOpen(false)
                        onTaskCompleted?.(item.workItemId)
                    }}
                />
            ) : null}

            <FormalActionConfirmDialog
                idPrefix={`${id}-confirm-abandon`}
                open={abandonOpen}
                onOpenChange={(open) => {
                    if (!mutation.isPending && !unknown) setAbandonOpen(open)
                }}
                title="放弃本次办理"
                actionLabel="放弃本次办理"
                confirmLabel={unknown ? "重试本次操作" : "确认放弃"}
                actionVariant="destructive"
                fromStatus={{ label: "驳回待处理", tone: "warning" }}
                toStatus={{ label: "已关闭", tone: "neutral" }}
                description="放弃后关闭此待办。原单据仍为已驳回，内容和审批记录保留，后续可从单据详情再次办理。"
                pending={mutation.isPending}
                confirmDisabled={!reasonValid}
                onConfirm={() => form.handleSubmit()}
                formContent={
                    <div className="space-y-4">
                        <form.Field name="reason">
                            {(field) => {
                                const fieldError = field.state.meta.isTouched
                                    ? toFieldErrors(
                                          field.state.meta.errors,
                                      ).find((entry) => entry?.message)?.message
                                    : undefined
                                return (
                                    <ReasonTextareaField
                                        id={`${id}-reason`}
                                        name={field.name}
                                        mode="chars"
                                        max={WORK_ITEM_ABANDON_REASON_MAX_CHARS}
                                        label="放弃原因"
                                        required
                                        rows={3}
                                        placeholder="说明放弃本次办理的原因"
                                        value={field.state.value}
                                        onChange={field.handleChange}
                                        onBlur={field.handleBlur}
                                        disabled={mutation.isPending || unknown}
                                        error={fieldError}
                                        onCtrlEnterSubmit={() => {
                                            if (
                                                reasonValid &&
                                                !mutation.isPending &&
                                                !unknown
                                            ) {
                                                void form.handleSubmit()
                                            }
                                        }}
                                    />
                                )
                            }}
                        </form.Field>
                        {error ? (
                            <p
                                role="alert"
                                className="text-body-compact text-destructive"
                            >
                                {unknown
                                    ? "处理结果待确认，请保留输入并重试同一次操作。"
                                    : error}
                            </p>
                        ) : null}
                    </div>
                }
            />
        </>
    )
}
