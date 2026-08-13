import type { AccessView } from "@/features/access-audit/types"

function parseView(raw: string | null): AccessView {
    // 字段策略无后端资源（backend_gap），入口隐藏；旧 URL 回退到 roles
    if (
        raw === "roles" ||
        raw === "users" ||
        raw === "scopes" ||
        raw === "audit"
    ) {
        return raw
    }
    return "roles"
}

export { parseView }
