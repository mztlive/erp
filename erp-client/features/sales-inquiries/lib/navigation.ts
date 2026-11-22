import type { DetailWorkspaceBackLink } from "@/components/business"

/** 校验同站相对路径，移除内部工作台框架参数。 */
export function sanitizeReturnTo(
    value: string | null | undefined,
): string | null {
    if (!value) return null
    const trimmed = value.trim()
    if (!trimmed.startsWith("/") || trimmed.startsWith("//")) return null
    try {
        const parsed = new URL(trimmed, "https://erp.invalid")
        if (parsed.origin !== "https://erp.invalid") return null
        parsed.searchParams.delete("_workspace_frame")
        return `${parsed.pathname}${parsed.search}${parsed.hash}`
    } catch {
        return null
    }
}

export function resolveInquiryBackLink({
    id,
    procurement,
    returnTo,
}: {
    id: string
    procurement: boolean
    returnTo?: string | null
}): DetailWorkspaceBackLink {
    const safeReturnTo = sanitizeReturnTo(returnTo)
    const defaultHref = procurement
        ? "/procurement/inquiries"
        : "/sales/inquiries"
    const defaultLabel = procurement ? "返回采购询价列表" : "返回销售询价列表"

    if (!safeReturnTo) {
        return {
            id,
            href: defaultHref,
            label: defaultLabel,
            visibility: "standalone-only",
        }
    }

    const label = safeReturnTo.startsWith("/procurement/inquiries")
        ? "返回采购询价列表"
        : safeReturnTo.startsWith("/sales/inquiries")
          ? "返回销售询价列表"
          : "返回上一页"

    return {
        id,
        href: safeReturnTo,
        label,
        visibility: "always",
    }
}
