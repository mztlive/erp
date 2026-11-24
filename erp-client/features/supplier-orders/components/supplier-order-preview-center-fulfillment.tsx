"use client"

import { EyeIcon, EyeOffIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    DetailWorkspaceSection,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { DescriptionList } from "@/components/ui/description-list"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import { formatDateTime } from "@/lib/datetime"
import { Item } from "@/features/supplier-orders/components/supplier-order-preview-center-section-parts"

const SOURCE_LABELS: Record<string, string> = {
    erp: "ERP 系统",
    supplier_callback: "供应商回调",
    manual_import: "人工导入",
}

const TRACK_LABELS: Record<
    SupplierOrderDetailView["statusHistory"][number]["track"],
    string
> = {
    fulfillment: "履约",
    cancel: "取消",
    refund: "退款",
}

export function FulfillmentSection({
    logistics,
    idPrefix = "supplier-order-center-fulfillment",
    investigation = false,
    address,
    statusHistory,
    canReveal,
    revealPending,
    onReveal,
    onHide,
}: {
    idPrefix?: string
    investigation?: boolean
    logistics: SupplierOrderDetailView["logistics"]
    address: SupplierOrderDetailView["address"]
    statusHistory: SupplierOrderDetailView["statusHistory"]
    canReveal: boolean
    revealPending: boolean
    onReveal: () => void
    onHide: () => void
}) {
    return (
        <>
            <DetailWorkspaceSection
                title="履约与物流"
                width="form"
                contentClassName="@container/fulfillment"
            >
                <DescriptionList
                    columns="one"
                    className="mb-4 gap-y-3 @[420px]/fulfillment:grid-cols-2"
                >
                    <Item
                        label="接单时间"
                        value={
                            logistics.acceptedAt ? (
                                <span className="num">
                                    {formatDateTime(
                                        logistics.acceptedAt,
                                        "fullIntl",
                                        "passthrough",
                                    )}
                                </span>
                            ) : (
                                "—"
                            )
                        }
                    />
                    <Item
                        label="发货时间"
                        value={
                            logistics.shippedAt ? (
                                <span className="num">
                                    {formatDateTime(
                                        logistics.shippedAt,
                                        "fullIntl",
                                        "passthrough",
                                    )}
                                </span>
                            ) : (
                                "—"
                            )
                        }
                    />
                    <Item
                        label="完成时间"
                        value={
                            logistics.completedAt ? (
                                <span className="num">
                                    {formatDateTime(
                                        logistics.completedAt,
                                        "fullIntl",
                                        "passthrough",
                                    )}
                                </span>
                            ) : (
                                "—"
                            )
                        }
                    />
                    <Item label="承运商" value={logistics.carrier ?? "—"} />
                    <Item
                        label="物流号"
                        value={
                            logistics.trackingNo ? (
                                <span className="num">
                                    {logistics.trackingNo}
                                </span>
                            ) : (
                                "—"
                            )
                        }
                    />
                </DescriptionList>
            </DetailWorkspaceSection>
            <DetailWorkspaceSection
                title="收货信息"
                width="form"
                contentClassName="space-y-2 text-body-compact"
            >
                <div>
                    收件人：
                    {address.recipientRevealed ?? address.recipientMasked}
                </div>
                <div>
                    手机：
                    <span className="num">
                        {address.phoneRevealed ?? address.phoneMasked}
                    </span>
                </div>
                <div>地址：{address.revealed ?? address.masked}</div>
                {address.auditNote ? (
                    <p className="text-xs text-muted-foreground">
                        {address.auditNote}
                    </p>
                ) : null}
                <div className="flex gap-2 pt-1">
                    {canReveal && (
                        <LoadingButton
                            id={`${idPrefix}-reveal`}
                            loading={revealPending}
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={!canReveal || revealPending}
                            onClick={onReveal}
                        >
                            {!revealPending && (
                                <EyeIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                            )}
                            {revealPending ? "揭示中…" : "短时揭示"}
                        </LoadingButton>
                    )}
                    {address.revealed ? (
                        <Button
                            id={`${idPrefix}-hide`}
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={onHide}
                        >
                            <EyeOffIcon
                                className="size-3.5"
                                aria-hidden="true"
                            />
                            立即隐藏
                        </Button>
                    ) : null}
                </div>
            </DetailWorkspaceSection>
            <DetailWorkspaceSection title="状态历史" width="form">
                {statusHistory.length === 0 ? (
                    <p className="text-xs text-muted-foreground">
                        暂无状态历史
                    </p>
                ) : (
                    <ul className="space-y-2">
                        {statusHistory.map((h) => (
                            <li
                                key={h.id}
                                className="border-t border-border/60 py-2 text-xs first:border-t-0 first:pt-0"
                            >
                                <div className="flex flex-wrap items-center gap-2">
                                    <BusinessStatusBadge
                                        context="list"
                                        appearance="quiet"
                                        tone="neutral"
                                        label={TRACK_LABELS[h.track] ?? h.track}
                                    />
                                    <span>
                                        {h.fromLabel} → {h.toLabel}
                                    </span>
                                    <span className="text-muted-foreground">
                                        <span className="num">
                                            {formatDateTime(
                                                h.at,
                                                "fullIntl",
                                                "passthrough",
                                            )}
                                        </span>{" "}
                                        ·{" "}
                                        {investigation
                                            ? (SOURCE_LABELS[h.source] ??
                                              "来源待核实")
                                            : h.source}
                                    </span>
                                </div>
                                {h.note ? (
                                    <p className="mt-1 text-muted-foreground">
                                        {h.note}
                                    </p>
                                ) : null}
                            </li>
                        ))}
                    </ul>
                )}
            </DetailWorkspaceSection>
        </>
    )
}
