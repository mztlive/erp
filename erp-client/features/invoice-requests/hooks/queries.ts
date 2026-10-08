"use client"
import {
    useQuery,
    useQueries,
    useMutation,
    useQueryClient,
} from "@tanstack/react-query"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { hasPermission } from "@/lib/permissions"
import {
    listRequests,
    getRequest,
    getRequestAmounts,
    submitRequest,
    cancelRequest,
    normalizeInvoiceRequest,
    type RequestQuery,
} from "../api"
import {
    rememberSubmittedInvoiceRequest,
    submittedInvoiceRequest,
} from "../pending-detail"
export const requestKeys = { all: queryKeyRoots.invoiceRequests }
/** 当前账号的申请与查询能力，服务端仍执行最终授权。 */
export function useInvoiceRequestPermissions() {
    const profile = useAccountProfileQuery()
    return {
        userId: profile.data?.userid,
        canRead: hasPermission(
            profile.data?.permissions,
            "sales_invoice_request:list",
        ),
        canSubmit: hasPermission(
            profile.data?.permissions,
            "sales_invoice_request:submit",
        ),
        canCancel: hasPermission(
            profile.data?.permissions,
            "sales_invoice_request:cancel",
        ),
    }
}
/** 分页查询；隐藏无权限内容时不发送请求。 */
export const useInvoiceRequests = (query: RequestQuery, enabled = true) =>
    useQuery({
        queryKey: [...requestKeys.all, "list", query],
        queryFn: () => listRequests(query),
        enabled,
    })
/** 详情查询支持从销售单、集中列表和工作台进入。 */
export const useInvoiceRequest = (id?: string) => {
    const seeded = id ? submittedInvoiceRequest(id) : undefined
    return useQuery({
        queryKey: [...requestKeys.all, "detail", id],
        queryFn: () => getRequest(id!),
        enabled: Boolean(id),
        ...(seeded ? { initialData: seeded, initialDataUpdatedAt: 0 } : {}),
    })
}
/** 应收额度是服务端事实，禁止用列表合计替代。 */
export const useInvoiceRequestAmounts = (id?: string, enabled = true) =>
    useQuery({
        staleTime: 0,
        queryKey: [...requestKeys.all, "amounts", id],
        queryFn: () => getRequestAmounts(id!),
        enabled: Boolean(id) && enabled,
    })
/** 多结算主体逐笔核对准入；任一笔可申请才开放销售单入口。 */
export const useInvoiceRequestAvailability = (
    ids: string[],
    enabled: boolean,
) =>
    useQueries({
        queries: ids.map((id) => ({
            queryKey: [...requestKeys.all, "amounts", id],
            queryFn: () => getRequestAmounts(id),
            staleTime: 0,
            enabled,
        })),
    })
/** 审批与财务命令完成后刷新申请、票款、销售和任务。 */
export function useInvoiceRequestCommands() {
    const client = useQueryClient()
    const refresh = async () => {
        await Promise.all(
            [
                "invoice-requests",
                "customer-receivables",
                "sales-orders",
                "work-items",
                "workspace-home",
                "approval",
            ].map((root) => client.invalidateQueries({ queryKey: [root] })),
        )
    }
    const submit = useMutation({
        mutationFn: submitRequest,
        onSuccess: async (payload) => {
            const request = normalizeInvoiceRequest(payload)
            if (request.id) rememberSubmittedInvoiceRequest(request)
            await refresh()
        },
    })
    const cancel = useMutation({
        mutationFn: cancelRequest,
        onSuccess: refresh,
    })
    return { submit, cancel, refresh }
}
