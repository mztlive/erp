import { DataFreshness } from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
import { formatDateTime } from "@/lib/datetime"
import { freshnessText } from "@/lib/ui-text"

export function IntegrationPageHeader({
    focusMode,
    itemNumber,
    updatedAt,
}: {
    focusMode: boolean
    itemNumber: string | undefined
    updatedAt: string | undefined
}) {
    return (
        <ListWorkspaceHeader
            eyebrow="治理"
            title={
                focusMode
                    ? (itemNumber ?? "接口错误与对账中心")
                    : "接口错误与对账中心"
            }
        >
            <DataFreshness
                state="fresh"
                label={freshnessText.dataUpdatedAt}
                updatedAt={formatDateTime(updatedAt, "default")}
                dateTime={updatedAt}
            />
        </ListWorkspaceHeader>
    )
}
