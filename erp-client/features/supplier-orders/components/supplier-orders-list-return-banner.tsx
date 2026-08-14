"use client"

import Link from "next/link"

import { surfacePanelClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

export function SupplierOrdersListReturnBanner({
    returnTo,
}: {
    returnTo: string
}) {
    return (
        <div
            className={cn(
                surfacePanelClassName,
                "flex flex-wrap items-center justify-between gap-2 px-4 py-2.5 text-sm",
            )}
        >
            <span className="text-muted-foreground">
                从关联页面进来的。返回时会回到原来的位置。
            </span>
            <Button
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
