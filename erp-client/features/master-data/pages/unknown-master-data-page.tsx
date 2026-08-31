"use client"

import { PageHeader, PageScaffold } from "@/components/business"
import { ResourceNav } from "@/features/master-data/components/list/list-chrome"
import { masterDataCopy } from "@/features/master-data/lib/copy"

export function UnknownMasterDataPage() {
    return (
        <PageScaffold>
            <PageHeader
                title={masterDataCopy.unknownResourceTitle}
                description={masterDataCopy.unknownResourceDesc()}
            />
            <ResourceNav resource="" idPrefix="master-data-unknown-nav" />
        </PageScaffold>
    )
}
