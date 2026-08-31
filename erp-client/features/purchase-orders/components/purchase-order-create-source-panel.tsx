"use client"

import * as React from "react"
import type { ReactNode } from "react"
import {
    ArrowLeftRightIcon,
    ArrowUpRightIcon,
    FileTextIcon,
} from "lucide-react"

import {
    MoneyValue,
    surfaceClassName,
    surfaceInsetClassName,
    workspaceTaskSurfacePadClassName,
} from "@/components/business"
import { Button, buttonVariants } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { PurchaseOrderCreateSourcePickerDialog } from "@/features/purchase-orders/components/purchase-order-create-source-picker-dialog"
import {
    summarizeSourcingOrder,
    type SourcingOrderSummary,
    type SourcingSalesOrder,
} from "@/features/purchase-orders/lib/purchase-order-create-model"
import { PURCHASE_TYPE_LABEL } from "@/features/purchase-orders/types"
import { cn } from "@/lib/utils"

export type PurchaseOrderCreateSourcePanelProps = {
    workspace: readonly SourcingSalesOrder[]
    selectedSalesOrderId: string
    selectedOrder?: SourcingSalesOrder
    disabled?: boolean
    /** 嵌入工作台时去掉卡片壳，只保留分区线，避免再套一层表面。 */
    flat?: boolean
    /** 打开销售单后返回的地址；工作台嵌入时带回当前队列。 */
    salesOrderReturnTo?: string
    /** 预览原始销售单；由上层按销售单身份拉详情，不得用供给投影凑纸。 */
    onPreviewSalesOrder?: (salesOrderId: string, title: string) => void
    onSalesOrderChange: (salesOrderId: string) => void
}

/** 打开销售单工作面；嵌入工作台时带回当前队列。 */
function salesOrderOpenHref(salesOrderId: string, returnTo?: string): string {
    const path = `/sales/orders/${encodeURIComponent(salesOrderId)}`
    if (!returnTo?.trim()) return path
    return `${path}?${new URLSearchParams({
        from: "workspace",
        returnTo: returnTo.trim(),
    }).toString()}`
}

function joinLabels(labels: readonly string[]): string {
    return labels.length > 0 ? labels.join("、") : "—"
}

/** 单号下方的往来、合同和销售负责人，空项不占位。 */
function identitySubtitle(order: SourcingSalesOrder): string {
    return [order.customerName, order.contractNumber, order.salesOwnerName]
        .map((value) => value?.trim())
        .filter(Boolean)
        .join(" · ")
}

/**
 * 来源销售单：先看单号与往来，再看分配数字，业务条件放在下面。
 */
