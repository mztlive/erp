import type { Metadata } from "next"

import { SalesOrdersListPage } from "@/features/sales-orders/sales-orders-list-page"

export const metadata: Metadata = {
  title: "销售单",
}

export default async function SalesOrdersPage({
  searchParams,
}: {
  searchParams: Promise<{ search?: string; nature?: string }>
}) {
  const { search = "", nature } = await searchParams
  const initialNature =
    nature === "card_voucher" || nature === "physical_service"
      ? nature
      : "all"
  return (
    <SalesOrdersListPage
      key={`${search}:${initialNature}`}
      initialSearch={search}
      initialNature={initialNature}
    />
  )
}
