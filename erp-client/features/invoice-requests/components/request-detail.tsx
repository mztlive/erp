"use client"
import Link from "next/link"
import { useState, useRef } from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { MoneyValue } from "@/components/business"
import { ApprovalReadonly } from "@/features/approval-workflow/components/approval-readonly"
import { mapDocumentApprovalViewDto } from "@/features/approval-workflow/types"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { getErrorMessage } from "@/lib/api/errors"
import { subtractFixed } from "@/lib/fixed-decimal"
import {
    useInvoiceRequest,
    useInvoiceRequestCommands,
    useInvoiceRequestPermissions,
} from "../hooks/queries"
import { cancelRequest, requestStatusLabels, type InvoiceRequest } from "../api"

/** 单据详情呈现批准与执行的独立进度；原申请人可撤回或编辑草稿。 */
export function InvoiceRequestDetail({
    id,
    onBack,
    onEdit,
}: {
    id: string
    onBack: () => void
    onEdit: (request: InvoiceRequest) => void
}) {
    const query = useInvoiceRequest(id)
    const permissions = useInvoiceRequestPermissions()
    const [cancelling, setCancelling] = useState(false)
    const request = query.data
    if (query.isPending) return <p>正在读取开票申请…</p>
    if (!request)
        return (
            <div role="alert">
                {getErrorMessage(query.error, "申请读取失败")}
                <Button
                    id="invoice-request-detail-retry"
                    onClick={() => void query.refetch()}
                >
                    重试
                </Button>
            </div>
        )
    if (cancelling)
        return (
            <CancelRequestForm
                request={request}
                onDone={() => {
                    setCancelling(false)
                    void query.refetch()
                }}
                onBack={() => setCancelling(false)}
            />
        )
    return (
        <section className="space-y-5" aria-label="开票申请详情">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                    <h2 className="text-lg font-semibold">
                        {request.request_no}
                    </h2>
                    <p className="text-sm text-muted-foreground">
                        {request.sales_order_no} · {request.data.invoice_title}{" "}
                        · {requestStatusLabels[request.status]}
                    </p>
                </div>
                <Button
                    id="invoice-request-detail-back"
                    variant="outline"
                    onClick={onBack}
                >
                    返回申请列表
                </Button>
            </div>
            <div className="grid gap-4 rounded-lg bg-muted p-4 sm:grid-cols-3">
                {[
                    ["申请金额", request.data.amount],
                    ["已开票", request.invoiced_amount],
                    [
                        "本次尚未开票",
                        subtractFixed(
                            request.data.amount,
                            request.invoiced_amount,
                            { maxScale: 2, outputScale: 2 },
                        ),
                    ],
                ].map(([label, amount]) => (
                    <div key={label}>
                        <p className="text-sm text-muted-foreground">{label}</p>
                        <div className="mt-1 text-xl font-semibold">
                            <MoneyValue value={amount!} />
                        </div>
                    </div>
                ))}
            </div>
            <dl className="grid gap-4 text-sm sm:grid-cols-2">
                {[
                    ["开票抬头", request.data.invoice_title],
                    ["税号", request.data.tax_number],
                    ["开票内容", request.data.invoice_content],
                    ["申请事由", request.data.reason],
                ].map(([label, value]) => (
                    <div key={label}>
                        <dt className="text-muted-foreground">{label}</dt>
                        <dd className="mt-1 break-words">{value}</dd>
                    </div>
                ))}
            </dl>
            {request.approval ? (
                <ApprovalReadonly
                    id={`invoice-request-${id}-approval`}
                    approval={mapDocumentApprovalViewDto(request.approval)}
                />
            ) : null}
            <div className="flex flex-wrap gap-2">
                <Button
                    id="invoice-request-source"
                    variant="outline"
                    render={
                        <Link
                            href={`/sales/orders/${encodeURIComponent(request.sales_order_id)}?section=receivable`}
                        />
                    }
                >
                    查看销售单
                </Button>
                {request.work_item_id && request.status === "approved" ? (
                    <Button
                        id="invoice-request-open-task"
                        variant="outline"
                        render={
                            <Link
                                href={`/workspace?currentWorkItemId=${encodeURIComponent(request.work_item_id)}`}
                            />
                        }
                    >
                        查看开票任务
                    </Button>
                ) : null}
                {request.created_by === permissions.userId &&
                request.status === "in_approval" &&
                permissions.canCancel ? (
                    <Button
                        id="invoice-request-cancel"
                        variant="outline"
                        onClick={() => setCancelling(true)}
                    >
                        撤回申请
                    </Button>
                ) : null}
                {request.created_by === permissions.userId &&
                request.status === "draft" &&
                permissions.canSubmit ? (
                    <Button
                        id="invoice-request-edit"
                        onClick={() => onEdit(request)}
                    >
                        修改并重新提交
                    </Button>
                ) : null}
            </div>
        </section>
    )
}
/** 撤回必须保留原因；未知结果期间仅允许同载荷重试。 */
function CancelRequestForm({
    request,
    onDone,
    onBack,
}: {
    request: InvoiceRequest
    onDone: () => void
    onBack: () => void
}) {
    const { cancel } = useInvoiceRequestCommands()
    const pending = useRef<Parameters<typeof cancelRequest>[0] | null>(null)
    const [uncertain, setUncertain] = useState(false)
    const form = useAppForm({
        defaultValues: { reason: "" },
        validators: {
            onSubmit: z.object({
                reason: z.string().trim().min(1, "请输入撤回原因").max(1000),
            }),
        },
        onSubmit: async ({ value }) => {
            const input = pending.current ?? {
                id: request.id,
                expected_version: request.version,
                reason: value.reason,
                idempotency_key: crypto.randomUUID(),
            }
            pending.current = input
            try {
                await cancel.mutateAsync(input)
                pending.current = null
                setUncertain(false)
                onDone()
            } catch (error) {
                const unknown = classifyFormalCommandError(error) === "unknown"
                setUncertain(unknown)
                if (!unknown) pending.current = null
            }
        },
    })
    return (
        <form
            id="invoice-request-cancel-form"
            className="space-y-4"
            onSubmit={(e) => {
                e.preventDefault()
                void form.handleSubmit()
            }}
        >
            <h3 className="font-semibold">撤回 {request.request_no}</h3>
            <p className="text-sm text-muted-foreground">
                撤回后释放本次申请额度，申请保留为草稿。
            </p>
            <form.AppField name="reason">
                {(field) => (
                    <field.TextareaField
                        id="invoice-request-cancel-reason"
                        label="撤回原因"
                        disabled={cancel.isPending || uncertain}
                    />
                )}
            </form.AppField>
            {cancel.error ? (
                <p role="alert" className="text-sm text-destructive">
                    {uncertain
                        ? "结果尚未确认，请核对撤回结果。"
                        : getErrorMessage(cancel.error, "撤回失败")}
                </p>
            ) : null}
            <div className="flex justify-end gap-2">
                <Button
                    id="invoice-request-cancel-back"
                    type="button"
                    variant="outline"
                    disabled={cancel.isPending || uncertain}
                    onClick={onBack}
                >
                    返回
                </Button>
                <Button
                    id="invoice-request-cancel-submit"
                    type="submit"
                    disabled={cancel.isPending}
                >
                    {uncertain ? "核对撤回结果" : "撤回申请"}
                </Button>
            </div>
        </form>
    )
}
