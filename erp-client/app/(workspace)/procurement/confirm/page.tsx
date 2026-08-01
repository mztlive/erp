import type { Metadata } from "next"

import { ProcurementConfirmationPage } from "@/features/procurement-confirmation/procurement-confirmation-page"

export const metadata: Metadata = {
  title: "采购二次确认",
}

export default async function ProcurementConfirmPage({
  searchParams,
}: {
  searchParams: Promise<{ task?: string; completed?: string }>
}) {
  const { task, completed } = await searchParams
  return (
    <ProcurementConfirmationPage
      key={`${task ?? "first"}-${completed ?? "active"}`}
      initialTaskId={task}
      initialCompleted={completed === "1"}
    />
  )
}
