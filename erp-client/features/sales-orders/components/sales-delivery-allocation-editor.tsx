"use client"

import { useState } from "react"
import { ChevronDownIcon, ChevronUpIcon, MapPinIcon } from "lucide-react"
import { QuantityValue } from "@/components/business"
import { toFieldErrors } from "@/components/form"
import { Button } from "@/components/ui/button"
import { FieldError } from "@/components/ui/field"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toast } from "@/components/ui/toast"
import { RegionName } from "@/features/regions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { DeliveryCoverageHint as CoverageCode } from "../api/delivery-coverage"
import { useDeliveryCoverage } from "../hooks/use-delivery-coverage"
import { allocationSummary } from "../lib/sales-delivery-allocation-summary"
import type { SalesOrderCreateFormApi } from "../lib/sales-order-create-form-types"
import {
    applyDeliveryAddressToLines,
    type CreateSalesOrderFormValues,
    type UnifiedDeliveryAddress,
} from "../lib/sales-order-create-model"
import { DeliveryCoverageHint } from "./delivery-coverage-hint"
import { SalesDeliveryAllocationDialog } from "./sales-delivery-allocation-dialog"
import { SalesOrderCreateUnifiedAddressDialog } from "./sales-order-create-unified-address-dialog"

// 改单页面继续复用相同的数量与完整性摘要。
export { allocationSummary } from "../lib/sales-delivery-allocation-summary"

type DeliveryLine = CreateSalesOrderFormValues["lineItems"][number]

/** 每条销售明细独立保存地址；只在展示时合并完全相同且分配完整的地址。 */
function unifiedAddress(lines: DeliveryLine[]) {
    const first = lines[0]?.deliveryAllocations?.[0]
    if (!first) return undefined
    return lines.every((line) => {
        const rows = line.deliveryAllocations ?? []
        const row = rows[0]
        return (
            rows.length === 1 &&
            allocationSummary(line).complete &&
            row.recipient === first.recipient &&
            row.phone === first.phone &&
            row.region_code === first.region_code &&
            row.address === first.address
        )
    })
        ? first
        : undefined
}

function DeliveryLineRow({
    form,
    line,
    rowIndex,
    coverageHint,
    onEdit,
}: {
    form: SalesOrderCreateFormApi
    line: DeliveryLine
    rowIndex: number
    onEdit: () => void
    coverageHint: (sku: string, regionCode: string) => CoverageCode
}) {
    const rows = line.deliveryAllocations ?? []
    const first = rows[0]
    const summary = allocationSummary(line)
    const prefix = `sales-delivery-${toAutomationIdSegment(line.rowKey)}`
    return (
        <TableRow>
            <TableCell className="max-w-56 whitespace-normal">
                <p className="break-words font-medium">
                    {line.name || `商品 ${rowIndex + 1}`}
                </p>
                {line.specification ? (
                    <p className="mt-1 text-xs text-muted-foreground">
                        {line.specification}
                    </p>
                ) : null}
            </TableCell>
            <TableCell>
                <QuantityValue value={line.quantity} unit={line.unit} />
            </TableCell>
            <TableCell className="min-w-48 max-w-md whitespace-normal">
                {first ? (
                    <div className="space-y-1">
                        {rows.length === 1 ? (
                            <>
                                <p>
                                    {first.recipient || "待填收货人"} ·{" "}
                                    {first.phone || "待填联系电话"}
                                </p>
                                <p className="break-words text-xs text-muted-foreground">
                                    <RegionName code={first.region_code} />
                                    {first.address}
                                </p>
                            </>
                        ) : (
                            <p>{rows.length} 个地址</p>
                        )}
                        <p
                            className={`text-xs ${summary.complete ? "text-muted-foreground" : "text-warning-soft-foreground"}`}
                        >
                            {summary.summary}
                            {!summary.complete ? ` · ${summary.status}` : ""}
                        </p>
                        {[...new Set(rows.map((row) => row.region_code))]
                            .filter(Boolean)
                            .map((code) => (
                                <DeliveryCoverageHint
                                    key={code}
                                    id={`${prefix}-coverage-${toAutomationIdSegment(code)}`}
                                    hint={coverageHint(line.sku, code)}
                                />
                            ))}
                    </div>
                ) : (
                    <span className="text-muted-foreground">未设置</span>
                )}
                <form.AppField
                    name={`lineItems[${rowIndex}].deliveryAllocations`}
                >
                    {(field) => (
                        <FieldError
                            errors={toFieldErrors(field.state.meta.errors)}
                        />
                    )}
                </form.AppField>
            </TableCell>
            <TableCell className="text-right">
                <SalesDeliveryAllocationDialog
                    line={line}
                    onApply={(rows) => {
                        onEdit()
                        // 保存时按稳定行键定位，避免误写删除或重排后的其它商品。
                        const index = form
                            .getFieldValue("lineItems")
                            .findIndex((item) => item.rowKey === line.rowKey)
                        if (index >= 0)
                            form.setFieldValue(
                                `lineItems[${index}].deliveryAllocations`,
                                rows,
                            )
                    }}
                />
            </TableCell>
        </TableRow>
    )
}

/** 地址为空时显示入口，统一地址显示摘要，复杂安排按商品展开。 */
export function SalesDeliveryAllocationEditor({
    form,
}: {
    form: SalesOrderCreateFormApi
}) {
    return (
        <form.Subscribe
            selector={(state) => ({
                nature: state.values.nature,
                lines: state.values.lineItems,
            })}
        >
            {({ nature, lines }) =>
                nature !== "physical_service" || !lines.length ? null : (
                    <DeliveryAllocationSection form={form} lines={lines} />
                )
            }
        </form.Subscribe>
    )
}

