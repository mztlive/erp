"use client"

import * as React from "react"
import type { ReactNode } from "react"
import { ArrowLeftRightIcon } from "lucide-react"

import { MoneyValue, surfacePanelClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { PurchaseOrderCreateSourcePickerDialog } from "@/features/purchase-orders/components/purchase-order-create-source-picker-dialog"
import {
    summarizeSourcingOrder,
    type SourcingSalesOrder,
} from "@/features/purchase-orders/lib/purchase-order-create-model"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
} from "@/features/purchase-orders/types"
import { cn } from "@/lib/utils"

export type PurchaseOrderCreateSourcePanelProps = {
    workspace: readonly SourcingSalesOrder[]
    selectedSalesOrderId: string
    selectedOrder?: SourcingSalesOrder
    disabled?: boolean
    onSalesOrderChange: (salesOrderId: string) => void
}

function joinLabels(labels: readonly string[]): string {
    return labels.length > 0 ? labels.join("、") : "—"
}

/**
 * 来源销售单选择与密集摘要：单号、客户、合同、覆盖与最低含税估算。
 */
export function PurchaseOrderCreateSourcePanel({
    workspace,
    selectedSalesOrderId,
    selectedOrder,
    disabled,
    onSalesOrderChange,
}: PurchaseOrderCreateSourcePanelProps) {
    const [pickerOpen, setPickerOpen] = React.useState(false)
    const summary = selectedOrder
        ? summarizeSourcingOrder(selectedOrder)
        : undefined

    return (
        <section className={cn(surfacePanelClassName, "overflow-hidden")}>
            <div className="flex flex-col gap-3 p-4 md:p-5">
                <div className="flex items-center justify-between gap-3">
                    <h2 className="font-heading text-sm font-semibold">
                        来源销售单
                    </h2>
                    {disabled ? null : (
                        <Button
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
                {selectedOrder && summary ? (
                    <DescriptionList
                        columns="four"
                        className="gap-x-4 gap-y-2"
                        aria-label="来源销售单摘要"
                    >
                        <SourceFact
                            term="销售单号"
                            numeric
                            value={selectedOrder.salesOrderNo}
                        />
                        <SourceFact
                            term="客户"
                            value={selectedOrder.customerName}
                        />
                        <SourceFact
                            term="合同"
                            numeric
                            value={selectedOrder.contractNumber ?? "无合同"}
                        />
                        <SourceFact
                            term="负责销售"
                            value={selectedOrder.salesOwnerName ?? "—"}
                        />
                        <SourceFact
                            term="待采购明细"
                            numeric
                            value={`${summary.lineCount} 行`}
                        />
                        <SourceFact
                            term="已覆盖明细"
                            numeric
                            value={`${summary.coveredLineCount} 行`}
                        />
                        <SourceFact
                            term="可选供应商"
                            numeric
                            value={`${summary.uniqueSupplierCount} 家`}
                        />
                        <SourceFact
                            term="最低含税估算"
                            value={
                                <MoneyValue value={summary.minEstimatedGross} />
                            }
                        />
                        <SourceFact
                            term="采购类型"
                            value={joinLabels(
                                summary.purchaseTypes.map(
                                    (type) => PURCHASE_TYPE_LABEL[type],
                                ),
                            )}
                        />
                        <SourceFact
                            term="履约责任"
                            value={joinLabels(
                                summary.fulfillmentResponsibilities.map(
                                    (item) =>
                                        FULFILLMENT_RESPONSIBILITY_LABEL[item],
                                ),
                            )}
                        />
                        <SourceFact
                            term="付款条件"
                            value={joinLabels(summary.paymentTermLabels)}
                        />
                        {summary.businessCategories.length > 0 ? (
                            <SourceFact
                                term="经营类目"
                                value={joinLabels(summary.businessCategories)}
                            />
                        ) : null}
                    </DescriptionList>
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

function SourceFact({
    term,
    value,
    numeric,
}: {
    term: string
    value: ReactNode
    numeric?: boolean
}) {
    return (
        <DescriptionItem className="flex flex-col gap-0.5 space-y-0">
            <DescriptionTerm>{term}</DescriptionTerm>
            <DescriptionDetails className={numeric ? "num" : undefined}>
                {value}
            </DescriptionDetails>
        </DescriptionItem>
    )
}
