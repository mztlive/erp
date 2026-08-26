import type { Metadata } from "next"

import { ListRouteRedirect } from "@/features/master-data/pages/list-route-redirect"

export const metadata: Metadata = {
    title: "品牌",
}

export default function Page() {
    return <ListRouteRedirect href="/master-data/brands" label="品牌列表" />
}
