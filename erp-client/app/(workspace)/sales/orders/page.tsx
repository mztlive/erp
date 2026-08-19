import type { Metadata } from "next"
import { Suspense } from "react"

import { SalesOrdersListPage } from "@/features/sales-orders/pages/sales-orders-list-page"
import { SalesOrderCreatePage } from "@/features/sales-orders/pages/sales-order-create-page"

export const metadata: Metadata = {
    title: "销售单",
}

function SalesOrdersListFallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
            <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
            <div className="h-16 animate-pulse rounded-xl bg-muted" />
            <div className="h-24 animate-pulse rounded-lg bg-muted" />
            <div className="h-72 animate-pulse rounded-lg bg-muted" />
        </div>
    )
}

/**
 * SPA 壳：URL 查询由客户端恢复 mode / nature / customerId 等。
 * 业务数据不在服务端 fetch。
 *
 * 实物/卡券销售单与销售变更单走通用审批区。
 * SalesReturnCase 为 NO_APPROVAL：本页列表与建单路径不展示销售退货审批区，
 * 不接入绑定卡、运行摘要或决定弹窗。
 */
export default async function SalesOrdersPage({
    searchParams,
}: {
    searchParams: Promise<{
        search?: string
        nature?: string
        businessType?: string
        mode?: string
        customerId?: string
        contractId?: string
        contractRevisionId?: string
        salesOrderId?: string
    }>
}) {
    const params = await searchParams
    const { nature, businessType } = params
    const requestedNature = nature ?? businessType?.toLowerCase()
    const initialNature =
        requestedNature === "card_voucher" || requestedNature === "voucher"
            ? "card_voucher"
            : requestedNature === "physical_service" ||
                requestedNature === "goods_service"
              ? "physical_service"
              : "all"

    if (params.mode === "create") {
        return (
            <SalesOrderCreatePage
                initialCustomerId={params.customerId}
                initialContractId={params.contractId}
                initialContractRevisionId={params.contractRevisionId}
                initialNature={
                    initialNature === "card_voucher"
                        ? "card_voucher"
                        : "physical_service"
                }
                initialSalesOrderId={params.salesOrderId}
            />
        )
    }

    return (
        <Suspense fallback={<SalesOrdersListFallback />}>
            <SalesOrdersListPage />
        </Suspense>
    )
}
