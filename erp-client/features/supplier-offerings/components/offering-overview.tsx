"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import {
    DetailWorkspaceMetrics,
    DetailWorkspaceMetricItem,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business/detail-workspace"
import { StatusBadge } from "@/components/ui/status-badge"
import { Button } from "@/components/ui/button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
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

/** 供给资料页顶部：两种供给价、税率和当前可供，先回答「多少钱、现在能不能供」。 */
export function OfferingSummary({
    offering,
    canViewCosts,
}: {
    offering: SupplierOfferingView
    canViewCosts: boolean
}) {
    const available = offering.availability_status === "AVAILABLE"
    return (
        <DetailWorkspaceMetrics
            ariaLabel="供给摘要"
            columns={canViewCosts ? 4 : 2}
        >
            {canViewCosts && (
                <>
                    <DetailWorkspaceMetricItem
                        label="一件代发价（含税）"
                        value={
                            <MoneyValue
                                value={offering.dropship_supply_price_gross}
                            />
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
                        value={
                            <MoneyValue
                                value={offering.bulk_supply_price_gross}
                            />
                        }
                        detail={
                            <>
                                起订{" "}
                                {offering.bulk_minimum_order_quantity !=
                                null ? (
                                    <QuantityValue
                                        value={
                                            offering.bulk_minimum_order_quantity
                                        }
                                        unit=""
                                    />
                                ) : (
                                    "未提供"
                                )}
                                {" · 不含税 "}
                                <MoneyValue
                                    value={offering.bulk_supply_price_net}
                                />
                            </>
                        }
                    />
                    <DetailWorkspaceMetricItem
                        label="进项税率"
                        value={
                            offering.input_tax_rate != null ? (
                                <RateValue
                                    value={percentageFromRate(
                                        offering.input_tax_rate,
                                    )}
                                    precision={2}
                                />
                            ) : (
                                "—"
                            )
                        }
                        detail="不含税价按此税率换算"
                    />
                </>
            )}
            <DetailWorkspaceMetricItem
                label="当前可供"
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
                detail={`更新于 ${offeringTime(offering.availability_source_updated_at)}`}
            />
        </DetailWorkspaceMetrics>
    )
}

/** 当前条款：价格对照表、起订量、有效期与商品能力。 */
export function OfferingTermsSection({
    offering,
    canViewCosts,
}: {
    offering: SupplierOfferingView
    canViewCosts: boolean
}) {
    const rows = [
        {
            key: "dropship",
            label: "一件代发",
            gross: offering.dropship_supply_price_gross,
            net: offering.dropship_supply_price_net,
        },
        {
            key: "bulk",
            label: "集采",
            gross: offering.bulk_supply_price_gross,
            net: offering.bulk_supply_price_net,
        },
    ]
    return (
        <DetailWorkspaceSection
            headingId="offering-terms-title"
            title="商业条款"
            subtitle={`当前第 ${offering.current_revision_no ?? "—"} 版`}
        >
            {canViewCosts && (
                <div className={detailWorkspaceStyles.dataTableWrapper}>
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>价格方式</TableHead>
                                <TableHead className="text-right">
                                    含税价
                                </TableHead>
                                <TableHead className="text-right">
                                    不含税价
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {rows.map((row) => (
                                <TableRow key={row.key}>
                                    <TableCell className="font-medium">
                                        {row.label}
                                    </TableCell>
                                    <TableCell className="text-right">
                                        <MoneyValue value={row.gross} />
                                    </TableCell>
                                    <TableCell className="text-right text-muted-foreground">
                                        <MoneyValue value={row.net} />
                                    </TableCell>
                                </TableRow>
                            ))}
                        </TableBody>
                    </Table>
                </div>
            )}
            <dl className="grid gap-x-8 gap-y-3 sm:grid-cols-2">
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
                {offering.product_capabilities.length > 0 ? (
                    <OfferingField label="商品能力">
                        {offering.product_capabilities
                            .map(capabilityLabel)
                            .join("、")}
                    </OfferingField>
                ) : null}
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
        <DetailWorkspaceSection
            headingId="offering-identity-title"
            title="供给资料"
            className="py-0 first:pt-0 last:pb-0"
        >
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
                        {offering.supplier_product_code || "未标注"}
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
        </DetailWorkspaceSection>
    )
}
