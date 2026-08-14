"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import {
    buildImportOpeningSearchParams,
    parseImportOpeningSearchParams,
    type ImportOpeningUrlState,
} from "@/features/import-opening/lib/url-state"

/** 导入工作面的 URL 状态：解析 → 全量替换 → 局部 patch，写入 router 历史（不滚动）。 */
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
