"use client"

import * as React from "react"
import { MapPinIcon, TruckIcon } from "lucide-react"
import { InquiryIconAction } from "./inquiry-icon-action"
import { QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { InquiryLine } from "../types"
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
                        className="h-auto min-h-6 gap-1 px-1 py-0.5 font-normal text-muted-foreground hover:text-primary"
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
