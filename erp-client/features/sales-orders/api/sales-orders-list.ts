/**
 * W05 销售单列表查询（queryFn 纯函数）。
 *
 * 后端域：sales_order。在本文件内将后端 DTO 适配为既有前端视图契约。
 * 失败统一抛 ApiError（@/lib/api），禁止 throw new Error("string")。
 */

import { apiGet } from "@/lib/api"
import type {
    BackendContractDetail,
    BackendSalesOrderView,
    PageView,
    SalesOrderListView,
    SalesOrdersListQuery,
} from "@/features/sales-orders/api/contracts"
import {
    formatIsoNow,
    mapCloseFilterToBackend,
    mapCollectionFilterToBackend,
    mapCommercialStatusFilterToBackend,
    mapFulfillmentFilterToBackend,
    mapInvoiceFilterToBackend,
    mapListItemFromBackend,
    mapReviewStatusFilterToBackend,
    mapSortBy,
} from "@/features/sales-orders/api/mappers"

export async function fetchSalesOrders(
    query: SalesOrdersListQuery,
): Promise<SalesOrderListView> {
    const businessType =
        query.nature === "card_voucher"
            ? "VOUCHER"
            : query.nature === "physical_service"
              ? "GOODS_SERVICE"
              : undefined

    // "待我处理"/"我创建的" 都按创建人过滤；"待我处理"额外限定草稿/驳回回销售，
    // "异常"限定审核轨被驳回。前两类视图与精细商业/审核状态互斥，避免形成
    // 同字段冲突条件；其余结构化筛选可继续与固定视图 AND 组合。
    const createdBy =
        query.createdBy?.trim() ||
        (query.summary === "mine" || query.summary === "createdByMe"
            ? query.currentUserId?.trim() || undefined
            : undefined)
    const myTodo = query.summary === "mine"
    const exceptionOnly = query.summary === "exception"

    const page = await apiGet<PageView<BackendSalesOrderView>>(
        "/admin/sales-orders",
        {
            page: query.page,
            page_size: query.pageSize,
            order_no: query.search?.trim() || undefined,
            customer_id: query.customerId,
            contract_id: query.contractId,
            business_type: businessType,
            origin_system:
                query.origin === "erp"
                    ? "ERP"
                    : query.origin === "mall"
                      ? "MALL"
                      : undefined,
            commercial_status:
                myTodo || exceptionOnly
                    ? undefined
                    : mapCommercialStatusFilterToBackend(
                          query.commercialStatus,
                      ),
            review_status:
                myTodo || exceptionOnly
                    ? undefined
                    : mapReviewStatusFilterToBackend(query.reviewStatus),
            fulfillment_progress: mapFulfillmentFilterToBackend(
                query.fulfillment,
            ),
            collection_progress: mapCollectionFilterToBackend(query.collection),
            invoice_progress: mapInvoiceFilterToBackend(query.invoice),
            close_status: mapCloseFilterToBackend(query.closeStatus),
            created_from: query.createdFrom,
            created_to: query.createdTo,
            created_by: createdBy,
            my_todo: myTodo || undefined,
            exception_only: exceptionOnly || undefined,
            sort_by: mapSortBy(query.sortBy),
            sort_dir: query.sortDir,
        },
    )

    const contractDisplays = await loadContractDisplays(
        page.items
            .map((row) => row.contract_id)
            .filter((id): id is string => Boolean(id)),
    )
    const items = page.items.map((row) => {
        const display = row.contract_id
            ? contractDisplays.get(row.contract_id)
            : undefined
        return mapListItemFromBackend(row, {
            contractNumber: display?.contractNumber,
            contractCompanyName: display?.companyName,
            customerName: display?.companyName || undefined,
        })
    })

    return {
        items,
        total: page.total,
        page: page.page,
        pageSize: page.page_size,
        queriedAt: formatIsoNow(),
    }
}

type ContractDisplay = {
    contractNumber: string
    companyName: string
}

/**
 * 按合同 ID 批量补齐编号与公司名称；当前页去重后分批拉取，单份失败不拖垮整表。
 */
async function loadContractDisplays(
    contractIds: string[],
): Promise<Map<string, ContractDisplay>> {
    const unique = [
        ...new Set(contractIds.map((id) => id.trim()).filter(Boolean)),
    ]
    const displays = new Map<string, ContractDisplay>()
    const chunkSize = 8
    for (let index = 0; index < unique.length; index += chunkSize) {
        const chunk = unique.slice(index, index + chunkSize)
        const loaded = await Promise.all(
            chunk.map(async (contractId) => {
                try {
                    const contract = await apiGet<BackendContractDetail>(
                        `/admin/contracts/${contractId}`,
                    )
                    const revision = contract.revisions.find(
                        (item) => item.id === contract.current_revision_id,
                    )
                    return [
                        contractId,
                        {
                            contractNumber: contract.contract_no,
                            companyName: revision?.customer_name ?? "",
                        },
                    ] as const
                } catch {
                    return [
                        contractId,
                        { contractNumber: "", companyName: "" },
                    ] as const
                }
            }),
        )
        for (const [contractId, display] of loaded) {
            displays.set(contractId, display)
        }
    }
    return displays
}
