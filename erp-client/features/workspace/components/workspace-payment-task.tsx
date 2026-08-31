"use client"

import * as React from "react"
import { useQueryClient, type QueryClient } from "@tanstack/react-query"
import { usePathname, useSearchParams } from "next/navigation"
import { ArrowUpRightIcon, FileTextIcon } from "lucide-react"

import {
    MoneyValue,
    WorkspaceTaskPane,
    workspaceTaskSurfacePadClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button, buttonVariants } from "@/components/ui/button"
import { toast } from "@/components/ui/toast"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { allocationSessionMatchesIdentity } from "@/features/supplier-payables/lib/allocation-session-identity"
import { cents } from "@/features/supplier-payables/lib/allocation-model"
import { SupplierAllocationWorkspace } from "@/features/supplier-payables/components/allocation-workspace"
import {
    supplierPayablesKeys,
    usePayableDetailQuery,
} from "@/features/supplier-payables/hooks/queries"
import { useAllocationSession } from "@/features/supplier-payables/hooks/use-allocation-session"
import type {
    FormalSubmitResult,
    PaymentRecipient,
} from "@/features/supplier-payables/types"
import { fulfillmentKeys } from "@/features/fulfillment-operations/queries"
import { purchaseOrderKeys } from "@/features/purchase-orders/queries"
import { workItemKeys } from "@/features/work-items/queries"
import { workspaceHomeKeys } from "@/features/workspace/hooks/queries"
import { workspaceReadActionLabel } from "@/features/workspace/api/work-item-meta"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

import type { WorkspaceWorkItem } from "../types"
import {
    workspacePaymentDescriptor,
    workspacePaymentMatchesPayable,
} from "../lib/workspace-payment"
import {
    WorkspaceDocumentPaperDialog,
    type WorkspacePaperTarget,
} from "./workspace-document-paper-dialog"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

type WorkspacePaymentTaskProps = Readonly<{
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}>

