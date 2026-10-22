"use client"

import * as React from "react"
import { QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { InquiryLine } from "../types"
import { InquiryRequirementDetail } from "./inquiry-requirement-detail"

/** 采购录入及已发布反馈共用需求入口，调用方提供对应版本的需求行。 */
export function InquiryFeedbackDemand({
    line,
    unit,
    idPrefix,
}: {
    line: InquiryLine
    unit: string
    idPrefix: string
}) {
    const [open, setOpen] = React.useState(false)
    const trigger = React.useRef<HTMLButtonElement>(null)
    return (
        <>
            <Button
                ref={trigger}
                id={`${idPrefix}-demand-open`}
                type="button"
                size="sm"
                variant="outline"
                aria-label={`${line.product_name}需求详情`}
                onClick={() => setOpen(true)}
            >
                需求详情
            </Button>
            <QuickPreviewSheet
                idPrefix={`${idPrefix}-demand`}
                size="detail"
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
                <div className="min-h-0 flex-1 overflow-y-auto px-7 py-6">
                    <InquiryRequirementDetail
                        line={line}
                        unit={unit}
                        procurement
                        image={null}
                    />
                </div>
            </QuickPreviewSheet>
        </>
    )
}
