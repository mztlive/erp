"use client"

import Link from "next/link"
import { useEffect, useRef, useState } from "react"
import { format } from "date-fns"
import { ArrowLeftIcon, ExternalLinkIcon } from "lucide-react"
import { z } from "zod"

import { useAppForm } from "@/components/form"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { toast } from "@/components/ui/toast"
import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    FormalActionConfirmDialog,
    MoneyValue,
    detailWorkspaceStyles,
    type DetailWorkspaceBackLink,
} from "@/components/business"
import type { StatusTone } from "@/components/ui/status-badge"
import { ApprovalReadonly } from "@/features/approval-workflow/components/approval-readonly"
import {
    mapDocumentApprovalViewDto,
    type DocumentApprovalView,
} from "@/features/approval-workflow/types"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { getErrorMessage, isNotFoundError } from "@/lib/api/errors"
import { subtractFixed } from "@/lib/fixed-decimal"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { hasPermission } from "@/lib/permissions"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { WORKSPACE_QUEUE_ROUTES } from "@/lib/workspace-queue-routes"

import { cancelRequest, requestStatusLabels, type InvoiceRequest } from "../api"
import {
    useInvoiceRequest,
    useInvoiceRequestCommands,
    useInvoiceRequestPermissions,
} from "../hooks/queries"
import {
    InvoiceRequestInfoFacts,
    InvoiceRequestLinesTable,
    resolveInvoiceRequestLines,
} from "./request-details"

export type InvoiceRequestDetailPresentation =
    | "workspace"
    | "embedded"
    | "dialog"
    | "inline"

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

