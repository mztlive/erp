"use client"

import * as React from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import {
    Card,
    CardAction,
    CardContent,
    CardDescription,
    CardFooter,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import {
    NumericValue,
    ShanghaiTime,
    type DomainDateTime,
    type DomainPanelProps,
} from "@/components/business/domain-shared"
import { cn } from "@/lib/utils"

export type CardVoucherFieldKey =
    | "category"
    | "fulfillmentDeadline"
    | "faceValue"
    | "cardCount"
    | "unitPriceGross"
    | "transactionAmount"
    | "faceValueTotal"
    | "giftAmount"
    | "giftRate"
    | "cardForm"

export type CardVoucherDisplayValues = Readonly<{
    category: React.ReactNode
    fulfillmentDeadline: DomainDateTime
    faceValue: React.ReactNode
    cardCount: React.ReactNode
    unitPriceGross: React.ReactNode
    transactionAmount: React.ReactNode
    faceValueTotal: React.ReactNode
    giftAmount: React.ReactNode
    giftRate: React.ReactNode
    cardForm: React.ReactNode
}>

export type CardVoucherDerivedValues = Readonly<
    Pick<
        CardVoucherDisplayValues,
        "transactionAmount" | "faceValueTotal" | "giftAmount" | "giftRate"
    >
>

export type CardVoucherEditFields = Readonly<{
    category: React.ReactNode
    fulfillmentDeadline: React.ReactNode
    faceValue: React.ReactNode
    cardCount: React.ReactNode
    unitPriceGross: React.ReactNode
    cardForm: React.ReactNode
}>

export type CardVoucherAnomaly = Readonly<{
    title: string
    description: React.ReactNode
    code?: React.ReactNode
    tone?: Extract<StatusTone, "warning" | "destructive">
}>

type CardVoucherLineItemBaseProps = Omit<DomainPanelProps, "title"> & {
    title?: React.ReactNode
    description?: React.ReactNode
    anomaly?: CardVoucherAnomaly
}

export type CardVoucherLineItemProps =
    | (CardVoucherLineItemBaseProps & {
          mode: "edit"
          fields: CardVoucherEditFields
          derived: CardVoucherDerivedValues
          values?: never
          before?: never
          after?: never
          changedFields?: never
      })
    | (CardVoucherLineItemBaseProps & {
          mode: "readonly"
          values: CardVoucherDisplayValues
          fields?: never
          derived?: never
          before?: never
          after?: never
          changedFields?: never
      })
    | (CardVoucherLineItemBaseProps & {
          mode: "compare"
          before: CardVoucherDisplayValues
          after: CardVoucherDisplayValues
          changedFields: readonly CardVoucherFieldKey[]
          fields?: never
          derived?: never
          values?: never
      })
    | (Omit<CardVoucherLineItemBaseProps, "anomaly"> & {
          mode: "anomaly"
          values: CardVoucherDisplayValues
          anomaly: CardVoucherAnomaly
          fields?: never
          derived?: never
          before?: never
          after?: never
          changedFields?: never
      })

type VoucherDisplayItem = Readonly<{
    key: CardVoucherFieldKey
    label: string
    value: React.ReactNode
    numeric?: boolean
}>

const cardVoucherModeStatus = {
    edit: { label: "编辑中", tone: "info" },
    readonly: { label: "只读", tone: "neutral" },
    compare: { label: "版本对比", tone: "warning" },
    anomaly: { label: "异常待处理", tone: "destructive" },
} satisfies Record<
    CardVoucherLineItemProps["mode"],
    { label: string; tone: StatusTone }
>

function voucherDisplayItems(
    values: CardVoucherDisplayValues,
): readonly VoucherDisplayItem[] {
    return [
        { key: "category", label: "卡券类目", value: values.category },
        {
            key: "fulfillmentDeadline",
            label: "精确履约期限",
            value: <ShanghaiTime value={values.fulfillmentDeadline} />,
        },
        {
            key: "faceValue",
            label: "单卡面额",
            value: values.faceValue,
            numeric: true,
        },
        {
            key: "cardCount",
            label: "卡张数",
            value: values.cardCount,
            numeric: true,
        },
        {
            key: "unitPriceGross",
            label: "单卡含税单价",
            value: values.unitPriceGross,
            numeric: true,
        },
        {
            key: "transactionAmount",
            label: "派生成交额",
            value: values.transactionAmount,
            numeric: true,
        },
        {
            key: "faceValueTotal",
            label: "面值合计",
            value: values.faceValueTotal,
            numeric: true,
        },
        {
            key: "giftAmount",
            label: "配赠金额",
            value: values.giftAmount,
            numeric: true,
        },
        {
            key: "giftRate",
            label: "配赠率",
            value: values.giftRate,
            numeric: true,
        },
        { key: "cardForm", label: "卡形态", value: values.cardForm },
    ]
}

function voucherEditItems(
    fields: CardVoucherEditFields,
    derived: CardVoucherDerivedValues,
): readonly VoucherDisplayItem[] {
    return [
        { key: "category", label: "卡券类目", value: fields.category },
        {
            key: "fulfillmentDeadline",
            label: "精确履约期限",
            value: (
                <div className="space-y-2">
                    {fields.fulfillmentDeadline}
                    <Badge variant="neutral">Asia/Shanghai</Badge>
                </div>
            ),
        },
        { key: "faceValue", label: "单卡面额", value: fields.faceValue },
        { key: "cardCount", label: "卡张数", value: fields.cardCount },
        {
            key: "unitPriceGross",
            label: "单卡含税单价",
            value: fields.unitPriceGross,
        },
        {
            key: "transactionAmount",
            label: "派生成交额",
            value: derived.transactionAmount,
            numeric: true,
        },
        {
            key: "faceValueTotal",
            label: "面值合计",
            value: derived.faceValueTotal,
            numeric: true,
        },
        {
            key: "giftAmount",
            label: "配赠金额",
            value: derived.giftAmount,
            numeric: true,
        },
        {
            key: "giftRate",
            label: "配赠率",
            value: derived.giftRate,
            numeric: true,
        },
        { key: "cardForm", label: "卡形态", value: fields.cardForm },
    ]
}

function VoucherValueGrid({ items }: { items: readonly VoucherDisplayItem[] }) {
    return (
        <DescriptionList columns="two" className="lg:grid-cols-5">
            {items.map((item) => (
                <DescriptionItem key={item.key}>
                    <DescriptionTerm>{item.label}</DescriptionTerm>
                    <DescriptionDetails>
                        {item.numeric ? (
                            <NumericValue>{item.value}</NumericValue>
                        ) : (
                            item.value
                        )}
                    </DescriptionDetails>
                </DescriptionItem>
            ))}
        </DescriptionList>
    )
}

function VoucherComparisonGrid({
    before,
    after,
    changedFields,
}: {
    before: CardVoucherDisplayValues
    after: CardVoucherDisplayValues
    changedFields: readonly CardVoucherFieldKey[]
}) {
    const beforeItems = voucherDisplayItems(before)
    const afterItems = voucherDisplayItems(after)
    const changed = new Set(changedFields)

    return (
        <DescriptionList columns="two">
            {afterItems.map((item, index) => {
                const previousItem = beforeItems[index]
                const hasChanged = changed.has(item.key)

                return (
                    <DescriptionItem
                        key={item.key}
                        className={cn(
                            "rounded-lg border border-border p-3",
                            hasChanged && "bg-warning-soft",
                        )}
                    >
                        <DescriptionTerm className="flex items-center justify-between gap-2">
                            <span>{item.label}</span>
                            {hasChanged ? (
                                <Badge variant="warning">已变更</Badge>
                            ) : null}
                        </DescriptionTerm>
                        <DescriptionDetails>
                            <div
                                className={cn(
                                    item.numeric && "num font-medium",
                                )}
                            >
                                {item.value}
                            </div>
                            <div className="mt-2 text-xs text-muted-foreground">
                                原值：{previousItem?.value}
                            </div>
                        </DescriptionDetails>
                    </DescriptionItem>
                )
            })}
        </DescriptionList>
    )
}

function VoucherAnomalyAlert({ anomaly }: { anomaly: CardVoucherAnomaly }) {
    return (
        <Alert variant={anomaly.tone ?? "destructive"}>
            <AlertTitle className="flex flex-wrap items-center gap-2">
                <span>{anomaly.title}</span>
                {anomaly.code != null ? (
                    <Badge variant="outline">{anomaly.code}</Badge>
                ) : null}
            </AlertTitle>
            <AlertDescription>{anomaly.description}</AlertDescription>
        </Alert>
    )
}

/**
 * 单张卡券销售单的唯一明细。组件没有 children、增行入口或内部表单状态。
 */
function CardVoucherLineItem(props: CardVoucherLineItemProps) {
    const {
        mode,
        title = "卡券唯一明细",
        description = "类目与期限位于单据表头，面额、张数、成交和配赠保持单行表达。",
        anomaly,
        fields,
        derived,
        values,
        before,
        after,
        changedFields,
        className,
        ...cardProps
    } = props
    const status = cardVoucherModeStatus[mode]

    return (
        <Card
            data-slot="card-voucher-line-item"
            data-mode={mode}
            data-line-count="1"
            className={className}
            {...cardProps}
        >
            <CardHeader className="border-b border-border">
                <CardTitle>{title}</CardTitle>
                <CardDescription>{description}</CardDescription>
                <CardAction className="flex flex-wrap items-center justify-end gap-2">
                    <Badge variant="neutral">唯一明细</Badge>
                    <StatusBadge tone={status.tone} label={status.label} />
                    <StatusBadge
                        tone={
                            anomaly
                                ? (anomaly.tone ?? "destructive")
                                : "success"
                        }
                        label={anomaly ? "存在异常" : "无异常"}
                    />
                </CardAction>
            </CardHeader>

            <CardContent>
                {mode === "edit" ? (
                    <VoucherValueGrid
                        items={voucherEditItems(fields, derived)}
                    />
                ) : null}
                {mode === "readonly" || mode === "anomaly" ? (
                    <VoucherValueGrid items={voucherDisplayItems(values)} />
                ) : null}
                {mode === "compare" ? (
                    <VoucherComparisonGrid
                        before={before}
                        after={after}
                        changedFields={changedFields}
                    />
                ) : null}
            </CardContent>

            {anomaly ? (
                <CardFooter className="border-t border-border">
                    <VoucherAnomalyAlert anomaly={anomaly} />
                </CardFooter>
            ) : null}
        </Card>
    )
}

export { CardVoucherLineItem }
