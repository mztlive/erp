"use client"

import * as React from "react"
import { SearchIcon, XIcon } from "lucide-react"

import {
    DetailWorkspaceMeta,
    MoneyValue,
    PaperDocument,
    QuantityValue,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { PurchaseOrderCreateSwitchList } from "@/features/purchase-orders/components/purchase-order-create-switch-list"
import {
    summarizeSourcingOrder,
    type SourcingProductLine,
    type SourcingSalesOrder,
} from "@/features/purchase-orders/lib/purchase-order-create-model"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
} from "@/features/purchase-orders/types"

export type PurchaseOrderCreateSourcePickerDialogProps = {
    open: boolean
    workspace: readonly SourcingSalesOrder[]
    selectedSalesOrderId: string
    onOpenChange: (open: boolean) => void
    onSelect: (salesOrderId: string) => void
}

function joinLabels(labels: readonly string[]): string {
    return labels.length > 0 ? labels.join("、") : "—"
}

/**
 * 更换来源销售单：左侧切换候选，右侧用纸质单据预览，确认后作为建单来源。
 */
export function PurchaseOrderCreateSourcePickerDialog({
    open,
    workspace,
    selectedSalesOrderId,
    onOpenChange,
    onSelect,
}: PurchaseOrderCreateSourcePickerDialogProps) {
    const [draftId, setDraftId] = React.useState(selectedSalesOrderId)
    const [query, setQuery] = React.useState("")
    const wasOpenRef = React.useRef(open)
    const filteredWorkspace = React.useMemo(() => {
        const keyword = query.trim().toLocaleLowerCase()
        if (!keyword) return workspace
        return workspace.filter((order) =>
            `${order.salesOrderNo} ${order.customerName} ${order.contractNumber ?? ""}`
                .toLocaleLowerCase()
                .includes(keyword),
        )
    }, [query, workspace])
    const draftOrder =
        filteredWorkspace.find((order) => order.salesOrderId === draftId) ??
        filteredWorkspace[0] ??
        workspace.find((order) => order.salesOrderId === draftId) ??
        workspace[0]
    const draftSummary = React.useMemo(
        () => (draftOrder ? summarizeSourcingOrder(draftOrder) : null),
        [draftOrder],
    )

    React.useEffect(() => {
        const justOpened = open && !wasOpenRef.current
        wasOpenRef.current = open
        if (!justOpened) return
        setDraftId(selectedSalesOrderId || workspace[0]?.salesOrderId || "")
        setQuery("")
    }, [open, selectedSalesOrderId, workspace])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                size="2xl"
                scroll="body"
                className="h-[88dvh] bg-card"
                closeButtonId="procurement-orders-create-source-picker-close"
            >
                <DialogHeader className="border-border/60 bg-card py-3 text-left">
                    <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
                        <div className="min-w-0 flex-1 space-y-1">
                            <div className="flex flex-wrap items-center gap-2">
                                <DialogTitle>选择来源销售单</DialogTitle>
                                <Badge variant="neutral" className="num">
                                    共 {workspace.length} 单
                                </Badge>
                            </div>
                            {draftOrder ? (
                                <DetailWorkspaceMeta
                                    items={[
                                        <span key="so" className="num">
                                            {draftOrder.salesOrderNo}
                                        </span>,
                                        draftOrder.customerName,
                                        draftOrder.contractNumber ?? "无合同",
                                        draftOrder.salesOwnerName,
                                    ]}
                                />
                            ) : null}
                        </div>
                        {draftSummary ? (
                            <div className="shrink-0 text-left sm:text-right">
                                <div className="text-xs text-muted-foreground">
                                    推荐采购含税估算
                                </div>
                                <MoneyValue
                                    size="compact"
                                    display="exact"
                                    value={draftSummary.minEstimatedGross}
                                />
                            </div>
                        ) : null}
                    </div>
                </DialogHeader>

                <DialogBody className="flex flex-col overflow-hidden bg-card p-0 md:flex-row">
                    {workspace.length > 1 ? (
                        <aside
                            aria-label="候选销售单列表"
                            className="flex max-h-56 w-full shrink-0 flex-col border-b border-border/60 bg-card md:max-h-none md:w-76 md:border-r md:border-b-0"
                        >
                            {workspace.length >= 4 ? (
                                <div className="border-b border-border/60 p-3">
                                    <div className="relative">
                                        <SearchIcon
                                            aria-hidden="true"
                                            className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                                        />
                                        <Input
                                            id="procurement-orders-create-source-picker-search"
                                            size="sm"
                                            aria-label="搜索待分配销售单"
                                            placeholder="搜索单号、客户或合同"
                                            className="border-transparent bg-surface-sunken pr-7 pl-8 shadow-none hover:bg-muted/70 focus-visible:border-primary focus-visible:bg-card"
                                            value={query}
                                            onChange={(event) =>
                                                setQuery(event.target.value)
                                            }
                                        />
                                        {query ? (
                                            <Button
                                                id="procurement-orders-create-source-picker-search-clear"
                                                type="button"
                                                variant="ghost"
                                                size="icon-xs"
                                                aria-label="清除搜索"
                                                className="absolute top-1/2 right-0.5 -translate-y-1/2"
                                                onClick={() => setQuery("")}
                                            >
                                                <XIcon />
                                            </Button>
                                        ) : null}
                                    </div>
                                </div>
                            ) : null}
                            <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
                                <PurchaseOrderCreateSwitchList
                                    label="待分配供给销售单"
                                    activeId={draftOrder?.salesOrderId ?? ""}
                                    items={filteredWorkspace.map((order) => {
                                        const orderSummary =
                                            summarizeSourcingOrder(order)
                                        return {
                                            id: order.salesOrderId,
                                            title: order.salesOrderNo,
                                            description: order.customerName,
                                            meta: `${orderSummary.lineCount} 行 · 已覆盖 ${orderSummary.coveredLineCount} 行`,
                                            amount: orderSummary.minEstimatedGross,
                                        }
                                    })}
                                    onSelect={setDraftId}
                                />
                            </div>
                        </aside>
                    ) : null}

                    <div
                        role="region"
                        aria-label="来源销售单预览"
                        className="min-h-0 flex-1 overflow-y-auto overscroll-contain bg-card px-5 py-4"
                    >
                        {draftOrder ? (
                            <SourcingSalesOrderPaper order={draftOrder} />
                        ) : (
                            <p className="py-10 text-center text-xs text-muted-foreground">
                                当前没有可预览的销售单。
                            </p>
                        )}
                    </div>
                </DialogBody>

                <DialogFooter className="items-center border-border/60 bg-card py-2.5 sm:justify-between">
                    {draftOrder && draftSummary ? (
                        <DetailWorkspaceMeta
                            items={[
                                <span key="so" className="num">
                                    已选 {draftOrder.salesOrderNo}
                                </span>,
                                <span key="lines" className="num">
                                    待分配 {draftSummary.lineCount} 行
                                </span>,
                            ]}
                        />
                    ) : (
                        <span />
                    )}
                    <div className="flex items-center gap-2">
                        <Button
                            id="procurement-orders-create-source-picker-cancel"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <Button
                            id="procurement-orders-create-source-picker-confirm"
                            type="button"
                            size="sm"
                            data-testid="purchase-create-source-confirm"
                            disabled={!draftOrder}
                            onClick={() => {
                                if (!draftOrder) return
                                onSelect(draftOrder.salesOrderId)
                                onOpenChange(false)
                            }}
                        >
                            使用该销售单
                        </Button>
                    </div>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}

