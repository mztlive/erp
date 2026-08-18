/**
 * W05 销售单详情与附属信息查询（queryFn 纯函数）。
 *
 * 后端域：sales_order / sales_change_order / customer。失败统一抛
 * ApiError（@/lib/api），禁止 throw new Error("string")。
 */

import { apiGet } from "@/lib/api"
import type { ApiError } from "@/lib/api/errors"
import { downloadFileAsset } from "@/features/file-assets/api"
import {
    PERMISSION_VERSION,
    type BackendContractDetail,
    type BackendCustomerDetail,
    type BackendPartyContact,
    type BackendSalesChangeOrder,
    type BackendSalesOrderDetail,
    type PageView,
    type SalesOrderDetailView,
} from "@/features/sales-orders/api/contracts"
import { fetchSalesChangeOrderDetail } from "@/features/sales-orders/api/sales-orders-change"
import {
    formatInstant,
    formatIsoNow,
    mapChangeOrder,
    mapDetailToListItem,
    mapNature,
    throwValidation,
} from "@/features/sales-orders/api/mappers"
import type { SalesOrderNature } from "@/features/sales-orders/types"

/**
 * 下载销售单关联合同的当前修订 PDF。
 *
 * @param contractId 合同稳定身份
 */
export async function downloadSalesOrderContractPdf(
    contractId: string,
): Promise<void> {
    const id = contractId.trim()
    if (!id) {
        throwValidation("该销售单没有关联合同")
    }
    const contract = await apiGet<BackendContractDetail>(
        `/admin/contracts/${id}`,
    )
    const revision = contract.revisions.find(
        (item) => item.id === contract.current_revision_id,
    )
    const fileId = revision?.contract_pdf_file_id?.trim()
    if (!fileId) {
        throwValidation("合同尚未归档 PDF，无法下载")
    }
    await downloadFileAsset(fileId, `${contract.contract_no}.pdf`)
}

/**
 * 详情页附属信息：卡券审批与在途改单。
 *
 * 采购驳回摘要由销售单详情字段 `open_procurement_rejection` 权威下发，
 * 不再侧查采购确认列表（销售角色通常无 `procurement_confirmation:list`，
 * 且旧实现仅取全局第 1 页 50 条，会静默丢入口）。
 */
async function loadDetailExtras(
    salesOrderId: string,
    nature: SalesOrderNature,
) {
    const changeOrdersPage = await apiGet<PageView<BackendSalesChangeOrder>>(
        "/admin/sales-change-orders",
        {
            sales_order_id: salesOrderId,
            page: 1,
            page_size: 10,
        },
    ).catch(() => ({
        items: [] as BackendSalesChangeOrder[],
        total: 0,
        page: 1,
        page_size: 10,
    }))

    const activeChange =
        changeOrdersPage.items.find(
            (c) =>
                c.sales_order_id === salesOrderId &&
                c.status !== "EFFECTIVE" &&
                c.status !== "VOIDED" &&
                c.status !== "REJECTED",
        ) ?? null

    if (!activeChange) {
        return { activeChangeOrder: null }
    }

    const detailed = await fetchSalesChangeOrderDetail(
        activeChange.id,
        nature,
    ).catch(() => mapChangeOrder(activeChange, nature))

    return {
        activeChangeOrder: detailed,
    }
}

async function loadCustomerDisplay(customerId: string): Promise<{
    customerName?: string
    customerContact?: string
}> {
    try {
        const customer = await apiGet<BackendCustomerDetail>(
            `/admin/customers/${customerId}`,
        )
        const contacts = await apiGet<PageView<BackendPartyContact>>(
            `/admin/parties/${customer.party_id}/contacts`,
            {
                status: "active",
                page: 1,
                page_size: 100,
                sort_by: "created_at",
                sort_dir: "desc",
            },
        ).catch(() => null)
        const contact =
            contacts?.items.find((item) => item.is_default) ??
            contacts?.items[0]

        return {
            customerName: customer.legal_name || customer.customer_no,
            customerContact: contact?.contact_name,
        }
    } catch {
        return {}
    }
}

export async function fetchSalesOrderDetail(
    id: string,
): Promise<SalesOrderDetailView | null> {
    let detail: BackendSalesOrderDetail
    try {
        detail = await apiGet<BackendSalesOrderDetail>(
            `/admin/sales-orders/${id}`,
        )
    } catch (err) {
        const apiErr = err as ApiError
        if (apiErr?.status === 404) return null
        throw err
    }

    const customerDisplay = await loadCustomerDisplay(detail.customer_id)
    let contractNumber = ""
    let customerName = customerDisplay.customerName || detail.customer_id
    if (detail.contract_id) {
        try {
            const contract = await apiGet<BackendContractDetail>(
                `/admin/contracts/${detail.contract_id}`,
            )
            contractNumber = contract.contract_no
            const rev = contract.revisions.find(
                (r) => r.id === contract.current_revision_id,
            )
            if (rev?.customer_name) customerName = rev.customer_name
        } catch {
            // 合同域缺口时保留 id 展示
        }
    }

    const extras = await loadDetailExtras(id, mapNature(detail.business_type))
    const order = mapDetailToListItem(detail, {
        customerName,
        contractNumber,
        ownerName: detail.owner_user_name || "—",
        customerContact: customerDisplay.customerContact,
        ...extras,
    })

    // 最近验收摘要（可选）
    let acceptance: SalesOrderDetailView["acceptance"] = null
    try {
        const accPage = await apiGet<
            PageView<{
                id: string
                acceptance_no: string
                sales_order_id: string
                accepted_at: number
                result: string
                status: string
                version: number
                created_at: number
            }>
        >("/admin/customer-acceptances", {
            sales_order_id: id,
            status: "POSTED",
            page: 1,
            page_size: 1,
            sort_by: "accepted_at",
            sort_dir: "desc",
        })
        const latest = accPage.items[0]
        if (latest) {
            acceptance = {
                acceptedQuantity: "",
                note: latest.result,
                reference: latest.acceptance_no,
                postedAt: formatInstant(latest.accepted_at),
            }
        }
    } catch {
        // 验收域失败不阻塞详情
    }

    const queriedAt = formatIsoNow()
    return {
        ...order,
        acceptance,
        permissionVersion: PERMISSION_VERSION,
        sourceAsOf: queriedAt,
        queriedAt,
    }
}
