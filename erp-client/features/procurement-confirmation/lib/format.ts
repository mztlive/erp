/** 人民币金额展示（含税口径），页面与弹窗共用。 */
export const money = new Intl.NumberFormat("zh-CN", {
    style: "currency",
    currency: "CNY",
    minimumFractionDigits: 2,
})
