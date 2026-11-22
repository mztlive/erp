"use client"

import * as React from "react"
import Link from "next/link"
import {
    ChevronDownIcon,
    FilePenLineIcon,
    PlusIcon,
    SendIcon,
    Trash2Icon,
} from "lucide-react"

import {
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspaceMeta,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Popover,
    PopoverContent,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { PurchaseOrderCancelApprovalButton } from "@/features/purchase-orders/components/purchase-order-cancel-approval-button"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"
import { isPurchaseOrderApprovalInProgress } from "@/features/purchase-orders/lib/purchase-order-approval"
import {
    purchaseOrderDetailHref,
    resolvePurchaseOrderBackContext,
    type PurchaseOrderDetailMode,
} from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import {
    PURCHASE_TYPE_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"
import { formatDateTime } from "@/lib/datetime"
import { useRouter, useSearchParams } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import { ExpensePurchaseStartDialog } from "./expense-purchase-start-dialog"
import { PurchaseOrderCorrespondence } from "./purchase-order-correspondence"

const MENU_ITEM_CLASS =
    "w-full justify-start font-normal text-foreground/85 shadow-none hover:bg-muted/80 hover:text-foreground"
const MENU_DANGER_ITEM_CLASS =
    "w-full justify-start font-normal text-muted-foreground shadow-none hover:bg-destructive-soft/75 hover:text-destructive-soft-foreground"

export function PurchaseOrderDetailHeader({
    order,
    mode,
    displayNo,
    modeLabel,
    titleRef,
    router,
    canEdit,
    canSubmit,
    canVoid,
    canChange,
    continueChange,
    changePending,
    editDirty = false,
    draftEditToken = null,
    savePending = false,
    requestLeave,
    onExitEdit,
    onSaveDraft,
    onRequestVoid,
    onRequestChange,
    onRequestSubmit,
    onCancelResult,
    tabs,
}: {
    order: PurchaseOrderCenterView
    mode: PurchaseOrderDetailMode
    displayNo: string
    modeLabel: string
    titleRef: React.RefObject<HTMLHeadingElement | null>
    router: ReturnType<typeof useRouter>
    canEdit: boolean
    canSubmit: boolean
    canVoid: boolean
    canChange: boolean
    /** 已有可编辑的改单草稿，入口显示「继续改单」。 */
    continueChange: boolean
    changePending: boolean
    editDirty?: boolean
    draftEditToken?: string | null
    savePending?: boolean
    requestLeave: (go: () => void) => void
    onExitEdit?: () => void
    onSaveDraft?: () => void
    onRequestVoid: () => void
    onRequestChange: () => void
    onRequestSubmit: () => void
    onCancelResult: (result: PurchaseOrderDetailResult) => void
    tabs?: React.ReactNode
}) {
    const [expenseOpen, setExpenseOpen] = React.useState(false)
    const [moreOpen, setMoreOpen] = React.useState(false)
    const searchParams = useSearchParams()
    const backContext = resolvePurchaseOrderBackContext(searchParams)
    const editHref = purchaseOrderDetailHref(
        order.identity.purchaseOrderId,
        searchParams,
        { mode: "edit" },
    )
    const profile = useAccountProfileQuery()
    const canCreateExpense =
        !profile.isError &&
        hasPermission(profile.data?.permissions, "purchase_order:create") &&
        mode !== "edit" &&
        order.identity.status !== "VOID"
    const hasCancelApproval =
        isPurchaseOrderApprovalInProgress(order) &&
        hasPermission(
            profile.data?.permissions,
            "purchase_order:cancel_approval",
        )
    const hasDestructiveMenuActions =
        mode !== "edit" && (hasCancelApproval || canVoid)

    const isEditing = mode === "edit"
    const changeIsPrimary = !isEditing && continueChange && !canSubmit
    const navigationMetaParts = [
        backContext.sourceHint,
        isEditing ? modeLabel : null,
    ].filter(Boolean)

    return (
        <>
            <ExpensePurchaseStartDialog
                open={expenseOpen}
                onOpenChange={setExpenseOpen}
                salesOrderId={order.header.salesOrderId}
                supplierId={order.header.supplierId}
                onCreated={(created) =>
                    router.push(
                        `/procurement/orders/${created.purchaseOrderId}`,
                    )
                }
            />
            <DetailWorkspaceChrome
                back={{
                    id: "procurement-orders-detail-back",
                    label: backContext.label,
                    visibility: backContext.hasReturnTo
                        ? "always"
                        : "standalone-only",
                    onClick: () =>
                        requestLeave(() => router.push(backContext.href)),
                }}
                navigationMeta={
                    navigationMetaParts.length > 0
                        ? navigationMetaParts.join(" · ")
                        : undefined
                }
                headingProps={{
                    id: "procurement-orders-detail-heading",
                    ref: titleRef,
                    tabIndex: -1,
                    className: "outline-none",
                }}
                title={order.header.supplierSnapshot || "采购单"}
                documentNumber={displayNo}
                version={
                    order.identity.revisionNo != null
                        ? `v${order.identity.revisionNo}`
                        : undefined
                }
                primaryStatus={{
                    label: order.identity.statusLabel,
                    tone: order.identity.statusTone,
                }}
                titleExtra={
                    <>
                        <Badge variant="secondary" className="font-normal">
                            {PURCHASE_TYPE_LABEL[order.header.purchaseType]}
                        </Badge>
                        {isEditing && editDirty ? (
                            <DetailWorkspaceDirtyBadge />
                        ) : null}
                        {!isEditing && order.activeChangeOrder ? (
                            <DetailWorkspaceDirtyBadge label="改单中" />
                        ) : null}
                    </>
                }
                meta={
                    <DetailWorkspaceMeta
                        items={[
                            order.identity.revisionNo == null
                                ? "尚未生效"
                                : null,
                            order.header.ownerName ? (
                                <span key="owner">
                                    采购负责人{" "}
                                    <span className="font-medium text-foreground">
                                        {order.header.ownerName}
                                    </span>
                                </span>
                            ) : null,
                            order.header.salesOrderNo ? (
                                <span key="sales" className="num">
                                    来源销售{" "}
                                    <Link
                                        id="procurement-orders-detail-header-sales-order"
                                        href={`/sales/orders/${encodeURIComponent(order.header.salesOrderId)}`}
                                        className="font-medium text-foreground underline-offset-4 hover:text-primary hover:underline"
                                    >
                                        {order.header.salesOrderNo}
                                    </Link>
                                </span>
                            ) : null,
                            order.header.createdAt ? (
                                <span key="created" className="num">
                                    创建于{" "}
                                    {formatDateTime(
                                        order.header.createdAt,
                                        "default",
                                    )}
                                </span>
                            ) : null,
                        ]}
                    />
                }
                secondaryActions={
                    isEditing ? (
                        <>
                            {onExitEdit ? (
                                <Button
                                    id="procurement-orders-detail-exit-edit"
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className={
                                        detailWorkspaceStyles.quietButton
                                    }
                                    disabled={savePending}
                                    onClick={onExitEdit}
                                >
                                    退出编辑
                                </Button>
                            ) : null}
                            <LoadingButton
                                id={`procurement-orders-detail-edit-save-${order.identity.purchaseOrderId}`}
                                loading={savePending}
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.subtleButton}
                                disabled={!draftEditToken || savePending}
                                onClick={onSaveDraft}
                            >
                                {savePending ? "保存中…" : "保存草稿"}
                            </LoadingButton>
                        </>
                    ) : (
                        <>
                            <Popover open={moreOpen} onOpenChange={setMoreOpen}>
                                <PopoverTrigger
                                    id="procurement-orders-detail-more-actions"
                                    render={
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            className={
                                                detailWorkspaceStyles.quietButton
                                            }
                                        />
                                    }
                                >
                                    更多
                                    <ChevronDownIcon
                                        aria-hidden="true"
                                        data-icon="inline-end"
                                    />
                                </PopoverTrigger>
                                <PopoverContent
                                    align="end"
                                    keepMounted
                                    className="w-48 gap-1.5 p-2"
                                >
                                    <PopoverTitle className="px-2 pt-1 text-xs font-medium text-muted-foreground">
                                        采购单操作
                                    </PopoverTitle>
                                    <div className="flex flex-col gap-0.5">
                                        <PurchaseOrderCorrespondence
                                            order={order}
                                            menuItem
                                            onActionSelect={() =>
                                                setMoreOpen(false)
                                            }
                                        />
                                        {canCreateExpense ? (
                                            <Button
                                                id="procurement-orders-detail-add-expense"
                                                type="button"
                                                size="sm"
                                                variant="ghost"
                                                className={MENU_ITEM_CLASS}
                                                onClick={() => {
                                                    setMoreOpen(false)
                                                    setExpenseOpen(true)
                                                }}
                                            >
                                                <PlusIcon
                                                    data-icon="inline-start"
                                                    aria-hidden="true"
                                                />
                                                追加费用
                                            </Button>
                                        ) : null}
                                        {hasDestructiveMenuActions ? (
                                            <hr className="my-1 border-0 border-t border-border/60" />
                                        ) : null}
                                        <PurchaseOrderCancelApprovalButton
                                            order={order}
                                            onResult={onCancelResult}
                                            onTriggerClick={() =>
                                                setMoreOpen(false)
                                            }
                                            triggerVariant="ghost"
                                            triggerClassName={
                                                MENU_DANGER_ITEM_CLASS
                                            }
                                        />
                                        {canVoid ? (
                                            <Button
                                                id="procurement-orders-detail-void"
                                                type="button"
                                                size="sm"
                                                variant="ghost"
                                                className={
                                                    MENU_DANGER_ITEM_CLASS
                                                }
                                                onClick={() => {
                                                    setMoreOpen(false)
                                                    onRequestVoid()
                                                }}
                                            >
                                                <Trash2Icon
                                                    data-icon="inline-start"
                                                    aria-hidden="true"
                                                />
                                                作废草稿
                                            </Button>
                                        ) : null}
                                    </div>
                                </PopoverContent>
                            </Popover>
                            {canEdit ? (
                                <Button
                                    id="procurement-orders-detail-edit"
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className={
                                        detailWorkspaceStyles.subtleButton
                                    }
                                    onClick={() => router.push(editHref)}
                                >
                                    <FilePenLineIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    编辑草稿
                                </Button>
                            ) : null}
                            {(canChange || continueChange) &&
                            !changeIsPrimary ? (
                                <Button
                                    id="procurement-orders-detail-change"
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className={
                                        detailWorkspaceStyles.subtleButton
                                    }
                                    disabled={changePending}
                                    onClick={onRequestChange}
                                >
                                    <FilePenLineIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    {changePending
                                        ? "正在打开改单…"
                                        : continueChange
                                          ? "继续改单"
                                          : "发起改单"}
                                </Button>
                            ) : null}
                        </>
                    )
                }
                primaryAction={
                    isEditing ? (
                        <Button
                            id={`procurement-orders-detail-edit-submit-${order.identity.purchaseOrderId}`}
                            type="button"
                            size="sm"
                            className="px-3.5"
                            disabled={
                                !draftEditToken || !canSubmit || savePending
                            }
                            onClick={onRequestSubmit}
                        >
                            <SendIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            提交审批
                        </Button>
                    ) : canSubmit ? (
                        <Button
                            id="procurement-orders-detail-submit"
                            type="button"
                            size="sm"
                            className="px-3.5"
                            onClick={onRequestSubmit}
                        >
                            <SendIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            提交审批
                        </Button>
                    ) : changeIsPrimary ? (
                        <Button
                            id="procurement-orders-detail-change"
                            type="button"
                            size="sm"
                            className="px-3.5"
                            disabled={changePending}
                            onClick={onRequestChange}
                        >
                            <FilePenLineIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {changePending ? "正在打开改单…" : "继续改单"}
                        </Button>
                    ) : null
                }
                tabs={tabs}
            />
        </>
    )
}

/** 置于吸顶头下方的通栏无框金额摘要带：整页唯一的大号 KPI 金额汇总，随页面自然滚走。 */
export function PurchaseOrderAmountSummary({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    const amount = (value?: string) =>
        costMasked ? (
            <span className="num text-muted-foreground" aria-label="金额已隐藏">
                •••
            </span>
        ) : value == null ? (
            <span className="text-muted-foreground">—</span>
        ) : (
            <MoneyValue value={value} />
        )

    return (
        <DetailWorkspaceMetrics
            columns={4}
            variant="bar"
            ariaLabel="采购单金额摘要"
        >
            <DetailWorkspaceMetricItem
                emphasis="primary"
                label="采购金额（含税）"
                value={amount(order.currentContent.totals.gross)}
                detail={
                    costMasked ? (
                        "含税/税额已按权限隐藏"
                    ) : (
                        <>
                            <span>
                                不含税{" "}
                                <MoneyValue
                                    value={order.currentContent.totals.net}
                                />
                            </span>
                            <span
                                aria-hidden="true"
                                className="select-none text-muted-foreground/40"
                            >
                                ·
                            </span>
                            <span>
                                税额{" "}
                                <MoneyValue
                                    value={order.currentContent.totals.tax}
                                />
                            </span>
                        </>
                    )
                }
            />
            <DetailWorkspaceMetricItem
                label="已付并核销"
                value={amount(order.payableSummary?.paidAllocatedAmount)}
            />
            <DetailWorkspaceMetricItem
                label="待付余额"
                value={amount(order.payableSummary?.payableOpenAmount)}
                detail={
                    !order.payableSummary ? "审批通过后形成应付" : undefined
                }
            />
            <DetailWorkspaceMetricItem
                label="已收票并核销"
                value={amount(
                    order.payableSummary?.purchaseInvoiceAllocatedAmount,
                )}
            />
        </DetailWorkspaceMetrics>
    )
}
