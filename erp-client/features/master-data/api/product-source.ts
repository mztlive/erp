import { apiGet, apiPost } from "@/lib/api"
import type {
    ProductSourceDiffView,
    ResolveProductSourceDiff,
} from "./types-product-source"

export function fetchProductSourceDiff(productId: string) {
    return apiGet<ProductSourceDiffView | null>(
        `/admin/products/${encodeURIComponent(productId)}/source-diff`,
    )
}

export function resolveProductSourceDiff(
    productId: string,
    request: ResolveProductSourceDiff,
) {
    return apiPost<ProductSourceDiffView | null>(
        `/admin/products/${encodeURIComponent(productId)}/source-diff/resolve`,
        request,
    )
}
