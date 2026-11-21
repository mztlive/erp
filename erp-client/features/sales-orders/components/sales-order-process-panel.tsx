"use client"

import { useState } from "react"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import {
    SalesTimeline,
    timelineCategories,
} from "@/features/document-timeline/components/sales-timeline"
import type { TimelineCategory } from "@/features/document-timeline/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { SalesOrderDetailView } from "../api/sales-orders"
import type { NavSectionId } from "../lib/sales-order-detail-model"
import { SalesOrderProcessDiagram } from "./sales-order-process-diagram"
import type { DeliveryProgressQuery } from "./sales-order-delivery-overview"
import { ApprovalPanel } from "./sales-order-detail-approval-panel"

/** 全流程共用一个记录筛选；保留 section=approval 的原有定位入口。 */
export function SalesOrderProcessPanel({
    order,
    section,
    onSelectSection,
    deliveryQuery,
    canReadDelivery,
    visibleSections,
}: {
    deliveryQuery: DeliveryProgressQuery
    canReadDelivery: boolean
    visibleSections: NavSectionId[]
    order: SalesOrderDetailView
    section: NavSectionId
    onSelectSection: (section: NavSectionId) => void
}) {
    const [category, setCategory] = useState<TimelineCategory>()
    const [includeDetails, setIncludeDetails] = useState(false)
    const selected = section === "approval" ? "approval" : category
    const prefix = `sales-order-process-${toAutomationIdSegment(order.id)}`
    return (
        <section aria-label="销售单全流程" className="space-y-8">
            <SalesOrderProcessDiagram
                order={order}
                deliveryQuery={deliveryQuery}
                canReadDelivery={canReadDelivery}
                visibleSections={visibleSections}
                onSelectSection={onSelectSection}
            />
            <section aria-label="流程记录" className="space-y-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                    <h2 className="text-section-title font-semibold tracking-tight">
                        流程记录
                    </h2>
                    <div className="flex flex-wrap items-center gap-3">
                        <label
                            htmlFor={`${prefix}-category`}
                            className="text-xs text-muted-foreground"
                        >
                            记录类型
                        </label>
                        <NativeSelect
                            id={`${prefix}-category`}
                            value={selected ?? ""}
                            onChange={(event) => {
                                const next = event.target.value as
                                    | TimelineCategory
                                    | ""
                                setCategory(
                                    next === "approval"
                                        ? undefined
                                        : next || undefined,
                                )
                                if (next === "approval")
                                    onSelectSection("approval")
                                else if (section === "approval")
                                    onSelectSection("timeline")
                            }}
                        >
                            <NativeSelectOption value="">
                                全部记录
                            </NativeSelectOption>
                            {timelineCategories.map((item) => (
                                <NativeSelectOption
                                    key={item.value}
                                    value={item.value}
                                >
                                    {item.label}
                                </NativeSelectOption>
                            ))}
                        </NativeSelect>
                        {selected !== "approval" ? (
                            <label
                                htmlFor={`${prefix}-details`}
                                className="inline-flex items-center gap-2 text-xs text-muted-foreground"
                            >
                                <NativeCheckbox
                                    id={`${prefix}-details`}
                                    checked={includeDetails}
                                    onChange={(event) =>
                                        setIncludeDetails(event.target.checked)
                                    }
                                />
                                包含草稿与资料修改
                            </label>
                        ) : null}
                    </div>
                </div>
                {selected === "approval" ? (
                    <ApprovalPanel order={order} />
                ) : (
                    <SalesTimeline
                        salesOrderId={order.id}
                        idPrefix={`${prefix}-timeline`}
                        fixedCategory={selected}
                        embedded
                        includeDetails={includeDetails}
                        groupByDocument
                        plainApprovalReasons
                    />
                )}
            </section>
        </section>
    )
}
