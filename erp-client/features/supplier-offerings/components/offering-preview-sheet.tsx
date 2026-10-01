"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"
import { QuickPreviewSheet } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { OfferingTerms } from "./offering-terms"
import { OfferingAvailability, OfferingLinks } from "./offering-context"
import { offeringDetailHref } from "../lib/detail"
import { statusVariant } from "../lib/presentation"
import { OFFERING_STATUS_LABELS, type SupplierOfferingView } from "../types"

export function OfferingPreviewSheet({
    offering,
    returnTo,
    onClose,
    onClosed,
}: {
    offering: SupplierOfferingView | null
    returnTo: string
    onClose: () => void
    onClosed: () => void
}) {
    const { data } = useAccountProfileQuery()
    return (
        <QuickPreviewSheet
            idPrefix="supplier-offering-preview"
            size="preview"
            open={offering != null}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
            onOpenChangeComplete={(open) => {
                if (!open) onClosed()
            }}
            identity={
                offering ? (
                    <span className="num">
                        供应商订货编码：{offering.supplier_sku_code}
                    </span>
                ) : undefined
            }
            title={offering?.sku_name || offering?.sku_no || "供给资料"}
            description={
                offering
                    ? [
                          offering.specification,
                          offering.supplier_name || offering.supplier_no,
                      ]
                          .filter(Boolean)
                          .join(" · ")
                    : undefined
            }
            summary={
                offering ? (
                    <div className="flex flex-wrap items-center gap-2">
                        <Badge variant={statusVariant(offering.status)}>
                            {OFFERING_STATUS_LABELS[offering.status]}
                        </Badge>
                        <span className="text-xs text-muted-foreground">
                            条款 v{offering.current_revision_no ?? "—"}
                        </span>
                    </div>
                ) : undefined
            }
            footer={
                <>
                    <Button
                        id="supplier-offering-preview-footer-close"
                        variant="outline"
                        onClick={onClose}
                    >
                        关闭
                    </Button>
                    {offering ? (
                        <Button
                            id="supplier-offering-preview-open-detail"
                            render={
                                <Link
                                    href={offeringDetailHref(
                                        offering.id,
                                        returnTo,
                                    )}
                                />
                            }
                        >
                            打开供给资料
                            <ArrowUpRightIcon
                                data-icon="inline-end"
                                aria-hidden
                            />
                        </Button>
                    ) : null}
                </>
            }
        >
            {offering ? (
                <div className="space-y-6">
                    <OfferingTerms
                        terms={offering}
                        canViewCosts={hasPermission(
                            data?.permissions,
                            "supplier_offering_cost:detail",
                        )}
                        compact
                    />
                    <div className="border-t border-border pt-6">
                        <OfferingAvailability offering={offering} />
                    </div>
                    <OfferingLinks
                        offering={offering}
                        idPrefix="supplier-offering-preview-open"
                    />
                </div>
            ) : null}
        </QuickPreviewSheet>
    )
}
