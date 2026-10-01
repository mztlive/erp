"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"
import { QuantityValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { OfferingField } from "./offering-terms"
import { offeringTime } from "../lib/detail"
import {
    AVAILABILITY_STATUS_LABELS,
    SOURCE_TYPE_LABELS,
    type SupplierOfferingView,
} from "../types"

export function OfferingAvailability({
    offering,
}: {
    offering: SupplierOfferingView
}) {
    return (
        <section className="space-y-3 text-sm">
            <h3 className="font-medium">当前可供</h3>
            <dl className="space-y-3">
                <OfferingField label="可供状态">
                    <Badge
                        variant={
                            offering.availability_status === "AVAILABLE"
                                ? "success"
                                : "outline"
                        }
                    >
                        {offering.availability_status
                            ? AVAILABILITY_STATUS_LABELS[
                                  offering.availability_status
                              ]
                            : "未更新"}
                    </Badge>
                </OfferingField>
                <OfferingField label="可供数量">
                    {offering.available_quantity != null ? (
                        <QuantityValue
                            value={offering.available_quantity}
                            unit=""
                        />
                    ) : (
                        "未提供"
                    )}
                </OfferingField>
                <OfferingField label="来源更新时间">
                    <span className="num">
                        {offeringTime(offering.availability_source_updated_at)}
                    </span>
                </OfferingField>
            </dl>
            <p className="text-xs leading-5 text-muted-foreground">
                关系状态、条款有效期与当前可供分别核对；采购时以最新校验结果为准。
            </p>
        </section>
    )
}

export function OfferingIdentity({
    offering,
}: {
    offering: SupplierOfferingView
}) {
    return (
        <section className="space-y-3 text-sm">
            <h3 className="font-medium">供给资料</h3>
            <dl className="space-y-3">
                <OfferingField label="公司商品编号">
                    {offering.product_no || "—"}
                </OfferingField>
                <OfferingField label="公司 SKU 编号">
                    {offering.sku_no || "—"}
                </OfferingField>
                <OfferingField label="供应商">
                    {offering.supplier_name || offering.supplier_no || "未提供"}
                </OfferingField>
                <OfferingField label="供应商订货编码">
                    <span className="num">{offering.supplier_sku_code}</span>
                </OfferingField>
                <OfferingField label="供应商商品编码">
                    {offering.supplier_product_code || "未标注"}
                </OfferingField>
                <OfferingField label="登记来源">
                    {SOURCE_TYPE_LABELS[offering.source_type]}
                </OfferingField>
                <OfferingField label="维护人">
                    {offering.maintainer_user_name || "未提供"}
                </OfferingField>
            </dl>
        </section>
    )
}

/** 关联对象仍由目标资料页独立校验其数据范围。 */
export function OfferingLinks({
    offering,
    idPrefix,
}: {
    offering: SupplierOfferingView
    idPrefix: string
}) {
    const { data } = useAccountProfileQuery()
    const links = [
        ...(offering.product_id &&
        hasPermission(data?.permissions, "product:detail")
            ? [
                  {
                      key: "product",
                      label: "打开商品资料",
                      href: `/master-data/products/${encodeURIComponent(offering.product_id)}`,
                  },
              ]
            : []),
        ...(hasPermission(data?.permissions, "supplier:detail")
            ? [
                  {
                      key: "supplier",
                      label: "打开供应商资料",
                      href: `/master-data/suppliers/${encodeURIComponent(offering.supplier_id)}`,
                  },
              ]
            : []),
    ]
    if (links.length === 0) return null
    return (
        <div className="flex flex-wrap gap-2">
            {links.map((link) => (
                <Button
                    key={link.key}
                    id={`${idPrefix}-${link.key}`}
                    variant="outline"
                    size="sm"
                    render={<Link href={link.href} />}
                >
                    {link.label}
                    <ArrowUpRightIcon data-icon="inline-end" aria-hidden />
                </Button>
            ))}
        </div>
    )
}
