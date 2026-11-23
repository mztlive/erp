"use client"

import { useState } from "react"

import { DetailWorkspaceSection, OptionCombobox } from "@/components/business"
import { NativeCheckbox } from "@/components/ui/checkbox"
import {
    SalesTimeline,
    timelineCategories,
} from "@/features/document-timeline/components/sales-timeline"
import type { TimelineCategory } from "@/features/document-timeline/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { SalesOrderDetailView } from "../api/sales-orders"
import type { NavSectionId } from "../lib/sales-order-detail-model"
import type { DeliveryProgressQuery } from "./sales-order-delivery-overview"
import { ApprovalPanel } from "./sales-order-detail-approval-panel"
import { SalesOrderProcessDiagram } from "./sales-order-process-diagram"

const PROCESS_CATEGORY_OPTIONS = [
    { value: "all", label: "全部记录" },
    ...timelineCategories,
] as const

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
        <>
            <DetailWorkspaceSection width="full">
                <SalesOrderProcessDiagram
                    order={order}
                    deliveryQuery={deliveryQuery}
                    canReadDelivery={canReadDelivery}
                    visibleSections={visibleSections}
                    onSelectSection={onSelectSection}
                />
            </DetailWorkspaceSection>

            <DetailWorkspaceSection
                width="form"
                title="流程记录"
                actions={
                    <div className="flex flex-wrap items-center gap-3">
                        <OptionCombobox
                            id={`${prefix}-category`}
                            options={PROCESS_CATEGORY_OPTIONS}
                            value={selected ?? "all"}
                            onValueChange={(value) => {
                                const next =
                                    !value || value === "all"
                                        ? undefined
                                        : (value as TimelineCategory)
                                setCategory(
                                    next === "approval" ? undefined : next,
                                )
                                if (next === "approval")
                                    onSelectSection("approval")
                                else if (section === "approval")
                                    onSelectSection("timeline")
                            }}
                            allowClear={false}
                            size="sm"
                            aria-label="记录类型"
                            className="w-36"
                            inputClassName="w-36"
                        />
                        {selected !== "approval" ? (
                            <label
                                htmlFor={`${prefix}-details`}
                                className="inline-flex cursor-pointer items-center gap-2 text-xs text-muted-foreground"
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
                }
            >
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
                        currentDocument={{ type: "sales_order", id: order.id }}
                    />
                )}
            </DetailWorkspaceSection>
        </>
    )
}
