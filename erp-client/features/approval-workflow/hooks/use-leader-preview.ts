"use client"
import { useQueries } from "@tanstack/react-query"
import { fetchOwnApprovalLeader } from "../api"
import type { ApprovalDefinitionNode } from "../types"

/** 当前账号预览不替代提交时解析，关闭确认后不持续轮询。 */
export function useLeaderPreview(nodes: readonly ApprovalDefinitionNode[]) {
    const kinds = [
        ...new Set(
            nodes.flatMap((node) =>
                node.assigneeSource === "APPLICANT_TEAM_LEADER"
                    ? ["team" as const]
                    : node.assigneeSource === "APPLICANT_DEPARTMENT_LEADER"
                      ? ["department" as const]
                      : [],
            ),
        ),
    ]
    const queries = useQueries({
        queries: kinds.map((kind) => ({
            queryKey: ["approval-leader", kind],
            queryFn: () => fetchOwnApprovalLeader(kind),
            staleTime: 0,
            retry: false,
        })),
    })
    return {
        loading: queries.some((q) => q.isPending),
        error: queries.find((q) => q.isError)?.error,
        nodes: nodes.map((node) => {
            const kind =
                node.assigneeSource === "APPLICANT_TEAM_LEADER"
                    ? "team"
                    : node.assigneeSource === "APPLICANT_DEPARTMENT_LEADER"
                      ? "department"
                      : undefined
            const result = kind ? queries[kinds.indexOf(kind)]?.data : undefined
            return result
                ? {
                      ...node,
                      assigneeName: `${result.name}（${result.organization_name}）`,
                  }
                : node
        }),
    }
}
