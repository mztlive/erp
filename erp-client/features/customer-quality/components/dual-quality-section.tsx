"use client"

import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { BusinessEmptyState } from "@/components/business"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import type { QualityCaliber } from "../dual-types"
import { parseCaliber } from "../lib/dual-url-state"
import { DualCaliberSwitch } from "./dual-caliber-switch"
import { CurrentCaliberPanel } from "./dual-quality/current-caliber-panel"
import { HistoryCaliberPanel } from "./dual-quality/history-caliber-panel"

/**
 * S3-05 M10 双口径独立查询区：当前负责与历史贡献各走本口径端点，
 * 人员／组织／业务条件全部进入 URL 与 Query key；候选各自区分；
 * 无范围／筛选为空／请求失败三分呈现；390px 下无页面横溢。
 */
export function DualQualitySection({
    from,
    to,
}: {
    from?: string
    to?: string
}) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const caliber: QualityCaliber = parseCaliber(searchParams.get("caliber"))

    function patchDual(
        patch: Record<string, string | null | undefined>,
        options?: { replace?: boolean; scroll?: boolean },
    ) {
        patchSearchParams(
            { router, pathname, searchParams },
            { ...patch, page: null },
            { replace: true, scroll: false, ...options },
        )
    }

    function handleCaliberChange(next: QualityCaliber) {
        // 切换口径即切换查询与缓存；对方口径的版本与分页不带入新口径。
        patchDual({
            caliber: next === "current" ? null : "history",
            scopeVersion: null,
            dualPage: null,
            ownerGroup: null,
            attributionGroup: null,
        })
    }

    if (!from || !to) {
        return (
            <section
                aria-label="客户经营质量双口径"
                className="flex min-w-0 flex-col gap-3"
            >
                <DualCaliberSwitch
                    caliber={caliber}
                    onChange={handleCaliberChange}
                />
                <BusinessEmptyState
                    kind="no-data"
                    title="请先确定统计期间"
                    description="选择起止日期后，方可按口径查询当前负责与历史贡献。"
                />
            </section>
        )
    }

    return (
        <section
            aria-label="客户经营质量双口径"
            className="flex min-w-0 flex-col gap-3"
        >
            <DualCaliberSwitch
                caliber={caliber}
                onChange={handleCaliberChange}
            />
            {caliber === "current" ? (
                <CurrentCaliberPanel
                    from={from}
                    to={to}
                    patchDual={patchDual}
                />
            ) : (
                <HistoryCaliberPanel
                    from={from}
                    to={to}
                    patchDual={patchDual}
                />
            )}
        </section>
    )
}
