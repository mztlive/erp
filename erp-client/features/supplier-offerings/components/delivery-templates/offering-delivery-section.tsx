"use client"

import * as React from "react"

import { Button } from "@/components/ui/button"

import { useSupplierDeliveryTemplatesQuery } from "../../hooks/queries"
import { ruleSetSummary } from "../../lib/delivery-template-summary"
import type { SupplierOfferingView } from "../../types"
import { AssignDeliveryTemplateDialog } from "./assign-delivery-template-dialog"

/** 供给资料页的配送模板：当前生效模板、规则摘要与设置入口。 */
export function OfferingDeliverySection({
    offering,
    canEdit,
}: {
    offering: SupplierOfferingView
    canEdit: boolean
}) {
    const [assigning, setAssigning] = React.useState(false)
    const templates = useSupplierDeliveryTemplatesQuery(offering.supplier_id)
    const current = offering.delivery_template
    const template = templates.data?.find(
        (item) => item.id === current?.template_id,
    )
    return (
        <section
            className="space-y-2"
            aria-labelledby="offering-delivery-title"
        >
            <div className="flex items-center justify-between gap-2">
                <h3
                    id="offering-delivery-title"
                    className="text-sm font-medium"
                >
                    配送模板
                </h3>
                {canEdit ? (
                    <Button
                        id="supplier-offering-detail-delivery-template"
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() => setAssigning(true)}
                    >
                        设置配送模板
                    </Button>
                ) : null}
            </div>
            <p className="text-sm">
                {current?.display_name ?? "未设置（全国配送、包邮）"}
            </p>
            {template ? (
                <div className="space-y-1 text-xs text-muted-foreground">
                    <p>代发：{ruleSetSummary("dropship", template.dropship)}</p>
                    <p>集采：{ruleSetSummary("bulk", template.bulk)}</p>
                </div>
            ) : null}
            {assigning ? (
                <AssignDeliveryTemplateDialog
                    offerings={[offering]}
                    onOpenChange={(open) => {
                        if (!open) setAssigning(false)
                    }}
                />
            ) : null}
        </section>
    )
}
