"use client"

import * as React from "react"
import Link from "next/link"

import { PrepaymentGate, StatusTrackSummary } from "@/components/business"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { Separator } from "@/components/ui/separator"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
} from "@/features/purchase-orders/types"
import { cn } from "@/lib/utils"

export function SectionTitle({ children }: { children: React.ReactNode }) {
    return (
        <h3 className="font-heading text-sm font-semibold text-foreground">
            {children}
        </h3>
    )
}

export function CompactField({
    label,
    value,
    numeric,
}: {
    label: string
    value: React.ReactNode
    numeric?: boolean
}) {
    return (
        <DescriptionItem className="gap-0.5">
            <DescriptionTerm className="text-xs">{label}</DescriptionTerm>
            <DescriptionDetails
                className={cn("text-sm font-medium", numeric && "num")}
            >
                {value}
            </DescriptionDetails>
        </DescriptionItem>
    )
}

export function RelatedPill({
    label,
    count,
    muted,
    href,
}: {
    label: string
    count: number
    muted?: boolean
    href?: string
}) {
    const content = (
        <>
            <span className="text-muted-foreground">{label}</span>
            <span className="num font-semibold">{count}</span>
        </>
    )
    return (
        <span
            className={cn(
                "inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-xs",
                muted
                    ? "border-dashed border-border bg-muted/40 text-muted-foreground"
                    : "border-border bg-card text-foreground",
            )}
        >
            {href ? (
                <Link
                    href={href}
                    className="inline-flex items-center gap-1.5 hover:underline"
                >
                    {content}
                </Link>
            ) : (
                content
            )}
        </span>
    )
}

/** 预览左列：进度轨、拆单维度、先款门禁与关联对象。 */
export function PurchaseOrderPreviewOverview({
    order,
}: {
    order: PurchaseOrderCenterView
}) {
    const { identity, header, progress, currentContent } = order
    const costMasked = currentContent.costMasked
    const gate = progress.prepaymentGate

    return (
        <div className="space-y-4 p-4 md:p-5">
            <section className="space-y-2" aria-label="进度">
                <SectionTitle>进度</SectionTitle>
                <StatusTrackSummary
                    variant="table"
                    className="sm:grid-cols-1 lg:grid-cols-1"
                    tracks={[
                        {
                            id: "review",
                            label: "审核",
                            status: {
                                label: identity.reviewLabel,
                                tone:
                                    identity.reviewStatus === "PENDING"
                                        ? "warning"
                                        : identity.reviewStatus === "APPROVED"
                                          ? "success"
                                          : identity.reviewStatus === "REJECTED"
                                            ? "destructive"
                                            : "neutral",
                            },
                        },
                        {
                            id: "payment",
                            label: "付款",
                            status: {
                                label: progress.payment,
                                tone:
                                    progress.payment === "已付"
                                        ? "success"
                                        : progress.payment === "部分"
                                          ? "info"
                                          : "neutral",
                            },
                        },
                        {
                            id: "invoice",
                            label: "进项票",
                            status: {
                                label: progress.invoice,
                                tone:
                                    progress.invoice === "完成"
                                        ? "success"
                                        : progress.invoice === "部分"
                                          ? "info"
                                          : "neutral",
                            },
                        },
                        {
                            id: "fulfillment",
                            label: "履约",
                            status: {
                                label: progress.fulfillment,
                                tone: order.fulfillmentSummary.progressTone,
                            },
                        },
                    ]}
                />
            </section>

            <Separator />

            <section className="space-y-2" aria-label="拆单维度">
                <SectionTitle>拆单维度（唯一）</SectionTitle>
                <p className="text-tiny leading-relaxed text-muted-foreground">
                    一张采购单 = 一张销售单 × 一个供应商 × 一种采购类型 ×
                    一套付款条件 × 一个履约责任。
                </p>
                <DescriptionList columns="one" className="gap-y-2.5">
                    <CompactField
                        label="来源销售单"
                        value={
                            <Link
                                href={`/sales/orders/${header.salesOrderId}`}
                                className="num text-primary underline-offset-2 hover:underline"
                            >
                                {header.salesOrderNo}
                            </Link>
                        }
                    />
                    <CompactField
                        label="供应商"
                        value={header.supplierSnapshot}
                    />
                    <CompactField
                        label="采购类型"
                        value={PURCHASE_TYPE_LABEL[header.purchaseType]}
                    />
                    <CompactField
                        label="付款条件"
                        value={header.paymentTermLabel}
                    />
                    <CompactField
                        label="履约责任"
                        value={
                            FULFILLMENT_RESPONSIBILITY_LABEL[
                                header.fulfillmentResponsibility
                            ]
                        }
                    />
                    <CompactField label="负责人" value={header.ownerName} />
                    {header.expectedDate ? (
                        <CompactField
                            label="最近预计交期"
                            value={header.expectedDate}
                            numeric
                        />
                    ) : null}
                    {header.creationBasisId ? (
                        <CompactField
                            label="创建依据"
                            value={`采购二次确认 · 销售单 ${header.salesOrderNo}`}
                        />
                    ) : null}
                </DescriptionList>
            </section>

            {gate.state !== "NOT_APPLICABLE" ? (
                <>
                    <Separator />
                    <section className="space-y-2" aria-label="先款门禁">
                        <PrepaymentGate
                            condition={{
                                kind: "amount",
                                required: costMasked ? "•••" : gate.required,
                                description: gate.message,
                            }}
                            allocated={costMasked ? "•••" : gate.allocated}
                            gap={costMasked ? "•••" : gate.gap}
                            updatedAt={{
                                dateTime: gate.updatedAt,
                                label: gate.updatedAt,
                            }}
                            allowed={gate.state === "SATISFIED"}
                        />
                    </section>
                </>
            ) : null}

            <Separator />

            <section className="space-y-2" aria-label="关联">
                <SectionTitle>关联对象</SectionTitle>
                <div className="flex flex-wrap items-center gap-1.5">
                    <Link
                        href={`/fulfillment?lane=procurement&scope=mine&purchaseOrderId=${encodeURIComponent(identity.purchaseOrderId)}&from=W08&returnTo=${encodeURIComponent("/procurement/orders")}`}
                        className="inline-flex h-7 items-center rounded-md border border-border bg-background px-2 text-xs font-medium text-primary hover:bg-accent"
                    >
                        去交付与代发
                    </Link>
                    <RelatedPill
                        label="销售"
                        count={1}
                        href={`/sales/orders/${header.salesOrderId}`}
                    />
                    <RelatedPill
                        label="变更"
                        count={order.changes.length}
                        muted={order.changes.length === 0}
                    />
                    <RelatedPill
                        label="应付"
                        count={order.payableSummary ? 1 : 0}
                        muted={!order.payableSummary}
                        href={
                            order.payableSummary
                                ? `/finance/supplier-accounts?purchaseOrderId=${encodeURIComponent(identity.purchaseOrderId)}&supplierId=${encodeURIComponent(header.supplierId)}`
                                : undefined
                        }
                    />
                </div>
            </section>
        </div>
    )
}
