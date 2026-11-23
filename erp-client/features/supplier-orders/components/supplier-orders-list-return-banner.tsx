"use client"

import Link from "next/link"

import { Button } from "@/components/ui/button"

export function SupplierOrdersListReturnBanner({
    returnTo,
}: {
    returnTo: string
}) {
    return (
        <div className="flex min-w-0 flex-wrap items-center justify-between gap-2 rounded-lg border border-border/60 bg-background px-4 py-2.5 text-body-compact">
            <span className="text-muted-foreground">
                从关联页面进来的。返回时会回到原来的位置。
            </span>
            <Button
                id="supplier-orders-list-return-back"
                type="button"
                size="sm"
                variant="secondary"
                className="rounded-lg shadow-none"
                render={<Link href={returnTo} />}
            >
                返回来源
            </Button>
        </div>
    )
}
