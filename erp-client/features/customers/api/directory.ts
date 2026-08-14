import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api"
import type {
    CustomerDirectoryQuery,
    CustomerDirectoryResult,
} from "@/features/customers/types"
import { isApiError } from "./errors"
import { mapDirectoryItem } from "./mappers"
import type { BackendCustomerView } from "./wire-types"

/**
 * 查询客户目录。范围、过滤、排序和分页均由服务端执行。
 */
export async function fetchCustomerDirectory(
    query: CustomerDirectoryQuery,
): Promise<CustomerDirectoryResult> {
    const status = query.status === "all" ? undefined : query.status
    const path =
        query.scope === "all_authorized"
            ? "/admin/customers/all-authorized"
            : "/admin/customers"
    try {
        const page = await apiGet<Page<BackendCustomerView>>(path, {
            scope: query.scope,
            keyword: query.query?.trim() || undefined,
            status,
            page: query.page,
            page_size: query.pageSize,
            sort_by: "updated_at",
            sort_dir: query.sortDir === "asc" ? "asc" : "desc",
        })
        return {
            hasCustomerScope: true,
            items: page.items.map(mapDirectoryItem),
            totalInScope: page.total,
            page: page.page,
            pageSize: page.page_size,
            queriedAt: new Date().toISOString(),
        }
    } catch (error) {
        if (isApiError(error) && error.status === 403) {
            return {
                hasCustomerScope: false,
                items: [],
                totalInScope: 0,
                page: query.page,
                pageSize: query.pageSize,
                queriedAt: new Date().toISOString(),
            }
        }
        throw error
    }
}
