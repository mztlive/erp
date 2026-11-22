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
    flat,
}: {
    item: IntegrationResolutionItemView
    errorClass: InterfaceErrorClass
    flat?: boolean
}) {
    return (
        <InterfaceErrorResolutionPanel
            errorClass={errorClass}
            className={
                flat
                    ? "rounded-none border-0 border-t border-border/60 bg-transparent pt-5 pb-0 shadow-none [--card-spacing:0px] [&>[data-slot=card-content]]:px-0 [&>[data-slot=card-content]]:pt-4 [&>[data-slot=card-footer]]:border-border/60 [&>[data-slot=card-footer]]:px-0 [&>[data-slot=card-footer]]:pt-3 [&>[data-slot=card-header]]:border-border/60 [&>[data-slot=card-header]]:px-0 [&>[data-slot=card-header]]:pb-3"
                    : undefined
            }
            status={mapPanelStatus(item)}
            businessImpact={
                <span>
                    {item.businessObject.title} · {item.fundsImpactLabel}
                    {item.compensationOpen ? " · 后续补救未完成" : ""}
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
