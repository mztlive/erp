import type { Metadata } from "next"

import { PublicSelectionPage } from "@/features/sales-selection/pages/public-selection-page"

export const metadata: Metadata = {
    title: "客户选品",
    referrer: "no-referrer",
}

export default async function PublicSelectionRoute({
    params,
}: {
    params: Promise<{ token: string }>
}) {
    const { token } = await params
    return (
        <>
            <meta name="referrer" content="no-referrer" />
            <PublicSelectionPage token={token} />
        </>
    )
}
