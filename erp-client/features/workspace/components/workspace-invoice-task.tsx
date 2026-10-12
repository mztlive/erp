"use client"

import Link from "next/link"
import { sourceSalesOrderHref } from "../lib/source-sales-order"
import { useInvoiceRequests } from "@/features/invoice-requests/hooks/queries"
import { subtractFixed } from "@/lib/fixed-decimal"
import type { InvoiceRequest } from "@/features/invoice-requests/api"

import {
    MoneyValue,
    WorkspaceTaskPane,
    workspaceTaskSurfacePadClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { WorkspaceInvoiceUpload } from "./workspace-invoice-upload"
import { useCustomerAccountsDetailQuery } from "@/features/customer-receivables/hooks/queries"
import type { ReceivableAccountRow } from "@/features/customer-receivables/types"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

import type { WorkspaceWorkItem } from "../types"
import {
    workspaceInvoiceDescriptor,
    workspaceInvoiceMatchesReceivable,
} from "../lib/workspace-invoice"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

type WorkspaceInvoiceTaskProps = Readonly<{
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}>

/**
 * 范围行只有销售单和行级金额，不一定带回应收子账。
 * 已有子账必须一致；缺子账时用销售单核对，避免整页因缺字段关掉作业面。
 */
function approvedRequestMatches(
    request: InvoiceRequest,
    receivable: { accountId: string; salesOrderId: string },
): boolean {
    if (request.status !== "approved") return false
    if (
        request.receivable_account_id &&
        request.receivable_account_id !== receivable.accountId
    ) {
        return false
    }
    if (
        request.sales_order_id &&
        request.sales_order_id !== receivable.salesOrderId
    ) {
        return false
    }
    return Boolean(request.receivable_account_id || request.sales_order_id)
}

function remainingApproved(amount: string, invoiced: string): string {
    if (
        !/^-?\d+(?:\.\d{1,2})?$/.test(amount) ||
        !/^-?\d+(?:\.\d{1,2})?$/.test(invoiced)
    ) {
        return amount || "0.00"
    }
    return subtractFixed(amount, invoiced, { maxScale: 2, outputScale: 2 })
}

/** W01 开票作业面：任务身份锁定一个销售应收，登记销项发票不离开工作台。 */
export function WorkspaceInvoiceTask({
    item,
    onTaskCompleted,
}: WorkspaceInvoiceTaskProps) {
    const requestQuery = useInvoiceRequests({
        work_item_id: item.workItemId,
        page_size: 2,
    })
    const approvedRequest =
        requestQuery.data?.items.length === 1
            ? requestQuery.data.items[0]
            : undefined
    const descriptor = workspaceInvoiceDescriptor(item)
    const receivableQuery = useCustomerAccountsDetailQuery(
        descriptor ? "receivable" : null,
        descriptor?.receivableAccountId ?? null,
    )
    const receivable = receivableQuery.data?.receivable
    const identityOk = Boolean(
        descriptor &&
        receivable &&
        workspaceInvoiceMatchesReceivable(descriptor, receivable),
    )
    const executionAuthorized = item.allowedActions.includes("PROCESS")

    return (
        <WorkspaceTaskPane
            header={
                <WorkspaceTaskIdentityHeader
                    item={item}
                    title="登记销项发票"
                    subtitle={
                        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                            <span>
                                {receivable?.counterpartyPartyName ||
                                    item.counterpartyName}
                            </span>
                            {receivable?.salesOrderId ? (
                                <Link
                                    id="workspace-invoice-source-sales"
                                    href={sourceSalesOrderHref(
                                        receivable.salesOrderId,
                                    )}
                                    className="underline-offset-4 hover:underline"
                                >
                                    销售单 {receivable.salesOrderNo}
                                </Link>
                            ) : null}
                        </div>
                    }
                />
            }
            aria-label="当前开票任务"
        >
            {!descriptor ? (
                <Alert variant="destructive">
                    <AlertTitle>任务责任与开票对象不一致</AlertTitle>
                    <AlertDescription>
                        请联系管理员核对责任人、应收子账与销售来源后重试。
                    </AlertDescription>
                </Alert>
            ) : receivableQuery.isPending ? (
                <InvoiceSessionSkeleton />
            ) : receivableQuery.isError ? (
                <Alert variant="destructive">
                    <AlertTitle>应收子账加载失败</AlertTitle>
                    <AlertDescription className="flex flex-col gap-3">
                        <span>
                            {getErrorMessage(
                                receivableQuery.error,
                                "请刷新后重试",
                            )}
                        </span>
                        <Button
                            id={`workspace-invoice-receivable-retry-${toAutomationIdSegment(item.workItemId)}`}
                            type="button"
                            variant="outline"
                            size="sm"
                            className="self-start"
                            onClick={() => void receivableQuery.refetch()}
                        >
                            重试
                        </Button>
                    </AlertDescription>
                </Alert>
            ) : !identityOk || !receivable || !descriptor ? (
                <Alert variant="destructive">
                    <AlertTitle>应收子账与任务冻结事实不一致</AlertTitle>
                    <AlertDescription>
                        当前任务绑定的应收不是该销售单来源，已停止展开开票作业。
                    </AlertDescription>
                </Alert>
            ) : !executionAuthorized ? (
                <Alert variant="warning">
                    <AlertTitle>当前无法登记销项发票</AlertTitle>
                    <AlertDescription>
                        {item.actionBlockers[0]?.message ??
                            "当前账号没有处理此开票任务的资格。"}
                    </AlertDescription>
                </Alert>
            ) : requestQuery.isPending ? (
                <InvoiceSessionSkeleton />
            ) : requestQuery.isError ? (
                <Alert variant="destructive">
                    <AlertTitle>开票申请读取失败</AlertTitle>
                    <AlertDescription>
                        {getErrorMessage(requestQuery.error, "请刷新后重试")}
                        <Button
                            id="workspace-invoice-request-retry"
                            variant="outline"
                            onClick={() => void requestQuery.refetch()}
                        >
                            重试
                        </Button>
                    </AlertDescription>
                </Alert>
            ) : approvedRequest?.status === "completed" ? (
                <Alert>
                    <AlertTitle>本次申请已全部开票</AlertTitle>
                    <AlertDescription>
                        批准额度已执行完毕，可返回工作台查看其他任务。
                    </AlertDescription>
                </Alert>
            ) : !approvedRequest ||
              !approvedRequestMatches(approvedRequest, receivable) ? (
                <Alert variant="warning">
                    <AlertTitle>此任务没有可执行的已批准申请</AlertTitle>
                    <AlertDescription>
                        请从销售单发起开票申请，或刷新查看最新处理结果。
                    </AlertDescription>
                </Alert>
            ) : (
                <WorkspaceInvoiceSession
                    request={approvedRequest}
                    item={item}
                    receivable={{
                        ...receivable,
                        openInvoiceableTotal: remainingApproved(
                            approvedRequest.data?.amount || "0.00",
                            approvedRequest.invoiced_amount || "0.00",
                        ),
                    }}
                    onTaskCompleted={onTaskCompleted}
                />
            )}
        </WorkspaceTaskPane>
    )
}

/** 开票作业面加载占位。 */
function InvoiceSessionSkeleton() {
    return (
        <div
            className={cn(workspaceTaskSurfacePadClassName, "grid gap-4 py-5")}
        >
            <div className="h-40 animate-pulse rounded-lg bg-muted" />
            <div className="h-40 animate-pulse rounded-lg bg-muted" />
        </div>
    )
}

/** 应收身份确认后展开核销工作面；提交结果留在当前任务。 */
function WorkspaceInvoiceSession({
    request,
    item,
    receivable,
    onTaskCompleted,
}: {
    request: InvoiceRequest
    item: WorkspaceWorkItem
    receivable: ReceivableAccountRow
    onTaskCompleted?: (workItemId: string) => void
}) {
    return (
        <div
            className={cn(
                workspaceTaskSurfacePadClassName,
                "@container/invoice space-y-6 py-5",
            )}
        >
            <dl className="grid grid-cols-1 gap-4 rounded-lg border border-grid p-5 @min-[420px]/invoice:grid-cols-3 @min-[420px]/invoice:gap-0">
                {[
                    ["批准金额", request.data.amount],
                    ["已登记", request.invoiced_amount],
                    ["剩余可开票", receivable.openInvoiceableTotal],
                ].map(([label, value]) => (
                    <div
                        key={label}
                        className="min-w-0 space-y-2 border-grid @min-[420px]/invoice:border-l @min-[420px]/invoice:px-5 @min-[420px]/invoice:first:border-l-0 @min-[420px]/invoice:first:pl-0 @min-[420px]/invoice:last:pr-0"
                    >
                        <dt className="text-sm text-muted-foreground">
                            {label}
                        </dt>
                        <dd className="overflow-x-auto">
                            <MoneyValue
                                value={value}
                                className="text-xl font-semibold leading-tight"
                            />
                        </dd>
                    </div>
                ))}
            </dl>
            <section aria-label="已批准开票资料" className="space-y-3">
                <h3 className="text-base font-semibold">已批准开票资料</h3>
                <dl className="grid gap-x-6 gap-y-4 rounded-lg border border-grid bg-muted/30 p-4 text-sm @min-[540px]/invoice:grid-cols-2">
                    {[
                        ["开票抬头", request.data.invoice_title],
                        ["税号", request.data.tax_number],
                        ["开票内容", request.data.invoice_content],
                        ["申请事由", request.data.reason],
                    ].map(([label, value]) => (
                        <div
                            key={label}
                            className="grid grid-cols-[5rem_minmax(0,1fr)] gap-3"
                        >
                            <dt className="text-muted-foreground">{label}</dt>
                            <dd className="break-words">{value || "—"}</dd>
                        </div>
                    ))}
                </dl>
                <p className="text-xs text-muted-foreground">
                    关联应收：{item.objectTitle} · {receivable.salesOrderNo}
                </p>
            </section>
            <WorkspaceInvoiceUpload
                key={`${item.workItemId}:${item.taskVersion}`}
                item={item}
                request={request}
                amount={receivable.openInvoiceableTotal}
                onTaskCompleted={onTaskCompleted}
            />
        </div>
    )
}
