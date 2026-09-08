"use client"

import { useEffect, useState } from "react"
import { ChevronDownIcon, ChevronUpIcon, PlusIcon } from "lucide-react"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { multiplyFixed } from "@/lib/fixed-decimal"
import {
    findSourcingOption,
    type SourcingProductLine,
    type SourcingLineInput,
} from "../lib/purchase-order-create-model"
import { sourcingFormValidationError } from "../lib/purchase-order-create-validation"
import {
    canSplitSourcingProduct,
    sourcingUnitQuantityError,
} from "../lib/sourcing-quantity"
import { FULFILLMENT_RESPONSIBILITY_LABEL } from "../types"
import type { PurchaseOrderCreateSourcingTableProps } from "./purchase-order-create-sourcing-table"
import { SourcingAllocationEditor } from "./sourcing-allocation-editor"

/** 默认阅读当前方案，需要调整或补齐信息时才展开表单。 */
export function PurchaseOrderCreateSourcingCards(
    props: PurchaseOrderCreateSourcingTableProps,
) {
    const values = props.form.state.values
    const errors =
        sourcingFormValidationError(props.order, values)?.fields ?? {}
    return (
        <div className="@container/sourcing space-y-4">
            {props.order.lines.map((product) => {
                const allocations = values.lines.flatMap((line, index) =>
                    line.salesOrderLineId === product.salesOrderLineId
                        ? [{ line, index }]
                        : [],
                )
                if (!allocations.length) return null
                const issues = [
                    ...new Set(
                        allocations.flatMap(({ index }) =>
                            Object.entries(errors)
                                .filter(([path]) =>
                                    path.startsWith(`lines[${index}].`),
                                )
                                .map(([, error]) => error),
                        ),
                    ),
                ]
                return (
                    <SourcingProductCard
                        key={product.salesOrderLineId}
                        {...props}
                        product={product}
                        allocations={allocations}
                        issues={issues}
                    />
                )
            })}
        </div>
    )
}

/** 商品需求与当前方案分层展示，编辑状态不代表供给已提交。 */
function SourcingProductCard({
    form,
    order,
    product,
    allocations,
    issues,
    onAddSplit,
    onRemoveSplit,
}: PurchaseOrderCreateSourcingTableProps & {
    product: SourcingProductLine
    allocations: { line: SourcingLineInput; index: number }[]
    issues: string[]
}) {
    const hasIssues = issues.length > 0
    const [editing, setEditing] = useState(hasIssues)
    useEffect(() => {
        if (hasIssues) setEditing(true)
    }, [hasIssues])
    const expanded = editing || issues.length > 0
    const id = `procurement-orders-create-product-${toAutomationIdSegment(product.salesOrderLineId)}`
    const splittable = canSplitSourcingProduct(product, allocations.length)
    return (
        <section
            aria-label={`${product.itemName}的供给方案`}
            className="min-w-0 overflow-hidden rounded-lg border border-border bg-card"
        >
            <header className="flex flex-wrap items-start justify-between gap-3 border-b border-border/60 px-4 py-4">
                <div className="min-w-0 space-y-1.5">
                    <h3 className="break-words text-sm font-semibold">
                        {product.itemName}
                    </h3>
                    <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
                        <span className="font-medium text-foreground">
                            需供给{" "}
                            <QuantityValue
                                value={product.remainingQuantity}
                                unit={product.unit}
                            />
                        </span>
                        {product.deliveryDeadline ? (
                            <span>最晚交付 {product.deliveryDeadline}</span>
                        ) : null}
                    </div>
                </div>
                <Button
                    id={`${id}-adjust`}
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="shrink-0"
                    aria-expanded={expanded}
                    aria-controls={`${id}-editor`}
                    disabled={hasIssues}
                    onClick={() => setEditing(!editing)}
                >
                    {expanded ? (
                        <>
                            收起调整
                            <ChevronUpIcon aria-hidden="true" />
                        </>
                    ) : (
                        <>
                            调整方案
                            <ChevronDownIcon aria-hidden="true" />
                        </>
                    )}
                </Button>
            </header>
            {/* 保持字段挂载，收起时隐藏，避免丢失 TanStack Form 的校验注册。 */}
            <div id={`${id}-editor`} hidden={!expanded} className="bg-muted/15">
                {issues.length > 0 ? (
                    <ul
                        role="alert"
                        className="space-y-1 px-4 pt-3 text-xs text-destructive"
                    >
                        {issues.map((issue) => (
                            <li key={issue}>{issue}</li>
                        ))}
                    </ul>
                ) : null}
                <div className="divide-y divide-border/60">
                    {allocations.map(({ line, index }) => (
                        <SourcingAllocationEditor
                            key={line.rowKey}
                            form={form}
                            order={order}
                            product={product}
                            line={line}
                            index={index}
                            canRemove={allocations.length > 1}
                            onRemoveSplit={onRemoveSplit}
                        />
                    ))}
                </div>
                {splittable ? (
                    <div className="px-4 pb-4">
                        <Button
                            id={`${id}-split-add`}
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => {
                                setEditing(true)
                                onAddSplit(product.salesOrderLineId)
                            }}
                        >
                            <PlusIcon aria-hidden="true" />
                            拆分给其他来源
                        </Button>
                    </div>
                ) : null}
            </div>
            {!expanded ? (
                <div className="divide-y divide-border/60">
                    {allocations.map(({ line }) => (
                        <SourcingAllocationSummary
                            key={line.rowKey}
                            product={product}
                            line={line}
                        />
                    ))}
                </div>
            ) : null}
        </section>
    )
}

