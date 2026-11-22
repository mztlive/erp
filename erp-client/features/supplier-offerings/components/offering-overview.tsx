"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import {
    DetailWorkspaceMetrics,
    DetailWorkspaceMetricItem,
    DetailWorkspaceSection,
} from "@/components/business/detail-workspace"
import { StatusBadge } from "@/components/ui/status-badge"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"

import { offeringTime, offeringValidity } from "../lib/detail"
import { percentageFromRate } from "../lib/offering-forms"
import {
    AVAILABILITY_STATUS_LABELS,
    SOURCE_TYPE_LABELS,
    type SupplierOfferingView,
} from "../types"
import { capabilityLabel, OfferingField } from "./offering-terms"

/** 供给资料页顶部：有成本权限时展示供给价、税率与当前可供；无成本权限时展示可供、起订与有效期。 */
export function OfferingSummary({
    offering,
    canViewCosts,
}: {
    offering: SupplierOfferingView
    canViewCosts: boolean
}) {
    const available = offering.availability_status === "AVAILABLE"
    const availabilityMetric = (
        <DetailWorkspaceMetricItem
            label="当前可供"
            emphasis={!canViewCosts ? "primary" : undefined}
            value={
                <span className="inline-flex items-center gap-2">
                    {offering.available_quantity != null ? (
                        <QuantityValue
                            value={offering.available_quantity}
                            unit=""
                        />
                    ) : (
                        <span className="text-body-compact font-medium text-muted-foreground">
                            数量未提供
                        </span>
                    )}
                    <StatusBadge
                        tone={available ? "success" : "neutral"}
                        label={
                            offering.availability_status
                                ? AVAILABILITY_STATUS_LABELS[
                                      offering.availability_status
                                  ]
                                : "未更新"
                        }
                    />
                </span>
            }
            detail={
                <span className="num">
                    更新于{" "}
                    {offeringTime(offering.availability_source_updated_at)}
                </span>
            }
        />
    )
    if (!canViewCosts) {
        return (
            <DetailWorkspaceMetrics ariaLabel="供给摘要" columns={3}>
                {availabilityMetric}
                <DetailWorkspaceMetricItem
                    label="集采起订量"
                    value={
                        offering.bulk_minimum_order_quantity != null ? (
                            <QuantityValue
                                value={offering.bulk_minimum_order_quantity}
                                unit=""
                            />
                        ) : (
                            <span className="text-body-compact font-medium text-muted-foreground">
                                未提供
                            </span>
                        )
                    }
                    detail={
                        <span className="num">
                            当前条款第 {offering.current_revision_no ?? "—"} 版
                        </span>
                    }
                />
                <DetailWorkspaceMetricItem
                    label="条款有效期"
                    value={
                        <span className="num text-body-compact font-semibold">
                            {offeringValidity(offering)}
                        </span>
                    }
                    detail={`登记来源 · ${SOURCE_TYPE_LABELS[offering.source_type]}`}
                />
            </DetailWorkspaceMetrics>
        )
    }
    return (
        <DetailWorkspaceMetrics ariaLabel="供给摘要" columns={4}>
            <DetailWorkspaceMetricItem
                label="一件代发价（含税）"
                emphasis="primary"
                value={
                    <MoneyValue value={offering.dropship_supply_price_gross} />
                }
                detail={
                    <>
                        不含税{" "}
                        <MoneyValue
                            value={offering.dropship_supply_price_net}
                        />
                    </>
                }
            />
            <DetailWorkspaceMetricItem
                label="集采价（含税）"
                value={<MoneyValue value={offering.bulk_supply_price_gross} />}
                detail={
                    <>
                        起订{" "}
                        {offering.bulk_minimum_order_quantity != null ? (
                            <QuantityValue
                                value={offering.bulk_minimum_order_quantity}
                                unit=""
                            />
                        ) : (
                            "未提供"
                        )}
                        {" · 不含税 "}
                        <MoneyValue value={offering.bulk_supply_price_net} />
                    </>
                }
            />
            <DetailWorkspaceMetricItem
                label="进项税率"
                value={
                    offering.input_tax_rate != null ? (
                        <RateValue
                            value={percentageFromRate(offering.input_tax_rate)}
                            precision={2}
                        />
                    ) : (
                        "—"
                    )
                }
                detail="不含税价按此税率换算"
            />
            {availabilityMetric}
        </DetailWorkspaceMetrics>
    )
}

