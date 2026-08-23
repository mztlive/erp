import type { AccessView } from "@/features/access-audit/types"

function parseView(raw: string | null): AccessView {
    // 数据范围收进主体详情、字段策略无后端资源（backend_gap）：旧 URL 回退到 roles
    if (raw === "roles" || raw === "users" || raw === "audit") return raw
    return "roles"
}

export { parseView }
