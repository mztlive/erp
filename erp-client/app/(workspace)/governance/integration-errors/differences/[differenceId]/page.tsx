import type { Metadata } from "next"
import { Suspense } from "react"

import { IntegrationDifferenceDetailPage } from "@/features/integration-errors/integration-error-detail-page"

export const metadata: Metadata = {
  title: "对账差异",
}

function Fallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
      <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
      <div className="h-80 animate-pulse rounded-lg bg-muted" />
    </div>
  )
}

type PageProps = {
  params: Promise<{ differenceId: string }>
}

export default async function Page({ params }: PageProps) {
  const { differenceId } = await params
  return (
    <Suspense fallback={<Fallback />}>
      <IntegrationDifferenceDetailPage differenceId={differenceId} />
    </Suspense>
  )
}