/** 当前条款：起订量、有效期、版次与商品能力（价格已在顶部摘要展示，不再重复列表）。 */
export function OfferingTermsSection({
    offering,
}: {
    offering: SupplierOfferingView
}) {
    return (
        <DetailWorkspaceSection
            headingId="offering-terms-title"
            title="商业条款"
            subtitle={
                <span className="num">
                    当前第 {offering.current_revision_no ?? "—"} 版
                </span>
            }
            width="form"
        >
            <dl className="grid gap-x-8 gap-y-3 sm:grid-cols-2">
                <OfferingField label="条款版次">
                    <span className="num">
                        第 {offering.current_revision_no ?? "—"} 版
                    </span>
                </OfferingField>
                <OfferingField label="集采起订量">
                    {offering.bulk_minimum_order_quantity != null ? (
                        <QuantityValue
                            value={offering.bulk_minimum_order_quantity}
                            unit=""
                        />
                    ) : (
                        "未提供"
                    )}
                </OfferingField>
                <OfferingField label="条款有效期">
                    <span className="num">{offeringValidity(offering)}</span>
                </OfferingField>
                <OfferingField label="商品能力">
                    {offering.product_capabilities.length > 0
                        ? offering.product_capabilities
                              .map(capabilityLabel)
                              .join("、")
                        : "未标注"}
                </OfferingField>
            </dl>
        </DetailWorkspaceSection>
    )
}

/** 供给身份：公司商品、供应商与订货编码，以及跳转到两端资料。 */
export function OfferingIdentitySection({
    offering,
}: {
    offering: SupplierOfferingView
}) {
    const { data } = useAccountProfileQuery()
    const productHref =
        offering.product_id &&
        hasPermission(data?.permissions, "product:detail")
            ? `/master-data/products/${encodeURIComponent(offering.product_id)}`
            : null
    const supplierHref = hasPermission(data?.permissions, "supplier:detail")
        ? `/master-data/suppliers/${encodeURIComponent(offering.supplier_id)}`
        : null
    return (
        <div
            aria-labelledby="offering-identity-title"
            className="min-w-0 space-y-5"
        >
            <h2
                id="offering-identity-title"
                className="text-section-title font-semibold tracking-tight text-foreground"
            >
                供给资料
            </h2>
            <section className="space-y-3">
                <h3 className="text-xs font-medium text-muted-foreground">
                    公司商品
                </h3>
                <dl className="space-y-3">
                    <OfferingField label="商品编号">
                        <span className="num">
                            {offering.product_no || "—"}
                        </span>
                    </OfferingField>
                    <OfferingField label="SKU 编号">
                        <span className="num">{offering.sku_no || "—"}</span>
                    </OfferingField>
                    <OfferingField label="规格">
                        {offering.specification || "—"}
                    </OfferingField>
                </dl>
                {productHref ? (
                    <Button
                        id="supplier-offering-detail-open-product"
                        variant="outline"
                        size="sm"
                        className="w-full"
                        render={<Link href={productHref} />}
                    >
                        打开商品资料
                        <ArrowUpRightIcon data-icon="inline-end" aria-hidden />
                    </Button>
                ) : null}
            </section>
            <section className="space-y-3 border-t pt-5">
                <h3 className="text-xs font-medium text-muted-foreground">
                    供应商
                </h3>
                <dl className="space-y-3">
                    <OfferingField label="名称">
                        {offering.supplier_name ||
                            offering.supplier_no ||
                            "未提供"}
                    </OfferingField>
                    <OfferingField label="订货编码">
                        <span className="num">
                            {offering.supplier_sku_code}
                        </span>
                    </OfferingField>
                    <OfferingField label="商品编码">
                        <span className="num">
                            {offering.supplier_product_code || "未标注"}
                        </span>
                    </OfferingField>
                </dl>
                {supplierHref ? (
                    <Button
                        id="supplier-offering-detail-open-supplier"
                        variant="outline"
                        size="sm"
                        className="w-full"
                        render={<Link href={supplierHref} />}
                    >
                        打开供应商资料
                        <ArrowUpRightIcon data-icon="inline-end" aria-hidden />
                    </Button>
                ) : null}
            </section>
            <section className="space-y-3 border-t pt-5">
                <h3 className="text-xs font-medium text-muted-foreground">
                    维护
                </h3>
                <dl className="space-y-3">
                    <OfferingField label="维护人">
                        {offering.maintainer_user_name || "未提供"}
                    </OfferingField>
                    <OfferingField label="登记来源">
                        {SOURCE_TYPE_LABELS[offering.source_type]}
                    </OfferingField>
                </dl>
            </section>
        </div>
    )
}