export function PurchaseOrderCreateSourcePanel({
    workspace,
    selectedSalesOrderId,
    selectedOrder,
    disabled,
    flat = false,
    salesOrderReturnTo,
    onPreviewSalesOrder,
    onSalesOrderChange,
}: PurchaseOrderCreateSourcePanelProps) {
    const [pickerOpen, setPickerOpen] = React.useState(false)
    const summary = selectedOrder
        ? summarizeSourcingOrder(selectedOrder)
        : undefined
    const subtitle = selectedOrder ? identitySubtitle(selectedOrder) : ""
    const openHref = selectedOrder
        ? salesOrderOpenHref(selectedOrder.salesOrderId, salesOrderReturnTo)
        : undefined

    return (
        <section className={cn("overflow-hidden", surfaceClassName(flat))}>
            <div
                className={cn(
                    "flex flex-col gap-4",
                    flat
                        ? cn(workspaceTaskSurfacePadClassName, "py-5")
                        : "p-4 md:p-5",
                )}
            >
                <div className="flex items-start justify-between gap-3">
                    <div className="flex min-w-0 flex-col gap-1">
                        <h2 className="font-heading text-sm font-semibold">
                            来源销售单
                        </h2>
                        {selectedOrder ? (
                            <>
                                <p className="num text-xl font-semibold tracking-tight">
                                    {selectedOrder.salesOrderNo}
                                </p>
                                {subtitle ? (
                                    <p className="text-sm text-muted-foreground">
                                        {subtitle}
                                    </p>
                                ) : null}
                            </>
                        ) : null}
                    </div>
                    <div className="flex shrink-0 items-center gap-1">
                        {selectedOrder ? (
                            <>
                                {onPreviewSalesOrder ? (
                                    <IconActionButton
                                        id="procurement-orders-create-source-preview"
                                        label="预览销售单"
                                        testId="purchase-create-read-source"
                                        onClick={() =>
                                            onPreviewSalesOrder(
                                                selectedOrder.salesOrderId,
                                                selectedOrder.salesOrderNo,
                                            )
                                        }
                                    >
                                        <FileTextIcon aria-hidden="true" />
                                    </IconActionButton>
                                ) : null}
                                {openHref ? (
                                    <IconActionButton
                                        id="procurement-orders-create-source-open"
                                        label="打开销售单"
                                        testId="purchase-create-open-source"
                                        href={openHref}
                                    >
                                        <ArrowUpRightIcon aria-hidden="true" />
                                    </IconActionButton>
                                ) : null}
                            </>
                        ) : null}
                        {disabled ? null : (
                            <Button
                                id="procurement-orders-create-source-change"
                                type="button"
                                size="sm"
                                variant="outline"
                                data-testid="purchase-create-change-source"
                                onClick={() => setPickerOpen(true)}
                            >
                                <ArrowLeftRightIcon data-icon="inline-start" />
                                更换销售单
                            </Button>
                        )}
                    </div>
                </div>
                {summary ? (
                    <>
                        <SourceSnapshot summary={summary} />
                        <SourceTerms summary={summary} />
                    </>
                ) : null}
            </div>
            <PurchaseOrderCreateSourcePickerDialog
                open={pickerOpen}
                workspace={workspace}
                selectedSalesOrderId={selectedSalesOrderId}
                onOpenChange={setPickerOpen}
                onSelect={onSalesOrderChange}
            />
        </section>
    )
}

/** 标题栏图标动作：预览或跳转销售单。 */
function IconActionButton({
    id,
    label,
    testId,
    href,
    onClick,
    children,
}: {
    id?: string
    label: string
    testId: string
    href?: string
    onClick?: () => void
    children: ReactNode
}) {
    return (
        <Tooltip>
            <TooltipTrigger
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

/** 分配数字：推荐采购金额优先，行数和供应商家数随后。 */
function SourceSnapshot({ summary }: { summary: SourcingOrderSummary }) {
    return (
        <div
            className={cn(
                surfaceInsetClassName,
                "flex flex-wrap items-end gap-x-8 gap-y-3 px-4 py-4",
            )}
            aria-label="来源销售单摘要"
        >
            <div className="flex flex-col gap-1">
                <MoneyValue
                    value={summary.minEstimatedGross}
                    className="text-2xl font-semibold tracking-tight"
                />
                <span className="text-xs text-muted-foreground">
                    推荐采购估算
                </span>
            </div>
            <SnapshotStat
                label="待分配明细"
                value={`${summary.lineCount} 行`}
            />
            <SnapshotStat
                label="已供给覆盖"
                value={`${summary.coveredLineCount} 行`}
            />
            <SnapshotStat
                label="可选采购供应商"
                value={`${summary.uniqueSupplierCount} 家`}
            />
        </div>
    )
}

/** 摘要条里的次要数字。 */
function SnapshotStat({ label, value }: { label: string; value: string }) {
    return (
        <div className="flex flex-col gap-1">
            <span className="num text-lg font-medium">{value}</span>
            <span className="text-xs text-muted-foreground">{label}</span>
        </div>
    )
}

/** 采购类型和付款条件，两列排开避免和数字挤在一起。 */
function SourceTerms({ summary }: { summary: SourcingOrderSummary }) {
    return (
        <DescriptionList columns="two" className="gap-x-6 gap-y-3">
            <SourceFact
                term="采购类型"
                value={joinLabels(
                    summary.purchaseTypes.map(
                        (type) => PURCHASE_TYPE_LABEL[type],
                    ),
                )}
            />
            <SourceFact
                term="付款条件"
                value={joinLabels(summary.paymentTermLabels)}
            />
        </DescriptionList>
    )
}

function SourceFact({ term, value }: { term: string; value: ReactNode }) {
    return (
        <DescriptionItem className="flex flex-col gap-0.5 space-y-0">
            <DescriptionTerm>{term}</DescriptionTerm>
            <DescriptionDetails>{value}</DescriptionDetails>
        </DescriptionItem>
    )
}
