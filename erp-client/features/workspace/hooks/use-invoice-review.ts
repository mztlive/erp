"use client"

import { useQuery } from "@tanstack/react-query"
import { approvalKeys } from "@/features/approval-workflow/queries"
import { fetchInvoiceApprovalReview } from "../api/invoice-review"

/** 与审批缓存一起失效；票款核对不混入冻结资料缓存。 */
export const useInvoiceApprovalReview = (instanceId: string) =>
    useQuery({
        queryKey: [...approvalKeys.instance(instanceId), "invoice-review"],
        queryFn: () => fetchInvoiceApprovalReview(instanceId),
        enabled: Boolean(instanceId),
        staleTime: 0,
    })
