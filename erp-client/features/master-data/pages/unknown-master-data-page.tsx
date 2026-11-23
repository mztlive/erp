"use client"

import { PageScaffold } from "@/components/business"
import {
    ListWorkspaceHeader,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { ResourceNav } from "@/features/master-data/components/list/list-chrome"
import { masterDataCopy } from "@/features/master-data/lib/copy"

export function UnknownMasterDataPage() {
    return (
        <PageScaffold
            density="compact"
            className={`${listWorkspaceStyles.page} gap-4`}
        >
            <ListWorkspaceHeader
                title={masterDataCopy.unknownResourceTitle}
                description={masterDataCopy.unknownResourceDesc()}
            />
            <ResourceNav resource="" idPrefix="master-data-unknown-nav" />
        </PageScaffold>
    )
}
