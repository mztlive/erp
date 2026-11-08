import { PagePulseBars } from "@/components/layout/page-pulse"
import type { Metadata } from "next"
import { Suspense } from "react"

import { IntegrationDifferenceDetailPage } from "@/features/integration-errors/pages/integration-error-detail-page"

export const metadata: Metadata = {
    title: "对账差异",
}

function Fallback() {
    return (
        <PagePulseBars
            bars={[
                "h-10 w-56 animate-pulse rounded-lg bg-muted",
                "h-80 animate-pulse rounded-lg bg-muted",
            ]}
        />
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
