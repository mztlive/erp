"use client"

import { useEffect, useRef, useState } from "react"
import { useSelector } from "@tanstack/react-form"
import { useQueryClient } from "@tanstack/react-query"
import { ArrowUpRightIcon, CircleAlertIcon, PencilLineIcon } from "lucide-react"
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
import { getErrorMessage, isApiError } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    FormalCommandKeyLedger,
    isCommandOutcomeUnknown,
    type FormalCommandIdentity,
} from "@/lib/formal-command"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import { queryKeyRoots } from "@/lib/query-key-roots"
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

const ABANDON_COMMAND = "abandon"

type AbandonPayload = {
    workItemId: string
    expectedTaskVersion: string
    reason: string
}

type AbandonIssue =
    | { kind: "unknown" }
    | { kind: "conflict"; taskVersion: string }
    | { kind: "failed"; message: string }

type ReworkActionsProps = {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}

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
export function WorkspaceApprovalReworkActions(props: ReworkActionsProps) {
    return <ReworkActions key={props.item.workItemId} {...props} />
}

function ReworkActions({ item, onTaskCompleted }: ReworkActionsProps) {
    const router = useRouter()
    const profile = useAccountProfileQuery()
    const mutation = useAbandonApprovalReworkMutation()
    const queryClient = useQueryClient()
    const [ledger] = useState(() => new FormalCommandKeyLedger())
    const submitting = useRef(false)
    const [abandonOpen, setAbandonOpen] = useState(false)
    const [inlineEditOpen, setInlineEditOpen] = useState(false)
    const [issue, setIssue] = useState<AbandonIssue | null>(null)
    const [refreshing, setRefreshing] = useState(false)
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
    const canAbandon =
        item.status === "OPEN" && item.allowedActions.includes("ABANDON_REWORK")
    const unknown = issue?.kind === "unknown"
    const versionConflict =
        issue?.kind === "conflict" && issue.taskVersion === item.taskVersion

    // 外部刷新后任务已关闭或版本推进时自动解除滞留的未知结果锁。
    useEffect(() => {
        if (!unknown) return
        const lockedCommand = ledger.peek<AbandonPayload>(ABANDON_COMMAND)
        if (
            !canAbandon ||
            !lockedCommand ||
            lockedCommand.payload.expectedTaskVersion !== item.taskVersion
        ) {
            ledger.settle(ABANDON_COMMAND, "succeeded")
            setIssue(null)
        }
    }, [canAbandon, item.taskVersion, ledger, unknown])
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

    async function submitAbandon(
        command: FormalCommandIdentity<AbandonPayload>,
    ) {
        if (submitting.current) return
        submitting.current = true
        try {
            await mutation.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
        } catch (cause) {
            if (isCommandOutcomeUnknown(cause)) {
                ledger.settle(ABANDON_COMMAND, "unknown")
                setIssue({ kind: "unknown" })
                setAbandonOpen(false)
                throw cause
            }

            ledger.settle(ABANDON_COMMAND, "failed")
            setIssue(
                isApiError(cause) && cause.status === 409
                    ? {
                          kind: "conflict",
                          taskVersion: command.payload.expectedTaskVersion,
                      }
                    : {
                          kind: "failed",
                          message: getErrorMessage(
                              cause,
                              "放弃办理未完成，请重试",
                          ),
                      },
            )
            setAbandonOpen(canAbandon)
            throw cause
        } finally {
            submitting.current = false
        }

        ledger.settle(ABANDON_COMMAND, "succeeded")
        setIssue(null)
        setAbandonOpen(false)
        form.reset()
        onTaskCompleted?.(command.payload.workItemId)
    }

    async function refreshTask() {
        setRefreshing(true)
        try {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: queryKeyRoots.workItems,
                }),
                queryClient.invalidateQueries({
                    queryKey: queryKeyRoots.workspaceHome,
                }),
            ])
        } finally {
            setRefreshing(false)
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
            if (submitting.current || unknown || versionConflict || !canAbandon)
                return
            setIssue(null)
            let command: FormalCommandIdentity<AbandonPayload>
            try {
                command = ledger.acquire(
                    ABANDON_COMMAND,
                    `rework:${item.workItemId}`,
                    {
                        workItemId: item.workItemId,
                        expectedTaskVersion: item.taskVersion,
                        reason: value.reason.trim(),
                    },
                )
            } catch (cause) {
                setIssue({
                    kind: "failed",
                    message: getErrorMessage(cause, "无法提交，请重试"),
                })
                throw cause
            }
            await submitAbandon(command)
        },
    })
    const rawReason = useSelector(form.store, (state) => state.values.reason)
    const formSubmitting = useSelector(
        form.store,
        (state) => state.isSubmitting,
    )
    const pending = mutation.isPending || formSubmitting
    const reasonValid = workItemAbandonReasonSchema.safeParse(rawReason).success

    return (
        <>
            {unknown ? (
                <div className="flex min-w-0 flex-1 flex-wrap items-center justify-end gap-2">
                    <div
                        role="alert"
                        className="flex min-w-0 flex-[1_1_16rem] items-start gap-2 rounded-r-md border-l-2 border-l-warning bg-warning-soft/50 px-3 py-2 text-xs text-warning-soft-foreground"
                    >
                        <CircleAlertIcon
                            aria-hidden="true"
                            className="mt-0.5 size-3.5 shrink-0"
                        />
                        <p>
                            <strong className="font-semibold">
                                放弃办理结果待确认。
                            </strong>
                            点击核对提交结果，将沿用原提交凭证重试本次操作。
                        </p>
                    </div>
                    <Button
                        id={`${id}-unknown-refresh`}
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={pending || refreshing}
                        onClick={() => void refreshTask()}
                    >
                        {refreshing ? "正在刷新…" : "刷新最新数据"}
                    </Button>
                    <Button
                        id={`${id}-check-result`}
                        type="button"
                        variant="default"
                        size="sm"
                        disabled={pending}
                        onClick={() => {
                            const command =
                                ledger.peek<AbandonPayload>(ABANDON_COMMAND)
                            if (command)
                                void submitAbandon(command).catch(
                                    () => undefined,
                                )
                        }}
                    >
                        {pending ? "正在核对结果…" : "核对提交结果"}
                    </Button>
                </div>
            ) : (
                <div className="flex flex-wrap items-center justify-end gap-2 empty:hidden">
                    {!canAbandon && issue?.kind === "failed" && (
                        <p role="alert" className="text-xs text-destructive">
                            {issue.message}
                        </p>
                    )}
                    {canProcess && supportsInlineEdit && href && (
                        <Button
                            id={`${id}-edit`}
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={pending}
                            onClick={openInWorkspaceTab}
                        >
                            打开原单页面
                            <ArrowUpRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </Button>
                    )}
                    {canAbandon && (
                        <Button
                            id={`${id}-abandon`}
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={pending}
                            onClick={() => setAbandonOpen(true)}
                        >
                            放弃本次办理
                        </Button>
                    )}
                    {canProcess && supportsInlineEdit ? (
                        <Button
                            id={`${id}-inline-edit`}
                            type="button"
                            variant="default"
                            size="sm"
                            disabled={pending}
                            onClick={() => setInlineEditOpen(true)}
                        >
                            <PencilLineIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            就地修改并重提
                        </Button>
                    ) : canProcess && href ? (
                        <Button
                            id={`${id}-edit`}
                            type="button"
                            variant="default"
                            size="sm"
                            disabled={pending}
                            onClick={openInWorkspaceTab}
                        >
                            修改或重新提交
                            <ArrowUpRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </Button>
                    ) : null}
                </div>
            )}

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
                    if (!pending && !unknown) setAbandonOpen(open)
                }}
                title="放弃本次办理"
                actionLabel="放弃本次办理"
                confirmLabel="确认放弃"
                actionVariant="destructive"
                layout="compact"
                fromStatus={{ label: "驳回待处理", tone: "warning" }}
                toStatus={{ label: "已关闭", tone: "neutral" }}
                description="放弃后关闭此待办。原单据仍为已驳回，内容和审批记录保留，后续可从单据详情再次办理。"
                pending={pending}
                confirmDisabled={!reasonValid || versionConflict || !canAbandon}
                onConfirm={() => form.handleSubmit()}
                onConfirmError={() => undefined}
                formContent={
                    <div className="space-y-4">
                        <form.Field name="reason">
                            {(field) => {
                                const overLimit =
                                    Array.from(field.state.value.trim())
                                        .length >
                                    WORK_ITEM_ABANDON_REASON_MAX_CHARS
                                const fieldError =
                                    field.state.meta.isTouched || overLimit
                                        ? toFieldErrors(
                                              field.state.meta.errors,
                                          ).find((entry) => entry?.message)
                                              ?.message
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
                                        onBlur={() => {
                                            field.handleChange(
                                                field.state.value.trim(),
                                            )
                                            field.handleBlur()
                                        }}
                                        disabled={
                                            pending || unknown || !canAbandon
                                        }
                                        error={fieldError}
                                        onCtrlEnterSubmit={
                                            versionConflict || unknown
                                                ? undefined
                                                : () => {
                                                      if (
                                                          reasonValid &&
                                                          canAbandon &&
                                                          !pending
                                                      ) {
                                                          void form
                                                              .handleSubmit()
                                                              .catch(
                                                                  () =>
                                                                      undefined,
                                                              )
                                                      }
                                                  }
                                        }
                                    />
                                )
                            }}
                        </form.Field>
                        {versionConflict && (
                            <div className="flex flex-wrap items-center gap-2 rounded-r-md border-l-2 border-l-warning bg-warning-soft/50 px-3 py-2 text-xs text-warning-soft-foreground">
                                <p role="alert">
                                    任务已更新，请刷新最新数据后核对。
                                </p>
                                <Button
                                    id={`${id}-refresh-task`}
                                    type="button"
                                    variant="outline"
                                    size="xs"
                                    disabled={refreshing}
                                    onClick={() => void refreshTask()}
                                >
                                    {refreshing ? "正在刷新…" : "刷新最新数据"}
                                </Button>
                            </div>
                        )}
                        {!canAbandon && (
                            <p
                                role="alert"
                                className="text-xs text-destructive"
                            >
                                此任务当前无法放弃办理，请返回核对任务状态。
                            </p>
                        )}
                        {issue?.kind === "failed" && canAbandon && (
                            <p
                                role="alert"
                                className="text-body-compact text-destructive"
                            >
                                {issue.message}
                            </p>
                        )}
                    </div>
                }
            />
        </>
    )
}
