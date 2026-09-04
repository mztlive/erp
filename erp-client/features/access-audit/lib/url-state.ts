import type { AccessView } from "@/features/access-audit/types"

function parseView(raw: string | null): AccessView {
    // 数据范围收进主体详情、字段策略无后端资源（backend_gap）、用户授权已收拢到账号管理：
    // 旧 URL（含 view=users）回退到 roles
    if (raw === "roles" || raw === "audit") return raw
    return "roles"
}

export { parseView }
