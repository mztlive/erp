"use client"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { WORKSPACE_QUEUE_ROUTES } from "@/lib/workspace-queue-routes"
import Link from "next/link"
import { useEffect, useState, useRef } from "react"
import { ArrowLeftIcon, ExternalLinkIcon } from "lucide-react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    BusinessFailureState,
    BusinessStatusBadge,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    MoneyValue,
} from "@/components/business"
import type { StatusTone } from "@/components/ui/status-badge"
import { ApprovalReadonly } from "@/features/approval-workflow/components/approval-readonly"
import {
    mapDocumentApprovalViewDto,
    type DocumentApprovalView,
} from "@/features/approval-workflow/types"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { getErrorMessage } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { subtractFixed } from "@/lib/fixed-decimal"
import {
    useInvoiceRequest,
    useInvoiceRequestCommands,
    useInvoiceRequestPermissions,
} from "../hooks/queries"
import { cancelRequest, requestStatusLabels, type InvoiceRequest } from "../api"
import { InvoiceRequestDetails } from "./request-details"

const requestStatusTones: Record<InvoiceRequest["status"], StatusTone> = {
    draft: "neutral",
    rejected: "destructive",
    in_approval: "warning",
    approved: "info",
    completed: "success",
}

function textOrDash(value?: string | null): string {
    const text = value?.trim()
    return text ? text : "—"
}

function safeRemaining(amount: string, invoiced: string): string {
    if (
        !/^-?\d+(?:\.\d{1,2})?$/.test(amount) ||
        !/^-?\d+(?:\.\d{1,2})?$/.test(invoiced)
    ) {
        return amount
    }
    return subtractFixed(amount, invoiced, { maxScale: 2, outputScale: 2 })
}

/** 范围详情没有嵌套 data；缺字段时展示占位，禁止对 undefined 取属性。 */
function requestFacts(
    request: InvoiceRequest & {
        amount?: string
        applicant_user_id?: string
    },
) {
    const data = request.data
    const amount = data?.amount || request.amount || "0.00"
    const invoiced = request.invoiced_amount || "0.00"
    const approvalSource = request.approval
    let approval: DocumentApprovalView | undefined
    if (
        approvalSource &&
        (!approvalSource.definition ||
            Array.isArray(approvalSource.definition.nodes)) &&
        (approvalSource.recent_history == null ||
            Array.isArray(approvalSource.recent_history))
    ) {
        approval = mapDocumentApprovalViewDto({
            ...approvalSource,
            recent_history: approvalSource.recent_history ?? [],
            allowed_actions: approvalSource.allowed_actions ?? [],
        })
    }
    return {
        amount,
        invoiced,
        remaining: safeRemaining(amount, invoiced),
        title: textOrDash(data?.invoice_title),
        applicantId: request.created_by || request.applicant_user_id || "",
        approval,
    }
}

/** 单据详情呈现批准与执行的独立进度；inline 态使用完整 DetailWorkspacePage，dialog 态保持紧凑弹窗。 */
export function InvoiceRequestDetail({
    id,
    onBack,
    onEdit,
    presentation = "inline",
    currentSalesOrderId,
}: {
    id: string
    onBack: () => void
    onEdit: (request: InvoiceRequest) => void
    presentation?: "inline" | "dialog"
    currentSalesOrderId?: string
}) {
    const [closeBlocked, setCloseBlocked] = useState(false)
    const content = (
        <InvoiceRequestDetailBody
            id={id}
            onBack={onBack}
            onEdit={onEdit}
            presentation={presentation}
            currentSalesOrderId={currentSalesOrderId}
            onCloseBlockedChange={setCloseBlocked}
        />
    )
    if (presentation === "inline") return content
    return (
        <Dialog
            open
            onOpenChange={(open) => !open && !closeBlocked && onBack()}
        >
            <DialogContent
                id="invoice-request-detail-dialog"
                closeButtonId="invoice-request-detail-dialog-close"
                showCloseButton={!closeBlocked}
                className="flex max-h-[calc(100dvh-2rem)] flex-col sm:max-w-4xl"
            >
                <DialogHeader>
                    <DialogTitle>开票申请详情</DialogTitle>
                    <DialogDescription>
                        查看申请金额、开票资料与审批进度。
                    </DialogDescription>
                </DialogHeader>
                <div className="min-h-0 overflow-y-auto">{content}</div>
            </DialogContent>
        </Dialog>
    )
}

