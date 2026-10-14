"use client"

import { RequiredMark } from "@/components/ui/required-mark"

import { useRef } from "react"
import { PlusIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { toFieldErrors } from "@/components/form"
import { FieldError } from "@/components/ui/field"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compactFixed,
    compareDecimal,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import type { DeliveryCoverageHint as DeliveryCoverageHintCode } from "../api/delivery-coverage"
import { useDeliveryCoverage } from "../hooks/use-delivery-coverage"
import { toast } from "@/components/ui/toast"
import type { SalesOrderCreateFormApi } from "../lib/sales-order-create-form-types"
import {
    applyDeliveryAddressToLines,
    type CreateSalesOrderFormValues,
    type UnifiedDeliveryAddress,
} from "../lib/sales-order-create-model"
import { DeliveryCoverageHint } from "./delivery-coverage-hint"
import { DeliveryRegionField } from "./delivery-region-field"
import { SalesOrderCreateUnifiedAddressDialog } from "./sales-order-create-unified-address-dialog"

type DeliveryLine = CreateSalesOrderFormValues["lineItems"][number]

/** 开单与改单共用的分地址进度：数量合计与收货资料是否齐全。 */
export function allocationSummary(line: {
    deliveryAllocations?: {
        recipient: string
        phone: string
        region_code: string
        address: string
        quantity: string
    }[]
    quantity: string
    unit: string
}) {
    const rows = line.deliveryAllocations ?? []
    if (!rows.length)
        return {
            summary: "未指定，可在履约时填写",
            status: "",
            complete: false,
        }
    const detailsComplete = rows.every(
        (row) =>
            row.recipient.trim() &&
            row.recipient.trim().length <= 64 &&
            row.phone.trim() &&
            row.phone.trim().length <= 32 &&
            row.region_code &&
            row.address.trim() &&
            row.address.trim().length <= 512,
    )
    try {
        const options = { maxScale: 6, outputScale: 6 }
        const total = sumFixed(
            rows.map((row) => row.quantity || "0"),
            options,
        )
        const comparison = compareDecimal(total, line.quantity, 6)
        const summary = `已分配 ${compactFixed(total)} / ${compactFixed(line.quantity)} ${line.unit}`
        if (
            rows.some((row) => compareDecimal(row.quantity || "0", "0", 6) <= 0)
        ) {
            return {
                summary,
                status: "请填写大于 0 的分配数量",
                complete: false,
            }
        }
        if (comparison !== 0) {
            const difference =
                comparison < 0
                    ? subtractFixed(line.quantity, total, options)
                    : subtractFixed(total, line.quantity, options)
            return {
                summary,
                status: `${comparison < 0 ? "还差" : "超出"} ${compactFixed(difference)} ${line.unit}`,
                complete: false,
            }
        }
        return {
            summary,
            status: detailsComplete ? "已填写完整" : "待填写收货资料",
            complete: detailsComplete,
        }
    } catch {
        return {
            summary: "数量待核对",
            status: "请填写有效数量（最多 6 位小数）",
            complete: false,
        }
    }
}

const addressGrid =
    "grid gap-3 sm:grid-cols-2 @3xl/delivery:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)_minmax(0,2.6fr)_minmax(0,1.8fr)_7rem_3.5rem]"

function DeliveryAllocationGroup({
    form,
    line,
    rowIndex,
    coverageHint,
}: {
    form: SalesOrderCreateFormApi
    line: DeliveryLine
    rowIndex: number
    coverageHint: (sku: string, regionCode: string) => DeliveryCoverageHintCode
}) {
    const prefix = `sales-delivery-${toAutomationIdSegment(line.rowKey)}`
    const allocations = line.deliveryAllocations ?? []
    // 地址没有服务端 ID；用组内单调递增的编辑键，删除前行后其余输入 ID 不变。
    const identity = useRef({ sequence: 0, keys: [] as string[] })
    while (identity.current.keys.length < allocations.length) {
        identity.current.keys.push(`address-${++identity.current.sequence}`)
    }
    identity.current.keys.length = allocations.length
    const summary = allocationSummary(line)
    const labels = [
        ["recipient", "收货人"],
        ["phone", "联系电话"],
        ["region_code", "收货地区"],
        ["address", "详细地址"],
        ["quantity", `数量（${line.unit || "基础单位"}）`],
    ] as const

    return (
        <div className="min-w-0 space-y-3">
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 space-y-1.5">
                    <h3 className="break-words text-sm font-medium">
                        {line.name || `商品 ${rowIndex + 1}`}
                    </h3>
                    <div
                        className="flex flex-wrap items-center gap-x-3 gap-y-1.5 text-xs text-muted-foreground"
                        aria-live="polite"
                    >
                        <span className="num">{summary.summary}</span>
                        {summary.status ? (
                            <Badge
                                variant={
                                    summary.complete ? "success" : "warning"
                                }
                            >
                                {summary.status}
                            </Badge>
                        ) : null}
                    </div>
                </div>
                <Button
                    id={`${prefix}-add`}
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={allocations.length >= 100}
                    onClick={() =>
                        form.setFieldValue(
                            `lineItems[${rowIndex}].deliveryAllocations`,
                            [
                                ...allocations,
                                {
                                    recipient: "",
                                    phone: "",
                                    region_code: "",
                                    address: "",
                                    quantity: allocations.length
                                        ? ""
                                        : line.quantity,
                                },
                            ],
                        )
                    }
                >
                    <PlusIcon aria-hidden="true" />
                    添加收货地址
                </Button>
            </div>
            {allocations.length ? (
                <div>
                    <div
                        aria-hidden="true"
                        className={`${addressGrid} hidden border-b border-grid bg-muted/30 px-3 py-2 text-xs text-muted-foreground @3xl/delivery:grid`}
                    >
                        {labels.map(([name, label]) => (
                            <span key={name}>
                                {label}
                                <RequiredMark />
                            </span>
                        ))}
                        <span className="text-center">操作</span>
                    </div>
                    <div className="divide-y divide-grid">
                        {allocations.map((_, allocationIndex) => {
                            const addressKey =
                                identity.current.keys[allocationIndex]
                            return (
                                <div
                                    key={addressKey}
                                    className={`${addressGrid} items-start px-0 py-3 @3xl/delivery:px-3`}
                                >
                                    {labels.map(([name, label]) =>
                                        name === "region_code" ? (
                                            <form.AppField
                                                key={name}
                                                name={`lineItems[${rowIndex}].deliveryAllocations[${allocationIndex}].region_code`}
                                            >
                                                {(field) => (
                                                    <DeliveryRegionField
                                                        id={`${prefix}-${addressKey}-region`}
                                                        label={label}
                                                        required
                                                        labelClassName="@3xl/delivery:sr-only"
                                                        className="sm:col-span-2 @3xl/delivery:col-span-1"
                                                        value={
                                                            field.state.value ??
                                                            ""
                                                        }
                                                        onValueChange={(code) =>
                                                            field.handleChange(
                                                                code,
                                                            )
                                                        }
                                                        onBlur={
                                                            field.handleBlur
                                                        }
                                                        invalid={
                                                            field.state.meta
                                                                .isTouched &&
                                                            !field.state.meta
                                                                .isValid
                                                        }
                                                        errors={toFieldErrors(
                                                            field.state.meta
                                                                .errors,
                                                        )}
                                                        hint={
                                                            field.state
                                                                .value ? (
                                                                <DeliveryCoverageHint
                                                                    id={`${prefix}-${addressKey}-coverage`}
                                                                    hint={coverageHint(
                                                                        line.sku,
                                                                        field
                                                                            .state
                                                                            .value,
                                                                    )}
                                                                />
                                                            ) : null
                                                        }
                                                    />
                                                )}
                                            </form.AppField>
                                        ) : (
                                            <form.AppField
                                                key={name}
                                                name={`lineItems[${rowIndex}].deliveryAllocations[${allocationIndex}].${name}`}
                                            >
                                                {(field) => (
                                                    <field.TextField
                                                        id={`${prefix}-${addressKey}-${name}`}
                                                        label={label}
                                                        required
                                                        className={`min-w-0 gap-2 @3xl/delivery:[&>[data-slot=field-label]]:sr-only ${name === "address" ? "sm:col-span-2 @3xl/delivery:col-span-1" : ""}`}
                                                        inputClassName={
                                                            name === "quantity"
                                                                ? "num"
                                                                : undefined
                                                        }
                                                        placeholder={
                                                            name === "address"
                                                                ? "街道、门牌号"
                                                                : undefined
                                                        }
                                                        step={
                                                            name === "quantity"
                                                                ? "any"
                                                                : undefined
                                                        }
                                                        type={
                                                            name === "quantity"
                                                                ? "number"
                                                                : name ===
                                                                    "phone"
                                                                  ? "tel"
                                                                  : "text"
                                                        }
                                                    />
                                                )}
                                            </form.AppField>
                                        ),
                                    )}
                                    <Button
                                        id={`${prefix}-${addressKey}-remove`}
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        aria-label={`移除${line.name || "商品"}的第 ${allocationIndex + 1} 个收货地址`}
                                        className="self-end justify-self-end text-muted-foreground @3xl/delivery:mt-0.5 @3xl/delivery:self-start"
                                        onClick={() => {
                                            identity.current.keys.splice(
                                                allocationIndex,
                                                1,
                                            )
                                            form.setFieldValue(
                                                `lineItems[${rowIndex}].deliveryAllocations`,
                                                allocations.filter(
                                                    (_, index) =>
                                                        index !==
                                                        allocationIndex,
                                                ),
                                            )
                                        }}
                                    >
                                        移除
                                    </Button>
                                </div>
                            )
                        })}
                    </div>
                </div>
            ) : null}
            <form.AppField name={`lineItems[${rowIndex}].deliveryAllocations`}>
                {(field) => (
                    <FieldError
                        errors={toFieldErrors(field.state.meta.errors)}
                    />
                )}
            </form.AppField>
        </div>
    )
}

/** 每条商品独立分配地址，选品册按份展开的商品使用相同数量规则。 */
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
    lines: CreateSalesOrderFormValues["lineItems"]
}) {
    const coverageHint = useDeliveryCoverage(lines)
    const firstAddress = lines
        .flatMap((line) => line.deliveryAllocations ?? [])
        .find((row) => row.recipient || row.region_code || row.address)
    const applyUnifiedAddress = (address: UnifiedDeliveryAddress) => {
        const next = applyDeliveryAddressToLines(
            form.getFieldValue("lineItems"),
            address,
        )
        form.setFieldValue("lineItems", next)
        toast.add({
            title: "已设置统一收货地址",
            description: `${next.length} 个商品均按全部数量送到该地址。`,
            type: "success",
            timeout: 3000,
        })
    }
    return (
        <section
            className="@container/delivery min-w-0 space-y-5 border-t border-grid p-4 sm:p-5"
            aria-labelledby="sales-delivery-title"
        >
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 space-y-1">
                    <div className="flex flex-wrap items-baseline gap-2">
                        <h2
                            id="sales-delivery-title"
                            className="font-heading text-base font-semibold"
                        >
                            分地址交付
                        </h2>
                        <span className="text-sm text-muted-foreground">
                            选填
                        </span>
                    </div>
                    <p className="text-xs leading-relaxed text-muted-foreground">
                        全部商品送同一地址时，用“设置统一收货地址”一次填好；需要分开送时逐个商品添加地址，各地址数量合计须等于商品数量。也可在履约时补充。
                    </p>
                </div>
                <SalesOrderCreateUnifiedAddressDialog
                    lineCount={lines.length}
                    allocatedLineCount={
                        lines.filter(
                            (line) => (line.deliveryAllocations ?? []).length,
                        ).length
                    }
                    initialAddress={
                        firstAddress
                            ? {
                                  recipient: firstAddress.recipient,
                                  phone: firstAddress.phone,
                                  region_code: firstAddress.region_code,
                                  address: firstAddress.address,
                              }
                            : undefined
                    }
                    onApply={applyUnifiedAddress}
                />
            </div>
            <div className="divide-y divide-grid space-y-5 [&>div:not(:first-child)]:pt-5">
                {lines.map((line, rowIndex) => (
                    <DeliveryAllocationGroup
                        key={line.rowKey}
                        form={form}
                        line={line}
                        rowIndex={rowIndex}
                        coverageHint={coverageHint}
                    />
                ))}
            </div>
        </section>
    )
}
