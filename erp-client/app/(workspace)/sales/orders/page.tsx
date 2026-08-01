import type { Metadata } from "next"

import { SalesOrdersListPage } from "@/features/sales-orders/sales-orders-list-page"

export const metadata: Metadata = {
  title: "销售单",
}

export default async function SalesOrdersPage({
  searchParams,
}: {
  searchParams: Promise<{ search?: string }>
}) {
  const { search = "" } = await searchParams
  return <SalesOrdersListPage key={search} initialSearch={search} />
}
