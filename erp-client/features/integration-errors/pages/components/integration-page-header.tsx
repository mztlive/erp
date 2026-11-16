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
            eyebrow="外部协同"
            title={
                focusMode
                    ? (itemNumber ?? "外部同步异常与对账")
                    : "外部同步异常与对账"
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
