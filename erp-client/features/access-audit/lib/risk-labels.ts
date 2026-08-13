function riskLabel(flag: string) {
    const map: Record<string, string> = {
        HIGH_PRIVILEGE: "高权限",
        EMPTY_SCOPE: "空数据范围",
        EXPIRING_SOON: "即将过期",
        ACCESS_ADMIN: "权限管理",
        PENDING_DISABLE: "待停用",
        REVOKED: "已撤权",
    }
    return map[flag] ?? flag
}

export { riskLabel }