/**
 * 用来源销售单的可分配供给数据投影成纸质单据，不补全未加载的销售详情。
 */
function SourcingSalesOrderPaper({ order }: { order: SourcingSalesOrder }) {
    const summary = summarizeSourcingOrder(order)
    const purchaseTypeLabel = joinLabels(
        summary.purchaseTypes.map((type) => PURCHASE_TYPE_LABEL[type]),
    )
    const fulfillmentLabel = joinLabels(
        summary.fulfillmentResponsibilities.map(
            (item) => FULFILLMENT_RESPONSIBILITY_LABEL[item],
        ),
    )

    return (
        <PaperDocument<SourcingProductLine>
            frame="borderless"
            compact
            title="销售单"
            subtitle="供给分配来源"
            documentNumber={order.salesOrderNo}
            parties={[
                {
                    id: "buyer",
                    label: "客户",
                    name: order.customerName,
                    reference: order.contractNumber ?? "无合同",
                    fields: [
                        {
                            id: "contract",
                            label: "合同",
                            value: order.contractNumber ?? "无合同",
                            numeric: true,
                        },
                    ],
                },
                {
                    id: "owner",
                    label: "负责销售",
                    name: order.salesOwnerName ?? "—",
                    fields: [
                        {
                            id: "pending",
                            label: "待分配明细",
                            value: `${summary.lineCount} 行`,
                            numeric: true,
                        },
                        {
                            id: "suppliers",
                            label: "可选采购供应商",
                            value: `${summary.uniqueSupplierCount} 家`,
                            numeric: true,
                        },
                    ],
                },
            ]}
            metadata={[
                {
                    id: "covered",
                    label: "已供给覆盖",
                    value: `${summary.coveredLineCount} 行`,
                    numeric: true,
                },
                {
                    id: "purchase-type",
                    label: "采购类型",
                    value: purchaseTypeLabel,
                },
                {
                    id: "fulfillment",
                    label: "履约责任",
                    value: fulfillmentLabel,
                },
                {
                    id: "payment",
                    label: "付款条件",
                    value: joinLabels(summary.paymentTermLabels),
                },
                ...(summary.businessCategories.length > 0
                    ? [
                          {
                              id: "category",
                              label: "经营类目",
                              value: joinLabels(summary.businessCategories),
                          },
                      ]
                    : []),
            ]}
            lineItemLabel="待分配明细"
            columns={[
                {
                    id: "item",
                    header: "销售项目",
                    cell: (row) => (
                        <div>
                            <div>{row.itemName}</div>
                            {row.itemSku ? (
                                <div className="num mt-1 text-xs text-muted-foreground">
                                    {row.itemSku}
                                </div>
                            ) : null}
                        </div>
                    ),
                },
                {
                    id: "sales-qty",
                    header: "销售数量",
                    align: "end",
                    numeric: true,
                    cell: (row) => (
                        <QuantityValue
                            value={row.salesQuantity}
                            unit={row.unitName}
                        />
                    ),
                },
                {
                    id: "covered",
                    header: "已覆盖",
                    align: "end",
                    numeric: true,
                    cell: (row) => (
                        <QuantityValue
                            value={row.coveredQuantity}
                            unit={row.unitName}
                        />
                    ),
                },
                {
                    id: "remaining",
                    header: "剩余数量",
                    align: "end",
                    numeric: true,
                    cell: (row) => (
                        <QuantityValue
                            value={row.remainingQuantity}
                            unit={row.unitName}
                        />
                    ),
                },
                {
                    id: "options",
                    header: "可用供给",
                    align: "end",
                    numeric: true,
                    cell: (row) => `${row.options.length} 项`,
                },
            ]}
            rows={order.lines}
            getRowId={(row) => row.salesOrderLineId}
            totals={[
                {
                    id: "lines",
                    label: "待分配明细",
                    value: `${summary.lineCount} 行`,
                },
                {
                    id: "estimate",
                    label: "推荐采购含税估算",
                    value: <MoneyValue value={summary.minEstimatedGross} />,
                    emphasized: true,
                    description: "扣除推荐库存分配后，按采购推荐方案估算",
                },
            ]}
            remarks="本预览展示该销售单当前待分配供给的明细；库存优先，含税金额仅估算推荐采购缺口。"
        />
    )
}
