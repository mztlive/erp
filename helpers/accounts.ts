/**
 * E2E 岗位账号目录。密码全部为开发默认 123456。
 * 同时提供角色键（sales）与登录名键（xiaoshou），供各 flow spec 的别名查找。
 */

export const DEV_PASSWORD = "123456"

export type AccountRecord = {
    account: string
    password: string
    name: string
    roleId?: string
    label: string
}

function record(
    account: string,
    name: string,
    roleId?: string,
): AccountRecord {
    return {
        account,
        password: DEV_PASSWORD,
        name,
        roleId,
        label: name,
    }
}

const byRole = {
    admin: record("admin", "系统管理员"),
    sales: record("xiaoshou", "销售", "role-sales"),
    salesLeader: record("lisiyong", "销售领导", "role-sales-leader"),
    procurement: record("caigou", "采购", "role-procurement"),
    operations: record("yunying", "运营", "role-operations"),
    warehouse: record("cangchu", "仓储", "role-warehouse"),
    finance: record("caiwu", "财务总监", "role-finance"),
    payment: record("fukuan", "出纳", "role-finance"),
    invoice: record("kaipiao", "开票人", "role-finance"),
    management: record("guanli", "管理层", "role-management"),
    sysadmin: record("xitong", "系统管理员", "role-sysadmin"),
} as const satisfies Record<string, AccountRecord>

export const ACCOUNTS = {
    ...byRole,
    xiaoshou: byRole.sales,
    lisiyong: byRole.salesLeader,
    caigou: byRole.procurement,
    yunying: byRole.operations,
    cangchu: byRole.warehouse,
    caiwu: byRole.finance,
    fukuan: byRole.payment,
    kaipiao: byRole.invoice,
    guanli: byRole.management,
    xitong: byRole.sysadmin,
    sales_leader: byRole.salesLeader,
    ops: byRole.operations,
} as const satisfies Record<string, AccountRecord>

export type AccountKey = keyof typeof ACCOUNTS

export type LoginIdentity =
    | string
    | {
          account?: string
          username?: string
          login?: string
          password?: string
      }

/**
 * 把登录名、角色键或凭据对象解析成账号/密码。
 * 找不到目录项时按登录名 + 默认密码回落。
 */
export function resolveAccount(
    identity: LoginIdentity,
    passwordOverride?: string,
): AccountRecord {
    if (typeof identity === "string") {
        const key = identity.trim()
        const direct = (ACCOUNTS as Record<string, AccountRecord | undefined>)[key]
        if (direct) {
            return passwordOverride
                ? { ...direct, password: passwordOverride }
                : direct
        }
        for (const row of Object.values(ACCOUNTS)) {
            if (row.account === key) {
                return passwordOverride
                    ? { ...row, password: passwordOverride }
                    : row
            }
        }
        return {
            account: key,
            password: passwordOverride || DEV_PASSWORD,
            name: key,
            label: key,
        }
    }

    const account =
        identity.account?.trim() ||
        identity.username?.trim() ||
        identity.login?.trim() ||
        ""
    if (!account) {
        throw new Error("登录凭据缺少 account")
    }
    const catalog = (ACCOUNTS as Record<string, AccountRecord | undefined>)[
        account
    ]
    const nested = catalog ?? Object.values(ACCOUNTS).find((row) => row.account === account)
    const password =
        passwordOverride || identity.password || nested?.password || DEV_PASSWORD
    return {
        account: nested?.account ?? account,
        password,
        name: nested?.name ?? account,
        roleId: nested?.roleId,
        label: nested?.label ?? nested?.name ?? account,
    }
}
