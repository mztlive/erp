"use client"

import * as React from "react"

import { PageHeader, PageScaffold } from "@/components/business"
import { CategoryTreePage } from "@/features/master-data/category-tree-page"
import { masterDataCopy } from "@/features/master-data/copy"
import {
    isResource,
    ResourceNav,
} from "@/features/master-data/master-data-list-presentation"
import { MasterDataListWorkspace } from "@/features/master-data/master-data-list-workspace"

export function MasterDataPage({ resource }: { resource: string }) {
    const navRef = React.useRef<HTMLElement | null>(null)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const resultsHeadingRef = React.useRef<HTMLHeadingElement | null>(null)
    const lastFocusedRowId = React.useRef<string | null>(null)

    const valid = isResource(resource)

    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key === "/" &&
                !(event.target instanceof HTMLInputElement) &&
                !(event.target instanceof HTMLTextAreaElement)
            ) {
                // 弹窗 / 抽屉打开时不让 / 聚焦背景搜索框
                if (
                    document.querySelector(
                        '[role="dialog"], [data-slot="sheet"]',
                    )
                ) {
                    return
                }
                event.preventDefault()
                searchInputRef.current?.focus()
            }
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    React.useEffect(() => {
        if (!valid) return
        // Focus results title after resource switch for a11y announcement
        const t = window.setTimeout(() => {
            resultsHeadingRef.current?.focus()
        }, 0)
        return () => window.clearTimeout(t)
    }, [resource, valid])

    if (!valid) {
        return (
            <PageScaffold>
                <PageHeader
                    title={masterDataCopy.unknownResourceTitle}
                    description={masterDataCopy.unknownResourceDesc()}
                />
                <ResourceNav resource="" navRef={navRef} />
            </PageScaffold>
        )
    }

    /** 商品分类：树形维护，不走扁平列表。 */
    if (resource === "categories") {
        return <CategoryTreePage />
    }

    return (
        <MasterDataListWorkspace
            resource={resource}
            navRef={navRef}
            searchInputRef={searchInputRef}
            resultsHeadingRef={resultsHeadingRef}
            lastFocusedRowId={lastFocusedRowId}
        />
    )
}
