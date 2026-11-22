"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"

import {
    buildImportBatchDetailSearchParams,
    buildImportOpeningSearchParams,
    parseImportBatchDetailSearchParams,
    parseImportOpeningSearchParams,
    type ImportOpeningUrlState,
} from "@/features/import-opening/lib/url-state"

/** 导入列表工作面的 URL 状态：解析 → 全量替换 → 局部 patch，写入 router 历史（不滚动）。 */
export function useImportOpeningUrlState() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const urlState = React.useMemo(
        () => parseImportOpeningSearchParams(searchParams),
        [searchParams],
    )

    const replaceUrl = React.useCallback(
        (next: ImportOpeningUrlState) => {
            const qs = buildImportOpeningSearchParams(next)
            router.replace(`${pathname}${qs}`, { scroll: false })
        },
        [pathname, router],
    )

    const patchUrl = React.useCallback(
        (patch: Partial<ImportOpeningUrlState>) => {
            replaceUrl({ ...urlState, ...patch })
        },
        [replaceUrl, urlState],
    )

    return { urlState, replaceUrl, patchUrl }
}

/** 独立导入批次详情页 URL 状态：与列表分页/筛选彻底隔离。 */
export function useImportBatchDetailUrlState(batchId: string) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const urlState = React.useMemo<ImportOpeningUrlState>(
        () => ({
            ...parseImportBatchDetailSearchParams(searchParams),
            batchId,
        }),
        [batchId, searchParams],
    )

    const returnTo = React.useMemo(() => {
        const raw = urlState.returnTo
        return raw?.startsWith("/") && !raw.startsWith("//") ? raw : undefined
    }, [urlState.returnTo])

    const replaceUrl = React.useCallback(
        (next: ImportOpeningUrlState) => {
            const qs = buildImportBatchDetailSearchParams(next)
            router.replace(`${pathname}${qs}`, { scroll: false })
        },
        [pathname, router],
    )

    const patchUrl = React.useCallback(
        (patch: Partial<ImportOpeningUrlState>) => {
            replaceUrl({ ...urlState, ...patch, batchId })
        },
        [batchId, replaceUrl, urlState],
    )

    return { urlState, returnTo, replaceUrl, patchUrl }
}
