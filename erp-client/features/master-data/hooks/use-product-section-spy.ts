"use client"

import * as React from "react"

import {
    parseProductSectionId,
    type ProductEditorSectionId,
} from "@/features/master-data/lib/product-editor-model"

/**
 * 商品编辑分区 Tab。从 URL hash 恢复（如供给关系 returnTo），
 * 切换时写入 `#product-section-{id}`，不再做滚动 spy。
 */
export function useProductSectionSpy(
    isCreate: boolean,
    stableId: string | undefined,
) {
    const [activeSection, setActiveSectionState] =
        React.useState<ProductEditorSectionId>("basic")

    React.useLayoutEffect(() => {
        setActiveSectionState(
            parseProductSectionId(window.location.hash, isCreate),
        )
    }, [isCreate, stableId])

    const setActiveSection = React.useCallback((id: ProductEditorSectionId) => {
        setActiveSectionState(id)
        const nextHash = `#product-section-${id}`
        if (window.location.hash === nextHash) return
        window.history.replaceState(
            window.history.state,
            "",
            `${window.location.pathname}${window.location.search}${nextHash}`,
        )
    }, [])

    return { activeSection, setActiveSection }
}
