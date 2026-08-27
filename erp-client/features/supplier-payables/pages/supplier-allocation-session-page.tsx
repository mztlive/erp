"use client"

import { ArrowLeftIcon, SaveIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataFreshness,
    PageActions,
    PageHeader,
    PageScaffold,
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
 * 供应商付款/进项票核销作业页。与往来列表共用路由，但是独立页面骨架：
 * 页头、浮起工作面、提交结果均按作业页渲染，而不是列表内嵌组件。
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
        <PageHeader
            title={title}
            status={{ label: trackLabel, tone: "info" }}
            description={
                session
                    ? [
                          `供应商 ${session.supplierName} 已锁定；核销池仅含该供应商开放目标。采购单与结算单可混合，不同供应商禁止混入。`,
                          session.existingDocumentNo
                              ? `继续核销 ${session.existingDocumentNo}。`
                              : null,
                          draftHint ? `${draftHint}（不形成业务记录）` : null,
                      ]
                          .filter(Boolean)
                          .join(" ")
                    : sessionQuery.isError
                      ? "本次核销未能加载。"
                      : "正在加载本次核销…"
            }
            metadata={
                session ? (
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
                ) : null
            }
            actions={
                <PageActions
                    actions={[
                        {
                            actionKey: "back",
                            label: "返回列表",
                            icon: ArrowLeftIcon,
                            variant: "outline",
                            onClick: onClose,
                        },
                        {
                            actionKey: "save-draft",
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
            <PageScaffold density="compact">
                {header}
                <div className="grid gap-4 lg:grid-cols-2">
                    <div className="h-72 animate-pulse rounded-lg bg-muted" />
                    <div className="h-72 animate-pulse rounded-lg bg-muted" />
                </div>
            </PageScaffold>
        )
    }

    if (sessionQuery.isError) {
        return (
            <PageScaffold density="compact">
                {header}
                <BusinessFailureState
                    title="无法开始本次核销"
                    error={sessionQuery.error}
                    onRetry={() => void sessionQuery.refetch()}
                    action={
                        <Button
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (!session) {
        return (
            <PageScaffold density="compact">
                {header}
                <BusinessEmptyState
                    kind="no-data"
                    title="没有可核销内容"
                    description="请返回列表重新选择供应商往来。"
                    action={
                        <Button
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (track === "payment" && paymentTaskPending) {
        return (
            <PageScaffold density="compact">
                {header}
                <div className="h-72 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
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
            <PageScaffold density="compact">
                {header}
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
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold density="compact">
            {header}
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
        </PageScaffold>
    )
}
