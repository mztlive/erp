"use client"

import * as React from "react"
import {
    CheckIcon,
    ChevronDownIcon,
    ChevronRightIcon,
    CopyIcon,
    MapPinIcon,
    PackageSearchIcon,
    Trash2Icon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    MoneyValue,
    detailWorkspaceStyles,
    taxAmountToneClass,
} from "@/components/business"
import { TableRowActions } from "@/components/business/table-row-actions"
import { toFieldErrors } from "@/components/form"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { DatePicker } from "@/components/ui/date-picker"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuGroup,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { StatusBadge } from "@/components/ui/status-badge"
import { TableCell, TableRow } from "@/components/ui/table"
import { RegionName } from "@/features/regions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { DeliveryCoverageHint as CoverageCode } from "../api/delivery-coverage"
import { allocationSummary } from "../lib/sales-delivery-allocation-summary"
import { addressKey } from "../lib/sales-delivery-import"
import type { SalesOrderCreateFormApi } from "../lib/sales-order-create-form-types"
import { calculateRowGrossProfit } from "../lib/sales-order-create-line-health"
import { calculateTotals } from "../lib/sales-order-create-model"
import {
    supplyAmount,
    supplyReferencePrice,
} from "../lib/sales-procurement-price"
import type {
    SalesLineProcurementResponsibility,
    SalesOrderDraftLineInput,
    SalesProcurementPriceBasis,
} from "../types"
import { SkuThumbnail } from "./sku-thumbnail"

const INLINE_DATE_PICKER_CLASS =
    "w-full [&_[data-variant=control]]:border-transparent [&_[data-variant=control]]:bg-muted/52 [&_[data-variant=control]]:hover:bg-muted/85 [&_[data-variant=control]]:focus-visible:border-primary/65 [&_[data-variant=control]]:focus-visible:bg-card"

export type SalesOrderCreateLineRowProps = {
    form: SalesOrderCreateFormApi
    line: SalesOrderDraftLineInput
    rowIndex: number
    taxRatePercent: string
    inquiryLocked: boolean
    displayUnit: string
    quantityFromDelivery: boolean
    selected: boolean
    imageAssetId?: string
    unifiedAddressKey?: string
    procurementOwner?: SalesLineProcurementResponsibility
    procurementStatus: { fetching: boolean; error: boolean }
    coverageHint: (sku: string, regionCode: string) => CoverageCode
    onToggleSelect: (rowKey: string, checked: boolean) => void
    onPickSku?: (rowIndex: number) => void
    onOpenDelivery: (rowKey: string) => void
    onDuplicate: (line: SalesOrderDraftLineInput, rowIndex: number) => void
    onRemove: (line: SalesOrderDraftLineInput, rowIndex: number) => void
}

function ProcurementOwnerInline({
    line,
    responsibility,
    status,
}: {
    line: SalesOrderDraftLineInput
    responsibility?: SalesLineProcurementResponsibility
    status: { fetching: boolean; error: boolean }
}) {
    return (
        <span
            className="inline-flex min-w-0 items-center gap-1"
            data-testid={`sales-line-procurement-owner-${line.rowKey}`}
        >
            {!line.sku.trim() ? (
                <span className="text-xs text-muted-foreground">待选商品</span>
            ) : status.fetching ? (
                <span className="text-xs text-muted-foreground">
                    采购匹配中…
                </span>
            ) : responsibility?.resolved && responsibility.ownerName ? (
                <span
                    className="truncate text-xs text-muted-foreground"
                    title={`采购负责人：${responsibility.ownerName}`}
                >
                    采购：{responsibility.ownerName}
                </span>
            ) : status.error ? (
                <StatusBadge
                    tone="destructive"
                    appearance="text"
                    label="采购匹配失败"
                    title="采购负责人暂时无法匹配，请稍后重试"
                />
            ) : (
                <StatusBadge
                    tone="destructive"
                    appearance="text"
                    label="待配置采购"
                    title="暂未匹配采购负责人，请联系管理员维护采购责任规则"
                />
            )}
        </span>
    )
}

