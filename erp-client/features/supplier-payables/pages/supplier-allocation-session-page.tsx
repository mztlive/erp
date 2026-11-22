"use client"

import { ArrowLeftIcon, LockKeyholeIcon, SaveIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataFreshness,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    PageActions,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { SupplierAllocationWorkspace } from "@/features/supplier-payables/components/allocation-workspace"
import { PaymentRecipientCard } from "@/features/supplier-payables/components/payment-recipient-card"
import { useAllocationSession } from "@/features/supplier-payables/hooks/use-allocation-session"
import type {
    AllocationTrack,
    FormalSubmitResult,
    PaymentRecipient,
} from "@/features/supplier-payables/types"

export type SupplierAllocationSessionPageProps = {
    track: AllocationTrack
    supplierId: string
    draftSessionId?: string
    purchaseOrderId?: string
    returnTo?: string
    fromWorkspace?: string
    existingInvoiceId?: string
    preselectPayableAccountId?: string
    paymentWorkItemId?: string
    expectedPaymentTaskVersion?: string
    paymentPayableAccountId?: string
    paymentRecipient?: PaymentRecipient
    paymentTaskPending?: boolean
    onClose: () => void
    onCompleted?: (result: FormalSubmitResult) => void
    onGoToInvoiceView?: () => void
    onDraftSessionIdChange?: (draftSessionId: string) => void
}

/**
 * 供应商付款核销会话骨架。进项发票登记由独立路由 SupplierInvoiceRegistrationPage 承接。
 */
export function SupplierAllocationSessionPage({
    track,
    supplierId,
    draftSessionId,
    purchaseOrderId,
    returnTo,
    fromWorkspace,
    existingInvoiceId,
    preselectPayableAccountId,
    paymentWorkItemId,
    expectedPaymentTaskVersion,
    paymentPayableAccountId,
    paymentRecipient,
    paymentTaskPending = false,
    onClose,
    onCompleted,
    onGoToInvoiceView,
    onDraftSessionIdChange,
}: SupplierAllocationSessionPageProps) {
    const sessionState = useAllocationSession(
        {
            track,
            supplierId,
            draftSessionId,
            purchaseOrderId,
            returnTo,
            fromWorkspace,
            existingInvoiceId,
            preselectPayableAccountId,
            paymentWorkItemId,
            expectedPaymentTaskVersion,
            paymentPayableAccountId,
            paymentRecipientBankAccountId: paymentRecipient?.bankAccountId,
            paymentRecipientBankAccountVersion: paymentRecipient?.version,
        },
        { onCompleted, onDraftSessionIdChange },
    )
    const {
        sessionQuery,
        session,
        result,
        draftHint,
        isSavingDraft,
        handleSaveDraft,
    } = sessionState

    const title = track === "payment" ? "登记付款" : "登记进项发票"
    const trackLabel = track === "payment" ? "付款核销" : "进项票核销"

    const header = (
        <DetailWorkspaceChrome
            title={title}
            primaryStatus={{ label: trackLabel, tone: "info" }}
            meta={
                session ? (
                    <span className="inline-flex flex-wrap items-center gap-x-2.5 gap-y-1">
                        <span className="inline-flex items-center gap-1.5 text-foreground">
                            <span className="text-muted-foreground">
                                供应商
                            </span>
                            <LockKeyholeIcon
                                className="size-3.5 shrink-0 text-muted-foreground"
                                aria-hidden="true"
                            />
                            <span className="font-medium">
                                {session.supplierName}
                            </span>
                        </span>
                        {session.existingDocumentNo ? (
                            <span className="num text-muted-foreground">
                                · 继续核销 {session.existingDocumentNo}
                            </span>
                        ) : null}
                        {draftHint ? (
                            <span className="text-muted-foreground">
                                · {draftHint}
                            </span>
                        ) : null}
                        <DataFreshness
                            updatedAt={
                                sessionQuery.isError
                                    ? "查询失败"
                                    : session.queriedAt.slice(11, 16)
                            }
                            dateTime={session.queriedAt}
                            state={
                                sessionQuery.isError
                                    ? "failed"
                                    : sessionQuery.isFetching
                                      ? "syncing"
                                      : "fresh"
                            }
                        />
                    </span>
                ) : sessionQuery.isError ? (
                    "本次核销未能加载。"
                ) : (
                    "正在加载本次核销…"
                )
            }
            secondaryActions={
                <PageActions
                    size="sm"
                    actions={[
                        {
                            actionKey: "back",
                            id: "supplier-payables-session-page-back",
                            label: "返回列表",
                            icon: ArrowLeftIcon,
                            variant: "ghost",
                            onClick: onClose,
                        },
                        {
                            actionKey: "save-draft",
                            id: "supplier-payables-session-page-save-draft",
                            loading: isSavingDraft,
                            label: "保存草稿",
                            icon: SaveIcon,
                            variant: "outline",
                            disabled:
                                !session || isSavingDraft || Boolean(result),
                            onClick: () => void handleSaveDraft(),
                        },
                    ]}
                />
            }
        />
    )

    if (sessionQuery.isPending) {
        return (
            <DetailWorkspacePage>
                {header}
                <div className="grid gap-6 px-(--scaffold-px) py-6 lg:grid-cols-[380px_minmax(0,1fr)]">
                    <div className="h-72 animate-pulse rounded-lg bg-muted" />
                    <div className="h-72 animate-pulse rounded-lg bg-muted" />
                </div>
            </DetailWorkspacePage>
        )
    }

    if (sessionQuery.isError) {
        return (
            <DetailWorkspacePage>
                {header}
                <div className="mx-auto w-full max-w-3xl px-(--scaffold-px) py-8">
                    <BusinessFailureState
                        title="无法开始本次核销"
                        error={sessionQuery.error}
                        action={
                            <div className="flex flex-wrap gap-2">
                                <Button
                                    id="supplier-payables-session-page-retry"
                                    type="button"
                                    variant="outline"
                                    onClick={() => void sessionQuery.refetch()}
                                >
                                    重试
                                </Button>
                                <Button
                                    id="supplier-payables-session-page-error-back"
                                    type="button"
                                    variant="outline"
                                    onClick={onClose}
                                >
                                    返回列表
                                </Button>
                            </div>
                        }
                    />
                </div>
            </DetailWorkspacePage>
        )
    }

    if (!session) {
        return (
            <DetailWorkspacePage>
                {header}
                <div className="mx-auto w-full max-w-3xl px-(--scaffold-px) py-8">
                    <BusinessEmptyState
                        kind="no-data"
                        title="没有可核销内容"
                        description="请返回列表重新选择供应商往来。"
                        action={
                            <Button
                                id="supplier-payables-session-page-empty-back"
                                type="button"
                                variant="outline"
                                onClick={onClose}
                            >
                                返回列表
                            </Button>
                        }
                    />
                </div>
            </DetailWorkspacePage>
        )
    }

    if (track === "payment" && paymentTaskPending) {
        return (
            <DetailWorkspacePage>
                {header}
                <div className="px-(--scaffold-px) py-6">
                    <div className="h-72 animate-pulse rounded-lg bg-muted" />
                </div>
            </DetailWorkspacePage>
        )
    }

    if (
        track === "payment" &&
        (!paymentWorkItemId ||
            !expectedPaymentTaskVersion ||
            !paymentPayableAccountId ||
            !paymentRecipient)
    ) {
        return (
            <DetailWorkspacePage>
                {header}
                <div className="mx-auto w-full max-w-3xl px-(--scaffold-px) py-8">
                    <BusinessEmptyState
                        kind="no-scope"
                        title={
                            paymentWorkItemId && !paymentRecipient
                                ? "供应商未配置可用收款账户"
                                : "请从付款任务进入"
                        }
                        description={
                            paymentWorkItemId && !paymentRecipient
                                ? "当前付款任务不能执行。请先在供应商主数据中维护唯一的当前默认收款账户，再刷新任务。"
                                : "付款执行已按负责人分派。请回到工作台打开分配给你的供应商付款任务；普通列表入口只能查看，不能提交付款。"
                        }
                        action={
                            <Button
                                id="supplier-payables-session-page-scope-back"
                                type="button"
                                variant="outline"
                                onClick={onClose}
                            >
                                返回列表
                            </Button>
                        }
                    />
                </div>
            </DetailWorkspacePage>
        )
    }

    return (
        <DetailWorkspacePage>
            {header}
            <div className="flex min-w-0 flex-1 flex-col gap-6 px-(--scaffold-px) py-6">
                {track === "payment" &&
                paymentRecipient &&
                paymentWorkItemId &&
                expectedPaymentTaskVersion &&
                paymentPayableAccountId ? (
                    <PaymentRecipientCard
                        key={`${paymentWorkItemId}:${paymentRecipient.bankAccountId}:${paymentRecipient.version}`}
                        payableAccountId={paymentPayableAccountId}
                        workItemId={paymentWorkItemId}
                        expectedTaskVersion={expectedPaymentTaskVersion}
                        recipient={paymentRecipient}
                    />
                ) : null}
                <SupplierAllocationWorkspace
                    state={sessionState}
                    track={track}
                    fromWorkspace={fromWorkspace}
                    purchaseOrderId={purchaseOrderId}
                    returnTo={returnTo}
                    paymentRecipient={paymentRecipient}
                    onClose={onClose}
                    onGoToInvoiceView={onGoToInvoiceView}
                />
            </div>
        </DetailWorkspacePage>
    )
}
