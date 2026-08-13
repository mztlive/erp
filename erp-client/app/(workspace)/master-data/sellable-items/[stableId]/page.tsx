import type { Metadata } from "next"

import { ListRouteRedirect } from "@/features/master-data/components/pages/list-route-redirect"

export const metadata: Metadata = {
    title: "公司商品池",
}

export default function Page() {
    return (
        <ListRouteRedirect
            href="/master-data/sellable-items"
            label="公司商品池"
        />
    )
}