function InvoiceRequestDetailBody({
    id,
    onBack,
    onEdit,
    presentation,
    currentSalesOrderId,
    onCloseBlockedChange,
}: {
    id: string
    onBack: () => void
    onEdit: (request: InvoiceRequest) => void
    presentation: "inline" | "dialog"
    currentSalesOrderId?: string
    onCloseBlockedChange: (blocked: boolean) => void
}) {
    const query = useInvoiceRequest(id)
    const permissions = useInvoiceRequestPermissions()
    const profile = useAccountProfileQuery().data
    const [cancelling, setCancelling] = useState(false)
    const [reviseRejected, setReviseRejected] = useState(false)
    const request = query.data
    const isInline = presentation === "inline"

    if (query.isPending) {
        if (!isInline) {
            return (
                <div className="space-y-4 py-4">
                    <div className="h-16 animate-pulse rounded-lg bg-muted" />
                    <div className="h-44 animate-pulse rounded-lg bg-muted" />
                </div>
            )
        }
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="开票申请详情"
                    secondaryActions={
                        <Button
                            id="invoice-request-detail-back"
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={onBack}
                        >
                            <ArrowLeftIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            返回申请列表
                        </Button>
                    }
                />
                <div className="space-y-4 px-(--scaffold-px) py-6">
                    <div className="h-16 animate-pulse rounded-lg bg-muted" />
                    <div className="h-64 animate-pulse rounded-lg bg-muted" />
                </div>
            </DetailWorkspacePage>
        )
    }

    if (!request) {
        const failure = (
            <BusinessFailureState
                title="申请读取失败"
                description={getErrorMessage(query.error, "请重试加载")}
                action={
                    <Button
                        id="invoice-request-detail-retry"
                        variant="outline"
                        onClick={() => void query.refetch()}
                    >
                        重试
                    </Button>
                }
            />
        )
        if (!isInline) return <div role="alert">{failure}</div>
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="开票申请详情"
                    secondaryActions={
                        <Button
                            id="invoice-request-detail-back"
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={onBack}
                        >
                            <ArrowLeftIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            返回申请列表
                        </Button>
                    }
                />
                <div
                    role="alert"
                    className="mx-auto w-full max-w-3xl px-(--scaffold-px) py-8"
                >
                    {failure}
                </div>
            </DetailWorkspacePage>
        )
    }

    if (cancelling) {
        const cancelForm = (
            <CancelRequestForm
                request={request}
                onCloseBlockedChange={onCloseBlockedChange}
                reviseRejected={reviseRejected}
                onDone={async () => {
                    setCancelling(false)
                    const refreshed = await query.refetch()
                    if (reviseRejected && refreshed.data?.status === "draft")
                        onEdit(refreshed.data)
                }}
                onBack={() => setCancelling(false)}
            />
        )
        if (!isInline) return cancelForm
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title={reviseRejected ? "修改原单" : "撤回开票申请"}
                    meta={request.request_no}
                    secondaryActions={
                        <Button
                            id="invoice-request-detail-back"
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => setCancelling(false)}
                        >
                            <ArrowLeftIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            返回详情
                        </Button>
                    }
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        {cancelForm}
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const facts = requestFacts(request)
    const status = {
        label: requestStatusLabels[request.status] ?? "审批中",
        tone: requestStatusTones[request.status] ?? ("warning" as const),
    }

    const actionButtons = (
        <>
            {request.sales_order_id &&
            request.sales_order_id !== currentSalesOrderId ? (
                <Button
                    id="invoice-request-source"
                    variant="outline"
                    size={isInline ? "sm" : "default"}
                    render={
                        <Link
                            href={`/sales/orders/${encodeURIComponent(request.sales_order_id)}?section=invoice`}
                        />
                    }
                >
                    查看销售单
                    <ExternalLinkIcon
                        data-icon="inline-end"
                        aria-hidden="true"
                    />
                </Button>
            ) : null}
            {request.work_item_id &&
            (request.status === "approved" || request.status === "completed") &&
            hasPermission(profile?.permissions, "work_item:detail") ? (
                <Button
                    id="invoice-request-open-task"
                    variant="outline"
                    size={isInline ? "sm" : "default"}
                    render={
                        <Link
                            href={buildWorkspaceDetailHref(
                                "tasks",
                                request.work_item_id,
                                request.status === "completed"
                                    ? {
                                          returnTo:
                                              WORKSPACE_QUEUE_ROUTES.history
                                                  .href,
                                      }
                                    : undefined,
                            )}
                        />
                    }
                >
                    查看开票任务
                </Button>
            ) : null}
            {facts.applicantId === permissions.userId &&
            request.version > 0 &&
            request.status === "in_approval" &&
            permissions.canCancel ? (
                <Button
                    id="invoice-request-cancel"
                    variant="outline"
                    size={isInline ? "sm" : "default"}
                    onClick={() => {
                        setReviseRejected(
                            Boolean(facts.approval?.instance?.latestRejection),
                        )
                        setCancelling(true)
                    }}
                >
                    {facts.approval?.instance?.latestRejection
                        ? "修改原单"
                        : "撤回申请"}
                </Button>
            ) : null}
            {request.created_by === permissions.userId &&
            (request.status === "draft" || request.status === "rejected") &&
            permissions.canSubmit ? (
                <Button
                    id="invoice-request-edit"
                    size={isInline ? "sm" : "default"}
                    onClick={() => onEdit(request)}
                >
                    修改并重新提交
                </Button>
            ) : null}
        </>
    )

    const metrics = (
        <DetailWorkspaceMetrics
            columns={3}
            variant="bar"
            ariaLabel="开票申请详情金额摘要"
        >
            <DetailWorkspaceMetricItem
                emphasis="primary"
                label="申请金额"
                value={<MoneyValue value={facts.amount} display="exact" />}
            />
            <DetailWorkspaceMetricItem
                label="已开票"
                value={<MoneyValue value={facts.invoiced} display="exact" />}
            />
            <DetailWorkspaceMetricItem
                label="本次尚未开票"
                value={<MoneyValue value={facts.remaining} display="exact" />}
            />
        </DetailWorkspaceMetrics>
    )

    if (!isInline) {
        return (
            <section className="space-y-5" aria-label="开票申请详情">
                <div className="flex flex-wrap items-center justify-between gap-3">
                    <div>
                        <h2 className="num break-all text-section-title font-semibold tracking-tight">
                            {request.request_no || "开票申请"}
                        </h2>
                        <div className="mt-1 flex flex-wrap items-center gap-2 text-body-compact text-muted-foreground">
                            <span>
                                {request.sales_order_no || "—"} · {facts.title}
                            </span>
                            <BusinessStatusBadge
                                context="detail"
                                label={status.label}
                                tone={status.tone}
                            />
                        </div>
                    </div>
                </div>
                <div className="border-y border-border/60">{metrics}</div>
                <InvoiceRequestDetails data={request.data} />
                {facts.approval ? (
                    <ApprovalReadonly
                        id={`invoice-request-${id}-approval`}
                        approval={facts.approval}
                    />
                ) : null}
                <div className="flex flex-wrap justify-end gap-2 border-t border-border/60 pt-4">
                    {actionButtons}
                </div>
            </section>
        )
    }

    return (
        <DetailWorkspacePage aria-label="开票申请详情">
            <DetailWorkspaceChrome
                title={request.request_no || "开票申请"}
                primaryStatus={status}
                meta={
                    <span className="inline-flex flex-wrap items-center gap-x-2">
                        <span className="num">
                            销售单 {request.sales_order_no || "—"}
                        </span>
                        <span>·</span>
                        <span>{facts.title}</span>
                        {request.created_by_name ? (
                            <>
                                <span>·</span>
                                <span>申请人 {request.created_by_name}</span>
                            </>
                        ) : null}
                    </span>
                }
                secondaryActions={
                    <div className="flex flex-wrap items-center gap-2">
                        <Button
                            id="invoice-request-detail-back"
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={onBack}
                        >
                            <ArrowLeftIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            返回申请列表
                        </Button>
                        {actionButtons}
                    </div>
                }
            />
            <div className="border-b border-border/60 bg-card px-(--scaffold-px)">
                {metrics}
            </div>
            <DetailWorkspaceBody
                sidebar={
                    facts.approval ? (
                        <ApprovalReadonly
                            id={`invoice-request-${id}-approval`}
                            approval={facts.approval}
                        />
                    ) : undefined
                }
            >
                <DetailWorkspaceSection title="开票资料与明细">
                    <InvoiceRequestDetails data={request.data} />
                </DetailWorkspaceSection>
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}

