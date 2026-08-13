import type { Metadata } from "next"

import { UnknownMasterDataPage } from "@/features/master-data/components/pages/unknown-master-data-page"

export const metadata: Metadata = {
    title: "基础资料",
}

export default function Page() {
    return <UnknownMasterDataPage />
}
