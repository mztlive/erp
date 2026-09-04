/**
 * W05 销售单详情与附属信息查询（queryFn 纯函数）。
 *
 * 后端域：sales_order / sales_change_order / customer。失败统一抛
 * ApiError（@/lib/api），禁止 throw new Error("string")。
 */

import { apiGet, apiPost } from "@/lib/api"
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
    pickSalesOrderCommercialSource,
    throwValidation,
} from "@/features/sales-orders/api/mappers"
import type { SalesOrderNature } from "@/features/sales-orders/types"

/**
 * 下载销售单关联合同的当前修订 PDF。
 *
 * @param contractId 合同稳定身份
 */
/**
 * 撤回尚未最终通过的销售单审批（`POST .../cancel-approval`）。
 * 服务端按单据乐观锁与运行中实例撤回；前端不依赖详情里的 instance 投影。
 */
export async function cancelSalesOrderApproval(input: {
    salesOrderId: string
    expectedVersion: number
    reason: string
    idempotencyKey: string
}): Promise<void> {
    await apiPost(`/admin/sales-orders/${input.salesOrderId}/cancel-approval`, {
        expected_version: input.expectedVersion,
        reason: input.reason.trim(),
        idempotency_key: input.idempotencyKey,
    })
}

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

/** 详情页附属信息：在途改单。 */
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

async function loadContractDisplay(
    contractId: string | null | undefined,
    contractRevisionId: string | null | undefined,
): Promise<{
    contractNumber?: string
    contractRevisionLabel?: string
    customerName?: string
}> {
    if (!contractId) return {}
    try {
        const contract = await apiGet<BackendContractDetail>(
            `/admin/contracts/${contractId}`,
        )
        const revision =
            contract.revisions.find((item) => item.id === contractRevisionId) ??
            contract.revisions.find(
                (item) => item.id === contract.current_revision_id,
            )
        return {
            contractNumber: contract.contract_no,
            contractRevisionLabel: revision
                ? `${contract.contract_no}@v${revision.revision_no}`
                : contract.contract_no,
            customerName: revision?.customer_name,
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

    const commercialSource = pickSalesOrderCommercialSource(detail)
    const [customerDisplay, contractDisplay, extras] = await Promise.all([
        loadCustomerDisplay(detail.customer_id),
        loadContractDisplay(detail.contract_id, commercialSource?.contract_revision_id),
        loadDetailExtras(id, mapNature(detail.business_type)),
    ])
    const order = mapDetailToListItem(detail, {
        customerName:
            contractDisplay.customerName ||
            customerDisplay.customerName ||
            detail.customer_id,
        contractNumber: contractDisplay.contractNumber,
        contractRevisionLabel: contractDisplay.contractRevisionLabel,
        ownerUserId: detail.owner_user_id || "",
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
        customerId: detail.customer_id,
        settlementPartyId: detail.settlement_party_id,
        acceptance,
        permissionVersion: PERMISSION_VERSION,
        sourceAsOf: queriedAt,
        queriedAt,
    }
}
