"use client"

import * as React from "react"
import {
    ChevronDownIcon,
    CrosshairIcon,
    MapPinIcon,
    TruckIcon,
} from "lucide-react"
import { InquiryIconAction } from "./inquiry-icon-action"
import { QuantityValue, QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useInquirySkuDetailsQuery } from "../hooks/queries"
import { inquiryDateInput } from "../lib/format"
import type { InquiryLine, InquiryRequirement } from "../types"
import { InquiryRequirementDetail } from "./inquiry-requirement-detail"
import { InquiryDeliveryBadge } from "./inquiry-delivery-badge"

/** 采购录入及已发布反馈共用需求入口，调用方提供对应版本的需求行。 */
export function InquiryFeedbackDemand({
    line,
    unit,
    idPrefix,
    appearance = "icon",
}: {
    line: InquiryLine
    unit: string
    idPrefix: string
    appearance?: "icon" | "badge"
}) {
    const [open, setOpen] = React.useState(false)
    const trigger = React.useRef<HTMLButtonElement>(null)
    return (
        <>
            {appearance === "badge" ? (
                line.fulfillment_kind === "physical" ? (
                    <InquiryDeliveryBadge
                        ref={trigger}
                        id={`${idPrefix}-demand-open`}
                        line={line}
                        onOpen={() => setOpen(true)}
                    />
                ) : (
                    <Button
                        ref={trigger}
                        id={`${idPrefix}-demand-open`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        className="h-5 gap-1 rounded-md bg-muted/55 px-1.5 py-0 font-normal text-muted-foreground hover:bg-sidebar-accent/70 hover:text-sidebar-accent-foreground"
                        aria-label={`${line.product_name}交付要求`}
                        aria-haspopup="dialog"
                        onClick={() => setOpen(true)}
                    >
                        <TruckIcon className="size-3" aria-hidden="true" />
                        交付要求
                    </Button>
                )
            ) : (
                <InquiryIconAction
                    ref={trigger}
                    id={`${idPrefix}-demand-open`}
                    label={
                        line.fulfillment_kind === "physical"
                            ? "配送地址"
                            : "交付要求"
                    }
                    onClick={() => setOpen(true)}
                >
                    {line.fulfillment_kind === "physical" ? (
                        <MapPinIcon aria-hidden="true" />
                    ) : (
                        <TruckIcon aria-hidden="true" />
                    )}
                </InquiryIconAction>
            )}
            <QuickPreviewSheet
                idPrefix={`${idPrefix}-demand`}
                size="preview"
                open={open}
                onOpenChange={setOpen}
                finalFocus={trigger}
                title={line.product_name}
                identity={`SKU 编号：${line.sku_code}`}
                description={line.specification || undefined}
                footer={
                    <Button
                        id={`${idPrefix}-demand-done`}
                        type="button"
                        variant="outline"
                        onClick={() => setOpen(false)}
                    >
                        关闭
                    </Button>
                }
            >
                <InquiryRequirementDetail
                    line={line}
                    unit={unit}
                    procurement
                    compact
                    image={null}
                />
            </QuickPreviewSheet>
        </>
    )
}

export function InquiryDemandReference({
    idPrefix,
    requirement,
    onLocate,
}: {
    idPrefix: string
    requirement: InquiryRequirement
    onLocate?: (lineId: string) => void
}) {
    const sku = useInquirySkuDetailsQuery(
        requirement.lines.map((line) => line.sku_id),
    )

    return (
        <div className="min-w-0 text-body-compact">
            {requirement.note.trim() && (
                <p className="border-b border-border/60 py-3 text-muted-foreground whitespace-pre-wrap wrap-anywhere">
                    {requirement.note}
                </p>
            )}
            {requirement.lines.length === 0 && (
                <p className="py-4 text-xs text-muted-foreground">
                    暂无商品需求
                </p>
            )}
            <ul className="divide-y divide-border/60">
                {requirement.lines.map((line) => {
                    const prefix = `${idPrefix}-${toAutomationIdSegment(line.line_id)}`
                    const unit = sku.units.get(line.unit_code) ?? ""
                    const dueDate = inquiryDateInput(line.delivery_due_at)
                    return (
                        <li
                            key={line.line_id}
                            className="relative min-w-0 py-2"
                        >
                            <details className="group/demand min-w-0">
                                <summary
                                    id={`${prefix}-expand`}
                                    className="flex min-w-0 cursor-pointer list-none items-start gap-2 rounded-sm py-1 focus-visible:outline-2 focus-visible:outline-ring [&::-webkit-details-marker]:hidden"
                                >
                                    <ChevronDownIcon
                                        aria-hidden="true"
                                        className="mt-0.5 size-3.5 shrink-0 -rotate-90 text-muted-foreground group-open/demand:rotate-0"
                                    />
                                    <div className="min-w-0 flex-1">
                                        <p
                                            className="truncate font-medium"
                                            title={line.product_name}
                                        >
                                            {line.product_name}
                                        </p>
                                        <p className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                                            <span className="num wrap-anywhere">
                                                {line.sku_code}
                                            </span>
                                            <QuantityValue
                                                value={line.quantity}
                                                unit={unit}
                                            />
                                            {dueDate && (
                                                <span className="num">
                                                    {dueDate}
                                                </span>
                                            )}
                                        </p>
                                    </div>
                                    {onLocate && (
                                        <span
                                            aria-hidden="true"
                                            className="w-7 shrink-0"
                                        />
                                    )}
                                </summary>
                                <div className="pt-3 pl-5">
                                    {line.specification &&
                                        line.specification !== "无规格" && (
                                            <p className="pb-2 text-xs text-muted-foreground wrap-anywhere">
                                                {line.specification}
                                            </p>
                                        )}
                                    <InquiryRequirementDetail
                                        line={line}
                                        unit={unit}
                                        procurement
                                        compact
                                        image={null}
                                    />
                                </div>
                            </details>
                            {onLocate && (
                                <Button
                                    id={`${prefix}-locate`}
                                    type="button"
                                    variant="ghost"
                                    size="icon-sm"
                                    className="absolute top-2 right-0"
                                    aria-label={`定位${line.product_name}的采购反馈`}
                                    title="定位采购反馈"
                                    onClick={() => onLocate(line.line_id)}
                                >
                                    <CrosshairIcon aria-hidden="true" />
                                </Button>
                            )}
                        </li>
                    )
                })}
            </ul>
        </div>
    )
}
