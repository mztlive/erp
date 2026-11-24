"use client"

import { useMemo } from "react"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { accountProfileKeys } from "@/features/auth/hooks/queries"
import { fetchAccountProfile } from "@/features/auth/api"

import { useScopedListQuery } from "@/lib/use-scoped-list-query"

import {
    fetchSalesOrderContractCheck,
    type SalesOrderContractInput,
} from "@/features/sales-orders/api/sales-order-contract"
import { approvalKeys } from "@/features/approval-workflow/queries"
import { workItemKeys } from "@/features/work-items/queries"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { fetchHasEligibleAcceptance } from "@/features/sales-orders/api/acceptance"
import { fetchSalesOrderDeliveryProgress } from "../api/sales-order-delivery-progress"
import { acceptanceFactUnit } from "../lib/acceptance-model"
import type { SalesOrderLineItem } from "../types"
import {
    fetchSalesExtraCosts,
    fetchSalesInquiryFeeCandidates,
    reduceSalesExtraCost,
    registerSalesExtraCost,
    type ReduceSalesExtraCostInput,
    type RegisterSalesExtraCostInput,
} from "@/features/sales-orders/api/sales-order-extra-costs"
import {
    cancelSalesOrderApproval,
    createSalesOrder,
    fetchSalesOrderDetail,
    fetchSalesChangeOrderDetail,
    fetchSalesOrderDraftForResume,
    fetchSalesOrders,
    saveSalesOrderDraft,
    startSalesChangeOrder,
    submitSalesChangeOrder,
    submitSalesOrder,
    voidSalesChangeOrder,
    type SalesOrdersListQuery,
} from "@/features/sales-orders/api/sales-orders"

export const salesOrderKeys = {
    all: queryKeyRoots.salesOrders,
    contractCheck: (input: SalesOrderContractInput) =>
        [...salesOrderKeys.all, "contract-check", input] as const,
    list: (query: SalesOrdersListQuery) =>
        [...salesOrderKeys.all, "list", query] as const,
    detail: (id: string) => [...salesOrderKeys.all, "detail", id] as const,
    acceptanceRoot: (id: string) =>
        [...salesOrderKeys.all, "acceptance", id] as const,
    acceptance: (
        id: string,
        filters: {
            workItemId?: string | null
            expectedTaskVersion?: string
        },
    ) => [...salesOrderKeys.acceptanceRoot(id), filters] as const,
    acceptanceEligibility: (id: string) =>
        [...salesOrderKeys.acceptanceRoot(id), "eligibility"] as const,
    extraCosts: (id: string) =>
        [...salesOrderKeys.all, "extra-costs", id] as const,
    inquiryFees: (id: string) =>
        [...salesOrderKeys.all, "inquiry-fees", id] as const,
}

export function useSalesOrdersQuery(
    query: SalesOrdersListQuery,
    enabled = true,
    includeContractDetails = true,
) {
    return useScopedListQuery({
        query,
        queryKey: (input) => [
            ...salesOrderKeys.list(input),
            includeContractDetails,
        ],
        fetchPage: (input) =>
            fetchSalesOrders(input, { includeContractDetails }),
        enabled,
    })
}