/** 摘要只描述本次拟采用的供给来源和金额，不显示已完成等提交结果。 */
function SourcingAllocationSummary({
    product,
    line,
}: {
    product: SourcingProductLine
    line: SourcingLineInput
}) {
    const option = findSourcingOption(product, line.basisId)
    if (!line.selected)
        return (
            <p className="px-4 py-5 text-sm text-muted-foreground">
                本次暂不分配，可在“调整方案”中加入。
            </p>
        )
    if (!option) return null
    const stock = option.sourceType === "EXISTING_STOCK"
    const amount = sourcingUnitQuantityError(
        line.quantity,
        product.quantityScale,
    )
        ? undefined
        : multiplyFixed(option.unitCostGross, line.quantity, {
              leftMaxScale: 4,
              rightMaxScale: 6,
              outputScale: 2,
          })
    return (
        <div className="space-y-4 px-4 py-5">
            <div className="flex flex-wrap items-start justify-between gap-4">
                <div className="min-w-0 space-y-1.5">
                    <p className="break-words text-sm font-medium">
                        {stock
                            ? option.warehouseName || option.supplierName
                            : option.supplierName}
                    </p>
                    <p className="text-xs text-muted-foreground">
                        {stock
                            ? "现有库存"
                            : FULFILLMENT_RESPONSIBILITY_LABEL[
                                  option.fulfillmentResponsibility
                              ]}
                        {line.targetWarehouseName
                            ? ` · ${line.targetWarehouseName}`
                            : ""}
                    </p>
                </div>
                {!stock && amount ? (
                    <div className="shrink-0 text-right">
                        <MoneyValue
                            value={amount}
                            className="text-lg font-semibold"
                        />
                        <p className="mt-1 text-xs text-muted-foreground">
                            采购含税金额
                        </p>
                    </div>
                ) : null}
            </div>
            <div className="flex flex-wrap items-center gap-x-6 gap-y-2 text-sm">
                <span>
                    本次供给{" "}
                    <QuantityValue value={line.quantity} unit={product.unit} />
                </span>
                {!stock ? (
                    <span>
                        预计交付{" "}
                        <span className="num">{line.expectedDeliveryDate}</span>
                    </span>
                ) : null}
            </div>
            {!stock ? (
                <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
                    <span>
                        含税单价 <MoneyValue value={option.unitCostGross} />
                    </span>
                    <span>
                        税率{" "}
                        <RateValue
                            value={multiplyFixed(option.inputTaxRate, "100", {
                                leftMaxScale: 6,
                                rightMaxScale: 0,
                                outputScale: 2,
                            })}
                            precision={2}
                        />
                    </span>
                    <span>付款 {option.paymentTermLabel || "—"}</span>
                </div>
            ) : (
                <p className="text-xs text-muted-foreground">
                    确认后预留库存，成本按库存核算。
                </p>
            )}
        </div>
    )
}