/** 撤回必须保留原因；未知结果期间仅允许同载荷重试。 */
function CancelRequestForm({
    request,
    onDone,
    onBack,
    reviseRejected = false,
    onCloseBlockedChange,
}: {
    request: InvoiceRequest
    onDone: () => void | Promise<void>
    onBack: () => void
    reviseRejected?: boolean
    onCloseBlockedChange: (blocked: boolean) => void
}) {
    const { cancel } = useInvoiceRequestCommands()
    const pending = useRef<Parameters<typeof cancelRequest>[0] | null>(null)
    const [uncertain, setUncertain] = useState(false)
    useEffect(() => {
        onCloseBlockedChange(cancel.isPending || uncertain)
        return () => onCloseBlockedChange(false)
    }, [cancel.isPending, uncertain, onCloseBlockedChange])
    const form = useAppForm({
        defaultValues: {
            reason: reviseRejected ? "按驳回意见修改原单后重新提交" : "",
        },
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
                await onDone()
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
            <h3 className="font-semibold">
                {reviseRejected ? "修改原单" : "撤回"} {request.request_no}
            </h3>
            <p className="text-xs text-muted-foreground">
                撤回后释放本次申请额度，申请保留为草稿。
                {reviseRejected
                    ? "原申请编号和审批记录保留，修改后重新提交审批。"
                    : null}
            </p>
            <form.AppField name="reason">
                {(field) => (
                    <field.TextareaField
                        id="invoice-request-cancel-reason"
                        label="撤回原因"
                        required
                        disabled={cancel.isPending || uncertain}
                    />
                )}
            </form.AppField>
            {cancel.error ? (
                <p role="alert" className="text-body-compact text-destructive">
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
                <LoadingButton
                    id="invoice-request-cancel-submit"
                    type="submit"
                    loading={cancel.isPending}
                    disabled={cancel.isPending}
                >
                    {uncertain
                        ? "核对撤回结果"
                        : reviseRejected
                          ? "撤回并修改原单"
                          : "撤回申请"}
                </LoadingButton>
            </div>
        </form>
    )
}
