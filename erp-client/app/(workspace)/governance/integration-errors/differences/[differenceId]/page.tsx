import type { Metadata } from "next"
import { Suspense } from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
import { IntegrationDifferenceDetailPage } from "@/features/integration-errors/pages/integration-error-detail-page"

export const metadata: Metadata = {
    title: "对账差异",
}

type PageProps = {
    params: Promise<{ differenceId: string }>
}

export default async function Page({ params }: PageProps) {
    const { differenceId } = await params
    return (
        <Suspense
            fallback={<DetailWorkspaceSkeleton title="对账差异" sidebar />}
        >
            <IntegrationDifferenceDetailPage differenceId={differenceId} />
        </Suspense>
    )
}
