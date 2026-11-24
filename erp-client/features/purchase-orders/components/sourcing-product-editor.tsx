"use client"

import { useState } from "react"
import { InfoIcon, PlusIcon, SplitIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { canSplitSourcingProduct } from "../lib/sourcing-quantity"
import { supplierDeliveryRequired } from "../lib/sourcing/delivery"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import type {
    SourcingEditorProps,
    SourcingParticipationProps,
} from "./create-sourcing/types"
import { SourcingAllocationEditor } from "./sourcing-allocation-editor"
import { SourcingDeliveryRequirementSection } from "./sourcing-delivery-plan"
import { SourcingParticipationControl } from "./sourcing-participation-control"

/**
 * 商品供货方案编辑区（步骤 ①）：
 * 暂缓下单时仅展示温和提示而不锁死表单，方便采购员预先配置方案并保留草稿。
 */
export function SourcingProductEditor({
    row,
    onSetParticipation,
    onSplitResidual,
    onAddSplitAction,
    addSplitDisabledReason,
    onGoToAddresses,
    hideAddressEditor = false,
    showParticipation = true,
    condensed = false,
    activeRowKey,
    defaultCompareOpen = false,
    ...props
}: SourcingEditorProps &
    SourcingParticipationProps & {
        row: SourcingEditorRow
        onSplitResidual?: () => void
        onAddSplitAction?: () => void
        addSplitDisabledReason?: string
        onGoToAddresses?: () => void
        hideAddressEditor?: boolean
        showParticipation?: boolean
        condensed?: boolean
        activeRowKey?: string
        defaultCompareOpen?: boolean
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
    const insetClass = activeRowKey ? "px-4 @min-[560px]/sourcing:px-6" : "px-4"

    return (
        <div>
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
                <div className={cn("pt-3.5", insetClass)}>
                    <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border/70 bg-surface-sunken px-3.5 py-2 text-xs text-muted-foreground">
                        <span className="flex items-center gap-2">
                            <InfoIcon
                                aria-hidden="true"
                                className="size-3.5 shrink-0"
                            />
                            <span>
                                当前已暂缓下单：本次提交不会包含此商品，配置将保存在草稿中。
                            </span>
                        </span>
                        <Button
                            id={`sourcing-editor-${toAutomationIdSegment(row.product.salesOrderLineId)}-enable-participation`}
                            type="button"
                            variant="outline"
                            size="xs"
                            onClick={() =>
                                onSetParticipation(
                                    [row.product.salesOrderLineId],
                                    true,
                                )
                            }
                        >
                            恢复本次下单
                        </Button>
                    </div>
                </div>
            ) : activeRowKey && !visibleAllocations[0]?.line.selected ? (
                <div className={cn("pt-3.5", insetClass)}>
                    <p className="flex items-center gap-2 rounded-lg border border-border/70 bg-surface-sunken px-3.5 py-2 text-xs text-muted-foreground">
                        <InfoIcon
                            aria-hidden="true"
                            className="size-3.5 shrink-0"
                        />
                        <span>此来源当前已暂停使用，修改仅保留在草稿中。</span>
                    </p>
                </div>
            ) : null}

            <div className="min-w-0">
                <div className="divide-y divide-border/60">
                    {visibleAllocations.map(({ line, index }) => (
                        <SourcingAllocationEditor
                            compact
                            condensed={condensed}
                            expandDeliverySummary={Boolean(activeRowKey)}
                            hideAddressEditor={hideAddressEditor}
                            showParticipation={
                                !activeRowKey && row.allocations.length > 1
                            }
                            participationLabel="本次使用此来源"
                            defaultCompareOpen={defaultCompareOpen}
                            onAddSplit={
                                row.allocations.length === 1
                                    ? onAddSplitAction
                                    : undefined
                            }
                            canAddSplit={
                                row.allocations.length === 1 &&
                                row.product.options.length > 1
                            }
                            addSplitDisabledReason={addSplitDisabledReason}
                            onGoToAddresses={onGoToAddresses}
                            key={line.rowKey}
                            form={form}
                            order={order}
                            onRemoveSplit={props.onRemoveSplit}
                            product={row.product}
                            line={line}
                            index={index}
                            canRemove={
                                !activeRowKey && row.allocations.length > 1
                            }
                        />
                    ))}
                </div>
                <div className={cn("space-y-3 pb-4 empty:hidden", insetClass)}>
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
                            拆单给多家
                        </Button>
                    ) : null}
                    {commonWarehouse && targets.length > 0 ? (
                        <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border/60 bg-surface-sunken px-3.5 py-2.5">
                            <p className="text-xs text-muted-foreground">
                                将{" "}
                                <span className="font-medium text-foreground">
                                    {source.targetWarehouseName || "当前目标仓"}
                                </span>{" "}
                                应用到其余 {targets.length} 条已纳入入仓明细。
                            </p>
                            <Button
                                id={`sourcing-product-${toAutomationIdSegment(row.product.salesOrderLineId)}-apply-warehouse`}
                                type="button"
                                size="xs"
                                variant="outline"
                                onClick={() => {
                                    for (const targetIndex of targets) {
                                        form.setFieldValue(
                                            `lines[${targetIndex}].targetWarehouseId`,
                                            source.targetWarehouseId,
                                        )
                                        form.setFieldValue(
                                            `lines[${targetIndex}].targetWarehouseName`,
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
                            className="text-xs text-success-soft-foreground"
                        >
                            {notice}
                        </p>
                    ) : null}
                    {row.status === "部分分配" ? (
                        <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-warning-border bg-warning-soft/65 px-3.5 py-2.5 text-xs text-warning-soft-foreground">
                            <span className="flex items-center gap-2">
                                <InfoIcon
                                    aria-hidden="true"
                                    className="size-3.5 shrink-0"
                                />
                                <span>
                                    尚有{" "}
                                    <span className="num font-semibold">
                                        {row.remainingQuantity}{" "}
                                        {row.product.unit}
                                    </span>{" "}
                                    未分配；若需本次全量履约，可将剩余缺口拆给其他供应商。
                                </span>
                            </span>
                            {onSplitResidual ? (
                                <Button
                                    id={`sourcing-editor-${toAutomationIdSegment(row.product.salesOrderLineId)}-split-residual`}
                                    type="button"
                                    variant="outline"
                                    size="xs"
                                    className="bg-card text-foreground"
                                    onClick={onSplitResidual}
                                >
                                    <SplitIcon aria-hidden="true" />
                                    剩余缺口拆给新来源
                                </Button>
                            ) : null}
                        </div>
                    ) : null}
                </div>
                {showRequirements ? (
                    <SourcingDeliveryRequirementSection
                        product={row.product}
                        condensed={condensed}
                        note={
                            routed
                                ? "当前方式由仓库统一发货，客户收货地址将在仓库出库时安排。"
                                : undefined
                        }
                    />
                ) : null}
            </div>
        </div>
    )
}
