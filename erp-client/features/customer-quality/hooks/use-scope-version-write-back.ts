"use client"

import * as React from "react"

import type { PatchDual } from "../lib/dual-filter-state"

export function useScopeVersionWriteBack(
    scopeVersion: string | undefined,
    page: number,
    patchDual: PatchDual,
    searchParams: URLSearchParams,
) {
    const writtenRef = React.useRef<string | null>(null)
    React.useEffect(() => {
        if (!scopeVersion || page !== 1) return
        if (searchParams.get("scopeVersion") === scopeVersion) return
        if (writtenRef.current === scopeVersion) return
        writtenRef.current = scopeVersion
        patchDual({ scopeVersion })
    }, [scopeVersion, page, patchDual, searchParams])
}
