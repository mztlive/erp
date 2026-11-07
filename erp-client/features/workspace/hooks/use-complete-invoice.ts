"use client"

import * as React from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "@/components/ui/toast"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { completeInvoice } from "../api/invoice-evidence"
import type { WorkspaceWorkItem } from "../types"
import type { InvoiceRequest } from "@/features/invoice-requests/api"

/** 附件确认与重试冻结同一命令；确认成功后刷新销售、应收及待办。 */
export function useCompleteInvoice(
    item: WorkspaceWorkItem,
    request: InvoiceRequest,
    onCompleted?: (id: string) => void,
) {
    const client = useQueryClient()
    const commandKey = React.useRef(crypto.randomUUID())
    const submittedCommand = React.useRef<
        Parameters<typeof completeInvoice>[0] | undefined
    >(undefined)
    const mutation = useMutation({
        mutationFn: (files: File[]) => {
            submittedCommand.current ??= {
                work_item_id: item.workItemId,
                expected_task_version: item.taskVersion,
                expected_request_version: request.version,
                idempotency_key: commandKey.current,
            }
            return completeInvoice(submittedCommand.current, files)
        },
        onSuccess: () => {
            toast.add({
                title: "发票已登记，本次开票任务已完成",
                type: "success",
            })
            onCompleted?.(item.workItemId)
            void Promise.all(
                [
                    queryKeyRoots.invoiceRequests,
                    queryKeyRoots.customerReceivables,
                    queryKeyRoots.salesOrders,
                    queryKeyRoots.workItems,
                    queryKeyRoots.workspaceHome,
                ].map((root) => client.invalidateQueries({ queryKey: root })),
            )
        },
    })
    return {
        ...mutation,
        reset: () => {
            commandKey.current = crypto.randomUUID()
            submittedCommand.current = undefined
            mutation.reset()
        },
    }
}
