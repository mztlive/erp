/**
 * 测试账号（后端 accounts 集合，reset 时保留，不重置）。
 * 密码统一 123456；角色映射来自 casbin_rules（role:role-*）。
 */
export const ACCOUNTS = {
    /** 销售（role-sales） */
    sales: { account: "xiaoshou", password: "123456", name: "销售" },
    /** 采购（role-procurement） */
    procurement: { account: "caigou", password: "123456", name: "采购1" },
    /** 运营（role-operations） */
    operations: { account: "yunying", password: "123456", name: "运营" },
    /** 财务（role-finance） */
    finance: { account: "caiwu", password: "123456", name: "财务" },
    /** 销售领导（role-sales-leader） */
    salesLeader: { account: "lisiyong", password: "123456", name: "李思勇" },
    /** 超级管理员（role-root，用于发布审批定义等管理操作） */
    admin: { account: "admin", password: "123456", name: "System Admin" },
} as const

export type AccountKey = keyof typeof ACCOUNTS

/** 账号在 accounts 集合中的稳定 id（供审批定义 assignee 使用；reset 保留账号，id 不变）。 */
export const USER_IDS = {
    sales: "7e9e521afce041b79218edb9a246e974",
    procurement: "e9ca600460404aa48a1ff7b333933e3a",
    finance: "1c8219d55be14093b008f235850a4417",
    operations: "45734b6ee2df4b1bb3a0a5d49d7d1dd2",
    salesLeader: "d074e22850aa44a7908144a3ae8df806",
    admin: "7e647e47fc3049b9a0e0a8bf096ef24b",
} as const

export type UserIdKey = keyof typeof USER_IDS