function DeliveryAllocationSection({
    form,
    lines,
}: {
    form: SalesOrderCreateFormApi
    lines: DeliveryLine[]
}) {
    const [expanded, setExpanded] = useState(false)
    const coverageHint = useDeliveryCoverage(lines)
    const common = unifiedAddress(lines)
    const allocatedCount = lines.filter(
        (line) => line.deliveryAllocations?.length,
    ).length
    const incompleteCount = lines.filter(
        (line) =>
            line.deliveryAllocations?.length &&
            !allocationSummary(line).complete,
    ).length
    // 部分设置、不同地址和数量不匹配始终展开，避免掩盖待处理项。
    const requiresDetails = allocatedCount > 0 && !common
    const showLines = expanded || requiresDetails
    const firstAddress =
        common ??
        lines
            .flatMap((line) => line.deliveryAllocations ?? [])
            .find((row) => row.recipient || row.region_code || row.address)
    const applyUnifiedAddress = (address: UnifiedDeliveryAddress) => {
        const next = applyDeliveryAddressToLines(
            form.getFieldValue("lineItems"),
            address,
        )
        form.setFieldValue("lineItems", next)
        setExpanded(false)
        toast.add({
            title: "已设置统一收货地址",
            description: `${next.length} 项商品均按全部数量送到该地址。`,
            type: "success",
            timeout: 3000,
        })
    }
    const warnings = common
        ? lines.filter(
              (line) => coverageHint(line.sku, common.region_code) !== "NONE",
          )
        : []
    return (
        <section
            className="min-w-0 space-y-4 border-t border-grid p-4 sm:p-5"
            aria-labelledby="sales-delivery-title"
        >
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="min-w-0 space-y-1">
                    <div className="flex items-baseline gap-2">
                        <h2
                            id="sales-delivery-title"
                            className="font-heading text-base font-semibold"
                        >
                            收货安排
                        </h2>
                        <span className="text-xs text-muted-foreground">
                            选填
                        </span>
                    </div>
                    <p className="text-sm text-muted-foreground">
                        {common
                            ? `全部 ${lines.length} 项商品送到同一地址`
                            : allocatedCount
                              ? `已设置 ${allocatedCount} / ${lines.length} 项商品${incompleteCount ? `，${incompleteCount} 项待完善` : ""}`
                              : "暂未设置收货地址，可在履约前补充。"}
                    </p>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                    <SalesOrderCreateUnifiedAddressDialog
                        lineCount={lines.length}
                        allocatedLineCount={allocatedCount}
                        initialAddress={firstAddress}
                        triggerLabel={common ? "修改统一地址" : "设置统一地址"}
                        onApply={applyUnifiedAddress}
                    />
                    {!requiresDetails ? (
                        <Button
                            id="sales-delivery-toggle-lines"
                            type="button"
                            variant="ghost"
                            size="sm"
                            aria-expanded={showLines}
                            aria-controls="sales-delivery-lines"
                            onClick={() => setExpanded(!expanded)}
                        >
                            {showLines ? (
                                <ChevronUpIcon aria-hidden="true" />
                            ) : (
                                <ChevronDownIcon aria-hidden="true" />
                            )}
                            {showLines
                                ? "收起商品"
                                : common
                                  ? "按商品调整"
                                  : "按商品设置"}
                        </Button>
                    ) : null}
                </div>
            </div>
            {common ? (
                <div className="flex gap-3 rounded-lg bg-muted/30 px-4 py-3">
                    <MapPinIcon
                        className="mt-0.5 size-4 shrink-0 text-muted-foreground"
                        aria-hidden="true"
                    />
                    <div className="min-w-0 space-y-1 text-sm">
                        <p className="break-words font-medium">
                            {common.recipient}
                            <span className="ml-3 font-normal">
                                {common.phone}
                            </span>
                        </p>
                        <p className="break-words text-muted-foreground">
                            <RegionName code={common.region_code} />
                            {common.address}
                        </p>
                        {!showLines
                            ? warnings.map((line) => (
                                  <div key={line.rowKey} className="pt-1">
                                      <p className="text-xs text-muted-foreground">
                                          {line.name}
                                      </p>
                                      <DeliveryCoverageHint
                                          id={`sales-delivery-${toAutomationIdSegment(line.rowKey)}-coverage`}
                                          hint={coverageHint(
                                              line.sku,
                                              common.region_code,
                                          )}
                                      />
                                  </div>
                              ))
                            : null}
                    </div>
                </div>
            ) : null}
            <div id="sales-delivery-lines" hidden={!showLines}>
                {showLines ? (
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>商品</TableHead>
                                <TableHead>销售数量</TableHead>
                                <TableHead>收货安排</TableHead>
                                <TableHead className="text-right">
                                    操作
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {lines.map((line, rowIndex) => (
                                <DeliveryLineRow
                                    key={line.rowKey}
                                    form={form}
                                    line={line}
                                    rowIndex={rowIndex}
                                    coverageHint={coverageHint}
                                    onEdit={() => setExpanded(true)}
                                />
                            ))}
                        </TableBody>
                    </Table>
                ) : null}
            </div>
        </section>
    )
}
