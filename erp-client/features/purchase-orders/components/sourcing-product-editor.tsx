"use client"

import { useState } from "react"
import { PlusIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { canSplitSourcingProduct } from "../lib/sourcing-quantity"
import { supplierDeliveryRequired } from "../lib/sourcing/delivery"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import { SourcingParticipationControl } from "./sourcing-participation-control"
import type {
    SourcingParticipationProps,
    SourcingEditorProps,
} from "./create-sourcing/types"
import { SourcingDeliveryRequirementSection } from "./sourcing-delivery-plan"
import { SourcingAllocationEditor } from "./sourcing-allocation-editor"

/** 分配复选框可由外层标题区承载；地址要求仅在没有来源需要供应商送达时只读展示。 */
export function SourcingProductEditor({
    row,
    onSetParticipation,
    showParticipation = true,
    condensed = false,
    activeRowKey,
    ...props
}: SourcingEditorProps &
    SourcingParticipationProps & {
        row: SourcingEditorRow
        showParticipation?: boolean
        condensed?: boolean
        /** 分栏编辑时只挂载当前来源，字段仍绑定整单共享草稿。 */
        activeRowKey?: string
    }) {
    const { form, order } = props
    const included = row.selected || row.partiallySelected
    const visibleAllocations = activeRowKey
        ? row.allocations.filter(({ line }) => line.rowKey === activeRowKey)
        : row.allocations
    const selectedLines = visibleAllocations
        .filter(({ line }) => Boolean(activeRowKey) || line.selected)
        .map(({ line }) => line)
    const showRequirements =
        row.product.deliveryAllocations.length === 0 ||
        !selectedLines.some((line) =>
            supplierDeliveryRequired(row.product, line),
        )
    const routed = selectedLines.some((line) =>
        findSourcingOption(row.product, line.basisId),
    )
    const warehouseSources = visibleAllocations.filter(({ line }) => {
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
            {showParticipation ? (
                <div className="px-4 pt-4">
                    <SourcingParticipationControl
                        idPrefix={`sourcing-editor-${toAutomationIdSegment(row.product.salesOrderLineId)}`}
                        itemName={row.product.itemName}
                        included={included}
                        onChange={(next) =>
                            onSetParticipation(
                                [row.product.salesOrderLineId],
                                next,
                            )
                        }
                    />
                </div>
            ) : !included ? (
                <p className="px-4 pt-4 text-xs text-muted-foreground">
                    暂不分配：本次不提交，方案已保留。
                </p>
            ) : activeRowKey && !visibleAllocations[0]?.line.selected ? (
                <p className="px-4 pt-4 text-xs text-muted-foreground">
                    此来源暂不参与分配，修改仅保留在草稿中。
                </p>
            ) : null}
            <fieldset
                disabled={!included}
                className="min-w-0 disabled:opacity-60"
            >
                <div className="divide-y divide-border/60">
                    {visibleAllocations.map(({ line, index }) => (
                        <SourcingAllocationEditor
                            compact
                            condensed={condensed}
                            expandDeliverySummary={Boolean(activeRowKey)}
                            showParticipation={
                                !activeRowKey && row.allocations.length > 1
                            }
                            participationLabel="本次使用此来源"
                            key={line.rowKey}
                            {...props}
                            product={row.product}
                            line={line}
                            index={index}
                            canRemove={
                                !activeRowKey && row.allocations.length > 1
                            }
                        />
                    ))}
                </div>
                <div className="space-y-3 px-4 pb-4 empty:hidden">
                    {!condensed &&
                    canSplitSourcingProduct(
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
                {showRequirements ? (
                    <SourcingDeliveryRequirementSection
                        product={row.product}
                        condensed={condensed}
                        note={routed ? "客户地址由仓库发货时安排。" : undefined}
                    />
                ) : null}
            </fieldset>
        </div>
    )
}
