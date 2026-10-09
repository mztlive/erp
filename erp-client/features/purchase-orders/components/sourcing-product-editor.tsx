"use client"

import { useState } from "react"
import { PlusIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { canSplitSourcingProduct } from "../lib/sourcing-quantity"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import { SourcingParticipationControl } from "./sourcing-participation-control"
import type {
    SourcingParticipationProps,
    SourcingEditorProps,
} from "./create-sourcing/types"
import { SourcingAllocationEditor } from "./sourcing-allocation-editor"

export function SourcingProductEditor({
    row,
    onSetParticipation,
    ...props
}: SourcingEditorProps &
    SourcingParticipationProps & { row: SourcingEditorRow }) {
    const { form, order } = props
    const warehouseSources = row.allocations.filter(({ line }) => {
        const option = findSourcingOption(row.product, line.basisId)
        return (
            line.selected &&
            line.targetWarehouseId &&
            option?.sourceType === "PURCHASE" &&
            option.fulfillmentResponsibility === "WAREHOUSE"
        )
    })
    const targets = form.state.values.lines.flatMap((line, index) => {
        if (
            !line.selected ||
            line.salesOrderLineId === row.product.salesOrderLineId
        )
            return []
        const product = order.lines.find(
            (candidate) => candidate.salesOrderLineId === line.salesOrderLineId,
        )
        const option = findSourcingOption(product, line.basisId)
        return option?.sourceType === "PURCHASE" &&
            option.fulfillmentResponsibility === "WAREHOUSE"
            ? [index]
            : []
    })
    const source = warehouseSources[0]?.line
    const commonWarehouse =
        source &&
        warehouseSources.every(
            ({ line }) => line.targetWarehouseId === source.targetWarehouseId,
        )
    const [notice, setNotice] = useState("")
    return (
        <div className="@container/sourcing">
            <div className="px-4 pt-4">
                <SourcingParticipationControl
                    idPrefix={`sourcing-editor-${toAutomationIdSegment(row.product.salesOrderLineId)}`}
                    itemName={row.product.itemName}
                    included={row.selected || row.partiallySelected}
                    onChange={(included) =>
                        onSetParticipation(
                            [row.product.salesOrderLineId],
                            included,
                        )
                    }
                />
            </div>
            <fieldset
                disabled={!row.selected && !row.partiallySelected}
                className="min-w-0 disabled:opacity-60"
            >
                <div className="divide-y divide-border/60">
                    {row.allocations.map(({ line, index }) => (
                        <SourcingAllocationEditor
                            compact
                            showParticipation={row.allocations.length > 1}
                            participationLabel="本次使用此来源"
                            key={line.rowKey}
                            {...props}
                            product={row.product}
                            line={line}
                            index={index}
                            canRemove={row.allocations.length > 1}
                        />
                    ))}
                </div>
                <div className="space-y-3 px-4 pb-4">
                    {canSplitSourcingProduct(
                        row.product,
                        row.allocations.length,
                    ) ? (
                        <Button
                            id={`sourcing-product-${toAutomationIdSegment(row.product.salesOrderLineId)}-split`}
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() =>
                                props.onAddSplit(row.product.salesOrderLineId)
                            }
                        >
                            <PlusIcon />
                            拆分给其他来源
                        </Button>
                    ) : null}
                    {commonWarehouse && targets.length > 0 ? (
                        <div className="space-y-2 border-t border-border pt-3">
                            <p className="text-xs text-muted-foreground">
                                将 {source.targetWarehouseName || "当前目标仓"}{" "}
                                应用到其余 {targets.length} 条已勾选入仓明细。
                            </p>
                            <Button
                                id={`sourcing-product-${toAutomationIdSegment(row.product.salesOrderLineId)}-apply-warehouse`}
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={() => {
                                    for (const index of targets) {
                                        form.setFieldValue(
                                            `lines[${index}].targetWarehouseId`,
                                            source.targetWarehouseId,
                                        )
                                        form.setFieldValue(
                                            `lines[${index}].targetWarehouseName`,
                                            source.targetWarehouseName,
                                        )
                                    }
                                    setNotice(
                                        `已将目标仓应用到 ${targets.length} 条入仓明细。`,
                                    )
                                }}
                            >
                                应用目标仓到其他入仓明细
                            </Button>
                        </div>
                    ) : null}
                    {notice ? (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            {notice}
                        </p>
                    ) : null}
                    {row.status === "部分分配" ? (
                        <p className="text-xs text-muted-foreground">
                            本次尚未覆盖全部需求，可拆分其他来源；确认后剩余缺口继续保留。
                        </p>
                    ) : null}
                </div>
            </fieldset>
        </div>
    )
}
