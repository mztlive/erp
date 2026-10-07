"use client"

import { useMutation, useQueryClient } from "@tanstack/react-query"

import { toast } from "@/components/ui/toast"
import { supplementSalesOrderContract } from "@/features/sales-orders/api/sales-order-contract"
import { salesOrderKeys } from "@/features/sales-orders/hooks/queries"
import { getErrorMessage } from "@/lib/api/errors"

/** 补录成功后更新销售单列表、详情及草稿缓存。 */
export function useSupplementSalesOrderContract(onSupplemented: () => void) {
    const client = useQueryClient()
    return useMutation({
        mutationFn: supplementSalesOrderContract,
        onSuccess: async () => {
            onSupplemented()
            await client.invalidateQueries({ queryKey: salesOrderKeys.all })
            toast.add({
                title: "合同已补录",
                description: "销售单已关联合同。",
                type: "success",
            })
        },
        onError: (error, input) => {
            void client.invalidateQueries({
                queryKey: salesOrderKeys.contractCheck(input),
            })
            toast.add({
                title: "合同未补录",
                description: getErrorMessage(
                    error,
                    "请核对合同客户、结算主体和商业条款后重试",
                ),
                type: "error",
            })
        },
    })
}