/** W01 付款作业面：任务身份锁定一个采购应付，登记付款不离开工作台。 */
export function WorkspacePaymentTask({
    item,
    onTaskCompleted,
}: WorkspacePaymentTaskProps) {
    const descriptor = workspacePaymentDescriptor(item)
    const payableQuery = usePayableDetailQuery(
        descriptor?.payableAccountId ?? null,
    )
    const payable = payableQuery.data?.payable
    const identityOk = Boolean(
        descriptor &&
        payable &&
        workspacePaymentMatchesPayable(descriptor, payable),
    )
    const executionAuthorized = item.allowedActions.includes("PROCESS")
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const returnTo = `${pathname}${searchParams.toString() ? `?${searchParams}` : ""}`
    const [paper, setPaper] = React.useState<WorkspacePaperTarget | null>(null)
    const purchaseOrderId = descriptor?.purchaseOrderId
    const purchaseOrderHref = purchaseOrderId
        ? purchaseOrderOpenHref(purchaseOrderId, returnTo)
        : undefined
    const purchaseOrderNo = payable?.sourceDocumentNo
    const readPurchaseLabel = workspaceReadActionLabel("purchase_order")

    return (
        <WorkspaceTaskPane
            header={
                <WorkspaceTaskIdentityHeader
                    item={item}
                    title={`向${payable?.supplierName ?? item.counterpartyName ?? "供应商"}付款`}
                    subtitle={
                        payable ? (
                            <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
                                <span>
                                    待付{" "}
                                    <MoneyValue
                                        value={payable.openTotal}
                                        taxBasis="gross"
                                    />
                                </span>
                                <span className="inline-flex min-w-0 items-center gap-1">
                                    <span>采购单</span>
                                    <span className="num text-foreground">
                                        {payable.sourceDocumentNo}
                                    </span>
                                    {purchaseOrderId ? (
                                        <IconActionButton
                                            id={`workspace-payment-preview-po-${toAutomationIdSegment(item.workItemId)}`}
                                            label={readPurchaseLabel}
                                            testId={`work-item-read-purchase-order-${item.workItemId}`}
                                            onClick={() =>
                                                setPaper({
                                                    kind: "purchase_order",
                                                    objectId: purchaseOrderId,
                                                    title: purchaseOrderNo,
                                                })
                                            }
                                        >
                                            <FileTextIcon aria-hidden="true" />
                                        </IconActionButton>
                                    ) : null}
                                    {purchaseOrderHref ? (
                                        <IconActionButton
                                            id={`workspace-payment-open-po-${toAutomationIdSegment(item.workItemId)}`}
                                            label="打开采购单"
                                            testId={`work-item-open-purchase-order-${item.workItemId}`}
                                            href={purchaseOrderHref}
                                        >
                                            <ArrowUpRightIcon aria-hidden="true" />
                                        </IconActionButton>
                                    ) : null}
                                </span>
                                {payable.dueDate ? (
                                    <span>
                                        {payable.dueStateLabel} ·{" "}
                                        {payable.dueDate}
                                    </span>
                                ) : null}
                            </div>
                        ) : undefined
                    }
                />
            }
            aria-label="当前付款任务"
        >
            {!descriptor ? (
                <Alert variant="destructive">
                    <AlertTitle>任务责任与付款对象不一致</AlertTitle>
                    <AlertDescription>
                        请联系管理员核对责任人、应付子账与采购来源后重试。
                    </AlertDescription>
                </Alert>
            ) : payableQuery.isPending ? (
                <div
                    className={cn(
                        workspaceTaskSurfacePadClassName,
                        "grid gap-4 py-5",
                    )}
                >
                    <div className="h-40 animate-pulse rounded-lg bg-muted" />
                    <div className="h-40 animate-pulse rounded-lg bg-muted" />
                </div>
            ) : payableQuery.isError ? (
                <Alert variant="destructive">
                    <AlertTitle>应付子账加载失败</AlertTitle>
                    <AlertDescription className="flex flex-col gap-3">
                        <span>
                            {getErrorMessage(
                                payableQuery.error,
                                "请刷新后重试",
                            )}
                        </span>
                        <Button
                            id={`workspace-payment-payable-retry-${toAutomationIdSegment(item.workItemId)}`}
                            type="button"
                            variant="outline"
                            size="sm"
                            className="self-start"
                            onClick={() => void payableQuery.refetch()}
                        >
                            重试
                        </Button>
                    </AlertDescription>
                </Alert>
            ) : !identityOk || !payable || !descriptor ? (
                <Alert variant="destructive">
                    <AlertTitle>应付子账与任务冻结事实不一致</AlertTitle>
                    <AlertDescription>
                        当前任务绑定的应付不是该采购单来源，已停止展开付款作业。
                    </AlertDescription>
                </Alert>
            ) : !executionAuthorized ? (
                <Alert variant="warning">
                    <AlertTitle>当前无法登记付款</AlertTitle>
                    <AlertDescription>
                        {item.actionBlockers[0]?.message ??
                            "当前账号没有处理此付款任务的资格。"}
                    </AlertDescription>
                </Alert>
            ) : !payable.paymentRecipient ? (
                <Alert variant="warning">
                    <AlertTitle>供应商未配置可用收款账户</AlertTitle>
                    <AlertDescription>
                        当前付款任务不能执行。请先在供应商主数据中维护唯一的当前默认收款账户，再刷新任务。
                    </AlertDescription>
                </Alert>
            ) : (
                <WorkspacePaymentSession
                    key={`${item.workItemId}:${payable.paymentRecipient.bankAccountId}:${payable.paymentRecipient.version}`}
                    item={item}
                    supplierId={payable.supplierId}
                    purchaseOrderId={descriptor.purchaseOrderId}
                    payableAccountId={payable.payableAccountId}
                    openTotal={payable.openTotal}
                    paymentRecipient={payable.paymentRecipient}
                    onTaskCompleted={onTaskCompleted}
                />
            )}
            <WorkspaceDocumentPaperDialog
                target={paper}
                open={Boolean(paper)}
                onOpenChange={(open) => {
                    if (!open) setPaper(null)
                }}
            />
        </WorkspaceTaskPane>
    )
}