export function useSalesOrderDetailQuery(
    salesOrderId: string,
    refreshOnMount = false,
    expectedSubjectVersion?: string,
) {
    const client = useQueryClient()
    const result = useQuery({
        queryKey: expectedSubjectVersion
            ? [
                  ...salesOrderKeys.detail(salesOrderId),
                  "task-material",
                  expectedSubjectVersion,
              ]
            : salesOrderKeys.detail(salesOrderId),
        refetchOnMount: refreshOnMount ? "always" : true,
        queryFn: async () => {
            const account = await client.ensureQueryData({
                queryKey: accountProfileKeys.current(),
                queryFn: fetchAccountProfile,
            })
            const previous = client.getQueryData<
                | import("@/features/sales-orders/api/contracts").SalesOrderDetailView
                | null
            >(salesOrderKeys.detail(salesOrderId))
            return fetchSalesOrderDetail(
                salesOrderId,
                previous?.changeOrderScopeVersion,
                account.permissions,
                expectedSubjectVersion,
            )
        },
        enabled: Boolean(salesOrderId),
    })
    const order = result.data
    const skuIds = order
        ? [
              ...order.lineItems,
              ...order.revisions.flatMap((revision) => revision.lines ?? []),
          ].map((line) => line.skuId)
        : []
    const { names, nameForUnit } = useProductUnitNames(skuIds, Boolean(order))
    const data = useMemo(() => {
        if (!order) return order
        const lineItems = order.lineItems.map((line) => ({
            ...line,
            unitName: line.unitName ?? nameForUnit(line.unit, line.skuId),
        }))
        const linesById = new Map(lineItems.map((line) => [line.id, line]))
        const procurementProgress = order.related.procurementProgress
        return {
            ...order,
            lineItems,
            revisions: order.revisions.map((revision) => ({
                ...revision,
                lines: revision.lines?.map((line) => ({
                    ...line,
                    unitName:
                        line.unitName ?? names.get(line.unit?.trim() ?? ""),
                })),
            })),
            related: {
                ...order.related,
                procurementProgress: {
                    ...procurementProgress,
                    lines: procurementProgress.lines?.map((line) => {
                        const salesLine = linesById.get(line.id)
                        return {
                            ...line,
                            unitName:
                                !line.unit || line.unit === salesLine?.unit
                                    ? salesLine?.unitName
                                    : names.get(line.unit.trim()),
                        }
                    }),
                },
            },
        }
    }, [order, names, nameForUnit])
    return { ...result, data }
}

/** 按指定变更单读取，历史入口不得回退到当前活动变更单。 */
export function useSalesChangeOrderDetailQuery(
    salesOrderId: string,
    changeOrderId: string,
    nature: Parameters<typeof fetchSalesChangeOrderDetail>[1] | undefined,
) {
    return useQuery({
        queryKey: [
            ...salesOrderKeys.detail(salesOrderId),
            "change",
            changeOrderId,
            nature,
        ],
        queryFn: () => {
            if (!nature) throw new Error("销售单业务性质尚未加载")
            return fetchSalesChangeOrderDetail(
                changeOrderId,
                nature,
                salesOrderId,
            )
        },
        enabled: Boolean(changeOrderId && nature),
    })
}

export function useSalesOrderAcceptanceEligibilityQuery(
    salesOrderId: string,
    enabled: boolean,
) {
    return useQuery({
        queryKey: salesOrderKeys.acceptanceEligibility(salesOrderId),
        queryFn: () => fetchHasEligibleAcceptance(salesOrderId),
        enabled: enabled && Boolean(salesOrderId),
    })
}

/** 概览与验收入口共享交付进度，登记或冲正后随验收缓存一起刷新。 */
export function useSalesOrderDeliveryProgressQuery(
    id: string,
    enabled: boolean,
    salesLines: readonly SalesOrderLineItem[] = [],
) {
    const result = useQuery({
        queryKey: [...salesOrderKeys.acceptanceRoot(id), "delivery-progress"],
        queryFn: () => fetchSalesOrderDeliveryProgress(id),
        enabled: Boolean(id) && enabled,
    })
    const progress = result.data
    const { names } = useProductUnitNames(
        salesLines.map((line) => line.skuId),
        Boolean(progress),
    )
    const data = useMemo(() => {
        if (!progress) return progress
        const linesById = new Map(salesLines.map((line) => [line.id, line]))
        const lines = progress.lines.map((line) => {
            const salesLine = linesById.get(line.salesOrderLineId)
            const unitName =
                names.get(line.unitCode.trim()) ??
                (line.unitCode === salesLine?.unit
                    ? salesLine.unitName
                    : undefined)
            const withUnit = (fact: Parameters<typeof acceptanceFactUnit>[0]) =>
                acceptanceFactUnit(
                    fact,
                    { unitCode: line.unitCode, unitName },
                    names,
                )
            return {
                ...line,
                unitName,
                pendingFacts: line.pendingFacts.map(withUnit),
                fulfillmentFacts: line.fulfillmentFacts?.map(withUnit),
            }
        })
        return {
            ...progress,
            lines,
            unitName: progress.unitCode
                ? (names.get(progress.unitCode.trim()) ??
                  lines.find(
                      (line) =>
                          line.unitCode === progress.unitCode && line.unitName,
                  )?.unitName)
                : undefined,
        }
    }, [progress, salesLines, names])
    return { ...result, data }
}

export function useCancelSalesOrderApprovalMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: cancelSalesOrderApproval,
        onSuccess: async (_data, variables) => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.detail(variables.salesOrderId),
                }),
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: approvalKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: workItemKeys.all,
                }),
            ])
        },
    })
}

