"use client"
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { hasPermission } from "@/lib/permissions"
import {
    listRequests,
    getRequest,
    getRequestAmounts,
    submitRequest,
    cancelRequest,
    type RequestQuery,
} from "../api"
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
export const useInvoiceRequest = (id?: string) =>
    useQuery({
        queryKey: [...requestKeys.all, "detail", id],
        queryFn: () => getRequest(id!),
        enabled: Boolean(id),
    })
/** 应收额度是服务端事实，禁止用列表合计替代。 */
export const useInvoiceRequestAmounts = (id?: string, enabled = true) =>
    useQuery({
        queryKey: [...requestKeys.all, "amounts", id],
        queryFn: () => getRequestAmounts(id!),
        enabled: Boolean(id) && enabled,
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
        onSuccess: refresh,
    })
    const cancel = useMutation({
        mutationFn: cancelRequest,
        onSuccess: refresh,
    })
    return { submit, cancel, refresh }
}