/** 应付身份确认后展开核销工作面；成功用 Toast 收口，失败留在当前任务。 */
function WorkspacePaymentSession({
    item,
    supplierId,
    purchaseOrderId,
    payableAccountId,
    openTotal,
    paymentRecipient,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    supplierId: string
    purchaseOrderId: string
    payableAccountId: string
    openTotal: string
    paymentRecipient: PaymentRecipient
    onTaskCompleted?: (workItemId: string) => void
}) {
    const queryClient = useQueryClient()
    const [draftSessionId, setDraftSessionId] = React.useState<string>()
    const startFreshRef = React.useRef<() => string>(() => "")

    const sessionState = useAllocationSession(
        {
            track: "payment",
            supplierId,
            draftSessionId,
            purchaseOrderId,
            fromWorkspace: "W01",
            preselectPayableAccountId: item.businessObjectId,
            paymentWorkItemId: item.workItemId,
            expectedPaymentTaskVersion: item.taskVersion,
            paymentPayableAccountId: item.businessObjectId,
            paymentRecipientBankAccountId: paymentRecipient.bankAccountId,
            paymentRecipientBankAccountVersion: paymentRecipient.version,
        },
        {
            consumeSucceededResult: true,
            onDraftSessionIdChange: setDraftSessionId,
            onCompleted: (result) => {
                announcePaymentSucceeded(result)
                if (
                    onTaskCompleted &&
                    cents(result.allocatedTotal ?? "0") >= cents(openTotal)
                ) {
                    onTaskCompleted(item.workItemId)
                    return
                }
                setDraftSessionId(startFreshRef.current())
                void invalidatePaymentContext(queryClient)
            },
        },
    )
    startFreshRef.current = sessionState.startFreshAttempt

    async function startNextPaymentAttempt() {
        setDraftSessionId(sessionState.startFreshAttempt())
        await invalidatePaymentContext(queryClient)
    }

    if (sessionState.sessionQuery.isPending) {
        return (
            <div className="grid gap-4">
                <div className="h-40 animate-pulse rounded-lg bg-muted" />
                <div className="h-40 animate-pulse rounded-lg bg-muted" />
            </div>
        )
    }

    if (sessionState.sessionQuery.isError) {
        return (
            <Alert variant="destructive">
                <AlertTitle>无法开始本次付款</AlertTitle>
                <AlertDescription className="flex flex-col gap-3">
                    <span>
                        {getErrorMessage(
                            sessionState.sessionQuery.error,
                            "请刷新后重试",
                        )}
                    </span>
                    <Button
                        id={`workspace-payment-session-retry-${toAutomationIdSegment(item.workItemId)}`}
                        type="button"
                        variant="outline"
                        size="sm"
                        className="self-start"
                        onClick={() => void sessionState.sessionQuery.refetch()}
                    >
                        重试
                    </Button>
                </AlertDescription>
            </Alert>
        )
    }

    const session = sessionState.session

    if (!session) {
        return (
            <Alert>
                <AlertTitle>没有可核销内容</AlertTitle>
                <AlertDescription>
                    当前应付没有开放余额，或核销池未能加载。请刷新后重试。
                </AlertDescription>
            </Alert>
        )
    }

    if (
        !allocationSessionMatchesIdentity(session, {
            track: "payment",
            supplierId,
            purchaseOrderId,
            preselectPayableAccountId: item.businessObjectId,
        })
    ) {
        return (
            <Alert variant="destructive">
                <AlertTitle>付款会话与当前任务不一致</AlertTitle>
                <AlertDescription>
                    已停止载入上一条任务的付款草稿，请重新打开当前任务后重试。
                </AlertDescription>
            </Alert>
        )
    }

    if (
        !session.pool.some(
            (poolItem) => poolItem.payableAccountId === item.businessObjectId,
        )
    ) {
        return (
            <Alert variant="warning">
                <AlertTitle>当前应付已不可付款</AlertTitle>
                <AlertDescription>
                    当前任务绑定的应付没有开放余额，或已不在可付款范围内。请刷新任务列表后重试。
                </AlertDescription>
            </Alert>
        )
    }

    return (
        <SupplierAllocationWorkspace
            state={sessionState}
            track="payment"
            purchaseOrderId={purchaseOrderId}
            paymentRecipient={paymentRecipient}
            paymentRecipientReveal={{
                payableAccountId,
                workItemId: item.workItemId,
                expectedTaskVersion: item.taskVersion,
            }}
            embedded
            onClose={() => {
                void startNextPaymentAttempt()
            }}
        />
    )
}

function purchaseOrderOpenHref(
    purchaseOrderId: string,
    returnTo?: string,
): string {
    const path = `/procurement/orders/${encodeURIComponent(purchaseOrderId)}`
    if (!returnTo?.trim()) return path
    return `${path}?${new URLSearchParams({
        from: "workspace",
        returnTo: returnTo.trim(),
    }).toString()}`
}

function IconActionButton({
    id,
    label,
    testId,
    href,
    onClick,
    children,
}: {
    id: string
    label: string
    testId: string
    href?: string
    onClick?: () => void
    children: React.ReactNode
}) {
    return (
        <Tooltip>
            <TooltipTrigger
                id={id}
                render={
                    href ? (
                        <a
                            id={id}
                            href={href}
                            aria-label={label}
                            data-testid={testId}
                            className={buttonVariants({
                                variant: "ghost",
                                size: "icon-sm",
                            })}
                        />
                    ) : (
                        <Button
                            id={id}
                            type="button"
                            variant="ghost"
                            size="icon-sm"
                            aria-label={label}
                            data-testid={testId}
                            onClick={onClick}
                        />
                    )
                }
            >
                {children}
            </TooltipTrigger>
            <TooltipContent>{label}</TooltipContent>
        </Tooltip>
    )
}

function announcePaymentSucceeded(result: FormalSubmitResult) {
    const documentNo = result.documentNo ?? result.reference
    toast.add({
        title: "付款已登记",
        description: documentNo ? `${documentNo} 已过账并核销` : "已过账并核销",
        type: "success",
        timeout: 4000,
    })
}

function invalidatePaymentContext(queryClient: QueryClient) {
    return Promise.all([
        queryClient.invalidateQueries({
            queryKey: supplierPayablesKeys.all,
        }),
        queryClient.invalidateQueries({
            queryKey: purchaseOrderKeys.all,
        }),
        queryClient.invalidateQueries({
            queryKey: fulfillmentKeys.all,
        }),
        queryClient.invalidateQueries({
            queryKey: workItemKeys.all,
        }),
        queryClient.invalidateQueries({
            queryKey: workspaceHomeKeys.all,
        }),
    ])
}
