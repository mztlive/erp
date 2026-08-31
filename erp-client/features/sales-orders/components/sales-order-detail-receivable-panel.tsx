"use client"

import Link from "next/link"
import { FileTextIcon, LoaderCircleIcon, WalletIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DocumentSection,
    MoneyValue,
    RelatedDocumentList,
    surfaceInsetClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { CustomerAccountDetailPreview } from "@/features/customer-receivables/components/customer-account-detail-preview"
import { AllocationSessionScreen } from "@/features/customer-receivables/pages/components/allocation-session-screen"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { SectionLead } from "@/features/sales-orders/components/sales-order-detail-lifecycle-rail"
import { useSalesOrderReceivable } from "@/features/sales-orders/hooks/use-sales-order-receivable"
import { customerAccountsForOrderHref } from "@/features/sales-orders/lib/sales-order-detail-model"
import {
    mapOrderInvoices,
    mapOrderReceipts,
    mapOrderReceivableAccounts,
    type OrderReceivableDocument,
} from "@/features/sales-orders/lib/sales-order-receivable"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

function PreviewButton({
    id,
    onClick,
    label = "查看",
}: {
    id?: string
    onClick: () => void
    label?: string
}) {
    return (
        <Button
            id={id}
            type="button"
            size="sm"
            variant="secondary"
            onClick={onClick}
        >
            {label}
        </Button>
    )
}

export function ReceivablePanel({
    order,
    selfReturn,
    onDataChanged,
}: {
    order: SalesOrderDetailView
    selfReturn: string
    onDataChanged: () => void
}) {
    const receivable = useSalesOrderReceivable(order, onDataChanged)
    const {
        permissions,
        listQuery,
        data,
        accounts,
        receipts,
        invoices,
        targetIds,
        remaining,
        sessionId,
        sessionQuery,
        preview,
        detailQuery,
        actionError,
        createPending,
        canRegister,
        openPreview,
        closePreview,
        closeSession,
        startSession,
        openRegister,
        handlePosted,
    } = receivable

    if (sessionId) {
        return (
            <AllocationSessionScreen
                isPending={sessionQuery.isPending}
                session={sessionQuery.data}
                onBackToList={closeSession}
                onClose={closeSession}
                onPosted={handlePosted}
                canOperate={
                    sessionQuery.data
                        ? permissions.canStartSession(sessionQuery.data.mode)
                        : false
                }
                permissionReason={permissions.reason}
                embedded
            />
        )
    }

    const registerDisabledReason = !permissions.canRegisterReceipt
        ? permissions.reason
        : canRegister
          ? undefined
          : "当前销售单缺少结算主体，无法登记票款。"
    const invoiceDisabledReason = !permissions.canRegisterInvoice
        ? permissions.reason
        : canRegister
          ? undefined
          : "当前销售单缺少结算主体，无法登记票款。"

    return (
        <div className="space-y-4">
            <SectionLead>
                只看本单应收、核到本单的回款和发票。客户级往来、退款、冲正请到财务工作台处理。
            </SectionLead>

            <div
                className={cn(
                    surfaceInsetClassName,
                    "flex flex-col gap-3 px-3 py-3",
                )}
            >
                <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0">
                        <h3 className="text-sm font-medium">本单回款与开票</h3>
                        <p className="mt-1 text-xs text-muted-foreground">
                            回款 {order.collection.label} · 开票{" "}
                            {order.invoicing.label}
                            {order.closeEligibility.receivableSettled
                                ? " · 应收已结清"
                                : " · 应收尚未结清"}
                            。开票不挡结案。
                        </p>
                    </div>
                    <div className="flex flex-wrap justify-end gap-2">
                        <Button
                            id="sales-orders-detail-receivable-register-invoice"
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={
                                createPending ||
                                !permissions.canRegisterInvoice ||
                                !canRegister
                            }
                            title={invoiceDisabledReason}
                            data-testid="sales-order-register-invoice"
                            onClick={() => openRegister("invoice")}
                        >
                            {createPending ? (
                                <LoaderCircleIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                    className="animate-spin"
                                />
                            ) : (
                                <FileTextIcon data-icon="inline-start" />
                            )}
                            {createPending ? "创建中…" : "登记销项发票"}
                        </Button>
                        <Button
                            id="sales-orders-detail-receivable-register-receipt"
                            type="button"
                            size="sm"
                            disabled={
                                createPending ||
                                !permissions.canRegisterReceipt ||
                                !canRegister
                            }
                            title={registerDisabledReason}
                            data-testid="sales-order-register-receipt"
                            onClick={() => openRegister("receipt")}
                        >
                            {createPending ? (
                                <LoaderCircleIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                    className="animate-spin"
                                />
                            ) : (
                                <WalletIcon data-icon="inline-start" />
                            )}
                            {createPending ? "创建中…" : "登记回款"}
                        </Button>
                    </div>
                </div>
                <dl
                    className="grid grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-4"
                    aria-label="本单票款摘要"
                >
                    <div className="min-w-0">
                        <dt className="text-xs text-muted-foreground">
                            已回款
                        </dt>
                        <dd className="mt-0.5 text-sm font-medium">
                            <MoneyValue value={order.receivedAmount} />
                        </dd>
                    </div>
                    <div className="min-w-0">
                        <dt className="text-xs text-muted-foreground">
                            待回款
                        </dt>
                        <dd className="mt-0.5 text-sm font-medium">
                            <MoneyValue value={remaining} />
                        </dd>
                    </div>
                    <div className="min-w-0">
                        <dt className="text-xs text-muted-foreground">
                            已开票
                        </dt>
                        <dd className="mt-0.5 text-sm font-medium">
                            <MoneyValue value={order.invoicedAmount} />
                        </dd>
                    </div>
                    <div className="min-w-0">
                        <dt className="text-xs text-muted-foreground">
                            关联单据
                        </dt>
                        <dd className="num mt-0.5 text-sm font-medium">
                            回款 {order.related.receipts} 笔 · 发票{" "}
                            {order.related.invoices} 笔
                        </dd>
                    </div>
                </dl>
            </div>

            {actionError ? (
                <Alert variant="destructive">
                    <AlertTitle>操作未成功</AlertTitle>
                    <AlertDescription>{actionError}</AlertDescription>
                </Alert>
            ) : null}

            {listQuery.isError ? (
                <BusinessFailureState
                    id="sales-orders-detail-receivable-retry"
                    title="本单票款加载失败"
                    error={listQuery.error}
                    onRetry={() => {
                        void listQuery.refetch()
                    }}
                />
            ) : data && !data.moduleAllowed ? (
                <BusinessFailureState
                    kind="permission"
                    description="无客户往来模块权限或权限已收回。"
                />
            ) : data && !data.hasDataScope ? (
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色未配置客户往来范围"
                    description="不得用 0 元假装无应收。请申请财务数据范围。"
                />
            ) : listQuery.isPending ? (
                <div
                    className="h-48 animate-pulse rounded-lg bg-muted"
                    aria-busy="true"
                    aria-label="加载本单票款"
                />
            ) : (
                <>
                    <DocumentSection
                        title="本单应收"
                        description={
                            accounts.length === 0
                                ? "销售单生效后才会形成应收子账。"
                                : accounts.length === 1
                                  ? uniqueAccountReview(
                                        accounts[0]?.reviewStatusLabel,
                                    )
                                  : undefined
                        }
                    >
                        <RelatedDocumentList
                            documents={mapOrderReceivableAccounts(accounts).map(
                                (document) => ({
                                    ...toRelatedDocument(document),
                                    openAction: (
                                        <PreviewButton
                                            id={`sales-orders-detail-receivable-${toAutomationIdSegment(document.id)}-preview`}
                                            onClick={() =>
                                                openPreview({
                                                    kind: "receivable",
                                                    id: document.id,
                                                })
                                            }
                                        />
                                    ),
                                }),
                            )}
                            emptyContent="本单还没有应收子账。"
                        />
                    </DocumentSection>

                    <DocumentSection
                        title="核到本单的回款"
                        description="只列出已经核到本单应收的回款，不含该客户其他订单。"
                    >
                        <RelatedDocumentList
                            documents={mapOrderReceipts(
                                receipts,
                                targetIds,
                            ).map((document) => ({
                                ...toRelatedDocument(document),
                                openAction: (
                                    <PreviewButton
                                        id={`sales-orders-detail-receipt-${toAutomationIdSegment(document.id)}-preview`}
                                        onClick={() =>
                                            openPreview({
                                                kind: "receipt",
                                                id: document.id,
                                            })
                                        }
                                    />
                                ),
                            }))}
                            emptyContent="还没有核到本单的回款。"
                        />
                    </DocumentSection>

                    <DocumentSection
                        title="核到本单的发票"
                        description="只列出已经核到本单应收的销项发票。"
                    >
                        <RelatedDocumentList
                            documents={mapOrderInvoices(
                                invoices,
                                targetIds,
                            ).map((document) => ({
                                ...toRelatedDocument(document),
                                openAction: (
                                    <PreviewButton
                                        id={`sales-orders-detail-invoice-${toAutomationIdSegment(document.id)}-preview`}
                                        onClick={() =>
                                            openPreview({
                                                kind: "invoice",
                                                id: document.id,
                                            })
                                        }
                                    />
                                ),
                            }))}
                            emptyContent="还没有核到本单的销项发票。"
                        />
                    </DocumentSection>
                </>
            )}

            <p className="text-xs text-muted-foreground">
                退款、冲正、跨单核销和待核销池在财务工作台。
                <Button
                    id="sales-orders-detail-open-customer-accounts"
                    type="button"
                    size="xs"
                    variant="link"
                    className="h-auto px-1"
                    data-testid="sales-order-open-customer-accounts"
                    render={
                        <Link
                            href={customerAccountsForOrderHref(
                                order,
                                selfReturn,
                            )}
                        />
                    }
                >
                    打开客户往来
                </Button>
            </p>

            <CustomerAccountDetailPreview
                open={preview != null}
                data={detailQuery.data}
                isPending={detailQuery.isPending}
                isError={detailQuery.isError}
                error={detailQuery.error}
                onRetry={() => void detailQuery.refetch()}
                onClose={closePreview}
                onStartSession={startSession}
                canStartSession={permissions.canStartSession}
                startSessionPending={createPending}
                canRequestReverse={() => false}
                canSubmitRefund={false}
                canSubmitReversal={false}
                permissionReason={permissions.reason}
                showCorrectionActions={false}
                onRequestReverse={() => undefined}
            />
        </div>
    )
}

function uniqueAccountReview(reviewStatusLabel?: string) {
    if (!reviewStatusLabel || reviewStatusLabel === "不适用") {
        return undefined
    }
    return `票款复核：${reviewStatusLabel}`
}

function toRelatedDocument(document: OrderReceivableDocument) {
    return {
        id: document.id,
        documentType: document.documentType,
        documentNumber: document.documentNumber,
        status: {
            label: document.statusLabel,
            tone: document.statusTone,
        },
        measure: {
            kind: "amount" as const,
            value: <MoneyValue value={document.amount} />,
            label: document.amountLabel,
        },
        owner: document.owner,
    }
}
