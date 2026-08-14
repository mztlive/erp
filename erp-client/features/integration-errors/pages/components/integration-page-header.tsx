import { DataFreshness, PageHeader } from "@/components/business"
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
        <PageHeader
            title={
                focusMode
                    ? (itemNumber ?? "接口错误与对账中心")
                    : "接口错误与对账中心"
            }
            breadcrumbs={[
                {
                    id: "gov",
                    label: "治理",
                    href: "/governance/integration-errors",
                },
                {
                    id: "ie",
                    label: focusMode
                        ? (itemNumber ?? "详情")
                        : "接口错误与对账",
                    current: true,
                },
            ]}
            metadata={
                <DataFreshness
                    state="fresh"
                    label={freshnessText.dataUpdatedAt}
                    updatedAt={formatDateTime(updatedAt, "default")}
                    dateTime={updatedAt}
                />
            }
        />
    )
}
