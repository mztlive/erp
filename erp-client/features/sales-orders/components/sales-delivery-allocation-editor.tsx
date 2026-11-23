"use client"

import { useState } from "react"
import { ChevronDownIcon, ChevronUpIcon, MapPinIcon } from "lucide-react"

import {
    DetailWorkspaceSection,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { toFieldErrors } from "@/components/form"
import { Button } from "@/components/ui/button"
import { FieldError } from "@/components/ui/field"
import { StatusBadge } from "@/components/ui/status-badge"
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
import { SalesDeliveryWorkbookActions } from "./sales-delivery-workbook-actions"
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
            <TableCell className="max-w-56 whitespace-normal py-2.5 align-top">
                <p className="break-words font-medium text-foreground">
                    {line.name || `商品 ${rowIndex + 1}`}
                </p>
                {line.specification ? (
                    <p className="mt-0.5 text-xs text-muted-foreground">
                        {line.specification}
                    </p>
                ) : null}
            </TableCell>
            <TableCell data-align="end" className="py-2.5 align-top">
                <QuantityValue value={line.quantity} unit={line.unit} />
            </TableCell>
            <TableCell className="min-w-48 max-w-md whitespace-normal py-2.5 align-top">
                {first ? (
                    <div className="space-y-1">
                        {rows.length === 1 ? (
                            <>
                                <p className="text-foreground">
                                    {first.recipient || "待填收货人"}
                                    <span className="num ml-2 text-muted-foreground">
                                        {first.phone || "待填联系电话"}
                                    </span>
                                </p>
                                <p className="break-words text-xs text-muted-foreground">
                                    <RegionName code={first.region_code} />
                                    {first.address}
                                </p>
                            </>
                        ) : (
                            <p className="num font-medium text-foreground">
                                {rows.length} 个收货地址
                            </p>
                        )}
                        <p
                            className={`num text-xs ${summary.complete ? "text-muted-foreground" : "text-warning-soft-foreground"}`}
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
                    <span className="text-xs text-muted-foreground">
                        未设置
                    </span>
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
            <TableCell className="py-2.5 text-right align-top">
                <SalesDeliveryAllocationDialog
                    line={line}
                    onApply={(nextRows) => {
                        onEdit()
                        // 保存时按稳定行键定位，避免误写删除或重排后的其它商品。
                        const index = form
                            .getFieldValue("lineItems")
                            .findIndex((item) => item.rowKey === line.rowKey)
                        if (index >= 0)
                            form.setFieldValue(
                                `lineItems[${index}].deliveryAllocations`,
                                nextRows,
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
    draftId,
}: {
    form: SalesOrderCreateFormApi
    draftId?: string
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
                    <DeliveryAllocationSection
                        form={form}
                        lines={lines}
                        draftId={draftId}
                    />
                )
            }
        </form.Subscribe>
    )
}

function DeliveryAllocationSection({
    form,
    lines,
    draftId,
}: {
    form: SalesOrderCreateFormApi
    lines: DeliveryLine[]
    draftId?: string
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

    const badge = common ? (
        <StatusBadge tone="success" appearance="quiet" label="统一地址" />
    ) : incompleteCount > 0 ? (
        <StatusBadge
            tone="warning"
            appearance="badge"
            label={`${incompleteCount} 项待完善`}
        />
    ) : allocatedCount > 0 ? (
        <StatusBadge
            tone="success"
            appearance="quiet"
            label={`已设 ${allocatedCount}/${lines.length}`}
        />
    ) : (
        <span className="rounded-sm bg-muted/80 px-1.5 py-0.5 text-xs font-medium text-muted-foreground">
            选填
        </span>
    )

    const subtitle = common
        ? `全部 ${lines.length} 项商品送至同一地址`
        : allocatedCount
          ? `已设置 ${allocatedCount} / ${lines.length} 项商品`
          : "可在履约前补充收货地址"

    return (
        <DetailWorkspaceSection
            title="收货安排"
            headingId="sales-delivery-title"
            width="full"
            badge={badge}
            subtitle={subtitle}
            actions={
                <>
                    <SalesDeliveryWorkbookActions
                        form={form}
                        lines={lines}
                        draftId={draftId}
                        onApplied={() => setExpanded(true)}
                    />
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
                            className={detailWorkspaceStyles.quietButton}
                            aria-expanded={showLines}
                            aria-controls="sales-delivery-lines"
                            onClick={() => setExpanded(!expanded)}
                        >
                            {showLines ? (
                                <ChevronUpIcon
                                    data-icon="inline-start"
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                            ) : (
                                <ChevronDownIcon
                                    data-icon="inline-start"
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                            )}
                            {showLines
                                ? "收起商品"
                                : common
                                  ? "按商品调整"
                                  : "按商品设置"}
                        </Button>
                    ) : null}
                </>
            }
        >
            {common ? (
                <div className="flex max-w-4xl items-start gap-3 rounded-lg border border-border/70 bg-surface-sunken/70 px-4 py-3">
                    <MapPinIcon
                        className="mt-0.5 size-4 shrink-0 text-muted-foreground"
                        aria-hidden="true"
                    />
                    <div className="min-w-0 space-y-1">
                        <p className="break-words font-medium text-foreground">
                            {common.recipient}
                            <span className="num ml-2.5 font-normal text-muted-foreground">
                                {common.phone}
                            </span>
                        </p>
                        <p className="break-words text-xs text-muted-foreground">
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
                    <div
                        data-slot="detail-inline-table"
                        className={detailWorkspaceStyles.inlineTableWrapper}
                    >
                        <Table data-density="comfortable">
                            <TableHeader>
                                <TableRow>
                                    <TableHead>商品</TableHead>
                                    <TableHead data-align="end">
                                        销售数量
                                    </TableHead>
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
                    </div>
                ) : null}
            </div>
        </DetailWorkspaceSection>
    )
}
