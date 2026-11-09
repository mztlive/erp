import { apiGet, type Page } from "@/lib/api"
import type {
    WorkspaceChoiceKey,
    WorkspaceSearchOptions,
} from "@/lib/workspace-search"

/** 单据候选由所属业务目录授权，任务接口只接收选中的稳定 ID。 */
export async function taskOrderOptions(
    name: WorkspaceChoiceKey,
    query: string,
): Promise<WorkspaceSearchOptions> {
    const sales = name === "sales_orders"
    const page = await apiGet<
        Page<{ id: string; order_no?: string; purchase_no?: string }>
    >(sales ? "/admin/sales-orders" : "/admin/purchase-orders", {
        q: query || undefined,
        page: 1,
        page_size: 30,
        sort_by: "created_at",
        sort_dir: "desc",
    })
    return {
        [name]: page.items.map((item) => ({
            value: item.id,
            label: (sales ? item.order_no : item.purchase_no) || "单号未设置",
        })),
    }
}