/**
 * 实物与服务高密度明细行：
 * - 第一基准线对齐所有核心编辑控件（数量、成交单价、供给单价、交付日、收货地址、小计）；
 * - 第二行紧凑承载次级元信息（规格与采购负责人、取价方式与供给额、地址明细/进度、行毛利）。
 */
export const SalesOrderCreateLineRow = React.memo(
    function SalesOrderCreateLineRow({
        form,
        line,
        rowIndex,
        taxRatePercent,
        inquiryLocked,
        displayUnit,
        quantityFromDelivery,
        selected,
        imageAssetId,
        unifiedAddressKey,
        procurementOwner,
        procurementStatus,
        coverageHint,
        onToggleSelect,
        onPickSku,
        onOpenDelivery,
        onDuplicate,
        onRemove,
    }: SalesOrderCreateLineRowProps) {
        const seg = toAutomationIdSegment(line.rowKey)
        const linePrefix = `sales-orders-create-line-${seg}`
        const unitLocked = Boolean(line.sku.trim())
        const selectedLabel = line.name.trim() || line.sku.trim() || ""

        const lineGross = calculateTotals([line], taxRatePercent).gross
        const rowProfit = calculateRowGrossProfit(line)
        const profitSign =
            rowProfit === null ? 0 : compareDecimal(rowProfit, "0", 2)
        const supplyTotal = supplyAmount(
            line.procurementSupplyPrice,
            line.quantity,
        )
        const showSupplyTotal =
            supplyTotal !== null && compareDecimal(line.quantity, "1", 6) !== 0
        const missingReference =
            !inquiryLocked &&
            line.procurementPriceBasis &&
            !supplyReferencePrice(
                line.referencePrices,
                line.procurementPriceBasis,
            )
        const missingReferenceLabel =
            line.procurementPriceBasis === "BULK" ? "集采价" : "代发价"

        const allocations = line.deliveryAllocations ?? []
        const firstAddr = allocations[0]
        const addrSummary = allocationSummary({ ...line, unit: displayUnit })
        const isUnifiedMatch = Boolean(
            unifiedAddressKey &&
            allocations.length === 1 &&
            firstAddr &&
            addressKey(firstAddr) === unifiedAddressKey,
        )
        const coverageHints = allocations
            .map((row) =>
                row.region_code
                    ? coverageHint(line.sku, row.region_code)
                    : "NONE",
            )
            .filter((hint) => hint !== "NONE")
        const hasCoverageWarning = coverageHints.length > 0
        const coverageTitle = coverageHints.includes("NO_SUPPLY")
            ? "部分收货地区暂无可配送供给，点击查看详情"
            : "部分收货地区仅支持集采发货且需另付运费，点击查看详情"

        const setPriceBasis = (basis: SalesProcurementPriceBasis) => {
            if (inquiryLocked) return
            form.setFieldValue(
                `lineItems[${rowIndex}].procurementPriceBasis`,
                basis,
            )
            form.setFieldValue(
                `lineItems[${rowIndex}].procurementSupplyPrice`,
                supplyReferencePrice(line.referencePrices, basis),
            )
        }

        return (
            <TableRow
                data-state={selected ? "selected" : undefined}
                className="group/row"
            >
                {/* 1. 勾选 + 行号 */}
                <TableCell className="w-14 py-2.5 align-top">
                    <div className="flex h-control-sm items-center gap-1.5">
                        {!inquiryLocked ? (
                            <NativeCheckbox
                                id={`${linePrefix}-select`}
                                aria-label={`选择第 ${rowIndex + 1} 项 ${line.name || "商品"}`}
                                checked={selected}
                                onCheckedChange={(checked) =>
                                    onToggleSelect(line.rowKey, checked)
                                }
                            />
                        ) : null}
                        <span className="num text-xs text-muted-foreground">
                            {rowIndex + 1}
                        </span>
                    </div>
                </TableCell>

                {/* 2. 商品与采购负责人 */}
                <TableCell className="min-w-52 max-w-72 whitespace-normal py-2.5 align-top">
                    <form.AppField name={`lineItems[${rowIndex}].name`}>
                        {() => null}
                    </form.AppField>
                    <form.AppField name={`lineItems[${rowIndex}].sku`}>
                        {(field) => {
                            const isInvalid =
                                field.state.meta.isTouched &&
                                !field.state.meta.isValid
                            const errors = toFieldErrors(
                                field.state.meta.errors,
                            )
                            return (
                                <Field
                                    className="gap-0.5"
                                    data-invalid={isInvalid || undefined}
                                >
                                    <div className="flex items-start gap-2">
                                        {line.sku ? (
                                            <SkuThumbnail
                                                assetId={imageAssetId}
                                                label={selectedLabel || "商品"}
                                                className="mt-0.5 size-8 shrink-0 rounded-md"
                                            />
                                        ) : (
                                            <span className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-md bg-muted/70 text-muted-foreground">
                                                <PackageSearchIcon
                                                    className="size-4"
                                                    aria-hidden="true"
                                                />
                                            </span>
                                        )}
                                        <div className="min-w-0 flex-1 space-y-0.5">
                                            {inquiryLocked ? (
                                                <p className="line-clamp-2 font-medium text-foreground wrap-anywhere">
                                                    {selectedLabel || "商品"}
                                                </p>
                                            ) : (
                                                <button
                                                    id={`${linePrefix}-pick-sku`}
                                                    type="button"
                                                    aria-label={
                                                        selectedLabel
                                                            ? `更换销售项目 ${selectedLabel}`
                                                            : "选择商品"
                                                    }
                                                    className="group/sku -mx-1 -my-0.5 flex w-[calc(100%+0.5rem)] cursor-pointer items-baseline gap-1.5 rounded-sm px-1 py-0.5 text-left transition-colors hover:bg-muted/70 focus-visible:outline-2 focus-visible:outline-ring"
                                                    onClick={() =>
                                                        onPickSku?.(rowIndex)
                                                    }
                                                >
                                                    <span
                                                        className={cn(
                                                            "min-w-0 flex-1 line-clamp-2 wrap-anywhere",
                                                            selectedLabel
                                                                ? "font-medium text-foreground"
                                                                : "text-muted-foreground",
                                                        )}
                                                        title={selectedLabel}
                                                    >
                                                        {selectedLabel ||
                                                            "选择商品"}
                                                    </span>
                                                    {selectedLabel ? (
                                                        <span className="shrink-0 text-xs text-muted-foreground/70 opacity-0 transition-opacity group-hover/sku:opacity-100 group-hover/row:opacity-100 group-focus-visible/sku:opacity-100 group-hover/sku:text-primary">
                                                            更换
                                                        </span>
                                                    ) : null}
                                                </button>
                                            )}
                                            <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                                                {line.specification ? (
                                                    <>
                                                        <span
                                                            className="max-w-36 truncate"
                                                            title={
                                                                line.specification
                                                            }
                                                        >
                                                            {line.specification}
                                                        </span>
                                                        <span
                                                            aria-hidden="true"
                                                            className="text-muted-foreground/40"
                                                        >
                                                            ·
                                                        </span>
                                                    </>
                                                ) : null}
                                                <ProcurementOwnerInline
                                                    line={line}
                                                    responsibility={
                                                        procurementOwner
                                                    }
                                                    status={procurementStatus}
                                                />
                                            </div>
                                        </div>
                                    </div>
                                    {isInvalid ? (
                                        <FieldError errors={errors} />
                                    ) : null}
                                </Field>
                            )
                        }}
                    </form.AppField>
                </TableCell>

                {/* 3. 数量 / 单位 */}
                <TableCell data-align="end" className="w-26 py-2.5 align-top">
                    <div className="flex items-start justify-end gap-1">
                        <div className="w-18 shrink-0">
                            <form.AppField
                                name={`lineItems[${rowIndex}].quantity`}
                                listeners={{
                                    // 只有一个收货地址时，该地址承担全部数量。
                                    onChange: ({ value }) => {
                                        const currentAllocations =
                                            form.getFieldValue(
                                                `lineItems[${rowIndex}].deliveryAllocations`,
                                            ) ?? []
                                        if (currentAllocations.length !== 1)
                                            return
                                        form.setFieldValue(
                                            `lineItems[${rowIndex}].deliveryAllocations[0].quantity`,
                                            value,
                                        )
                                    },
                                }}
                            >
                                {(field) => {
                                    const invalid =
                                        field.state.meta.isTouched &&
                                        !field.state.meta.isValid
                                    return (
                                        <Field
                                            className="gap-0.5"
                                            data-invalid={invalid || undefined}
                                        >
                                            <FieldLabel
                                                htmlFor={`${linePrefix}-quantity`}
                                                className="sr-only"
                                            >
                                                数量
                                            </FieldLabel>
                                            <Input
                                                id={`${linePrefix}-quantity`}
                                                name={field.name}
                                                type="text"
                                                inputMode="decimal"
                                                readOnly={quantityFromDelivery}
                                                title={
                                                    quantityFromDelivery
                                                        ? "数量由询价配送分配合计"
                                                        : undefined
                                                }
                                                value={field.state.value}
                                                aria-invalid={
                                                    invalid || undefined
                                                }
                                                className="h-control-sm num text-right"
                                                onBlur={field.handleBlur}
                                                onChange={(event) =>
                                                    field.handleChange(
                                                        event.target.value,
                                                    )
                                                }
                                            />
                                            {invalid ? (
                                                <FieldError
                                                    errors={toFieldErrors(
                                                        field.state.meta.errors,
                                                    )}
                                                />
                                            ) : null}
                                        </Field>
                                    )
                                }}
                            </form.AppField>
                        </div>
                        <span
                            className="flex h-control-sm min-w-5 shrink-0 items-center text-left text-xs text-muted-foreground"
                            title={
                                unitLocked
                                    ? "单位随商品基础单位带出，不可改"
                                    : "选择商品后带出基础单位"
                            }
                        >
                            {displayUnit || "—"}
                        </span>
                    </div>
                </TableCell>

                {/* 4. 含税成交单价 */}
                <TableCell data-align="end" className="w-32 py-2.5 align-top">
                    <div className="space-y-1">
                        <form.AppField
                            name={`lineItems[${rowIndex}].pricingMode`}
                        >
                            {(modeField) => (
                                <form.AppField
                                    name={`lineItems[${rowIndex}].unitPriceGross`}
                                >
                                    {(field) => {
                                        const invalid =
                                            field.state.meta.isTouched &&
                                            !field.state.meta.isValid
                                        return (
                                            <Field
                                                className="gap-0.5"
                                                data-invalid={
                                                    invalid || undefined
                                                }
                                            >
                                                <FieldLabel
                                                    htmlFor={`${linePrefix}-unit-price`}
                                                    className="sr-only"
                                                >
                                                    含税成交单价
                                                </FieldLabel>
                                                <Input
                                                    id={`${linePrefix}-unit-price`}
                                                    name={field.name}
                                                    type="text"
                                                    inputMode="decimal"
                                                    value={field.state.value}
                                                    placeholder="请填写"
                                                    aria-invalid={
                                                        invalid || undefined
                                                    }
                                                    className="h-control-sm num text-right"
                                                    onBlur={field.handleBlur}
                                                    onChange={(event) => {
                                                        modeField.handleChange(
                                                            "MANUAL",
                                                        )
                                                        form.setFieldValue(
                                                            `lineItems[${rowIndex}].priceSource`,
                                                            undefined,
                                                        )
                                                        field.handleChange(
                                                            event.target.value,
                                                        )
                                                    }}
                                                />
                                                {invalid ? (
                                                    <FieldError
                                                        errors={toFieldErrors(
                                                            field.state.meta
                                                                .errors,
                                                        )}
                                                    />
                                                ) : null}
                                            </Field>
                                        )
                                    }}
                                </form.AppField>
                            )}
                        </form.AppField>
                    </div>
                </TableCell>

                {/* 5. 采购供给（前置代发/集采下拉 + 单价输入） */}
                <TableCell data-align="end" className="w-44 py-2.5 align-top">
                    <div className="space-y-1">
                        {inquiryLocked ? (
                            <div className="flex h-control-sm items-center justify-end gap-1.5">
                                <span className="text-xs text-muted-foreground">
                                    询价覆盖
                                </span>
                                <MoneyValue
                                    value={line.procurementSupplyPrice || null}
                                />
                            </div>
                        ) : (
                            <form.AppField
                                name={`lineItems[${rowIndex}].procurementSupplyPrice`}
                            >
                                {(field) => {
                                    const invalid =
                                        field.state.meta.isTouched &&
                                        !field.state.meta.isValid
                                    const basisShortLabel =
                                        line.procurementPriceBasis === "BULK"
                                            ? "集采"
                                            : line.procurementPriceBasis ===
                                                "DROPSHIP"
                                              ? "代发"
                                              : "取价"
                                    const basisFullLabel =
                                        line.procurementPriceBasis === "BULK"
                                            ? "集采"
                                            : line.procurementPriceBasis ===
                                                "DROPSHIP"
                                              ? "一件代发"
                                              : "未选择"
                                    const bulkMinQty =
                                        line.referencePrices?.bulkMinQuantity?.trim()
                                    const basisOptions = [
                                        {
                                            value: "DROPSHIP" as const,
                                            label: "一件代发",
                                            hint: undefined,
                                            refPrice: supplyReferencePrice(
                                                line.referencePrices,
                                                "DROPSHIP",
                                            ),
                                        },
                                        {
                                            value: "BULK" as const,
                                            label: "集采",
                                            hint: bulkMinQty
                                                ? `满 ${bulkMinQty}${displayUnit || ""}起`
                                                : undefined,
                                            refPrice: supplyReferencePrice(
                                                line.referencePrices,
                                                "BULK",
                                            ),
                                        },
                                    ]
                                    return (
                                        <Field
                                            className="gap-0.5"
                                            data-invalid={invalid || undefined}
                                        >
                                            <FieldLabel
                                                htmlFor={`${linePrefix}-supply-price`}
                                                className="sr-only"
                                            >
                                                采购供给单价
                                            </FieldLabel>
                                            <InputGroup className="h-control-sm">
                                                <InputGroupAddon
                                                    align="inline-start"
                                                    className="h-full cursor-default border-r border-border/85 p-0 has-[>button]:ml-0"
                                                >
                                                    <DropdownMenu>
                                                        <DropdownMenuTrigger
                                                            id={`${linePrefix}-supply-basis`}
                                                            render={
                                                                <button
                                                                    type="button"
                                                                    aria-label={`取价方式：${basisFullLabel}，点击切换`}
                                                                    className={cn(
                                                                        "inline-flex h-full shrink-0 cursor-pointer items-center gap-0.5 rounded-l-md px-2 text-xs font-medium transition-colors hover:bg-card/85 hover:text-primary focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring aria-expanded:bg-sidebar-accent aria-expanded:text-sidebar-accent-foreground",
                                                                        line.procurementPriceBasis
                                                                            ? "text-foreground/85"
                                                                            : "text-muted-foreground",
                                                                    )}
                                                                />
                                                            }
                                                        >
                                                            <span>
                                                                {
                                                                    basisShortLabel
                                                                }
                                                            </span>
                                                            <ChevronDownIcon
                                                                className="size-3 shrink-0 opacity-70"
                                                                aria-hidden="true"
                                                            />
                                                        </DropdownMenuTrigger>
                                                        <DropdownMenuContent
                                                            align="start"
                                                            className="w-60 max-w-[calc(100vw-2rem)]"
                                                        >
                                                            <DropdownMenuGroup>
                                                                <DropdownMenuLabel>
                                                                    切换取价方式（自动带入参考价）
                                                                </DropdownMenuLabel>
                                                                {basisOptions.map(
                                                                    (
                                                                        option,
                                                                    ) => {
                                                                        const active =
                                                                            line.procurementPriceBasis ===
                                                                            option.value
                                                                        return (
                                                                            <DropdownMenuItem
                                                                                key={
                                                                                    option.value
                                                                                }
                                                                                id={`${linePrefix}-supply-basis-${option.value.toLowerCase()}`}
                                                                                onClick={() =>
                                                                                    setPriceBasis(
                                                                                        option.value,
                                                                                    )
                                                                                }
                                                                            >
                                                                                <CheckIcon
                                                                                    className={cn(
                                                                                        "size-3.5 shrink-0 text-primary",
                                                                                        active
                                                                                            ? "opacity-100"
                                                                                            : "opacity-0",
                                                                                    )}
                                                                                    aria-hidden="true"
                                                                                />
                                                                                <div className="flex min-w-0 flex-1 flex-col">
                                                                                    <span className="text-body-compact font-medium">
                                                                                        {
                                                                                            option.label
                                                                                        }
                                                                                    </span>
                                                                                    {option.hint ? (
                                                                                        <span className="num text-tiny text-muted-foreground">
                                                                                            {
                                                                                                option.hint
                                                                                            }
                                                                                        </span>
                                                                                    ) : null}
                                                                                </div>
                                                                                <span className="num ml-2 shrink-0 text-xs text-muted-foreground">
                                                                                    {option.refPrice ? (
                                                                                        <MoneyValue
                                                                                            value={
                                                                                                option.refPrice
                                                                                            }
                                                                                        />
                                                                                    ) : (
                                                                                        "未配置"
                                                                                    )}
                                                                                </span>
                                                                            </DropdownMenuItem>
                                                                        )
                                                                    },
                                                                )}
                                                            </DropdownMenuGroup>
                                                        </DropdownMenuContent>
                                                    </DropdownMenu>
                                                </InputGroupAddon>
                                                <InputGroupInput
                                                    id={`${linePrefix}-supply-price`}
                                                    name={field.name}
                                                    type="text"
                                                    inputMode="decimal"
                                                    value={
                                                        field.state.value ?? ""
                                                    }
                                                    placeholder="请填写"
                                                    aria-invalid={
                                                        invalid || undefined
                                                    }
                                                    className="num pr-2 text-right"
                                                    onBlur={field.handleBlur}
                                                    onChange={(event) =>
                                                        field.handleChange(
                                                            event.target.value,
                                                        )
                                                    }
                                                />
                                            </InputGroup>
                                            {invalid ? (
                                                <FieldError
                                                    errors={toFieldErrors(
                                                        field.state.meta.errors,
                                                    )}
                                                />
                                            ) : null}
                                        </Field>
                                    )
                                }}
                            </form.AppField>
                        )}

                        {missingReference || showSupplyTotal ? (
                            <div className="flex flex-wrap items-center justify-end gap-1.5 text-xs">
                                {missingReference ? (
                                    <span
                                        className="text-tiny text-warning-soft-foreground"
                                        title={`未配置${missingReferenceLabel}，请手填供给单价`}
                                    >
                                        缺{missingReferenceLabel}
                                    </span>
                                ) : showSupplyTotal ? (
                                    <span className="num text-tiny text-muted-foreground">
                                        额 <MoneyValue value={supplyTotal} />
                                    </span>
                                ) : null}
                            </div>
                        ) : null}
                    </div>
                </TableCell>

                {/* 6. 期望交付日 */}
                <TableCell className="w-34 py-2.5 align-top">
                    <form.AppField name={`lineItems[${rowIndex}].dueDate`}>
                        {(field) => {
                            const invalid =
                                field.state.meta.isTouched &&
                                !field.state.meta.isValid
                            return (
                                <Field
                                    className="gap-0.5"
                                    data-invalid={invalid || undefined}
                                >
                                    <DatePicker
                                        id={`${linePrefix}-due-date`}
                                        size="sm"
                                        disabled={inquiryLocked}
                                        placeholder="选择日期"
                                        value={field.state.value || undefined}
                                        onValueChange={(next) =>
                                            field.handleChange(next ?? "")
                                        }
                                        aria-invalid={invalid || undefined}
                                        className={INLINE_DATE_PICKER_CLASS}
                                    />
                                    {invalid ? (
                                        <FieldError
                                            errors={toFieldErrors(
                                                field.state.meta.errors,
                                            )}
                                        />
                                    ) : null}
                                </Field>
                            )
                        }}
                    </form.AppField>
                </TableCell>

                {/* 7. 收货地址（直接集成在主表行内，点击滑出右侧工作台） */}
                <TableCell className="min-w-44 max-w-56 whitespace-normal py-2.5 align-top">
                    <button
                        id={`sales-delivery-${seg}-edit`}
                        type="button"
                        onClick={() => onOpenDelivery(line.rowKey)}
                        className={cn(
                            "group/addr -mx-1.5 -my-1 flex w-[calc(100%+0.75rem)] cursor-pointer flex-col items-start gap-0.5 rounded-md px-1.5 py-1 text-left transition-colors",
                            "hover:bg-muted/70 focus-visible:outline-2 focus-visible:outline-ring",
                        )}
                    >
                        {allocations.length === 0 ? (
                            <span className="flex h-control-sm w-full items-center gap-1 text-xs text-muted-foreground group-hover/addr:text-primary">
                                <MapPinIcon
                                    className="size-3.5 shrink-0 opacity-70"
                                    aria-hidden="true"
                                />
                                <span>
                                    {quantityFromDelivery
                                        ? "选择转入地址"
                                        : inquiryLocked
                                          ? "无实物地址"
                                          : "设置地址"}
                                </span>
                                <ChevronRightIcon
                                    className="ml-auto size-3.5 shrink-0 opacity-0 transition-opacity group-hover/addr:opacity-100"
                                    aria-hidden="true"
                                />
                            </span>
                        ) : allocations.length === 1 && firstAddr ? (
                            <>
                                <span className="flex w-full items-center gap-1 text-body-compact">
                                    <span className="min-w-0 truncate font-medium text-foreground">
                                        {firstAddr.recipient || "待填收货人"}
                                    </span>
                                    {isUnifiedMatch ? (
                                        <span className="shrink-0 rounded-sm bg-success-soft px-1 text-tiny font-medium text-success-soft-foreground">
                                            统一
                                        </span>
                                    ) : null}
                                    {hasCoverageWarning ? (
                                        <span
                                            title={coverageTitle}
                                            className="inline-flex shrink-0"
                                        >
                                            <TriangleAlertIcon
                                                className="size-3.5 text-warning"
                                                aria-label={coverageTitle}
                                            />
                                        </span>
                                    ) : null}
                                    <ChevronRightIcon
                                        className="ml-auto size-3.5 shrink-0 text-muted-foreground/45 transition-colors group-hover/addr:text-primary"
                                        aria-hidden="true"
                                    />
                                </span>
                                <span
                                    className={cn(
                                        "line-clamp-1 w-full text-xs",
                                        addrSummary.complete
                                            ? "text-muted-foreground"
                                            : "text-warning-soft-foreground",
                                    )}
                                    title={`${firstAddr.recipient} ${firstAddr.phone} ${firstAddr.address}`}
                                >
                                    {!addrSummary.complete ? (
                                        addrSummary.status || "地址待完善"
                                    ) : (
                                        <>
                                            {firstAddr.region_name ? (
                                                firstAddr.region_name
                                            ) : firstAddr.region_code ? (
                                                <RegionName
                                                    code={firstAddr.region_code}
                                                />
                                            ) : null}
                                            {firstAddr.address}
                                        </>
                                    )}
                                </span>
                            </>
                        ) : (
                            <>
                                <span className="flex w-full items-center gap-1.5">
                                    <span className="num font-medium text-foreground">
                                        {allocations.length} 个地址
                                    </span>
                                    {!addrSummary.complete ? (
                                        <span className="rounded-sm bg-warning-soft px-1 text-tiny font-medium text-warning-soft-foreground">
                                            待核对
                                        </span>
                                    ) : null}
                                    {hasCoverageWarning ? (
                                        <span
                                            title={coverageTitle}
                                            className="inline-flex shrink-0"
                                        >
                                            <TriangleAlertIcon
                                                className="size-3.5 text-warning"
                                                aria-label={coverageTitle}
                                            />
                                        </span>
                                    ) : null}
                                    <ChevronRightIcon
                                        className="ml-auto size-3.5 shrink-0 text-muted-foreground/45 transition-colors group-hover/addr:text-primary"
                                        aria-hidden="true"
                                    />
                                </span>
                                <span
                                    className={cn(
                                        "num line-clamp-1 w-full text-xs",
                                        addrSummary.complete
                                            ? "text-muted-foreground"
                                            : "text-warning-soft-foreground",
                                    )}
                                >
                                    {addrSummary.complete
                                        ? addrSummary.summary
                                        : addrSummary.status ||
                                          addrSummary.summary}
                                </span>
                            </>
                        )}
                    </button>
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

                {/* 8. 含税小计 + 预估行毛利 */}
                <TableCell data-align="end" className="w-28 py-2.5 align-top">
                    <div className="space-y-0.5">
                        <div className="flex h-control-sm items-center justify-end font-medium">
                            <MoneyValue
                                value={lineGross}
                                className={taxAmountToneClass("含税小计")}
                            />
                        </div>
                        <div className="num text-right text-xs">
                            {rowProfit === null ? (
                                <span className="text-muted-foreground/75">
                                    —
                                </span>
                            ) : (
                                <span
                                    title="该行预估含税毛利 =（成交单价 − 采购供给单价）× 数量"
                                    className={cn(
                                        profitSign < 0
                                            ? "font-medium text-destructive"
                                            : profitSign > 0
                                              ? "text-success-soft-foreground"
                                              : "text-muted-foreground",
                                    )}
                                >
                                    毛利 {profitSign > 0 ? "+" : ""}
                                    <MoneyValue value={rowProfit} />
                                </span>
                            )}
                        </div>
                    </div>
                </TableCell>

                {/* 9. 操作 */}
                <TableCell className="w-18 py-2.5 text-right align-top">
                    {inquiryLocked ? (
                        <button
                            id={`${linePrefix}-open-delivery-action`}
                            type="button"
                            className={cn(
                                "h-control-sm cursor-pointer rounded-md px-2 text-xs",
                                detailWorkspaceStyles.inlineLinkButton,
                            )}
                            onClick={() => onOpenDelivery(line.rowKey)}
                        >
                            分配
                        </button>
                    ) : (
                        <TableRowActions
                            moreId={`${linePrefix}-more`}
                            moreLabel={`${line.name || `第 ${rowIndex + 1} 项`}更多操作`}
                            maxInline={1}
                            actions={[
                                {
                                    id: `sales-orders-create-line-items-row-${seg}-remove`,
                                    label: "删除",
                                    icon: Trash2Icon,
                                    destructive: true,
                                    placement: "inline",
                                    onClick: () => onRemove(line, rowIndex),
                                },
                                {
                                    id: `${linePrefix}-edit-delivery-menu`,
                                    label: "收货地址",
                                    icon: MapPinIcon,
                                    placement: "menu",
                                    onClick: () => onOpenDelivery(line.rowKey),
                                },
                                {
                                    id: `${linePrefix}-duplicate`,
                                    label: "复制一行",
                                    icon: CopyIcon,
                                    placement: "menu",
                                    onClick: () => onDuplicate(line, rowIndex),
                                },
                            ]}
                        />
                    )}
                </TableCell>
            </TableRow>
        )
    },
)