export function useSalesOrderDraftResumeQuery(salesOrderId: string) {
    return useQuery({
        queryKey: [
            ...salesOrderKeys.detail(salesOrderId),
            "draft-resume",
        ] as const,
        queryFn: () => fetchSalesOrderDraftForResume(salesOrderId),
        enabled: salesOrderId.length > 0,
    })
}

export function useCreateSalesOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: createSalesOrder,
        onSuccess: async (data) => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.detail(data.salesOrderId),
                }),
                queryClient.invalidateQueries({ queryKey: ["contracts"] }),
                queryClient.invalidateQueries({
                    queryKey: approvalKeys.all,
                }),
            ])
        },
    })
}

export function useSaveSalesOrderDraftMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: saveSalesOrderDraft,
        onSuccess: async (_data, variables) => {
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.detail(variables.salesOrderId),
            })
        },
    })
}

export function useSubmitSalesOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitSalesOrder,
        onSuccess: async (data) => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.detail(data.salesOrderId),
                }),
                queryClient.invalidateQueries({
                    queryKey: approvalKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: workItemKeys.all,
                }),
            ])
        },
    })
}

export function useStartSalesChangeOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: startSalesChangeOrder,
        onSuccess: async (_data, variables) => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.detail(variables.salesOrderId),
                }),
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: approvalKeys.all,
                }),
            ])
        },
    })
}

/**
 * 提交销售变更审批。成功后刷新销售单、审批实例与任务。
 */
export function useSubmitSalesChangeOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitSalesChangeOrder,
        onSuccess: (_data, variables) => {
            void Promise.all([
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.detail(variables.salesOrderId),
                }),
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: approvalKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: workItemKeys.all,
                }),
            ])
        },
    })
}

/**
 * 作废销售改单草稿。成功后刷新销售单、审批实例与任务。
 */
export function useVoidSalesChangeOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: voidSalesChangeOrder,
        onSuccess: async (_data, variables) => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.detail(variables.salesOrderId),
                }),
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: approvalKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: workItemKeys.all,
                }),
            ])
        },
    })
}

/** 每次选择合同重新核对，最终补录仍由后端事务把关。 */
export function useSalesOrderContractCheck(
    input: SalesOrderContractInput,
    enabled: boolean,
) {
    return useQuery({
        queryKey: salesOrderKeys.contractCheck(input),
        queryFn: () => fetchSalesOrderContractCheck(input),
        enabled:
            enabled &&
            Boolean(input.contractId && input.requestedContractRevisionId),
        staleTime: 0,
        retry: false,
    })
}

/** 销售单附加费用；仅在当前账号可查看成本时请求。 */
export function useSalesExtraCostsQuery(
    salesOrderId: string,
    enabled: boolean,
) {
    return useQuery({
        queryKey: salesOrderKeys.extraCosts(salesOrderId),
        queryFn: () => fetchSalesExtraCosts(salesOrderId),
        enabled: enabled && Boolean(salesOrderId),
    })
}

/** 销售单可登记的询价费用候选；仅在具备成本登记权限且弹窗打开时读取。 */
export function useSalesInquiryFeeCandidatesQuery(
    salesOrderId: string,
    enabled: boolean,
) {
    return useQuery({
        queryKey: salesOrderKeys.inquiryFees(salesOrderId),
        queryFn: () => fetchSalesInquiryFeeCandidates(salesOrderId),
        enabled: enabled && Boolean(salesOrderId),
        staleTime: 0,
    })
}

export function useRegisterSalesExtraCostMutation(salesOrderId: string) {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: RegisterSalesExtraCostInput) =>
            registerSalesExtraCost(salesOrderId, input),
        onSuccess: async () => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.extraCosts(salesOrderId),
                }),
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.inquiryFees(salesOrderId),
                }),
            ])
        },
    })
}

export function useReduceSalesExtraCostMutation(salesOrderId: string) {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: {
            costEntryId: string
            value: ReduceSalesExtraCostInput
        }) =>
            reduceSalesExtraCost(salesOrderId, input.costEntryId, input.value),
        onSuccess: () =>
            queryClient.invalidateQueries({
                queryKey: salesOrderKeys.extraCosts(salesOrderId),
            }),
    })
}
