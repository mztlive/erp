import type { Metadata } from "next"

import { SalesOrdersListPage } from "@/features/sales-orders/sales-orders-list-page"
import { SalesOrderCreatePage } from "@/features/sales-orders/sales-order-create-page"

export const metadata: Metadata = {
  title: "销售单",
}

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
  }>
}) {
  const params = await searchParams
  const { search = "", nature, businessType } = params
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
          initialNature === "card_voucher" ? "card_voucher" : "physical_service"
        }
      />
    )
  }

  return (
    <SalesOrdersListPage
      key={`${search}:${initialNature}`}
      initialSearch={search}
      initialNature={initialNature}
    />
  )
}
