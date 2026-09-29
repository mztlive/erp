import { apiPost } from "@/lib/api"
export type AccessCheckInput = {
    user_id: string
    resource: string
    action: string
    object_id: string | null
}
export type AccessCheckView = {
    steps: {
        layer: string
        status: "passed" | "blocked" | "review"
        message: string
    }[]
    scope_version: string | null
    checked_at: string | null
}
export function checkAccess(input: AccessCheckInput) {
    return apiPost<AccessCheckView>("/admin/access-check", input)
}
