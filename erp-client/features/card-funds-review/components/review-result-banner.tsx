"use client"

import Link from "next/link"

import { FormalActionResult } from "@/components/business"
import { type ResultState as SharedResultState } from "@/components/business/feedback"
import { Button } from "@/components/ui/button"
import type { FormalOutcome } from "@/features/card-funds-review/types"
import { openWorkspaceLabel } from "@/lib/ui-text"
import { buildResultFacts } from "../lib/result-facts"

type ResultState = SharedResultState<FormalOutcome>

/** 提交后的复核结果条（含驳回后继未配置提示与下一项动作）。 */
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
    const followUpConfiguration =
        lastResult.outcome?.kind === "REJECTED"
            ? lastResult.outcome.business.followUpConfiguration
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
                    ...(followUpConfiguration
                        ? [
                              {
                                  label: "后继流程未配置",
                                  value: followUpConfiguration.collaborationMessage,
                              },
                          ]
                        : []),
                ]}
                actions={
                    <div className="flex flex-wrap gap-2">
                        <Button
                            type="button"
                            disabled={lastResult.status === "unknown"}
                            onClick={onNext}
                        >
                            下一项
                        </Button>
                        {hasTask ? (
                            <Button
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
