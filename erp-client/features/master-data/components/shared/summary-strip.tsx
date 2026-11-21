"use client"

import { surfaceInsetClassName } from "@/components/business"
import { cn } from "@/lib/utils"

/** 商品与供应商编辑页共用的摘要条。 */
export function MasterDataSummaryStrip({
    rows,
}: {
    rows: ReadonlyArray<Readonly<{ label: string; value: string }>>
}) {
    return (
        <dl
            className={cn(
                surfaceInsetClassName,
                "grid grid-cols-2 gap-x-6 gap-y-3 px-4 py-3 sm:grid-cols-4",
            )}
        >
            {rows.map((row) => (
                <div key={row.label} className="min-w-0">
                    <dt className="text-tiny text-muted-foreground">
                        {row.label}
                    </dt>
                    <dd
                        className="mt-0.5 truncate text-body-compact font-medium text-foreground"
                        title={row.value}
                    >
                        {row.value}
                    </dd>
                </div>
            ))}
        </dl>
    )
}
