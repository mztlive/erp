"use client"

import { Button } from "@/components/ui/button"
import type { PatchDual } from "../../lib/dual-filter-state"

export function Pager({
    idPrefix,
    page,
    pageSize,
    total,
    scopeVersion,
    patchDual,
}: {
    idPrefix: string
    page: number
    pageSize: number
    total: number
    scopeVersion?: string
    patchDual: PatchDual
}) {
    const pageCount = Math.max(1, Math.ceil(total / pageSize))
    const safePage = Math.min(page, pageCount)
    return (
        <div className="flex min-w-0 flex-wrap items-center gap-2 text-[13px]">
            <Button
                id={`${idPrefix}-prev-page`}
                type="button"
                variant="outline"
                size="sm"
                disabled={safePage <= 1}
                onClick={() =>
                    patchDual({
                        dualPage:
                            safePage - 1 <= 1 ? null : String(safePage - 1),
                        scopeVersion: scopeVersion ?? null,
                    })
                }
            >
                上一页
            </Button>
            <span className="num text-muted-foreground">
                第 {safePage} / {pageCount} 页 · 共 {total} 行
            </span>
            <Button
                id={`${idPrefix}-next-page`}
                type="button"
                variant="outline"
                size="sm"
                disabled={safePage >= pageCount}
                onClick={() =>
                    patchDual({
                        dualPage: String(safePage + 1),
                        scopeVersion: scopeVersion ?? null,
                    })
                }
            >
                下一页
            </Button>
        </div>
    )
}
