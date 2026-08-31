"use client"

import Link from "next/link"

import { FormalActionResult } from "@/components/business"
import { type ResultState as SharedResultState } from "@/components/business/feedback"
import { Button } from "@/components/ui/button"
import type { FormalOutcome } from "@/features/card-funds-review/types"
import { openWorkspaceLabel } from "@/lib/ui-text"
import { buildResultFacts } from "../lib/result-facts"

type ResultState = SharedResultState<FormalOutcome>

/** 提交后的复核结果条（含驳回后继待办与下一项动作）。 */
export function ReviewResultBanner({
    lastResult,
    onNext,
    w05Href,
    hasTask,
}: {
    lastResult: NonNullable<ResultState>
    onNext: () => void
    w05Href: string
    hasTask: boolean
}) {
    const followUpWorkItem =
        lastResult.outcome?.kind === "REJECTED"
            ? lastResult.outcome.business.followUpWorkItem
            : undefined

    return (
        <>
            <FormalActionResult
                status={
                    lastResult.status === "failed"
                        ? "blocked"
                        : lastResult.status
                }
                title={lastResult.title}
                description={lastResult.description}
                reference={lastResult.reference}
                facts={[
                    ...buildResultFacts(lastResult.outcome),
                    ...(followUpWorkItem
                        ? [
                              {
                                  label: "后继待办",
                                  value: `${followUpWorkItem.workItemId} · ${followUpWorkItem.status}`,
                              },
                          ]
                        : []),
                ]}
                actions={
                    <div className="flex flex-wrap gap-2">
                        <Button
                            id="card-contracts-funds-review-result-next"
                            type="button"
                            disabled={lastResult.status === "unknown"}
                            onClick={onNext}
                        >
                            下一项
                        </Button>
                        {hasTask ? (
                            <Button
                                id="card-contracts-funds-review-result-open-w05"
                                type="button"
                                variant="outline"
                                render={<Link href={w05Href} />}
                            >
                                {openWorkspaceLabel("W05")}
                            </Button>
                        ) : null}
                    </div>
                }
            />
        </>
    )
}
