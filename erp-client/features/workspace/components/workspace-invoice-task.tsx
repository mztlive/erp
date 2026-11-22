"use client"

import Link from "next/link"
import { isResourceAuthorizationPending } from "@/lib/resource-authorization"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { sourceSalesOrderHref } from "../lib/source-sales-order"
import { useInvoiceRequests } from "@/features/invoice-requests/hooks/queries"
import { subtractFixed } from "@/lib/fixed-decimal"
import type { InvoiceRequest } from "@/features/invoice-requests/api"
import { InvoiceRequestDetails } from "@/features/invoice-requests/components/request-details"

import {
    BusinessFailureState,
    MoneyValue,
    WorkspaceTaskPane,
    workspaceTaskSurfacePadClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { WorkspaceInvoiceUpload } from "./workspace-invoice-upload"
import { useCustomerAccountsDetailQuery } from "@/features/customer-receivables/hooks/queries"
import type { ReceivableAccountRow } from "@/features/customer-receivables/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

import type { WorkspaceWorkItem } from "../types"
import { workspaceInvoiceDescriptor } from "../lib/workspace-invoice"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

type WorkspaceInvoiceTaskProps = Readonly<{
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}>

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
    const profile = useAccountProfileQuery().data
    const canReadSalesOrder = hasPermission(
        profile?.permissions,
        "sales_order:detail",
    )
    const descriptor = workspaceInvoiceDescriptor(item)
    const canReadRequest = hasPermission(
        profile?.permissions,
        "sales_invoice_request:list",
    )
    const requestQuery = useInvoiceRequests(
        {
            work_item_id: item.workItemId,
            receivable_account_id: descriptor?.receivableAccountId,
            page_size: 2,
        },
        Boolean(descriptor && item.workItemId && canReadRequest),
    )
    const request =
        canReadRequest &&
        requestQuery.isSuccess &&
        requestQuery.data.items.length === 1
            ? requestQuery.data.items[0]
            : undefined
    const taskRequest =
        descriptor &&
        request?.work_item_id === item.workItemId &&
        request.receivable_account_id === descriptor.receivableAccountId
            ? request
            : undefined
    const canProcess =
        item.status === "OPEN" && item.allowedActions.includes("PROCESS")
    const readOnly = !canProcess || taskRequest?.status === "completed"
    const canOpenRequest =
        canReadRequest &&
        hasPermission(profile?.permissions, "sales_invoice_request:detail")
    const receivableQuery = useCustomerAccountsDetailQuery(
        taskRequest && !readOnly ? "receivable" : null,
        descriptor?.receivableAccountId ?? null,
    )
    const grantPending = isResourceAuthorizationPending(requestQuery.error)
    const receivable = receivableQuery.isSuccess
        ? receivableQuery.data?.receivable
        : undefined
    const identityOk = Boolean(
        descriptor &&
        taskRequest &&
        receivable?.salesOrderId &&
        receivable.accountId === descriptor.receivableAccountId &&
        (!taskRequest.sales_order_id ||
            taskRequest.sales_order_id === receivable.salesOrderId),
    )
    const salesOrderNo =
        (identityOk && receivable?.salesOrderNo) || taskRequest?.sales_order_no

    return (
        <WorkspaceTaskPane
            header={
                <WorkspaceTaskIdentityHeader
                    item={item}
                    title={readOnly ? "销项开票任务" : "登记销项发票"}
                    subtitle={
                        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                            <span>
                                {receivable?.counterpartyPartyName ||
                                    item.counterpartyName}
                            </span>
                            {identityOk &&
                            receivable?.salesOrderId &&
                            canReadSalesOrder ? (
                                <Link
                                    id="workspace-invoice-source-sales"
                                    href={sourceSalesOrderHref(
                                        receivable.salesOrderId,
                                    )}
                                    className="underline-offset-4 hover:underline"
                                >
                                    销售单 {receivable.salesOrderNo}
                                </Link>
                            ) : salesOrderNo ? (
                                <span>销售单 {salesOrderNo}</span>
                            ) : null}
                        </div>
                    }
                />
            }
            aria-label="当前开票任务"
        >
            {!descriptor ? (
                <Alert variant="destructive">
                    <AlertTitle>无法确认本条待办对应的开票资料</AlertTitle>
                    <AlertDescription>
                        请联系管理员核对开票负责人和关联销售单，再刷新重试。
                    </AlertDescription>
                </Alert>
            ) : profile && !canReadRequest ? (
                <Alert>
                    <AlertTitle>当前账号不能查看关联开票申请</AlertTitle>
                </Alert>
            ) : grantPending ? (
                <Alert>
                    <AlertTitle>资料授权正在处理</AlertTitle>
                    <AlertDescription>
                        审批已完成，资料准备好后将自动刷新。
                        <Button
                            id="workspace-invoice-grant-refresh"
                            variant="outline"
                            size="sm"
                            onClick={() => void requestQuery.refetch()}
                        >
                            刷新资料
                        </Button>
                    </AlertDescription>
                </Alert>
            ) : requestQuery.isError ? (
                <BusinessFailureState
                    idPrefix="workspace-invoice-request"
                    title="开票申请暂不可查看"
                    error={requestQuery.error}
                    onRetry={() => void requestQuery.refetch()}
                />
            ) : requestQuery.isPending ? (
                <InvoiceSessionSkeleton />
            ) : !taskRequest ? (
                <Alert variant="warning">
                    <AlertTitle>未能核对本任务关联的开票申请</AlertTitle>
                    <AlertDescription>
                        请刷新任务。若仍无法查看，请联系管理员核对任务和申请的关联或访问权限。
                    </AlertDescription>
                </Alert>
            ) : readOnly ? (
                <div
                    className={cn(
                        workspaceTaskSurfacePadClassName,
                        "flex flex-col gap-4 py-5",
                    )}
                >
                    <Alert>
                        <AlertTitle>
                            {taskRequest.status === "completed"
                                ? "本次申请已全部开票"
                                : item.status === "COMPLETED"
                                  ? "该开票任务已完成"
                                  : "当前仅可查看开票资料"}
                        </AlertTitle>
                        {item.actionBlockers[0] && item.status === "OPEN" && (
                            <AlertDescription>
                                {item.actionBlockers[0].message}
                            </AlertDescription>
                        )}
                    </Alert>
                    <dl className="grid gap-3 rounded-lg border border-grid p-4 text-body-compact sm:grid-cols-2">
                        <div>
                            <dt className="text-muted-foreground">申请金额</dt>
                            <dd className="mt-1">
                                <MoneyValue value={taskRequest.data.amount} />
                            </dd>
                        </div>
                        <div>
                            <dt className="text-muted-foreground">
                                已登记开票
                            </dt>
                            <dd className="mt-1">
                                <MoneyValue
                                    value={taskRequest.invoiced_amount}
                                />
                            </dd>
                        </div>
                    </dl>
                    <AppliedInvoiceDetails
                        request={taskRequest}
                        canOpenRequest={canOpenRequest}
                    />
                </div>
            ) : receivableQuery.isPending ? (
                <InvoiceSessionSkeleton />
            ) : receivableQuery.isError ? (
                <BusinessFailureState
                    idPrefix={`workspace-invoice-receivable-${toAutomationIdSegment(item.workItemId)}`}
                    title="销售应收资料暂不可查看"
                    error={receivableQuery.error}
                    onRetry={() => void receivableQuery.refetch()}
                />
            ) : !identityOk || !receivable ? (
                <Alert variant="destructive">
                    <AlertTitle>待办关联的销售应收不匹配</AlertTitle>
                    <AlertDescription>
                        请联系管理员核对本条待办关联的销售单和应收记录，修正后刷新再办理开票。
                    </AlertDescription>
                </Alert>
            ) : taskRequest.status !== "approved" ? (
                <Alert variant="warning">
                    <AlertTitle>此任务没有可执行的已批准申请</AlertTitle>
                    <AlertDescription>
                        请从销售单发起开票申请，或刷新查看最新处理结果。
                    </AlertDescription>
                </Alert>
            ) : (
                <WorkspaceInvoiceSession
                    request={taskRequest}
                    canOpenRequest={canOpenRequest}
                    item={item}
                    receivable={{
                        ...receivable,
                        openInvoiceableTotal: remainingApproved(
                            taskRequest.data?.amount || "0.00",
                            taskRequest.invoiced_amount || "0.00",
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
    canOpenRequest,
    item,
    receivable,
    onTaskCompleted,
}: {
    request: InvoiceRequest
    canOpenRequest: boolean
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
            <Alert variant="info">
                <AlertTitle>当前需要你处理：登记已开出的销项发票</AlertTitle>
                <AlertDescription>
                    按下方已批准资料开票后上传发票。本入口按申请的全部剩余金额登记，请先核对发票合计与下方剩余金额一致。
                </AlertDescription>
            </Alert>
            <dl className="grid grid-cols-1 gap-4 rounded-lg border border-grid p-5 @min-[420px]/invoice:grid-cols-3 @min-[420px]/invoice:gap-0">
                {[
                    ["本申请批准金额", request.data.amount],
                    ["本申请已登记", request.invoiced_amount],
                    ["本申请剩余可开票", receivable.openInvoiceableTotal],
                ].map(([label, value]) => (
                    <div
                        key={label}
                        className="min-w-0 space-y-2 border-grid @min-[420px]/invoice:border-l @min-[420px]/invoice:px-5 @min-[420px]/invoice:first:border-l-0 @min-[420px]/invoice:first:pl-0 @min-[420px]/invoice:last:pr-0"
                    >
                        <dt className="text-xs text-muted-foreground">
                            {label}
                        </dt>
                        <dd className="overflow-x-auto">
                            <MoneyValue
                                value={value}
                                className="text-value-compact font-semibold tracking-tight"
                            />
                        </dd>
                    </div>
                ))}
            </dl>
            <AppliedInvoiceDetails
                request={request}
                canOpenRequest={canOpenRequest}
            />
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

function AppliedInvoiceDetails({
    request,
    canOpenRequest,
}: {
    request: InvoiceRequest
    canOpenRequest: boolean
}) {
    return (
        <section
            aria-label="已批准开票资料"
            className="flex min-w-0 flex-col gap-3"
        >
            <div className="flex flex-wrap items-baseline justify-between gap-2">
                <h3 className="text-body-compact font-semibold">
                    已批准开票资料
                </h3>
                {request.request_no ? (
                    canOpenRequest ? (
                        <Link
                            id={`workspace-invoice-linked-request-${toAutomationIdSegment(request.id)}`}
                            href={`/finance/customer-accounts/invoice-requests/${encodeURIComponent(request.id)}`}
                            className="text-body-compact underline-offset-4 hover:underline"
                        >
                            开票申请 {request.request_no}
                        </Link>
                    ) : (
                        <p className="text-xs text-muted-foreground">
                            开票申请 {request.request_no}
                        </p>
                    )
                ) : null}
            </div>
            <InvoiceRequestDetails data={request.data} />
        </section>
    )
}
