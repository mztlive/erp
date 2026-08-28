"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"

import { MoneyValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { allocationSessionMatchesIdentity } from "@/features/supplier-payables/lib/allocation-session-identity"
import { SupplierAllocationWorkspace } from "@/features/supplier-payables/components/allocation-workspace"
import {
    supplierPayablesKeys,
    usePayableDetailQuery,
} from "@/features/supplier-payables/hooks/queries"
import { useAllocationSession } from "@/features/supplier-payables/hooks/use-allocation-session"
import type { PaymentRecipient } from "@/features/supplier-payables/types"
import { fulfillmentKeys } from "@/features/fulfillment-operations/queries"
import { purchaseOrderKeys } from "@/features/purchase-orders/queries"
import { workItemKeys } from "@/features/work-items/queries"
import { workspaceHomeKeys } from "@/features/workspace/hooks/queries"
import { getErrorMessage } from "@/lib/api/errors"

import type { WorkspaceWorkItem } from "../types"
import {
    workspacePaymentDescriptor,
    workspacePaymentMatchesPayable,
} from "../lib/workspace-payment"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"

type WorkspacePaymentTaskProps = Readonly<{
    item: WorkspaceWorkItem
}>

/** W01 付款作业面：任务身份锁定一个采购应付，登记付款不离开工作台。 */
export function WorkspacePaymentTask({ item }: WorkspacePaymentTaskProps) {
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

    return (
        <section
            className="flex h-full min-h-0 flex-col"
            aria-label="当前付款任务"
        >
            <header className="flex shrink-0 items-start justify-between gap-3 border-b border-grid py-5">
                <div className="flex min-w-0 flex-col gap-2">
                    <WorkspaceDocumentBadge item={item} />
                    <h2 className="text-xl font-semibold tracking-tight">
                        向
                        {payable?.supplierName ??
                            item.counterpartyName ??
                            "供应商"}
                        付款
                    </h2>
                    {payable ? (
                        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-muted-foreground">
                            <span>
                                待付{" "}
                                <MoneyValue
                                    value={payable.openTotal}
                                    taxBasis="gross"
                                />
                            </span>
                            <span>采购单 {payable.sourceDocumentNo}</span>
                            {payable.dueDate ? (
                                <span>
                                    {payable.dueStateLabel} · {payable.dueDate}
                                </span>
                            ) : null}
                        </div>
                    ) : null}
                </div>
            </header>

            <div className="min-h-0 flex-1 overflow-auto py-4">
                {!descriptor ? (
                    <Alert variant="destructive">
                        <AlertTitle>任务责任与付款对象不一致</AlertTitle>
                        <AlertDescription>
                            请联系管理员核对责任人、应付子账与采购来源后重试。
                        </AlertDescription>
                    </Alert>
                ) : payableQuery.isPending ? (
                    <div className="grid gap-4">
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
                        paymentRecipient={payable.paymentRecipient}
                    />
                )}
            </div>
        </section>
    )
}

/** 应付身份确认后展开核销工作面；提交结果留在当前任务。 */
function WorkspacePaymentSession({
    item,
    supplierId,
    purchaseOrderId,
    payableAccountId,
    paymentRecipient,
}: {
    item: WorkspaceWorkItem
    supplierId: string
    purchaseOrderId: string
    payableAccountId: string
    paymentRecipient: PaymentRecipient
}) {
    const queryClient = useQueryClient()
    const [draftSessionId, setDraftSessionId] = React.useState<string>()
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
        { onDraftSessionIdChange: setDraftSessionId },
    )

    async function startNextPaymentAttempt() {
        await Promise.all([
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
        setDraftSessionId(sessionState.startFreshAttempt())
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
