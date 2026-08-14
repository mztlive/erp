"use client"

import { useRouter, useSearchParams } from "next/navigation"

import type { OrderSection } from "@/features/supplier-orders/types"
import { SECTIONS } from "@/features/supplier-orders/types"

export function resolveSection(raw?: string | null): OrderSection {
    if (raw && (SECTIONS as string[]).includes(raw)) return raw as OrderSection
    return "overview"
}

/**
 * 当前 Tab 与 URL 的 section 参数一一对应；
 * 「概览」是默认值，选中时不写参数。
 */
export function useSupplierOrderCenterSection(
    supplierOrderId: string,
    sectionProp?: string,
) {
    const router = useRouter()
    const searchParams = useSearchParams()

    const activeSection = resolveSection(
        sectionProp ?? searchParams.get("section"),
    )

    const setSection = (section: OrderSection) => {
        const params = new URLSearchParams(searchParams.toString())
        if (section === "overview") params.delete("section")
        else params.set("section", section)
        const qs = params.toString()
        router.replace(
            `/supplier-api/orders/${supplierOrderId}${qs ? `?${qs}` : ""}`,
            { scroll: false },
        )
    }

    return { activeSection, setSection }
}
