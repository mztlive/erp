"use client"

import * as React from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { AllocationSessionScreen } from "@/features/customer-receivables/pages/components/allocation-session-screen"
import {
    useCreateAllocationSessionMutation,
    useCustomerAccountsDetailQuery,
    useAllocationSessionQuery,
} from "@/features/customer-receivables/hooks/queries"
import type { ReceivableAccountRow } from "@/features/customer-receivables/types"
import { getErrorMessage } from "@/lib/api/errors"

import type { WorkspaceWorkItem } from "../types"
import {
    invoiceExecutionIsComplete,
    workspaceInvoiceDescriptor,
    workspaceInvoiceMatchesReceivable,
} from "../lib/workspace-invoice"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"

type WorkspaceInvoiceTaskProps = Readonly<{
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}>

/** W01 开票作业面：任务身份锁定一个销售应收，登记销项发票不离开工作台。 */
export function WorkspaceInvoiceTask({
    item,
    onTaskCompleted,
}: WorkspaceInvoiceTaskProps) {
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
        <section
            className="flex h-full min-h-0 flex-col"
            aria-label="当前开票任务"
        >
            <header className="flex shrink-0 items-start justify-between gap-3 border-b border-grid py-5">
                <div className="flex min-w-0 flex-col gap-2">
                    <WorkspaceDocumentBadge item={item} />
                    <h2 className="text-xl font-semibold tracking-tight">
                        {item.objectTitle}
                    </h2>
                    <p className="text-sm text-muted-foreground">
                        {[
                            `${item.ownerRoleLabel} · ${item.ownerUserLabel}`,
                            receivable?.customerName,
                            receivable?.salesOrderNo,
                        ]
                            .filter(Boolean)
                            .join(" · ")}
                    </p>
                </div>
            </header>

            <div className="min-h-0 flex-1 overflow-auto py-4">
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
                ) : (
                    <WorkspaceInvoiceSession
                        item={item}
                        receivable={receivable}
                        onTaskCompleted={onTaskCompleted}
                    />
                )}
            </div>
        </section>
    )
}

/** 开票作业面加载占位。 */
function InvoiceSessionSkeleton() {
    return (
        <div className="grid gap-4">
            <div className="h-40 animate-pulse rounded-lg bg-muted" />
            <div className="h-40 animate-pulse rounded-lg bg-muted" />
        </div>
    )
}

/** 应收身份确认后展开核销工作面；提交结果留在当前任务。 */
function WorkspaceInvoiceSession({
    item,
    receivable,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    receivable: ReceivableAccountRow
    onTaskCompleted?: (workItemId: string) => void
}) {
    const [resetNonce, setResetNonce] = React.useState(0)
    const fingerprint = `${item.workItemId}:${item.taskVersion}:${receivable.accountId}:${resetNonce}`
    const [draftSessionId, setDraftSessionId] = React.useState<string>()
    const [createError, setCreateError] = React.useState<unknown>(null)
    const createSession = useCreateAllocationSessionMutation()
    const sessionQuery = useAllocationSessionQuery(draftSessionId ?? null)

    React.useEffect(() => {
        let cancelled = false
        setDraftSessionId(undefined)
        setCreateError(null)
        void createSession
            .mutateAsync({
                mode: "invoice",
                counterpartyPartyId: receivable.counterpartyPartyId,
                counterpartyPartyName: receivable.counterpartyPartyName,
                customerId: receivable.customerId,
                customerName: receivable.customerName,
                salesOrderId: receivable.salesOrderId,
                receivableAccountId: receivable.accountId,
                from: "W01",
            })
            .then((session) => {
                if (cancelled) return
                setDraftSessionId(session.draftSessionId)
            })
            .catch((error: unknown) => {
                if (cancelled) return
                setCreateError(error)
            })
        return () => {
            cancelled = true
        }
        // 只随任务身份与重置次数重建会话，不把 mutation 实例放进依赖。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [fingerprint])

    if (createError) {
        return (
            <Alert variant="destructive">
                <AlertTitle>无法开始本次开票</AlertTitle>
                <AlertDescription className="flex flex-col gap-3">
                    <span>{getErrorMessage(createError, "请刷新后重试")}</span>
                    <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        className="self-start"
                        onClick={() => setResetNonce((value) => value + 1)}
                    >
                        重试
                    </Button>
                </AlertDescription>
            </Alert>
        )
    }

    return (
        <AllocationSessionScreen
            isPending={!draftSessionId || sessionQuery.isPending}
            session={sessionQuery.data}
            onBackToList={() => setResetNonce((value) => value + 1)}
            onClose={() => setResetNonce((value) => value + 1)}
            onPosted={(result) => {
                if (
                    invoiceExecutionIsComplete(
                        result.allocatedTotal,
                        receivable.openInvoiceableTotal,
                    )
                ) {
                    onTaskCompleted?.(item.workItemId)
                }
            }}
            canOperate
            workItemId={item.workItemId}
            expectedTaskVersion={item.taskVersion}
            taskReceivableAccountId={item.businessObjectId}
            embedded
            hideSessionClose
            closeLabel="继续处理"
        />
    )
}
