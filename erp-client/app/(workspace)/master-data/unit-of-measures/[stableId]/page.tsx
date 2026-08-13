import type { Metadata } from "next"

import { ListRouteRedirect } from "@/features/master-data/components/pages/list-route-redirect"

export const metadata: Metadata = {
    title: "计量单位",
}

export default function Page() {
    return (
        <ListRouteRedirect
            href="/master-data/unit-of-measures"
            label="计量单位列表"
        />
    )
}
