"use client"

import * as React from "react"

import {
    PRODUCT_EDITOR_SECTIONS,
    type ProductEditorSectionId,
} from "@/features/master-data/lib/product-editor-model"

/** 商品编辑分区 Tab 的滚动高亮（P2-19 scroll spy）。 */
export function useProductSectionSpy(
    isCreate: boolean,
    stableId: string | undefined,
) {
    const [activeSection, setActiveSection] =
        React.useState<ProductEditorSectionId>("basic")

    React.useEffect(() => {
        if (isCreate) return
        const sections = PRODUCT_EDITOR_SECTIONS.map((s) =>
            document.getElementById(`product-section-${s.id}`),
        ).filter((el): el is HTMLElement => el !== null)
        if (sections.length === 0) return
        const observer = new IntersectionObserver(
            (entries) => {
                for (const entry of entries) {
                    if (entry.isIntersecting) {
                        const id = entry.target.id.replace(
                            "product-section-",
                            "",
                        )
                        setActiveSection(id as ProductEditorSectionId)
                    }
                }
            },
            { rootMargin: "-20% 0px -65% 0px", threshold: 0 },
        )
        for (const section of sections) observer.observe(section)
        return () => observer.disconnect()
    }, [isCreate, stableId])

    return { activeSection, setActiveSection }
}