function formatCreatedAt(seconds: number): string | null {
    if (!Number.isFinite(seconds) || seconds <= 0) return null
    return format(new Date(seconds * 1000), "yyyy-MM-dd HH:mm")
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

/**
 * 单据详情呈现批准与执行的独立进度：
 * - `workspace`：独立详情页壳（`DetailWorkspacePage` + 吸顶 `DetailWorkspaceChrome`）；
 * - `embedded`（含兼容值 `inline`）：内嵌于客户往来等父页子面板，不重复渲染页壳与吸顶头部；
 * - `dialog`：销售单内紧凑弹窗。
 */
export function InvoiceRequestDetail({
    id,
    onBack,
    onEdit,
    presentation = "workspace",
    currentSalesOrderId,
    backLabel = "返回申请列表",
    backVisibility = "standalone-only",
}: {
    id: string
    onBack: () => void
    onEdit: (request: InvoiceRequest) => void
    presentation?: InvoiceRequestDetailPresentation
    currentSalesOrderId?: string
    backLabel?: string
    backVisibility?: DetailWorkspaceBackLink["visibility"]
}) {
    const normalizedMode: "workspace" | "embedded" | "dialog" =
        presentation === "inline" ? "embedded" : presentation
    const [closeBlocked, setCloseBlocked] = useState(false)
    const content = (
        <InvoiceRequestDetailBody
            id={id}
            onBack={onBack}
            onEdit={onEdit}
            mode={normalizedMode}
            currentSalesOrderId={currentSalesOrderId}
            backLabel={backLabel}
            backVisibility={backVisibility}
            onCloseBlockedChange={setCloseBlocked}
        />
    )
    if (normalizedMode !== "dialog") return content
    return (
        <Dialog
            open
            onOpenChange={(open) => !open && !closeBlocked && onBack()}
        >
            <DialogContent
                id="invoice-request-detail-dialog"
                closeButtonId="invoice-request-detail-dialog-close"
                showCloseButton={!closeBlocked}
                className="flex max-h-[calc(100dvh-2rem)] flex-col overflow-hidden sm:max-w-4xl"
            >
                <DialogHeader className="shrink-0">
                    <DialogTitle>开票申请详情</DialogTitle>
                    <DialogDescription>
                        查看申请金额、开票资料与审批进度。
                    </DialogDescription>
                </DialogHeader>
                <div className="min-h-0 flex-1 overflow-y-auto">{content}</div>
            </DialogContent>
        </Dialog>
    )
}

function InvoiceRequestDetailBody({
    id,
    onBack,
    onEdit,
    mode,
    currentSalesOrderId,
    backLabel,
    backVisibility,
    onCloseBlockedChange,
}: {
    id: string
    onBack: () => void
    onEdit: (request: InvoiceRequest) => void
    mode: "workspace" | "embedded" | "dialog"
    currentSalesOrderId?: string
    backLabel: string
    backVisibility: DetailWorkspaceBackLink["visibility"]
    onCloseBlockedChange: (blocked: boolean) => void
}) {
    const query = useInvoiceRequest(id)
    const permissions = useInvoiceRequestPermissions()
    const profile = useAccountProfileQuery().data
    const [cancelOpen, setCancelOpen] = useState(false)
    const [reviseRejected, setReviseRejected] = useState(false)
    const request = query.data
    const isWorkspace = mode === "workspace"
    const isEmbedded = mode === "embedded"

    if (query.isPending) {
        if (isWorkspace) {
            return (
                <DetailWorkspaceSkeleton
                    title="开票申请详情"
                    description="正在读取申请金额、开票资料与审批进度…"
                    summary
                    sidebar
                />
            )
        }
        return (
            <div className="space-y-4 py-4" aria-busy="true">
                {isEmbedded ? (
                    <Button
                        id="invoice-request-detail-back"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.quietButton}
                        onClick={onBack}
                    >
                        <ArrowLeftIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        {backLabel}
                    </Button>
                ) : null}
                <div className="h-16 animate-pulse rounded-lg bg-muted" />
                <div className="h-44 animate-pulse rounded-lg bg-muted" />
            </div>
        )
    }

    if (query.isError || !request) {
        const notFound = isNotFoundError(query.error)
        const failure = notFound ? (
            <BusinessEmptyState
                kind="no-data"
                title="未找到该开票申请"
                description="申请单可能已删除或编号不存在，请返回申请列表核对。"
                action={
                    <Button
                        id="invoice-request-detail-not-found-back"
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={onBack}
                    >
                        {backLabel}
                    </Button>
                }
            />
        ) : (
            <BusinessFailureState
                idPrefix="invoice-request-detail"
                error={query.error}
                title="申请读取失败"
                action={
                    <Button
                        id="invoice-request-detail-retry"
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => void query.refetch()}
                    >
                        重试
                    </Button>
                }
            />
        )

        if (isWorkspace) {
            return (
                <DetailWorkspacePage>
                    <DetailWorkspaceChrome
                        title="开票申请详情"
                        back={{
                            id: "invoice-request-detail-back",
                            label: backLabel,
                            visibility: backVisibility,
                            onClick: onBack,
                        }}
                    />
                    <DetailWorkspaceBody>
                        <div
                            role={notFound ? undefined : "alert"}
                            className="mx-auto w-full max-w-3xl py-8"
                        >
                            {failure}
                        </div>
                    </DetailWorkspaceBody>
                </DetailWorkspacePage>
            )
        }

        return (
            <div className="space-y-4 py-2">
                {isEmbedded ? (
                    <Button
                        id="invoice-request-detail-back"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.quietButton}
                        onClick={onBack}
                    >
                        <ArrowLeftIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        {backLabel}
                    </Button>
                ) : null}
                <div role={notFound ? undefined : "alert"}>{failure}</div>
            </div>
        )
    }

    const facts = requestFacts(request)
    const status = {
        label: requestStatusLabels[request.status] ?? "审批中",
        tone: requestStatusTones[request.status] ?? ("warning" as const),
    }
    const createdAtLabel = formatCreatedAt(request.created_at)
    const lines = resolveInvoiceRequestLines(request.data)

    const canViewSalesOrder = Boolean(
        permissions.canReadSalesOrder &&
        request.sales_order_id &&
        request.sales_order_id !== currentSalesOrderId,
    )
    const canOpenTask = Boolean(
        request.work_item_id &&
        (request.status === "approved" || request.status === "completed") &&
        hasPermission(profile?.permissions, "work_item:detail"),
    )
    const canCancelRequest = Boolean(
        facts.applicantId === permissions.userId &&
        request.version > 0 &&
        request.status === "in_approval" &&
        permissions.canCancel,
    )
    const canEditRequest = Boolean(
        request.created_by === permissions.userId &&
        (request.status === "draft" || request.status === "rejected") &&
        permissions.canSubmit,
    )
    const isRejectedInApproval = Boolean(
        facts.approval?.instance?.latestRejection,
    )

    const secondaryActionButtons = (
        <>
            {canViewSalesOrder ? (
                <Button
                    id="invoice-request-source"
                    variant={isWorkspace || isEmbedded ? "ghost" : "outline"}
                    size="sm"
                    className={
                        isWorkspace || isEmbedded
                            ? detailWorkspaceStyles.quietButton
                            : undefined
                    }
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
            {canOpenTask && request.work_item_id ? (
                <Button
                    id="invoice-request-open-task"
                    variant={isWorkspace || isEmbedded ? "ghost" : "outline"}
                    size="sm"
                    className={
                        isWorkspace || isEmbedded
                            ? detailWorkspaceStyles.quietButton
                            : undefined
                    }
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
            {canCancelRequest ? (
                <Button
                    id="invoice-request-cancel"
                    type="button"
                    variant={isWorkspace || isEmbedded ? "ghost" : "outline"}
                    size="sm"
                    className={
                        isWorkspace || isEmbedded
                            ? detailWorkspaceStyles.quietDangerButton
                            : undefined
                    }
                    onClick={() => {
                        setReviseRejected(isRejectedInApproval)
                        setCancelOpen(true)
                    }}
                >
                    {isRejectedInApproval ? "修改原单" : "撤回申请"}
                </Button>
            ) : null}
        </>
    )

    const primaryActionButton = canEditRequest ? (
        <Button
            id="invoice-request-edit"
            type="button"
            size="sm"
            onClick={() => onEdit(request)}
        >
            修改并重新提交
        </Button>
    ) : undefined

    const metaItems = [
        <span key="order" className="num">
            销售单 {request.sales_order_no || "—"}
        </span>,
        facts.title !== "—" ? facts.title : null,
        request.created_by_name ? `申请人 ${request.created_by_name}` : null,
        createdAtLabel ? (
            <span key="created-at" className="num">
                申请时间 {createdAtLabel}
            </span>
        ) : null,
    ]

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

    const cancelDialog = canCancelRequest ? (
        <CancelRequestDialog
            open={cancelOpen}
            onOpenChange={setCancelOpen}
            request={request}
            status={status}
            reviseRejected={reviseRejected}
            onCloseBlockedChange={onCloseBlockedChange}
            onDone={async () => {
                toast.add({
                    type: "success",
                    title: reviseRejected
                        ? "已撤回申请，请修改后重新提交"
                        : "开票申请已撤回",
                })
                const refreshed = await query.refetch()
                if (reviseRejected && refreshed.data?.status === "draft") {
                    onEdit(refreshed.data)
                }
            }}
        />
    ) : null

    if (mode === "dialog") {
        return (
            <>
                <section className="space-y-5" aria-label="开票申请详情">
                    <div className="space-y-1">
                        <div className="flex flex-wrap items-center gap-2">
                            <h2 className="num break-all text-section-title font-semibold tracking-tight">
                                {request.request_no || "开票申请"}
                            </h2>
                            <BusinessStatusBadge
                                context="detail"
                                label={status.label}
                                tone={status.tone}
                            />
                        </div>
                        <DetailWorkspaceMeta
                            items={[
                                request.version > 0 ? (
                                    <span key="version" className="num">
                                        版本 {request.version}
                                    </span>
                                ) : null,
                                ...metaItems,
                            ]}
                        />
                    </div>
                    <div className="border-y border-border/60">{metrics}</div>
                    <div className="space-y-5">
                        <InvoiceRequestInfoFacts data={request.data} />
                        <InvoiceRequestLinesTable
                            data={request.data}
                            showHeader
                        />
                    </div>
                    {facts.approval ? (
                        <ApprovalReadonly
                            id={`invoice-request-${id}-approval`}
                            approval={facts.approval}
                        />
                    ) : null}
                    {canViewSalesOrder ||
                    canOpenTask ||
                    canCancelRequest ||
                    canEditRequest ? (
                        <div className="flex flex-wrap justify-end gap-2 border-t border-border/60 pt-4">
                            {secondaryActionButtons}
                            {primaryActionButton}
                        </div>
                    ) : null}
                </section>
                {cancelDialog}
            </>
        )
    }

    if (isEmbedded) {
        return (
            <>
                <section className="space-y-5" aria-label="开票申请详情">
                    <div className="space-y-2 border-b border-border/60 pb-4">
                        <Button
                            id="invoice-request-detail-back"
                            type="button"
                            variant="ghost"
                            size="xs"
                            className="-ml-2 text-muted-foreground hover:text-foreground"
                            onClick={onBack}
                        >
                            <ArrowLeftIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {backLabel}
                        </Button>
                        <div className="flex flex-wrap items-start justify-between gap-3">
                            <div className="min-w-0 space-y-1">
                                <div className="flex flex-wrap items-center gap-2">
                                    <h2 className="num break-all text-section-title font-semibold tracking-tight text-foreground">
                                        {request.request_no || "开票申请"}
                                    </h2>
                                    <BusinessStatusBadge
                                        context="detail"
                                        label={status.label}
                                        tone={status.tone}
                                    />
                                </div>
                                <DetailWorkspaceMeta
                                    items={[
                                        request.version > 0 ? (
                                            <span key="version" className="num">
                                                版本 {request.version}
                                            </span>
                                        ) : null,
                                        ...metaItems,
                                    ]}
                                />
                            </div>
                            <div className="flex shrink-0 flex-wrap items-center gap-1.5">
                                {secondaryActionButtons}
                                {primaryActionButton}
                            </div>
                        </div>
                    </div>
                    <div className="border-b border-border/60">{metrics}</div>
                    <div className="grid items-start gap-8 lg:grid-cols-[minmax(0,1fr)_300px]">
                        <div className="min-w-0 space-y-6">
                            <div className="max-w-4xl space-y-3">
                                <h3 className="text-section-title font-semibold tracking-tight text-foreground">
                                    开票资料
                                </h3>
                                <InvoiceRequestInfoFacts data={request.data} />
                            </div>
                            <div className="space-y-3 border-t border-border/60 pt-5">
                                <div className="flex items-center gap-2">
                                    <h3 className="text-section-title font-semibold tracking-tight text-foreground">
                                        开票明细
                                    </h3>
                                    {lines.length ? (
                                        <span className="num rounded-md bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                                            共 {lines.length} 行
                                        </span>
                                    ) : null}
                                </div>
                                <InvoiceRequestLinesTable data={request.data} />
                            </div>
                        </div>
                        {facts.approval ? (
                            <aside
                                aria-label="审批进度"
                                className="min-w-0 border-t border-border/60 pt-5 lg:border-t-0 lg:border-l lg:pt-0 lg:pl-6"
                            >
                                <ApprovalReadonly
                                    id={`invoice-request-${id}-approval`}
                                    approval={facts.approval}
                                />
                            </aside>
                        ) : null}
                    </div>
                </section>
                {cancelDialog}
            </>
        )
    }

    return (
        <>
            <DetailWorkspacePage aria-label="开票申请详情">
                <DetailWorkspaceChrome
                    back={{
                        id: "invoice-request-detail-back",
                        label: backLabel,
                        visibility: backVisibility,
                        onClick: onBack,
                    }}
                    title={request.request_no || "开票申请"}
                    headingProps={{ className: "num" }}
                    version={request.version > 0 ? request.version : undefined}
                    primaryStatus={status}
                    meta={<DetailWorkspaceMeta items={metaItems} />}
                    secondaryActions={secondaryActionButtons}
                    primaryAction={primaryActionButton}
                />
                <DetailWorkspaceBody
                    summary={metrics}
                    sidebarLabel="审批进度"
                    sidebar={
                        facts.approval ? (
                            <ApprovalReadonly
                                id={`invoice-request-${id}-approval`}
                                approval={facts.approval}
                            />
                        ) : undefined
                    }
                >
                    <DetailWorkspaceSection title="开票资料" width="form">
                        <InvoiceRequestInfoFacts data={request.data} />
                    </DetailWorkspaceSection>
                    <DetailWorkspaceSection
                        title="开票明细"
                        width="full"
                        badge={
                            lines.length ? (
                                <span className="num rounded-md bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                                    共 {lines.length} 行
                                </span>
                            ) : undefined
                        }
                    >
                        <InvoiceRequestLinesTable data={request.data} />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
            {cancelDialog}
        </>
    )
}

/** 撤回申请确认弹窗：保留详情页上下文与原因校验，未知结果期间锁定输入仅允许同载荷重试。 */
function CancelRequestDialog({
    open,
    onOpenChange,
    request,
    status,
    reviseRejected,
    onDone,
    onCloseBlockedChange,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    request: InvoiceRequest
    status: { label: string; tone: StatusTone }
    reviseRejected: boolean
    onDone: () => void | Promise<void>
    onCloseBlockedChange: (blocked: boolean) => void
}) {
    const { cancel } = useInvoiceRequestCommands()
    const pending = useRef<Parameters<typeof cancelRequest>[0] | null>(null)
    const [uncertain, setUncertain] = useState(false)
    const [submitError, setSubmitError] = useState<string | null>(null)
    const reasonSchema = z
        .string()
        .trim()
        .min(1, "请输入撤回原因")
        .max(1000, "撤回原因最多 1000 字")

    const form = useAppForm({
        defaultValues: {
            reason: reviseRejected ? "按驳回意见修改原单后重新提交" : "",
        },
        validators: {
            onSubmit: z.object({
                reason: reasonSchema,
            }),
        },
    })

    useEffect(() => {
        if (!open) return
        pending.current = null
        setUncertain(false)
        setSubmitError(null)
        form.reset({
            reason: reviseRejected ? "按驳回意见修改原单后重新提交" : "",
        })
    }, [form, open, reviseRejected])

    useEffect(() => {
        onCloseBlockedChange(open && (cancel.isPending || uncertain))
        return () => onCloseBlockedChange(false)
    }, [cancel.isPending, onCloseBlockedChange, open, uncertain])

    const amount = request.data?.amount || "0.00"

    return (
        <FormalActionConfirmDialog
            idPrefix="invoice-request-cancel-dialog"
            open={open}
            onOpenChange={(next) => {
                if (!next && (cancel.isPending || uncertain)) return
                onOpenChange(next)
            }}
            pending={cancel.isPending}
            title={
                reviseRejected
                    ? `撤回并修改 ${request.request_no || "开票申请"}`
                    : `撤回开票申请 ${request.request_no || ""}`.trim()
            }
            description="请确认撤回影响并填写撤回原因，详情信息将继续保留在当前页面。"
            actionLabel={reviseRejected ? "撤回并修改原单" : "撤回申请"}
            confirmLabel={
                uncertain
                    ? "核对撤回结果"
                    : reviseRejected
                      ? "确认撤回并修改原单"
                      : "确认撤回申请"
            }
            cancelLabel="返回详情"
            actionVariant="destructive"
            fromStatus={status}
            toStatus={{ label: "草稿", tone: "neutral" }}
            summary={[
                <span key="no">
                    申请单号：
                    <span className="num font-medium text-foreground">
                        {request.request_no || "—"}
                    </span>
                </span>,
                <span key="amount" className="inline-flex items-baseline gap-1">
                    申请金额：
                    <MoneyValue value={amount} display="exact" />
                </span>,
            ]}
            effects={[
                "撤回后释放本次申请额度，申请保留为草稿。",
                ...(reviseRejected
                    ? ["原申请编号和审批记录保留，修改后可重新提交审批。"]
                    : []),
            ]}
            formContent={
                <div className="space-y-3">
                    <form.AppField name="reason">
                        {(field) => (
                            <field.TextareaField
                                id="invoice-request-cancel-reason"
                                label="撤回原因"
                                placeholder="请说明撤回本次开票申请的原因"
                                rows={3}
                                maxLength={1000}
                                required
                                disabled={cancel.isPending || uncertain}
                            />
                        )}
                    </form.AppField>
                    {submitError ? (
                        <Alert variant="destructive">
                            <AlertDescription>{submitError}</AlertDescription>
                        </Alert>
                    ) : null}
                </div>
            }
            onConfirmError={() => {
                // 校验错误与请求错误均在弹窗表单区内展示
            }}
            onConfirm={async () => {
                setSubmitError(null)
                if (!pending.current) {
                    const parsed = reasonSchema.safeParse(
                        form.state.values.reason,
                    )
                    if (!parsed.success) {
                        await form.validateAllFields("submit")
                        throw new Error("请输入撤回原因")
                    }
                    pending.current = {
                        id: request.id,
                        expected_version: request.version,
                        reason: parsed.data,
                        idempotency_key: crypto.randomUUID(),
                    }
                }
                try {
                    await cancel.mutateAsync(pending.current)
                    pending.current = null
                    setUncertain(false)
                    setSubmitError(null)
                    await onDone()
                } catch (error) {
                    const isUnknown =
                        classifyFormalCommandError(error) === "unknown"
                    setUncertain(isUnknown)
                    if (!isUnknown) pending.current = null
                    setSubmitError(
                        isUnknown
                            ? "结果尚未确认，请点击「核对撤回结果」核对原请求，切勿重复操作。"
                            : getErrorMessage(error, "撤回失败，请稍后重试。"),
                    )
                    throw error
                }
            }}
        />
    )
}
