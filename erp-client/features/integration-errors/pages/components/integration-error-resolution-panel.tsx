import {
    InterfaceErrorResolutionPanel,
    type InterfaceErrorClass,
} from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import type { IntegrationResolutionItemView } from "../../types"
import { mapPanelStatus } from "../lib/helpers"

export function IntegrationErrorResolutionPanel({
    item,
    errorClass,
}: {
    item: IntegrationResolutionItemView
    errorClass: InterfaceErrorClass
}) {
    return (
        <InterfaceErrorResolutionPanel
            errorClass={errorClass}
            status={mapPanelStatus(item)}
            businessImpact={
                <span>
                    {item.businessObject.title} · {item.fundsImpactLabel}
                    {item.compensationOpen ? " · 补偿未完成" : ""}
                </span>
            }
            latestAttempt={{
                attemptNumber: item.attempts[0]?.attemptNumber ?? 0,
                attemptedAt: {
                    dateTime: item.attempts[0]?.attemptedAt ?? item.createdAt,
                    label: formatDateTime(
                        item.attempts[0]?.attemptedAt ?? item.createdAt,
                        "default",
                    ),
                },
                result: item.attempts[0]?.result ?? "尚无尝试",
                requestSummary: item.attempts[0]?.requestSummary,
                responseSummary: item.attempts[0]?.responseSummary,
                nextRetryAt: item.attempts[0]?.nextRetryAt
                    ? {
                          dateTime: item.attempts[0].nextRetryAt,
                          label: formatDateTime(
                              item.attempts[0].nextRetryAt,
                              "default",
                          ),
                      }
                    : undefined,
            }}
            errorCode={item.classification.label}
        />
    )
}
